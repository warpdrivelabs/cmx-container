//! 触发绑定模型（W3）—— 把"事件主题 / cron / 业务钩子"绑到"插件函数"。
//!
//! 对应两张表 `cmx_plugin_event_binding` / `cmx_plugin_cron_binding`（store 在平台侧实现，本 crate
//! 只定义 DTO + 契约，保持零基础设施依赖）。

use serde::{Deserialize, Serialize};

/// 触发类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TriggerKind {
    /// 事件订阅（event_bus 主题）。
    Event,
    /// 定时任务（cron 表达式）。
    Cron,
    /// 业务钩子（流程 serviceTask / 规则 businessRuleTask）。
    BizHook,
}

/// 一条触发绑定：某触发源 → 某插件的某函数。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TriggerBinding {
    #[serde(default)]
    pub id: i64,
    pub kind: TriggerKind,
    /// 触发键：Event=topic；Cron=cron 表达式；BizHook=hook 标识（如 `flow:serviceTask:<nodeKey>`）。
    pub trigger_key: String,
    pub plugin_id: String,
    pub function_name: String,
    /// 租户（多租户隔离；空=默认租户）。
    #[serde(default)]
    pub tenant_id: Option<String>,
    /// 是否启用（停用即不触发）。
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// 可选：事件载荷过滤表达式（预留；为空则不过滤）。
    #[serde(default)]
    pub filter_expr: Option<String>,
    /// 可选：定时/钩子的静态附加载荷（合并进 FunctionInput）。
    #[serde(default)]
    pub payload_json: Option<serde_json::Value>,
}

fn default_true() -> bool {
    true
}

impl TriggerBinding {
    /// 构造一个事件绑定（测试/种子便捷）。
    pub fn event(topic: impl Into<String>, plugin_id: impl Into<String>, function_name: impl Into<String>) -> Self {
        Self {
            id: 0,
            kind: TriggerKind::Event,
            trigger_key: topic.into(),
            plugin_id: plugin_id.into(),
            function_name: function_name.into(),
            tenant_id: None,
            enabled: true,
            filter_expr: None,
            payload_json: None,
        }
    }

    /// 构造一个定时绑定。
    pub fn cron(expr: impl Into<String>, plugin_id: impl Into<String>, function_name: impl Into<String>) -> Self {
        Self {
            id: 0,
            kind: TriggerKind::Cron,
            trigger_key: expr.into(),
            plugin_id: plugin_id.into(),
            function_name: function_name.into(),
            tenant_id: None,
            enabled: true,
            filter_expr: None,
            payload_json: None,
        }
    }
}
