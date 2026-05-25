//! cmx-traits trait 实现
//!
//! 为 PluginManager 实现 cmx-traits 中定义的 trait，
//! 实现跨模块解耦的接口适配层。

use std::path::PathBuf;

use async_trait::async_trait;
use cmx_traits::{PluginFilter as TraitsPluginFilter, PluginQuery, PluginSnapshot, TraitError};

use crate::core::manager::PluginManager;
use crate::domain::plugin::{PluginFilter as DomainPluginFilter, PluginInfo, PluginStatus};
use crate::infrastructure::database::plugin::model::PluginRecord;

/// 从 PluginInfo 转换为 PluginSnapshot
///
/// 注意：PluginInfo 不包含 wasm_path，需要额外查询。
impl From<PluginInfo> for PluginSnapshot {
    fn from(info: PluginInfo) -> Self {
        Self {
            plugin_id: info.id,
            name: info.name,
            version: info.version,
            status: info.status.to_string(),
            install_path: info.install_path.to_string_lossy().to_string(),
            wasm_path: None,
            plugin_type: info.plugin_type,
            domain_code: info.domain_code,
            application_code: info.application_code,
            module_code: info.module_code,
            source_path: info.source_path,
        }
    }
}

/// 从 PluginRecord 转换为 PluginSnapshot
///
/// PluginRecord 包含完整的 wasm_path 信息。
impl From<PluginRecord> for PluginSnapshot {
    fn from(record: PluginRecord) -> Self {
        Self {
            plugin_id: record.plugin_id,
            name: record.name,
            version: record.version,
            status: record.status,
            install_path: record.install_path.clone(),
            wasm_path: Some(record.wasm_path.clone()),
            plugin_type: record.plugin_type.unwrap_or_else(|| "wasm".to_string()),
            domain_code: record.domain_code.unwrap_or_default(),
            application_code: record.application_code.unwrap_or_default(),
            module_code: record.module_code.unwrap_or_default(),
            source_path: record.source_path,
        }
    }
}

/// 从 cmx-traits 的 PluginFilter 转换为 cmx-plugin 的 PluginFilter
impl From<TraitsPluginFilter> for DomainPluginFilter {
    fn from(filter: TraitsPluginFilter) -> Self {
        Self {
            app_id: None,
            status: filter.status.and_then(|s| s.parse().ok()),
            name: filter.name,
            domain_code: filter.domain_code,
            application_code: filter.application_code,
            module_code: filter.module_code,
        }
    }
}

/// 为 PluginManager 实现 PluginQuery trait
#[async_trait]
impl PluginQuery for PluginManager {
    /// 根据插件ID查询插件快照
    async fn get_plugin(&self, plugin_id: &str) -> Result<Option<PluginSnapshot>, TraitError> {
        // 先从数据库查询完整记录（包含 wasm_path）
        let record = self.repository()
            .find_plugin(plugin_id, self.app_id())
            .await
            .map_err(|e| TraitError::Internal(format!("查询插件失败: {}", e)))?;

        match record {
            Some(r) => Ok(Some(PluginSnapshot::from(r))),
            None => {
                // 回退到内存注册表
                let info = self.get_plugin(plugin_id).await
                    .map_err(|e| TraitError::Internal(format!("查询插件失败: {}", e)))?;
                Ok(info.map(PluginSnapshot::from))
            }
        }
    }

    /// 检查插件是否已安装
    async fn is_installed(&self, plugin_id: &str) -> Result<bool, TraitError> {
        // // 先检查数据库
        // let record = self.repository()
        //     .find_plugin(plugin_id)
        //     .await
        //     .map_err(|e| TraitError::Internal(format!("查询插件失败: {}", e)))?;
        //
        // if record.is_some() {
        //     return Ok(true);
        // }

        // 回退到内存注册表检查
        let info = self.get_plugin(plugin_id).await
            .map_err(|e| TraitError::Internal(format!("查询插件失败: {}", e)))?;

        Ok(info.is_some())
    }

    /// 检查插件是否已激活（当前总是返回 false，插件激活功能未实现）
    async fn is_active(&self, _plugin_id: &str) -> Result<bool, TraitError> {
        Ok(false)
    }

    /// 获取插件的 WASM 文件绝对路径
    async fn get_wasm_path(&self, plugin_id: &str) -> Result<PathBuf, TraitError> {
        // 从数据库获取完整记录
        let record = self.repository()
            .find_plugin(plugin_id, self.app_id())
            .await
            .map_err(|e| TraitError::Internal(format!("查询插件失败: {}", e)))?;

        match record {
            Some(r) if !r.wasm_path.is_empty() => Ok(PathBuf::from(r.wasm_path)),
            Some(_) => {
                // wasm_path 为空，尝试从 context 获取
                let context = self.get_context(plugin_id).await;
                match context {
                    Some(ctx) if !ctx.wasm_path.as_os_str().is_empty() => Ok(ctx.wasm_path),
                    _ => Err(TraitError::Internal(format!(
                        "插件 {} 未配置 WASM 路径",
                        plugin_id
                    ))),
                }
            }
            None => Err(TraitError::PluginNotFound(plugin_id.to_string())),
        }
    }

    // /// 列出所有已激活的插件快照
    // async fn list_active_plugins(&self) -> Result<Vec<PluginSnapshot>, TraitError> {
    //     // 使用 filter 筛选已激活的插件
    //     let domain_filter = DomainPluginFilter {
    //         status: Some(PluginStatus::Activated),
    //         ..Default::default()
    //     };
    //     let infos = self.list_plugins(&domain_filter).await
    //         .map_err(|e| TraitError::Internal(format!("查询插件列表失败: {}", e)))?;
    //
    //     // 转换结果，补充 wasm_path
    //     let mut snapshots = Vec::new();
    //     for info in infos {
    //         if let Ok(Some(record)) = self.repository().find_plugin(&info.id, self.app_id()).await {
    //             snapshots.push(PluginSnapshot::from(record));
    //         } else {
    //             snapshots.push(PluginSnapshot::from(info));
    //         }
    //     }
    //
    //     Ok(snapshots)
    // }

    /// 根据筛选条件查询插件列表
    async fn list_plugins(&self, filter: &TraitsPluginFilter) -> Result<Vec<PluginSnapshot>, TraitError> {
        let domain_filter = DomainPluginFilter::from(filter.clone());
        let infos = self.list_plugins(&domain_filter).await
            .map_err(|e| TraitError::Internal(format!("查询插件列表失败: {}", e)))?;

        // 转换结果，补充 wasm_path
        let mut snapshots = Vec::new();
        for info in infos {
            if let Ok(Some(record)) = self.repository().find_plugin(&info.id, self.app_id()).await {
                snapshots.push(PluginSnapshot::from(record));
            } else {
                snapshots.push(PluginSnapshot::from(info));
            }
        }

        Ok(snapshots)
    }
}
