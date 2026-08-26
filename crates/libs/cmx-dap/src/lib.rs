//! cmx-dap —— 平台自建 DAP 桥（W2-b 骨架）。
//!
//! 让浏览器 DAP 客户端可**直连平台**做调试，摆脱对 code-server 扩展 Express `:9000` 进程存活的依赖
//! （方案 W2-b）。本 crate 提供两块自持、可单测的地基：
//! - [`protocol`]：DAP 线协议（`Content-Length` 帧 + 消息信封）编解码。
//! - [`bridge`]：把 DAP 请求按 command 路由、attach 到 [`cmx_debug`] 会话的桥调度。
//!
//! **诚实范围**：initialize/attach/disconnect 等不依赖 LLDB 转发的命令已实现；setBreakpoints/
//! stackTrace/variables 等需转发给真 LLDB 的命令返回明确"未实现"，不假成功。WebSocket 传输接入
//! （axum ws 端点）留给平台层，用 [`protocol::decode`]/[`protocol::encode`] + [`bridge::DapBridge`] 即可组装。

pub mod bridge;
pub mod protocol;

pub use bridge::{DapBridge, Outgoing};
pub use protocol::{decode, encode, DapMessage, DapType, FrameError};
