//! DAP 桥调度（W2-b 骨架）—— 把解帧后的 DAP 请求按 command 路由，attach 到 cmx-debug 会话。
//!
//! **范围诚实**：完整的 DAP 桥需把 setBreakpoints/stackTrace/variables 等转发给真 LLDB（经
//! cmx-debug attach 的进程），那是重工程且依赖 LLDB 版本。本骨架落地"传输 + 会话生命周期 +
//! initialize/attach/disconnect 三个不依赖 LLDB 转发的命令"，把自建 DAP 的地基做扎实且可单测；
//! 断点/单步等转发命令返回明确"未实现（需 LLDB 转发集成）"而非静默假成功。

use serde_json::json;

use crate::protocol::{DapMessage, DapType};

/// 桥会话状态。
#[derive(Debug, Default)]
pub struct DapBridge {
    seq: i64,
    /// 已 attach 的目标 pid（None=未 attach）。
    attached_pid: Option<u32>,
    initialized: bool,
}

/// 一次处理的产出：0..N 条外发消息（response + 可能的 event）。
pub type Outgoing = Vec<DapMessage>;

impl DapBridge {
    pub fn new() -> Self {
        Self::default()
    }

    fn next_seq(&mut self) -> i64 {
        self.seq += 1;
        self.seq
    }

    /// 当前是否已 attach。
    pub fn is_attached(&self) -> bool {
        self.attached_pid.is_some()
    }

    /// 处理一条入站消息，返回应外发的消息列表。非 request 一律忽略（返回空）。
    pub fn handle(&mut self, msg: &DapMessage) -> Outgoing {
        if msg.msg_type != DapType::Request {
            return Vec::new();
        }
        let req_seq = msg.seq;
        let command = msg.command.as_deref().unwrap_or("");
        match command {
            "initialize" => {
                self.initialized = true;
                let s = self.next_seq();
                // 上报支持的能力（保守子集）+ initialized 事件。
                let body = json!({
                    "supportsConfigurationDoneRequest": true,
                    "supportsTerminateRequest": true
                });
                let resp = DapMessage::response(s, req_seq, command, Some(body));
                let e = self.next_seq();
                vec![resp, DapMessage::event(e, "initialized", None)]
            }
            "attach" => {
                let pid = msg
                    .arguments
                    .as_ref()
                    .and_then(|a| a.get("pid"))
                    .and_then(|v| v.as_u64())
                    .map(|v| v as u32);
                match pid {
                    Some(pid) => {
                        // 复用 cmx-debug 探测该 pid 是否真有 LLDB attach（诊断用；桥本身不启动 LLDB）。
                        let already = cmx_debug::is_debugger_attached(pid);
                        self.attached_pid = Some(pid);
                        let s = self.next_seq();
                        let resp = DapMessage::response(
                            s,
                            req_seq,
                            command,
                            Some(json!({ "pid": pid, "lldbAttached": already })),
                        );
                        vec![resp]
                    }
                    None => {
                        let s = self.next_seq();
                        vec![DapMessage::error(s, req_seq, command, "attach 缺 pid 参数")]
                    }
                }
            }
            "configurationDone" => {
                let s = self.next_seq();
                vec![DapMessage::response(s, req_seq, command, None)]
            }
            "disconnect" | "terminate" => {
                self.attached_pid = None;
                let s = self.next_seq();
                vec![DapMessage::response(s, req_seq, command, None)]
            }
            // 需 LLDB 转发的命令：明确未实现（不假成功）。
            "setBreakpoints" | "stackTrace" | "variables" | "continue" | "next" | "stepIn" | "stepOut"
            | "threads" | "scopes" | "evaluate" => {
                let s = self.next_seq();
                vec![DapMessage::error(
                    s,
                    req_seq,
                    command,
                    "该命令需 LLDB 转发集成（W2-b 骨架未实现转发）",
                )]
            }
            other => {
                let s = self.next_seq();
                vec![DapMessage::error(s, req_seq, other, &format!("未知命令: {other}"))]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::DapMessage;
    use serde_json::json;

    fn req(seq: i64, cmd: &str, args: Option<serde_json::Value>) -> DapMessage {
        DapMessage {
            seq,
            msg_type: DapType::Request,
            command: Some(cmd.into()),
            arguments: args,
            request_seq: None,
            success: None,
            body: None,
            event: None,
            message: None,
        }
    }

    #[test]
    fn initialize_returns_response_and_event() {
        let mut b = DapBridge::new();
        let out = b.handle(&req(1, "initialize", None));
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].msg_type, DapType::Response);
        assert_eq!(out[0].success, Some(true));
        assert_eq!(out[1].msg_type, DapType::Event);
        assert_eq!(out[1].event.as_deref(), Some("initialized"));
    }

    #[test]
    fn attach_with_pid_sets_state() {
        let mut b = DapBridge::new();
        assert!(!b.is_attached());
        let out = b.handle(&req(2, "attach", Some(json!({ "pid": 12345 }))));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].success, Some(true));
        assert_eq!(out[0].body.as_ref().unwrap()["pid"], json!(12345));
        assert!(b.is_attached());
    }

    #[test]
    fn attach_without_pid_errors() {
        let mut b = DapBridge::new();
        let out = b.handle(&req(2, "attach", None));
        assert_eq!(out[0].success, Some(false));
        assert!(!b.is_attached());
    }

    #[test]
    fn disconnect_clears_attach() {
        let mut b = DapBridge::new();
        b.handle(&req(1, "attach", Some(json!({"pid": 1}))));
        assert!(b.is_attached());
        let out = b.handle(&req(2, "disconnect", None));
        assert_eq!(out[0].success, Some(true));
        assert!(!b.is_attached());
    }

    #[test]
    fn forwarding_commands_report_unimplemented_not_fake_success() {
        let mut b = DapBridge::new();
        for cmd in ["setBreakpoints", "stackTrace", "continue", "variables"] {
            let out = b.handle(&req(9, cmd, None));
            assert_eq!(out[0].success, Some(false), "{cmd} 不应假成功");
        }
    }

    #[test]
    fn non_request_ignored() {
        let mut b = DapBridge::new();
        let ev = DapMessage::event(1, "output", None);
        assert!(b.handle(&ev).is_empty());
    }
}
