//! OpenCode HTTP API 客户端（reqwest 封装）。
//!
//! 封装 OpenCode 的 session / prompt_async / question / permission 等接口，
//! 统一携带访问凭证（`OPENCODE_SERVER_PASSWORD`），并把上游错误映射为 [`AiError`]。
//!
//! 接口路径与返回值经 OpenCode OpenAPI 文档（`/doc`）核实：
//! - `POST /session` → `Session` 对象
//! - `POST /session/{sid}/prompt_async` → 204（结果走 SSE）
//! - `POST /session/{sid}/abort` → `boolean`
//! - `DELETE /session/{sid}` → `boolean`
//! - `POST /question/{rid}/reply`（body `{answers: string[][]}`）→ `boolean`
//! - `POST /question/{rid}/reject` → `boolean`
//! - `POST /permission/{rid}/reply`（body `{reply, message?}`）→ `boolean`
//! - `GET /event` → `text/event-stream`（SSE 全局单流）

use futures::stream::Stream;
use reqwest::{Client, Response};

use crate::config::OpenCodeConfig;
use crate::error::{AiError, AiResult};

/// OpenCode HTTP 客户端。
///
/// 复用一个 `reqwest::Client`（连接池）；`Clone` 廉价（内部 `Arc`）。
#[derive(Clone)]
pub struct OpenCodeClient {
    client: Client,
    config: OpenCodeConfig,
}

impl OpenCodeClient {
    /// 构造客户端（参照 cmx-portal::ai 模式：per-client timeout）。
    pub fn new(config: OpenCodeConfig) -> Self {
        let client = Client::builder()
            .timeout(config.request_timeout())
            .build()
            .expect("reqwest::Client 构建失败：请检查系统 TLS 配置");
        Self { client, config }
    }

    /// 配置引用。
    pub fn config(&self) -> &OpenCodeConfig {
        &self.config
    }

    /// 是否已配置可用。
    pub fn is_configured(&self) -> bool {
        self.config.is_configured()
    }

