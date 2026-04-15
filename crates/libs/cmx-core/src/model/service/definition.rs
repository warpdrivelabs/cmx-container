//! 服务定义模块
//!
//! 包含服务定义和服务运行时信息结构体。

use serde::{Deserialize, Serialize};

/// 服务定义 — 对应 cmx_service_define 表
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceDefinition {
    /// 主键ID
    pub id: String,
    /// 服务唯一标识（来自 JSON 的 code 字段）
    pub service_key: String,
    /// 服务名称
    pub service_name: String,
    /// 服务描述
    pub description: String,
    /// 所属插件ID
    pub plugin_id: String,
    /// 状态：0-禁用，1-启用
    pub status: i32,
    /// 服务版本
    pub version: String,
    /// 服务编排配置
    pub config: Option<String>,
    /// 所属域代码
    pub domain_code: String,
    /// 所属应用代码
    pub application_code: String,
    /// 所属模块代码
    pub module_code: String,
    /// 所属域名称（联查获取）
    #[serde(default)]
    pub domain_name: String,
    /// 所属应用名称（联查获取）
    #[serde(default)]
    pub application_name: String,
    /// 所属模块名称（联查获取）
    #[serde(default)]
    pub module_name: String,
}

/// 服务运行时信息 — 内存缓存用
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInfo {
    /// 主键ID
    pub id: String,
    /// 服务唯一标识
    pub service_key: String,
    /// 服务名称
    pub service_name: String,
    /// 服务描述
    pub description: String,
    /// 所属插件ID
    pub plugin_id: String,
    /// 状态：0-禁用，1-启用
    pub status: i32,
    /// 当前版本号
    pub version: String,
    /// 编排配置 JSON
    pub config: String,
    /// 所属域代码
    pub domain_code: String,
    /// 所属应用代码
    pub application_code: String,
    /// 所属模块代码
    pub module_code: String,
    /// 所属域名称（联查获取）
    #[serde(default)]
    pub domain_name: String,
    /// 所属应用名称（联查获取）
    #[serde(default)]
    pub application_name: String,
    /// 所属模块名称（联查获取）
    #[serde(default)]
    pub module_name: String,
}

impl From<ServiceDefinition> for ServiceInfo {
    fn from(def: ServiceDefinition) -> Self {
        Self {
            id: def.id,
            service_key: def.service_key,
            service_name: def.service_name,
            description: def.description,
            plugin_id: def.plugin_id,
            status: def.status,
            version: def.version,
            config: def.config.unwrap_or_default(),
            domain_code: def.domain_code,
            application_code: def.application_code,
            module_code: def.module_code,
            domain_name: def.domain_name,
            application_name: def.application_name,
            module_name: def.module_name,
        }
    }
}

impl From<ServiceInfo> for ServiceDefinition {
    fn from(info: ServiceInfo) -> Self {
        Self {
            id: info.id,
            service_key: info.service_key,
            service_name: info.service_name,
            description: info.description,
            plugin_id: info.plugin_id,
            status: info.status,
            version: info.version,
            config: if info.config.is_empty() { None } else { Some(info.config) },
            domain_code: info.domain_code,
            application_code: info.application_code,
            module_code: info.module_code,
            domain_name: info.domain_name,
            application_name: info.application_name,
            module_name: info.module_name,
        }
    }
}
