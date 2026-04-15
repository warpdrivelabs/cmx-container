//! 服务解析与保存工具模块
//!
//! 提供插件安装/升级时解析和保存服务定义的共用函数。

use std::path::Path;
use std::sync::Arc;
use cmx_core::model::meta::plugin::PluginDefinition;
use cmx_core::model::service::ServiceDefinition;
use crate::error::{PluginError, PluginResult};
use crate::service::data_parser::{ParsedServiceDefinition, ServiceDataParser};

/// 从插件目录解析服务定义（不保存到数据库）
///
/// 专门用于降级场景，从旧版本插件目录解析实际包含的服务定义列表，
/// 用于确定降级后需要删除哪些新增的服务。
///
/// # 参数
/// * `install_path` - 插件安装路径（应指向具体版本目录，如 plugin_id/v1.0.0/）
/// * `plugin_id` - 插件ID
/// * `plugin_version` - 插件版本
///
/// # 返回值
/// * `Ok(Vec<ServiceDefinition>)` - 解析出的服务定义列表（不包含编排配置）
///
/// # 与 parse_and_save_services 的区别
/// * parse_and_save_services: 解析并保存到数据库，用于安装/升级
/// * parse_services_from_plugin_dir: 只解析不保存，用于降级时获取旧版本实际服务列表
pub fn parse_services_from_plugin_dir(
    install_path: &Path,
    plugin_id: &str,
    plugin_version: &str,
) -> PluginResult<Vec<ServiceDefinition>> {
    let parsed = match ServiceDataParser::parse_servicedata(install_path, plugin_id, plugin_version) {
        Ok(services) => services,
        Err(e) => {
            tracing::warn!("解析服务数据失败: {:?}", e);
            return Err(PluginError::Plugin(format!("解析服务数据失败: {:?}", e)));
        }
    };

    Ok(parsed.into_iter().map(|p| p.definition).collect())
}

/// 解析并保存服务定义
///
/// 在插件安装或升级时调用，解析插件目录下的服务编排文件，
/// 并保存到数据库。
///
/// # 参数
/// * `install_path` - 插件安装路径
/// * `plugin_id` - 插件ID
/// * `plugin_version` - 插件版本
/// * `service_storage` - 服务存储接口
/// * `db_id` - 默认数据库ID
/// * `txn_id` - 事务ID（可选，用于在同一事务中完成保存）
///
/// # 返回值
/// * `Ok(Vec<ParsedServiceDefinition>)` - 解析出的服务定义列表
///
/// # 处理流程
/// 1. 解析 servicedata 目录下的所有 JSON 文件
/// 2. 对每个服务定义调用 save_service 保存
/// 3. 对每个服务版本调用 save_service_version 保存
pub async fn parse_and_save_services(
    install_path: &Path,
    plugin_id: &str,
    plugin_version: &str,
    service_storage: &Arc<dyn cmx_traits::ServiceStorage>,
    txn_id: Option<&str>,
) -> PluginResult<Vec<ParsedServiceDefinition>> {
    // 解析插件安装目录下的服务数据
    let parsed = match ServiceDataParser::parse_servicedata(install_path, plugin_id, plugin_version) {
        Ok(services) => services,
        Err(e) => {
            tracing::warn!("解析服务数据失败: {:?}", e);
            return Err(PluginError::Plugin(format!("解析服务数据失败: {:?}", e)));
        }
    };

    // 遍历并保存每个服务
    for svc in &parsed {
        // 保存服务定义（使用 db_id 和 txn_id）
        if let Err(e) = service_storage.save_service(
            &svc.definition,
            txn_id,
        ).await {
            tracing::error!("保存服务定义 {} 失败: {:?}", svc.definition.service_key, e);
            return Err(PluginError::Plugin(format!("保存服务定义失败: {:?}", e)));
        }

        // 获取编排配置的 JSON 字符串
        // 优先使用 source_str（原始 JSON 字符串），否则序列化
        let config = if !svc.orchestration.source_str.is_empty() {
            svc.orchestration.source_str.clone()
        } else {
            serde_json::to_string(&svc.orchestration)
                .map_err(|e| PluginError::Plugin(format!("序列化编排配置失败: {}", e)))?
        };

        // 保存服务版本
        if let Err(e) = service_storage.save_service_version(
            &svc.definition.service_key,
            plugin_version,
            plugin_id,
            plugin_version,
            &config,
            txn_id,
        ).await {
            tracing::error!(
                "保存服务版本 {}:{} 失败: {:?}",
                svc.definition.service_key,
                plugin_version,
                e
            );
            return Err(PluginError::Plugin(format!("保存服务版本失败: {:?}", e)));
        }
    }

    Ok(parsed)
}
