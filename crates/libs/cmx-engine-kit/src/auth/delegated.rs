//! 族 A 认证中间件：`X-Delegated-User-Token` 委托令牌 → `cmx_core::AuthContext`（model / mdm 形态）。
//!
//! 行为（收编自 cmx-model-app / cmx-mdm-app 的 auth.rs，两仓可执行代码逐行一致，本文件为唯一真源）：
//!   1. **白名单**直放（引擎内置 + toml `[auth].whitelist` 追加，前缀匹配）；
//!   2. `X-API-Key` 校验（配置了 `api_keys` 才启用；**仅此项失败才 401**）；
//!   3. `X-Delegated-User-Token` **始终验签**（HS256，密钥 = 平台签发 JWT 的密钥），解 `sub`
//!      （= user_id）构造 `AuthContext` 后经 `cmx_traits::auth::context_scope::scope_full` 建请求级
//!      scope；未配密钥或验签失败 → 退化为纯服务调用（匿名），**不 401**（服务身份已由 API Key 验过）。
//!
//! 引擎差异仅两处，经 [`DelegatedSpec`] 参数化：内置白名单（model 空 / mdm 探针+webhook）与
//! tracing target。
//!
//! ⚠️ task_local 不跨 `tokio::spawn`（分发 dispatcher 等后台任务读不到，本就无需用户身份）。

use std::sync::OnceLock;

use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;

use cmx_core::AuthContext;
use cmx_traits::auth::context_scope::scope_full;

use super::config::DelegatedAuthConfig;

/// 引擎专属参数（各 app 以 `static SPEC: DelegatedSpec = DelegatedSpec::new(...)` 声明一份）。
pub struct DelegatedSpec {
    /// 引擎内置免用户鉴权路径（请求路径已剥 `/api` 前缀）。
    pub builtin_whitelist: &'static [&'static str],
    /// 引擎标识（进日志 `engine` 字段区分调用方；tracing 宏的 `target:` 要求编译期常量，
    /// 无法运行时参数化，故统一用本 crate 的默认 target + engine 字段）。
    pub engine: &'static str,
    /// 配置快照缓存（首请求装载一次）。
    cfg: OnceLock<DelegatedAuthConfig>,
}

impl DelegatedSpec {
    /// 构建引擎专属参数（const，可进 static）。
    pub const fn new(builtin_whitelist: &'static [&'static str], engine: &'static str) -> Self {
        Self {
            builtin_whitelist,
            engine,
            cfg: OnceLock::new(),
        }
    }

    fn cfg(&self) -> &DelegatedAuthConfig {
        self.cfg
            .get_or_init(|| DelegatedAuthConfig::load(self.builtin_whitelist))
    }
}

/// 是否免用户鉴权路径（前缀匹配，对齐门户白名单语义：`/mdm/health` 也覆盖 `/mdm/health/x`）。
fn is_whitelisted(path: &str, whitelist: &[String]) -> bool {
    whitelist.iter().any(|w| path.starts_with(w.as_str()))
}

/// 401 响应（仅 API Key 校验失败时返回；委托令牌失败只降级不拒绝）。
///
/// 注：族 A 的 401 带 `WWW-Authenticate: X-API-Key` 头（对齐门户），与族 B
/// （[`super::common::unauthorized`]，Json 体）形态不同——保持各自既有 wire。
fn unauthorized(msg: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(axum::http::header::WWW_AUTHENTICATE, "X-API-Key")],
        format!("{{\"code\":401,\"msg\":\"{msg}\"}}"),
    )
        .into_response()
}

/// 请求级身份中间件主体（各 app 的 `auth::mw` 薄包装转调本函数，挂载侧零改动）。
pub async fn mw(req: Request, next: Next, spec: &DelegatedSpec) -> Response {
    let cfg = spec.cfg();
    if is_whitelisted(req.uri().path(), &cfg.whitelist) {
        return next.run(req).await;
    }

    // 服务身份：配置了 api_keys 才强制校验。平台反代与引擎间客户端均会携带
    // = [service_auth].outgoing_api_key。
    if !cfg.api_keys.is_empty() {
        let hit = req
            .headers()
            .get("x-api-key")
            .and_then(|v| v.to_str().ok())
            .map(str::trim)
            .map(|k| cfg.api_keys.iter().any(|allowed| allowed == k))
            .unwrap_or(false);
        if !hit {
            return unauthorized("无效或缺失 X-API-Key");
        }
    }

    // 终端用户身份：X-Delegated-User-Token 验签（始终验签；未配密钥/失败 → 匿名服务调用）。
    let (auth, original_token) = match delegated_auth(&req, &cfg.jwt_secret, spec.engine) {
        Delegated::Verified(auth, token) => (Some(auth), Some(token)),
        Delegated::Anonymous(reason) => {
            tracing::debug!(engine = spec.engine, reason, "无委托用户身份，按服务调用处理");
            (None, None)
        }
    };

    let request_id = req
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();

    // 请求全程在 scope 内：context_scope 的 current_auth / current_original_token 可用。
    scope_full(auth, original_token, request_id, None, next.run(req)).await
}

