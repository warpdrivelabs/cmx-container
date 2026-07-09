//! OpenCode 连接配置。
//!
//! 配置优先级（高 → 低）：
//! 1. 环境变量 `OPENCODE_ENABLED` / `OPENCODE_BASE_URL` / `OPENCODE_SERVER_PASSWORD`
//! 2. TOML `[opencode]` 段（`opencode.enabled` / `opencode.base_url` / `opencode.password`
//!    / `opencode.request_timeout_ms` / `opencode.sse_heartbeat_secs`）
//! 3. 内置默认值（`enabled = false`，`http://127.0.0.1:4096`，无密码）
//!
//! # 总开关 `enabled`（默认关闭）
//! OpenCode 服务未部署时，后台 SSE relay 会永久重连 127.0.0.1:4096 并刷错误日志。
//! 默认 `enabled = false` 跳过整个 AI 子系统初始化（不建客户端、不拉 relay task），
//! `/api/ai/*` 接口统一返回 503「AI 功能未启用」。需要启用时显式配置为 true。

use std::time::Duration;

use serde::Deserialize;

use crate::error::{AiError, AiResult};

/// OpenCode 连接配置。
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct OpenCodeConfig {
    /// 是否启用 AI 子系统（OpenCode 薄代理）。**默认关闭**。
    ///
    /// - `false`：`init_ai_subsystem` 直接返回，不创建客户端、不拉 SSE relay，
    ///   `/api/ai/*` 返回 503。用于 OpenCode 未部署的场景，避免后台日志刷连接错误。
    /// - `true`：按 `base_url` 正常初始化。需同时确保 OpenCode 服务已部署。
    ///
    /// 配置项 `opencode.enabled`；环境变量 `OPENCODE_ENABLED`（`"true"`/`"1"`/`"yes"` 视为启用）。
    pub enabled: bool,

    /// OpenCode 服务地址（含协议与端口，无尾斜杠）。
    /// 默认 `http://127.0.0.1:4096`（对应 `opencode serve --host 0.0.0.0 --port 4096`）。
    pub base_url: String,

    /// OpenCode 访问凭证（`OPENCODE_SERVER_PASSWORD`）。
    /// 为空表示 OpenCode 未启用鉴权（开发环境）；生产应配置强密码。
    /// cmx-ai 调用 OpenCode 的所有请求（含 SSE）会以 `Authorization: Bearer <password>` 携带。
    #[serde(skip_deserializing)]
    pub password: Option<String>,

    /// 普通 HTTP 请求（创建会话/发消息/abort 等）超时时间（毫秒）。
    /// 默认 30000（30 秒）。
    pub request_timeout_ms: u64,

    /// SSE 长连接的心跳/健康检查周期（秒）。
    /// 仅作日志参考与重连判断，不主动发心跳（OpenCode 每 10 秒推送 `server.heartbeat`）。
    /// 默认 30。
    pub sse_heartbeat_secs: u64,
}

impl Default for OpenCodeConfig {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            base_url: default_base_url(),
            password: None,
            request_timeout_ms: 30_000,
            sse_heartbeat_secs: 30,
        }
    }
}

/// 默认 enabled（环境变量 `OPENCODE_ENABLED` 优先，否则关闭）。
///
/// 接受的"启用"值（大小写不敏感）：`true` / `1` / `yes` / `on`。
fn default_enabled() -> bool {
    match std::env::var("OPENCODE_ENABLED") {
        Ok(v) => matches!(v.trim().to_ascii_lowercase().as_str(), "true" | "1" | "yes" | "on"),
        Err(_) => false,
    }
}

