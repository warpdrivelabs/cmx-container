//! 服务定义模块
//!
//! 包含服务定义和服务运行时信息结构体。

use serde::{Deserialize, Serialize};

/// 服务定义 — 对应 cmx_service_define 表
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceDefinition {
    /// 主键ID
    pub id: String,
    /// 应用隔离标识
    pub app_id: String,
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
    /// 所属插件名称（联查获取）
    #[serde(default)]
    pub plugin_name: String,
    /// 服务api 联查获取）
    pub api_doc: Option<String>,
}
