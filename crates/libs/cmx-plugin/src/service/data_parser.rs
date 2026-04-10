//! 服务数据解析器
//!
//! 解析插件目录下的服务编排文件。

use std::path::Path;
use cmx_core::model::service::{ServiceOrchestration, ServiceDefinition};
use crate::error::{PluginError, PluginResult};
use uuid::Uuid;

/// 服务数据解析器
pub struct ServiceDataParser;

impl ServiceDataParser {
    /// 解析插件安装目录下的所有服务编排文件
    pub fn parse_servicedata(
        install_path: &Path,
        plugin_id: &str,
        plugin_version: &str,
    ) -> PluginResult<Vec<ParsedServiceDefinition>> {
        let servicedata_path = install_path.join("servicedata");

        if !servicedata_path.exists() {
            return Ok(Vec::new());
        }

        let mut results = Vec::new();
        let entries = std::fs::read_dir(&servicedata_path)?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                match Self::parse_service_file(&path) {
                    Ok(orchestration) => {
                        let service_def = Self::orchestration_to_service_definition(
                            &orchestration,
                            plugin_id,
                            plugin_version,
                        )?;
                        results.push(ParsedServiceDefinition {
                            definition: service_def,
                            orchestration,
                        });
                    }
                    Err(e) => {
                        tracing::warn!("解析服务文件 {:?} 失败: {:?}", path, e);
                    }
                }
            }
        }

        Ok(results)
    }

    /// 解析单个服务编排 JSON 文件
    pub fn parse_service_file(json_path: &Path) -> PluginResult<ServiceOrchestration> {
        let content = std::fs::read_to_string(json_path)?;

        let orchestration: ServiceOrchestration = serde_json::from_str(&content)?;

        Self::validate_orchestration(&orchestration)?;

        Ok(orchestration)
    }

    /// 验证编排结构完整性
    pub fn validate_orchestration(orchestration: &ServiceOrchestration) -> PluginResult<()> {
        if orchestration.flow.nodes.is_empty() {
            return Err(PluginError::Plugin("编排节点列表为空".to_string()));
        }

        let has_start = orchestration.flow.nodes.iter()
            .any(|n| n.node_type == "skylake-start");
        let has_end = orchestration.flow.nodes.iter()
            .any(|n| n.node_type == "skylake-end");

        if !has_start || !has_end {
            return Err(PluginError::Plugin("编排必须包含开始节点和结束节点".to_string()));
        }

        Ok(())
    }

    /// 从编排的 code 字段提取 service_key
    pub fn extract_service_key(orchestration: &ServiceOrchestration) -> String {
        orchestration.code.clone()
    }

    fn orchestration_to_service_definition(
        orchestration: &ServiceOrchestration,
        plugin_id: &str,
        plugin_version: &str,
    ) -> PluginResult<ServiceDefinition> {
        let service_key = Self::extract_service_key(orchestration);

        Ok(ServiceDefinition {
            id: Uuid::new_v4().to_string(),
            service_key,
            service_name: orchestration.name.clone(),
            description: orchestration.description.clone(),
            plugin_id: plugin_id.to_string(),
            status: 1,
            version: plugin_version.to_string(),
        })
    }
}

/// 解析后的服务定义
pub struct ParsedServiceDefinition {
    pub definition: ServiceDefinition,
    pub orchestration: ServiceOrchestration,
}