/// 委托令牌解析结果。
enum Delegated {
    /// 验签通过：用户身份 + 原始 JWT（供继续透传）。
    Verified(AuthContext, String),
    /// 无令牌 / 未配密钥 / 验签失败（reason 供日志）。
    Anonymous(&'static str),
}

/// 委托令牌的 JWT claim（对齐平台 `cmx-auth` AccessClaims：`sub` = user_id、`username` =
/// 用户名；roles/username/nickname 可缺省——缺省时展示名按 nickname→username→sub 回退，
/// 兼容旧令牌与第三方精简令牌）。
#[derive(Debug, Deserialize)]
pub struct DelegatedClaims {
    pub sub: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub nickname: Option<String>,
    #[serde(default)]
    pub roles: Vec<String>,
}

/// 从 `X-Delegated-User-Token: Bearer <jwt>` 验签解出终端用户（`sub` = user_id）。
///
/// 未配 `jwt_secret` 时返回 Anonymous（服务 key 调用照常工作，仅 created_by / operated_by 类
/// 字段回退空/0）；密钥必须 = 平台签发 JWT 的密钥。
fn delegated_auth(req: &Request, secret: &str, engine: &str) -> Delegated {
    let Some(token) = req
        .headers()
        .get("x-delegated-user-token")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim())
        .and_then(|s| s.strip_prefix("Bearer ").or_else(|| s.strip_prefix("bearer ")))
        .filter(|s| !s.is_empty())
    else {
        return Delegated::Anonymous("无 X-Delegated-User-Token");
    };
    if secret.is_empty() {
        // 未配密钥：不能无签信任终端用户身份（「委托令牌始终验签」），降级服务调用。
        return Delegated::Anonymous("未配 auth.jwt_secret，跳过委托令牌解票");
    }
    let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::HS256);
    // 平台令牌不带 aud 约束校验（JwtConfig issuer/audience 仅作签发侧记录），这里只验签名 + exp。
    validation.validate_aud = false;
    validation.required_spec_claims.clear();
    let data = match jsonwebtoken::decode::<DelegatedClaims>(
        token,
        &jsonwebtoken::DecodingKey::from_secret(secret.as_bytes()),
        &validation,
    ) {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!(engine, error = %e, "X-Delegated-User-Token 验签失败，退化为纯服务调用");
            return Delegated::Anonymous("委托令牌验签失败");
        }
    };
    let user_id = data.claims.sub.trim().to_string();
    if user_id.is_empty() {
        return Delegated::Anonymous("委托令牌 sub 为空");
    }
    // username 是操作人姓名展示来源——优先 nickname（如"张三"），回退 username claim
    // （"admin"），再回退 user_id。不取姓名兜底 id 会让 created_by/operated_by 类展示变成雪花 id。
    let user_name = {
        let nick = data
            .claims
            .nickname
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let name = data.claims.username.trim();
        match (nick, name.is_empty()) {
            (Some(n), _) => n.to_string(),
            (None, false) => name.to_string(),
            (None, true) => user_id.clone(),
        }
    };
    let auth = AuthContext {
        username: user_name,
        user_id,
        roles: data.claims.roles,
        permissions: Vec::new(),
        org_id: None,
        session_id: None,
        device_type: None,
        auth_method: Some("delegated_jwt".to_string()),
    };
    Delegated::Verified(auth, token.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whitelist_prefix_match_and_api_prefix_stripped() {
        // mdm 形态：内置探针 + webhook 回调。
        let wl = vec!["/mdm/health".to_string(), "/mdm/flow/callback".to_string()];
        assert!(is_whitelisted("/mdm/health", &wl));
        assert!(is_whitelisted("/mdm/flow/callback", &wl));
        assert!(!is_whitelisted("/mdm/change-requests", &wl));
        // model 形态：toml 追加路径，前缀覆盖子路径。
        let wl2 = vec!["/model/db-state".to_string()];
        assert!(is_whitelisted("/model/db-state", &wl2));
        assert!(is_whitelisted("/model/db-state/detail", &wl2));
        assert!(!is_whitelisted("/model/deploy", &wl2));
        assert!(!is_whitelisted("/dct/entries", &wl2));
    }
}