    /// 统一注入鉴权头（password 非空时携带 Bearer）。
    fn with_auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if let Some(pw) = &self.config.password {
            req.bearer_auth(pw)
        } else {
            req
        }
    }

    /// 统一处理响应：检查状态码，提取错误消息。
    async fn check_status(&self, resp: Response) -> AiResult<Response> {
        let status = resp.status();
        if status.is_success() {
            Ok(resp)
        } else if status.as_u16() == 401 {
            let body = resp.text().await.unwrap_or_default();
            Err(AiError::Config(format!(
                "OpenCode 鉴权失败（401）：请检查 OPENCODE_SERVER_PASSWORD 配置。响应: {body}"
            )))
        } else {
            // 尝试解析错误体。
            let body = resp.text().await.unwrap_or_default();
            Err(AiError::UpstreamStatus(
                status.as_u16(),
                parse_error_message(&body).unwrap_or(body),
            ))
        }
    }

    // ───────────────────────── 会话管理 ─────────────────────────

    /// 创建新会话（`POST /session`）。
    ///
    /// `body` 可为空 JSON（`{}`）或携带 OpenCode 支持的可选字段。返回完整的 Session JSON。
    pub async fn create_session(&self, body: &serde_json::Value) -> AiResult<serde_json::Value> {
        let url = self.config.url("/session")?;
        let resp = self
            .with_auth(self.client.post(url).json(body))
            .send()
            .await
            .map_err(map_send_error)?;
        let resp = self.check_status(resp).await?;
        let session = resp.json::<serde_json::Value>().await?;
        Ok(session)
    }

    /// 异步发送消息（`POST /session/{sid}/prompt_async`），立即返回，结果走 SSE。
    ///
    /// 预期返回 204；返回 `Ok(())` 表示已触发生成。
    pub async fn prompt_async(&self, session_id: &str, body: &serde_json::Value) -> AiResult<()> {
        let url = self.config.url(&format!("/session/{session_id}/prompt_async"))?;
        let resp = self
            .with_auth(self.client.post(url).json(body))
            .send()
            .await
            .map_err(map_send_error)?;
        let _ = self.check_status(resp).await?;
        Ok(())
    }

    /// 中止会话当前生成（`POST /session/{sid}/abort`）。
    pub async fn abort(&self, session_id: &str) -> AiResult<()> {
        let url = self.config.url(&format!("/session/{session_id}/abort"))?;
        let resp = self
            .with_auth(self.client.post(url))
            .send()
            .await
            .map_err(map_send_error)?;
        let _ = self.check_status(resp).await?;
        Ok(())
    }

    /// 删除会话（`DELETE /session/{sid}`）。
    pub async fn delete_session(&self, session_id: &str) -> AiResult<()> {
        let url = self.config.url(&format!("/session/{session_id}"))?;
        let resp = self
            .with_auth(self.client.delete(url))
            .send()
            .await
            .map_err(map_send_error)?;
        let _ = self.check_status(resp).await?;
        Ok(())
    }

    // ───────────────────────── 询问 / 审批回复 ─────────────────────────

    /// 回复询问（`POST /question/{rid}/reply`，body `{answers: string[][]}`）。
    pub async fn reply_question(
        &self,
        request_id: &str,
        answers: Vec<Vec<String>>,
    ) -> AiResult<()> {
        let url = self.config.url(&format!("/question/{request_id}/reply"))?;
        let body = serde_json::json!({ "answers": answers });
        let resp = self
            .with_auth(self.client.post(url).json(&body))
            .send()
            .await
            .map_err(map_send_error)?;
        let _ = self.check_status(resp).await?;
        Ok(())
    }

    /// 拒绝询问（`POST /question/{rid}/reject`）。
    pub async fn reject_question(&self, request_id: &str) -> AiResult<()> {
        let url = self.config.url(&format!("/question/{request_id}/reject"))?;
        let resp = self
            .with_auth(self.client.post(url))
            .send()
            .await
            .map_err(map_send_error)?;
        let _ = self.check_status(resp).await?;
        Ok(())
    }

    /// 回复权限审批（`POST /permission/{rid}/reply`，body `{reply, message?}`）。
    ///
    /// `reply` ∈ `"once" | "always" | "reject"`。
    pub async fn reply_permission(
        &self,
        request_id: &str,
        reply: &str,
        message: Option<&str>,
    ) -> AiResult<()> {
        let url = self.config.url(&format!("/permission/{request_id}/reply"))?;
        let mut body = serde_json::json!({ "reply": reply });
        if let Some(msg) = message {
            body["message"] = serde_json::Value::String(msg.to_string());
        }
        let resp = self
            .with_auth(self.client.post(url).json(&body))
            .send()
            .await
            .map_err(map_send_error)?;
        let _ = self.check_status(resp).await?;
        Ok(())
    }

    // ───────────────────────── SSE 事件流 ─────────────────────────

    /// 建立 SSE 事件流连接（`GET /event`），返回字节流。
    ///
    /// 调用方（sse_relay）负责解析 SSE 帧。**不设请求超时**（长连接）——
    /// 通过单独的 `Client`（`timeout=None`）建立，避免被普通请求超时切断。
    ///
    /// 返回的 Stream 项为 `Result<bytes::Bytes, reqwest::Error>`。
    pub async fn stream_events(&self) -> AiResult<impl Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send + 'static> {
        // SSE 长连接专用 client：不设超时，避免 30s 后被切断。
        let sse_client = Client::builder()
            .build()
            .expect("reqwest::Client（SSE）构建失败：请检查系统 TLS 配置");
        let url = self.config.url("/event")?;
        let resp = self
            .with_auth(sse_client.get(url).header("Accept", "text/event-stream"))
            .send()
            .await
            .map_err(map_send_error)?;
        let resp = self.check_status(resp).await?;
        Ok(resp.bytes_stream())
    }
}

/// 把 `reqwest::Error` 映射为更友好的 [`AiError`]（区分超时）。
fn map_send_error(e: reqwest::Error) -> AiError {
    if e.is_timeout() {
        AiError::Timeout
    } else if e.is_connect() {
        AiError::Upstream(format!("无法连接 OpenCode 服务：{e}"))
    } else {
        AiError::Http(e)
    }
}

/// 尝试从错误响应体解析人类可读消息（OpenCode 常见格式 `{error: {message}}` 或纯文本）。
fn parse_error_message(body: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    // {error: {message: "..."}} 或 {message: "..."} 或 {error: "..."}.
    v.get("error")
        .and_then(|e| {
            e.get("message")
                .and_then(|m| m.as_str())
                .or_else(|| e.as_str())
        })
        .or_else(|| v.get("message").and_then(|m| m.as_str()))
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_error_prefers_nested_message() {
        let body = r#"{"error":{"message":"invalid session"}}"#;
        assert_eq!(
            parse_error_message(body),
            Some("invalid session".to_string())
        );
    }

    #[test]
    fn parse_error_handles_plain_string_error() {
        let body = r#"{"error":"bad request"}"#;
        assert_eq!(parse_error_message(body), Some("bad request".to_string()));
    }

    #[test]
    fn parse_error_returns_none_for_non_json() {
        assert!(parse_error_message("not json").is_none());
    }
}