/// 默认 base_url（环境变量 `OPENCODE_BASE_URL` 优先，否则 `http://127.0.0.1:4096`）。
fn default_base_url() -> String {
    std::env::var("OPENCODE_BASE_URL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "http://127.0.0.1:4096".to_string())
        .trim_end_matches('/')
        .to_string()
}

impl OpenCodeConfig {
    /// 普通请求超时时间（`Duration`）。
    pub fn request_timeout(&self) -> Duration {
        Duration::from_millis(self.request_timeout_ms.max(1_000))
    }

    /// 是否已配置（base_url 非空即视为已配置；password 可空）。
    pub fn is_configured(&self) -> bool {
        !self.base_url.is_empty()
    }

    /// 校验 base_url 合法性。
    pub fn validate(&self) -> AiResult<()> {
        if self.base_url.is_empty() {
            return Err(AiError::Config("opencode.base_url 不能为空".into()));
        }
        url::Url::parse(&self.base_url)
            .map_err(|e| AiError::Config(format!("opencode.base_url 非法: {e}")))?;
        Ok(())
    }

    /// 拼接 OpenCode 接口路径，返回完整 URL。
    ///
    /// # 示例
    /// `cfg.url("/session/{sid}/prompt_async")` → `http://127.0.0.1:4096/session/{sid}/prompt_async`
    pub fn url(&self, path: &str) -> AiResult<String> {
        if !path.starts_with('/') {
            return Err(AiError::Config(format!(
                "OpenCode 接口路径必须以 '/' 开头: {path}"
            )));
        }
        Ok(format!("{}{path}", self.base_url))
    }
}

/// 从 ConfigManager（TOML `[opencode]` 段）+ 环境变量加载配置。
///
/// 优先级：环境变量 > TOML > 默认值。
///
/// # Panics
/// ConfigManager 未初始化时 `get_as_or` 会回退默认值，不会 panic。
pub fn load_config() -> OpenCodeConfig {
    let mut cfg = cmx_utils::ConfigManager::global()
        .get_as_or("opencode", OpenCodeConfig::default());

    // 环境变量覆盖（最高优先级）。
    if let Ok(v) = std::env::var("OPENCODE_ENABLED") {
        cfg.enabled = matches!(v.trim().to_ascii_lowercase().as_str(), "true" | "1" | "yes" | "on");
    }
    if let Ok(v) = std::env::var("OPENCODE_BASE_URL") {
        let v = v.trim_end_matches('/').to_string();
        if !v.is_empty() {
            cfg.base_url = v;
        }
    }
    if let Ok(v) = std::env::var("OPENCODE_SERVER_PASSWORD") {
        if v.is_empty() {
            cfg.password = None;
        } else {
            cfg.password = Some(v);
        }
    }

    // TOML 中若直接配了 password 字段（字符串），从原始配置补读一次（serde skip_deserializing 跳过了）。
    if cfg.password.is_none()
        && let Ok(p) = cmx_utils::ConfigManager::global().get_string("opencode.password")
        && !p.is_empty()
    {
        cfg.password = Some(p);
    }

    if let Err(e) = cfg.validate() {
        tracing::warn!(error = %e, "OpenCode 配置校验失败，将禁用 AI 子系统（/api/ai/* 返回 503）");
        return OpenCodeConfig { enabled: false, ..OpenCodeConfig::default() };
    }

    tracing::info!(
        enabled = cfg.enabled,
        base_url = %cfg.base_url,
        has_password = cfg.password.is_some(),
        "OpenCode 配置加载完成"
    );
    cfg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_joins_correctly() {
        let cfg = OpenCodeConfig {
            base_url: "http://127.0.0.1:4096".into(),
            ..Default::default()
        };
        assert_eq!(
            cfg.url("/session").unwrap(),
            "http://127.0.0.1:4096/session"
        );
    }

    #[test]
    fn url_requires_leading_slash() {
        let cfg = OpenCodeConfig::default();
        assert!(cfg.url("session").is_err());
    }

    #[test]
    fn validate_rejects_empty_base_url() {
        let cfg = OpenCodeConfig {
            base_url: String::new(),
            ..Default::default()
        };
        assert!(cfg.validate().is_err());
    }
}
