//! W2-a 调试自持稳态化 —— 会话可观测 + 扩展可达性探测。
//!
//! 现状：`/debug` 自调试回调打到 code-server 扩展的 Express `:9000`，扩展进程挂则调试链断且缺清晰
//! 诊断。本模块**不改既有调试流**，只补两件事让链路稳态可观测：
//! 1. [`session_stats`]：调试会话管理器快照（总数/活跃数/已 attach 数），供 `/_mon` 大盘。
//! 2. [`probe_extension`]：显式探测扩展 `:9000` 可达性，返回**typed** 结果（可达/超时/错误），
//!    让上层在扩展不可用时给明确错误而非静默兜底。

use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::{is_debugger_attached, DEBUG_SESSIONS};

/// 调试会话管理器统计快照。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebugSessionStats {
    /// 会话总数。
    pub total: usize,
    /// 标记 active 的会话数。
    pub active: usize,
    /// 实际探测到 LLDB/CodeLLDB 已 attach 的会话数。
    pub attached: usize,
}

/// 取会话统计快照（不改变任何会话状态）。
pub fn session_stats() -> DebugSessionStats {
    let sessions = DEBUG_SESSIONS.lock().unwrap();
    let total = sessions.len();
    let active = sessions.values().filter(|s| s.is_active).count();
    let attached = sessions
        .values()
        .filter(|s| is_debugger_attached(s.cmx_pid))
        .count();
    DebugSessionStats {
        total,
        active,
        attached,
    }
}

/// 扩展可达性探测结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", tag = "status", rename_all_fields = "camelCase")]
pub enum ExtensionProbe {
    /// 可达，返回其上报的 code_server_url。
    Reachable { code_server_url: Option<String> },
    /// 超时（扩展进程可能已挂）。
    Timeout,
    /// 连接/解析错误。
    Error { detail: String },
}

impl ExtensionProbe {
    pub fn is_reachable(&self) -> bool {
        matches!(self, ExtensionProbe::Reachable { .. })
    }
}

/// 探测 code-server 扩展的 Express 服务（`/config`）可达性。`timeout` 建议 ≤2s。
///
/// 与既有 `get_code_server_url_async` 的区别：那个**静默兜底默认 URL**（适合取值），本函数返回
/// **typed 诊断**（适合健康检查/降级判断），让调用方知道"扩展到底活没活"。
pub async fn probe_extension(timeout: Duration) -> ExtensionProbe {
    let plugin_port = std::env::var("PLUGIN_PORT").unwrap_or_else(|_| "9000".to_string());
    let url = format!("http://localhost:{plugin_port}/config");

    let client = match reqwest::Client::builder().timeout(timeout).build() {
        Ok(c) => c,
        Err(e) => return ExtensionProbe::Error { detail: format!("构造 client 失败: {e}") },
    };

    match client.get(&url).send().await {
        Ok(resp) => match resp.json::<serde_json::Value>().await {
            Ok(json) => ExtensionProbe::Reachable {
                code_server_url: json
                    .get("code_server_url")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
            },
            Err(e) => ExtensionProbe::Error { detail: format!("响应解析失败: {e}") },
        },
        Err(e) if e.is_timeout() => ExtensionProbe::Timeout,
        Err(e) => ExtensionProbe::Error { detail: e.to_string() },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_on_empty_manager() {
        crate::clear_all_sessions();
        let s = session_stats();
        assert_eq!(s.total, 0);
        assert_eq!(s.active, 0);
        assert_eq!(s.attached, 0);
    }

    #[test]
    fn probe_serde_shape() {
        let r = ExtensionProbe::Reachable {
            code_server_url: Some("http://x".into()),
        };
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["status"], "reachable");
        assert_eq!(v["codeServerUrl"], "http://x");
        let t = serde_json::to_value(ExtensionProbe::Timeout).unwrap();
        assert_eq!(t["status"], "timeout");
    }
}
