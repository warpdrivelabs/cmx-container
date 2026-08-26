//! egress 审计 —— cmx:http 每次出站裁决留痕。
//!
//! W4 起步用 tracing 事件 + 进程内环形缓冲（可查最近 N 条）；后续可接 `cmx_plugin_http_audit` 表
//! 或既有审计中心。审计**不含请求体/响应体**（避免泄露敏感数据），只记裁决元信息。

use std::collections::VecDeque;
use std::sync::Mutex;

/// 一条 egress 审计。
#[derive(Debug, Clone)]
pub struct EgressAudit {
    pub plugin_id: String,
    pub method: String,
    pub host: String,
    /// `true`=放行并出站；`false`=被策略拒绝。
    pub allowed: bool,
    /// 放行时的上游状态码；拒绝时为 None。
    pub status: Option<u16>,
    /// 拒绝/失败原因；放行成功为 None。
    pub reason: Option<String>,
}

/// 审计沉降接口。默认实现走 tracing + 环形缓冲。
pub trait EgressAuditor: Send + Sync {
    fn record(&self, entry: EgressAudit);
    /// 取最近若干条（诊断/大盘用）。默认返回空。
    fn recent(&self, _limit: usize) -> Vec<EgressAudit> {
        Vec::new()
    }
}

/// 默认审计器：tracing 事件 + 有界环形缓冲（默认容量 512）。
pub struct DefaultAuditor {
    buf: Mutex<VecDeque<EgressAudit>>,
    cap: usize,
}

impl DefaultAuditor {
    pub fn new() -> Self {
        Self {
            buf: Mutex::new(VecDeque::with_capacity(512)),
            cap: 512,
        }
    }
}

impl Default for DefaultAuditor {
    fn default() -> Self {
        Self::new()
    }
}

impl EgressAuditor for DefaultAuditor {
    fn record(&self, entry: EgressAudit) {
        if entry.allowed {
            tracing::info!(
                plugin = %entry.plugin_id, method = %entry.method, host = %entry.host,
                status = entry.status, "cmx:http egress 放行"
            );
        } else {
            tracing::warn!(
                plugin = %entry.plugin_id, method = %entry.method, host = %entry.host,
                reason = entry.reason.as_deref().unwrap_or(""), "cmx:http egress 拒绝"
            );
        }
        let mut buf = self.buf.lock().unwrap();
        if buf.len() >= self.cap {
            buf.pop_front();
        }
        buf.push_back(entry);
    }

    fn recent(&self, limit: usize) -> Vec<EgressAudit> {
        let buf = self.buf.lock().unwrap();
        buf.iter().rev().take(limit).cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_buffer_bounds_and_order() {
        let a = DefaultAuditor::new();
        for i in 0..5 {
            a.record(EgressAudit {
                plugin_id: format!("p{i}"),
                method: "GET".into(),
                host: "h".into(),
                allowed: true,
                status: Some(200),
                reason: None,
            });
        }
        let recent = a.recent(2);
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].plugin_id, "p4"); // 最新在前。
    }
}
