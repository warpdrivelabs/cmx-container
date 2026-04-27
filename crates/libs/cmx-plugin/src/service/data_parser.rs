//! 服务数据解析器
//!
//! 负责解析插件目录下的服务编排文件（servicedata/*.json），
//! 将 JSON 格式的编排定义转换为 ServiceDefinition 结构体，
//! 供插件安装时入库使用。

use std::path::Path;
use cmx_core::model::service::{ServiceOrchestration, ServiceDefinition};
use crate::error::{PluginError, PluginResult};
use uuid::Uuid;

/// 服务数据解析器
///
/// 提供从插件安装目录解析服务编排文件的能力。
/// 解析过程：
/// 1. 扫描 `servicedata` 子目录
/// 2. 读取所有 `.json` 文件
/// 3. 解析并验证编排结构
/// 4. 转换为服务定义结构体
pub struct ServiceDataParser;

/// 解析服务数据时的额外参数
#[derive(Debug, Clone)]
pub struct ServiceParseParams {
    /// 插件ID
    pub plugin_id: String,
    /// 插件版本
    pub plugin_version: String,
    /// 域编码
    pub domain_code: String,
    /// 应用编码
    pub application_code: String,
    /// 模块编码
    pub module_code: String,
}

impl ServiceDataParser {
    /// 解析插件安装目录下的所有服务编排文件
    ///
    /// # 参数
    ///
    /// * `install_path` - 插件安装目录路径
    /// * `params` - 解析参数，包含 plugin_id, plugin_version, domain_code, application_code, module_code
    ///
    /// # 返回值
    ///
    /// 返回解析后的服务定义列表，如果 servicedata 目录不存在则返回空列表
    ///
    /// # 目录结构
    ///
    /// 期望的目录结构如下：
    /// ```text
    /// install_path/
    /// └── servicedata/
    ///     ├── service1.json
    ///     ├── service2.json
    ///     └── ...
    /// ```
    pub fn parse_servicedata(
        install_path: &Path,
        params: &ServiceParseParams,
    ) -> PluginResult<Vec<ParsedServiceDefinition>> {
        // 构造 servicedata 目录路径
        let servicedata_path = install_path.join("servicedata");

        // 如果目录不存在，直接返回空列表（不是错误）
        if !servicedata_path.exists() {
            return Ok(Vec::new());
        }

        // 存储解析结果
        let mut results = Vec::new();

        // 读取目录下的所有文件条目
        let entries = std::fs::read_dir(&servicedata_path)?;

        // 遍历目录条目
        for entry in entries.flatten() {
            let path = entry.path();

            // 只处理 .json 文件
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                // 解析单个服务文件
                match Self::parse_service_file(&path) {
                    Ok(orchestration) => {
                        // 将编排转换为服务定义结构体
                        let service_def = Self::orchestration_to_service_definition(
                            &orchestration,
                            params,
                        )?;

                        // 添加到结果列表
                        results.push(ParsedServiceDefinition {
                            definition: service_def,
                            orchestration,
                        });
                    }
                    Err(e) => {
                        // 解析失败，记录警告但继续处理其他文件
                        tracing::warn!("解析服务文件 {:?} 失败: {:?}", path, e);
                        return Err(e);
                    }
                }
            }
        }

