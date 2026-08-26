//! DAP 线协议编解码（W2-b）—— `Content-Length: N\r\n\r\n<json>` 帧 + 消息信封。
//!
//! Debug Adapter Protocol 与 LSP 同款传输：每条消息一个 `Content-Length` 头 + 空行 + JSON body。
//! 本模块自持编解码（不引 dap crate，可单测），是"平台自建 DAP 桥"的传输地基——浏览器 DAP 客户端
//! 经 WebSocket 直连平台时，平台用它解帧、按 command 路由到 cmx-debug 会话。

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// DAP 消息类型（ProtocolMessage.type）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DapType {
    Request,
    Response,
    Event,
}

/// DAP 协议消息（request / response / event 三合一的宽松信封）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DapMessage {
    /// 递增序号。
    pub seq: i64,
    #[serde(rename = "type")]
    pub msg_type: DapType,
    /// request：命令名（如 `initialize`/`attach`/`setBreakpoints`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// request 参数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Value>,
    /// response：对应 request 的 seq。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_seq: Option<i64>,
    /// response：是否成功。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub success: Option<bool>,
    /// response body / event body。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<Value>,
    /// event 名。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event: Option<String>,
    /// 失败信息。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl DapMessage {
    /// 构造对某 request 的成功响应。
    pub fn response(seq: i64, request_seq: i64, command: &str, body: Option<Value>) -> Self {
        Self {
            seq,
            msg_type: DapType::Response,
            command: Some(command.to_string()),
            arguments: None,
            request_seq: Some(request_seq),
            success: Some(true),
            body,
            event: None,
            message: None,
        }
    }

    /// 构造失败响应。
    pub fn error(seq: i64, request_seq: i64, command: &str, message: &str) -> Self {
        Self {
            seq,
            msg_type: DapType::Response,
            command: Some(command.to_string()),
            arguments: None,
            request_seq: Some(request_seq),
            success: Some(false),
            body: None,
            event: None,
            message: Some(message.to_string()),
        }
    }

    /// 构造事件。
    pub fn event(seq: i64, event: &str, body: Option<Value>) -> Self {
        Self {
            seq,
            msg_type: DapType::Event,
            command: None,
            arguments: None,
            request_seq: None,
            success: None,
            body,
            event: Some(event.to_string()),
            message: None,
        }
    }
}

/// 帧编码错误。
#[derive(Debug, PartialEq, Eq)]
pub enum FrameError {
    /// 头未完整（需更多字节）。
    Incomplete,
    /// 缺 Content-Length 头。
    NoContentLength,
    /// 头/体格式非法。
    Malformed(String),
}

/// 把一条消息编码为 DAP 帧（含 Content-Length 头）。
pub fn encode(msg: &DapMessage) -> Vec<u8> {
    let body = serde_json::to_vec(msg).unwrap_or_default();
    let header = format!("Content-Length: {}\r\n\r\n", body.len());
    let mut out = header.into_bytes();
    out.extend_from_slice(&body);
    out
}

/// 从缓冲区尝试解出一条帧。成功返回 `(消息, 消费的字节数)`；不足返回 [`FrameError::Incomplete`]。
///
/// 调用方循环调用并从缓冲区移除已消费字节即可支持粘包/半包。
pub fn decode(buf: &[u8]) -> Result<(DapMessage, usize), FrameError> {
    // 找头体分隔 \r\n\r\n。
    let sep = find_subslice(buf, b"\r\n\r\n").ok_or(FrameError::Incomplete)?;
    let header = std::str::from_utf8(&buf[..sep]).map_err(|_| FrameError::Malformed("头非 UTF-8".into()))?;

    let mut content_len: Option<usize> = None;
    for line in header.split("\r\n") {
        if let Some(v) = line.strip_prefix("Content-Length:") {
            content_len = v.trim().parse::<usize>().ok();
        }
    }
    let len = content_len.ok_or(FrameError::NoContentLength)?;

    let body_start = sep + 4;
    let body_end = body_start + len;
    if buf.len() < body_end {
        return Err(FrameError::Incomplete);
    }
    let msg: DapMessage = serde_json::from_slice(&buf[body_start..body_end])
        .map_err(|e| FrameError::Malformed(format!("body JSON 非法: {e}")))?;
    Ok((msg, body_end))
}

fn find_subslice(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn encode_decode_roundtrip() {
        let req = DapMessage {
            seq: 1,
            msg_type: DapType::Request,
            command: Some("attach".into()),
            arguments: Some(json!({ "pid": 4242 })),
            request_seq: None,
            success: None,
            body: None,
            event: None,
            message: None,
        };
        let framed = encode(&req);
        // 头存在。
        assert!(framed.starts_with(b"Content-Length: "));
        let (decoded, consumed) = decode(&framed).unwrap();
        assert_eq!(consumed, framed.len());
        assert_eq!(decoded.command.as_deref(), Some("attach"));
        assert_eq!(decoded.arguments.unwrap()["pid"], json!(4242));
    }

    #[test]
    fn incomplete_header_and_body() {
        assert_eq!(decode(b"Content-Len"), Err(FrameError::Incomplete));
        // 头全但体不足。
        let partial = b"Content-Length: 100\r\n\r\n{}";
        assert_eq!(decode(partial), Err(FrameError::Incomplete));
    }

    #[test]
    fn two_frames_in_buffer() {
        let a = encode(&DapMessage::event(1, "stopped", Some(json!({"reason":"breakpoint"}))));
        let b = encode(&DapMessage::event(2, "continued", None));
        let mut buf = a.clone();
        buf.extend_from_slice(&b);
        let (m1, n1) = decode(&buf).unwrap();
        assert_eq!(m1.event.as_deref(), Some("stopped"));
        let (m2, n2) = decode(&buf[n1..]).unwrap();
        assert_eq!(m2.event.as_deref(), Some("continued"));
        assert_eq!(n1 + n2, buf.len());
    }

    #[test]
    fn response_helpers() {
        let ok = DapMessage::response(2, 1, "initialize", Some(json!({"supportsConfigurationDoneRequest": true})));
        assert_eq!(ok.success, Some(true));
        assert_eq!(ok.request_seq, Some(1));
        let err = DapMessage::error(3, 1, "attach", "no such pid");
        assert_eq!(err.success, Some(false));
        assert_eq!(err.message.as_deref(), Some("no such pid"));
    }

    #[test]
    fn missing_content_length() {
        let bad = b"X-Foo: 1\r\n\r\n{}";
        assert_eq!(decode(bad), Err(FrameError::NoContentLength));
    }
}
