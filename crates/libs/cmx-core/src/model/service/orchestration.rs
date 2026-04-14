//! 服务编排定义模块
//!
//! 包含服务编排结构，从 JSON 解析而来。

use serde::{Deserialize, Serialize};
use super::flow::ServiceFlow;

/// 服务编排定义 — 从 服务.json 解析
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceOrchestration {
    /// 编排名称
    pub name: String,
    /// 服务key（唯一标识）
    pub code: String,
    /// 描述信息
    pub description: String,
    /// 流程定义
    pub flow: ServiceFlow,
    /// 所属域代码
    #[serde(default)]
    pub domain_code: Option<String>,
    /// 所属应用代码
    #[serde(default)]
    pub application_code: Option<String>,
    /// 所属模块代码
    #[serde(default)]
    pub module_code: Option<String>,
    /// 原始json字符
    #[serde(skip)]
    pub source_str: String,
}