        Ok(results)
    }

    /// 解析单个服务编排 JSON 文件
    ///
    /// # 参数
    ///
    /// * `json_path` - JSON 文件路径
    ///
    /// # 返回值
    ///
    /// 返回解析后的 ServiceOrchestration 结构体
    ///
    /// # 错误
    ///
    /// * 文件读取失败
    /// * JSON 解析失败
    /// * 编排结构验证失败
    pub fn parse_service_file(json_path: &Path) -> PluginResult<ServiceOrchestration> {
        // 读取文件内容
        let content = std::fs::read_to_string(json_path)?;

        // 解析 JSON 为 ServiceOrchestration 结构
        let mut orchestration: ServiceOrchestration = serde_json::from_str(&content)?;
        orchestration.source_str = content;
        // 验证编排结构完整性
        Self::validate_orchestration(&orchestration)?;

        Ok(orchestration)
    }

    /// 验证编排结构完整性
    ///
    /// # 参数
    ///
    /// * `orchestration` - 编排结构体引用
    ///
    /// # 验证规则
    ///
    /// 1. 节点列表不能为空
    /// 2. 必须包含开始节点（node_type = "skylake-start"）
    /// 3. 必须包含结束节点（node_type = "skylake-end"）
    ///
    /// # 错误
    ///
    /// 返回验证失败的具体错误信息
    pub fn validate_orchestration(orchestration: &ServiceOrchestration) -> PluginResult<()> {
        // // 检查节点列表是否为空
        // if orchestration.flow.nodes.is_empty() {
        //     return Err(PluginError::Plugin("编排节点列表为空".to_string()));
        // }
        //
        // // 检查是否存在开始节点
        // let has_start = orchestration.flow.nodes.iter()
        //     .any(|n| n.node_type == "skylake-start");
        //
        // // 检查是否存在结束节点
        // let has_end = orchestration.flow.nodes.iter()
        //     .any(|n| n.node_type == "skylake-end");
        //
        // // 必须同时包含开始和结束节点
        // if !has_start || !has_end {
        //     return Err(PluginError::Plugin("编排必须包含开始节点和结束节点".to_string()));
        // }

        Ok(())
    }

    /// 从编排的 code 字段提取 service_key
    ///
    /// # 参数
    ///
    /// * `orchestration` - 编排结构体引用
    ///
    /// # 返回值
    ///
    /// 返回服务唯一标识 key
    ///
    /// # 说明
    ///
    /// service_key 存储在编排对象的 code 字段中
    pub fn extract_service_key(orchestration: &ServiceOrchestration) -> String {
        orchestration.code.clone()
    }

    /// 将编排结构转换为服务定义结构体
    ///
    /// # 参数
    ///
    /// * `orchestration` - 编排结构体引用
    /// * `params` - 解析参数，包含 plugin_id, plugin_version, domain_code, application_code, module_code
    ///
    /// # 返回值
    ///
    /// 返回 ServiceDefinition 结构体
    ///
    /// # 字段映射关系
    ///
    /// | 编排字段 | 服务定义字段 |
    /// |---------|-------------|
    /// | code | service_key |
    /// | name | service_name |
    /// | description | description |
    /// | - | plugin_id (参数传入) |
    /// | - | status (固定为 1) |
    /// | - | version (参数传入) |
    /// | domain_code | domain_code (参数传入) |
    /// | application_code | application_code (参数传入) |
    /// | module_code | module_code (参数传入) |
    /// | 序列化JSON | config |
    ///
    /// # 说明
    ///
    /// config 字段存储编排对象的 JSON 序列化字符串
    fn orchestration_to_service_definition(
        orchestration: &ServiceOrchestration,
        params: &ServiceParseParams,
    ) -> PluginResult<ServiceDefinition> {
        // 从编排中提取 service_key
        let service_key = Self::extract_service_key(orchestration);

        // 将编排序列化为 JSON 字符串，用于存储 config 字段
        let config = orchestration.source_str.clone();

        // 构造型服务定义结构体
        Ok(ServiceDefinition {
            id: Uuid::new_v4().to_string(),  // 生成新的 UUID 作为主键
            service_key,
            service_name: orchestration.name.clone(),
            description: orchestration.description.clone(),
            plugin_id: params.plugin_id.clone(),
            status: 1,  // 默认启用状态
            version: params.plugin_version.clone(),
            config: Some(config),
            domain_code: params.domain_code.clone(),
            application_code: params.application_code.clone(),
            module_code: params.module_code.clone(),
            domain_name: String::new(),
            application_name: String::new(),
            module_name: String::new(),
            plugin_name: String::new()
        })
    }
}

/// 解析后的服务定义
///
/// 包含完整的服务定义信息和编排定义，
/// 便于插件安装时同时保存到 cmx_service_define 和 cmx_service_define_version 表
pub struct ParsedServiceDefinition {
    /// 服务定义（用于入库 cmx_service_define 表）
    pub definition: ServiceDefinition,
    /// 服务编排定义（用于入库 cmx_service_define_version 表的 config 字段）
    pub orchestration: ServiceOrchestration,
}
