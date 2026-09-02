//! 族 B 认证中间件：JWT / API-Key → [`crate::tenant::TenantCtx`]（flow / rule 形态）。
//!
//! 收编自 cmx-flow-app / cmx-rule-app 的 auth.rs，以 **flow 超集语义**为基线（rule 随抽取
//! 升级：exp 校验开启、off 模式吃 `X-Tenant`/`X-User` 头、bearer 大小写容忍、roles 逗号串
//! 容忍、API-Key 委托桥激活）：
//!
//!   1. `X-API-Key` 优先（服务间 M2M）：命中 key 绑定租户建 scope；若同时携带
//!      `X-Delegated-User-Token`（委托用户令牌）则**始终验签**叠加解出真实办理人 + 租户
//!      （多租户下租户优先取委托令牌 claim），追加 `service` 角色；未命中 → 401。
//!   2. `off` 模式：`X-Tenant`/`X-User` 头建 scope（缺省默认租户/无用户）。
//!   3. `jwt` 模式：`Authorization: Bearer` 验签（HS256/RS256，exp 默认校验），解
//!      tenant/user/username/nickname/roles claim；SSE 票据白名单路径（EventSource 无法带
//!      header）接受 `?ticket=` 一次性票据。
//!
//! 引擎差异经 [`JwtSpec`]（SSE 票据路径与消费回调）与 `auth_mw` 的 `ensure_ready` 回调
//! （rule 的多租户懒备库，在 scope 内、handler 前执行）参数化。

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use axum::extract::Request;
use axum::http::header;
use axum::middleware::Next;
use axum::response::Response;
use jsonwebtoken::{Validation, decode};
use serde::Deserialize;
use serde_json::Value;

use super::common::unauthorized;
use super::config::{AuthMode, JwtAuthConfig};
use crate::tenant::{DEFAULT_TENANT, TenantCtx, scope};

/// 引擎专属参数（各 app 以 `static SPEC: JwtSpec = JwtSpec::new(...)` 声明一份）。
pub struct JwtSpec {
    /// 引擎标识（进日志区分调用方）。
    pub engine: &'static str,
    /// SSE 一次性票据路径白名单（路径 `ends_with` 匹配，如 flow 的
    /// `["/design/collab", "/events"]`；无 SSE 的引擎传空）。
    pub sse_ticket_paths: &'static [&'static str],
    /// 票据消费回调（issue/consume 的状态在引擎侧 sse 模块，kit 不持状态）。
    pub consume_ticket: Option<fn(&str) -> Option<TenantCtx>>,
}

impl JwtSpec {
    /// 构建引擎专属参数（const，可进 static）。
    pub const fn new(
        engine: &'static str,
        sse_ticket_paths: &'static [&'static str],
        consume_ticket: Option<fn(&str) -> Option<TenantCtx>>,
    ) -> Self {
        Self {
            engine,
            sse_ticket_paths,
            consume_ticket,
        }
    }
}

static AUTH: OnceLock<JwtAuthConfig> = OnceLock::new();

/// auth 中间件是否已在本进程处理过请求（即宿主确实挂载了本中间件）。
///
/// 任务端点授权的 fail-open/fail-close 判据：宿主未挂载本中间件（平台内嵌形态）时
/// `current_user()` 恒 None——此形态维持平台 mw_auth 边界、引擎层放行（现状兼容）；
/// 已挂载仍拿不到用户（纯服务调用 / 委托令牌验签失败）则按端点语义收紧。
static AUTH_ACTIVE: AtomicBool = AtomicBool::new(false);

/// auth 中间件是否生效（宿主挂载且有请求流过）。
pub fn auth_middleware_active() -> bool {
    AUTH_ACTIVE.load(Ordering::Relaxed)
}

fn auth_config() -> &'static JwtAuthConfig {
    AUTH.get_or_init(JwtAuthConfig::load)
}

/// 启动期预热认证配置（004 小项 fail-fast）：auth.mode 缺失/非法时在启动阶段即 panic 终止，
/// 而不是等第一个请求才炸（server bin 的 init 钩子里调用；单测/工具形态可不调）。
pub fn auth_config_warmup() {
    let _ = auth_config();
}

/// JWT claim 壳（宽松：只取需要的，其余忽略）。
#[derive(Debug, Deserialize)]
struct Claims {
    /// 用户 id（标准 sub）。
    #[serde(default)]
    sub: Option<String>,
    /// 其余 claim 动态取（tenant/roles 名可配）。
    #[serde(flatten)]
    extra: std::collections::HashMap<String, Value>,
}

/// axum 中间件主体（无就绪钩子版；各 app 的 `auth::auth` 薄包装转调，挂载侧零改动）。
pub async fn auth_mw(req: Request, next: Next, spec: &'static JwtSpec) -> Response {
    AUTH_ACTIVE.store(true, Ordering::Relaxed);
    match resolve_ctx(&req, spec) {
        Ok(ctx) => scope(ctx, next.run(req)).await,
        Err(resp) => resp,
    }
}

/// axum 中间件主体（带就绪钩子版）：在租户 scope 内、handler 前执行 `ensure_ready`
///（rule 的多租户懒备库注入点——钩子内可经 `current_tenant()` 读到本次请求的租户）。
pub async fn auth_mw_with_ready<F, Fut>(
    req: Request,
    next: Next,
    spec: &'static JwtSpec,
    ensure_ready: F,
) -> Response
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    AUTH_ACTIVE.store(true, Ordering::Relaxed);
    match resolve_ctx(&req, spec) {
        Ok(ctx) => {
            scope(ctx, async move {
                ensure_ready().await;
                next.run(req).await
            })
            .await
        }
        Err(resp) => resp,
    }
}

/// 判定本次请求的租户上下文（与 scope 无关，两入口共用）。
fn resolve_ctx(req: &Request, spec: &'static JwtSpec) -> Result<TenantCtx, Response> {
    let cfg = auth_config();
    // 先查 X-Api-Key（服务间 M2M）：命中即以该 key 绑定的租户建 scope，免 JWT。
    // S-14：与 Authorization 并存时 key 优先、JWT 被忽略——warn 显性化（调用方常以为
    // 带 JWT 即有用户身份，行为静默偏离预期；文档同步声明优先级）。
    if let Some(key) = header_str(req, "x-api-key") {
        if req.headers().get(axum::http::header::AUTHORIZATION).is_some() {
            tracing::warn!(
                "X-Api-Key 与 Authorization 并存：以 API Key 服务身份为准（JWT 被忽略）"
            );
        }
        return match cfg.api_keys.get(&key) {
            Some(key_tenant) => {
                // 委托令牌桥：服务身份已验（API Key 合法）。若平台再带上**委托用户令牌**
                // （X-Delegated-User-Token: Bearer <终端用户 JWT>），则解它取真实办理人 +
                // 租户——否则退化为纯服务调用。
                //
                // 关键：多租户下一个服务 key 服务多个平台租户，故**租户优先取委托令牌的
                // claim**，而非 key 绑定的租户（key_tenant 仅作无委托令牌时的回退）。
                let has_delegation_header = header_str(req, "x-delegated-user-token").is_some();
                let ctx = match delegated_user_ctx(req, cfg) {
                    Some(mut ctx) => {
                        // 委托令牌解出用户/租户；追加 "service" 角色标记本跳是经服务代理来的。
                        ctx.roles.push("service".to_string());
                        ctx
                    }
                    None => {
                        let mut c = TenantCtx::new(key_tenant.clone())
                            .with_roles(vec!["service".to_string()]);
                        // 技术债 004/003-3 fail-close：带了委托令牌头但验签失败 → 打标。
                        // 涉及身份的动作端点（发起/取消/跳转等）据此拒绝；纯查询降级放行。
                        c.delegation_failed = has_delegation_header;
                        c
                    }
                };
                // 技术债 003：结构化 key 声明的 system / 定义白名单贯通上下文
                // （归属过滤、发起白名单校验、命名空间校验）。
                let decl = cfg.api_key_decls.get(&key);
                Ok(ctx
                    .with_system(decl.and_then(|d| d.system.clone()))
                    .with_allowed_definition_keys(
                        decl.map(|d| d.allowed_definition_keys.clone()).unwrap_or_default(),
                    ))
            }
            None => Err(unauthorized("无效 API Key")),
        };
    }
    match cfg.mode {
        AuthMode::Off => Ok(ctx_from_headers(req)),
        AuthMode::Jwt => verify_jwt(req, cfg, spec),
    }
}

/// SSE 白名单判定：路径 `ends_with` 匹配 spec 声明的票据路径。
fn is_sse_ticket_path(path: &str, spec: &JwtSpec) -> bool {
    spec.sse_ticket_paths.iter().any(|p| path.ends_with(p))
}

/// 从 query string 取 `ticket` 参数值（不引入额外依赖，手工扫 `k=v&`）。
fn ticket_from_query(req: &Request) -> Option<String> {
    let q = req.uri().query()?;
    for pair in q.split('&') {
        if let Some(v) = pair.strip_prefix("ticket=") {
            let decoded = urldecode(v);
            if !decoded.is_empty() {
                return Some(decoded);
            }
        }
    }
    None
}

/// 极简 percent-decode（票据是 uuid，仅可能含 `%` 转义；容忍非法转义原样保留）。
fn urldecode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(h), Some(l)) = (hi, lo) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// off 模式：从 X-Tenant / X-User 头取（缺省默认租户 / 无用户）。
fn ctx_from_headers(req: &Request) -> TenantCtx {
    let tenant = header_str(req, "x-tenant").unwrap_or_else(|| DEFAULT_TENANT.to_string());
    let user = header_str(req, "x-user");
    TenantCtx::new(tenant).with_user(user)
}

/// jwt 模式：从 `Authorization: Bearer` 取令牌验签 + 解 claim。失败返回 401 响应。
///
/// 例外：SSE 白名单路径（EventSource 无法带 header）在 header 缺失时改用 `?ticket=` 一次性票据。
fn verify_jwt(req: &Request, cfg: &JwtAuthConfig, spec: &JwtSpec) -> Result<TenantCtx, Response> {
    let bearer = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer ").or_else(|| s.strip_prefix("bearer ")));
    match bearer {
        Some(token) => decode_claims(token, cfg).map_err(|e| unauthorized(&format!("JWT 校验失败: {e}"))),
        None => {
            // header 缺失：SSE 白名单路径接受一次性票据（浏览器 EventSource 场景）。
            if is_sse_ticket_path(req.uri().path(), spec) {
                if let Some(consume) = spec.consume_ticket {
                    if let Some(ticket) = ticket_from_query(req) {
                        if let Some(ctx) = consume(&ticket) {
                            return Ok(ctx);
                        }
                        return Err(unauthorized("SSE 票据无效或已过期"));
                    }
                }
            }
            Err(unauthorized("缺少 Authorization: Bearer <token>"))
        }
    }
}

/// 委托令牌桥：从 `X-Delegated-User-Token: Bearer <jwt>` 解出委托的终端用户上下文。
///
/// 平台经反代出站时，把当前登录用户的原始 JWT 放此头。这里**始终验签**（无论
/// auth.mode）——委托令牌是终端用户身份的唯一凭据，不能无签信任。无密钥（未配 JWT）或
/// 验签失败 → 返回 None（退化为纯服务调用，不 401，因服务身份本身已由 API Key 验过）。
fn delegated_user_ctx(req: &Request, cfg: &JwtAuthConfig) -> Option<TenantCtx> {
    let token = req
        .headers()
        .get("x-delegated-user-token")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer ").or_else(|| s.strip_prefix("bearer ")).or(Some(s)))?;
    match decode_claims(token, cfg) {
        Ok(ctx) => Some(ctx),
        Err(e) => {
            tracing::warn!(error = %e, "X-Delegated-User-Token 验签失败，退化为纯服务调用");
            None
        }
    }
}

/// 验签一个 JWT 字符串 → 租户上下文（tenant/user/username/nickname/roles claim）。
/// 供 Bearer 与委托令牌两路复用。
///
/// 需已配解码密钥（`decoding_key`）；未配时 Err（jwt 模式启动即告警，off 模式无委托令牌路径）。
fn decode_claims(token: &str, cfg: &JwtAuthConfig) -> Result<TenantCtx, String> {
    let key = cfg
        .decoding_key
        .as_ref()
        .ok_or_else(|| "服务未配置 JWT 密钥".to_string())?;

    let mut validation = Validation::new(cfg.alg);
    // 只信任 token 声明；不校验 aud（由签发方约束）。exp 默认校验（过期即失败）。
    validation.validate_aud = false;

    let data = decode::<Claims>(token, key, &validation).map_err(|e| e.to_string())?;
    let claims = data.claims;

    // tenant claim（配置名）→ 字符串；缺失回退默认租户。
    let tenant = claims
        .extra
        .get(&cfg.tenant_claim)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| DEFAULT_TENANT.to_string());
    // roles claim → Vec<String>（数组或逗号分隔字符串都容忍）。
    let roles = match claims.extra.get(&cfg.roles_claim) {
        Some(Value::Array(a)) => a
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect(),
        Some(Value::String(s)) => s.split(',').map(|r| r.trim().to_string()).collect(),
        _ => Vec::new(),
    };

    // username claim → 展示名（平台 AccessClaims 自带；缺省 None，留痕经
    // current_display_user 回退用户 id，避免把 id 当姓名写台账）。
    let username = claims
        .extra
        .get("username")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    // nickname claim → 昵称（平台 2026-08 起签发；旧令牌无 → None，展示经
    // current_display_nickname 自然落到 username）。
    let nickname = claims
        .extra
        .get("nickname")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    Ok(TenantCtx::new(tenant)
        .with_user(claims.sub)
        .with_username(username)
        .with_nickname(nickname)
        .with_roles(roles))
}

fn header_str(req: &Request, name: &str) -> Option<String> {
    req.headers()
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{DecodingKey, EncodingKey, Header, encode};

    /// 构造一个 HS256 测试用 JwtAuthConfig（不碰进程 OnceLock / 环境变量）。
    fn hs256_cfg(secret: &str) -> JwtAuthConfig {
        JwtAuthConfig {
            mode: AuthMode::Jwt,
            alg: jsonwebtoken::Algorithm::HS256,
            decoding_key: Some(DecodingKey::from_secret(secret.as_bytes())),
            tenant_claim: "tenant".to_string(),
            roles_claim: "roles".to_string(),
            api_keys: std::collections::HashMap::new(),
            api_key_decls: std::collections::HashMap::new(),
        }
    }

    /// 签一个 HS256 JWT（含 sub/tenant/roles + 远期 exp）。
    fn sign(secret: &str, sub: &str, tenant: &str, roles: &[&str]) -> String {
        let claims = serde_json::json!({
            "sub": sub,
            "tenant": tenant,
            "roles": roles,
            "exp": 4_102_444_800u64, // 2100-01-01，避免过期
        });
        encode(
            &Header::new(jsonwebtoken::Algorithm::HS256),
            &claims,
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .unwrap()
    }

    #[test]
    fn decode_claims_extracts_user_tenant_roles() {
        let cfg = hs256_cfg("s6-secret");
        let token = sign("s6-secret", "u_alice", "tenantA", &["approver", "finance"]);
        let ctx = decode_claims(&token, &cfg).expect("应验签通过");
        assert_eq!(ctx.tenant, "tenantA");
        assert_eq!(ctx.user.as_deref(), Some("u_alice"));
        assert_eq!(ctx.roles, vec!["approver".to_string(), "finance".to_string()]);
    }

    #[test]
    fn decode_claims_extracts_username_and_nickname() {
        // 平台签发的新令牌（含 username/nickname）；旧令牌无此 claim → None（宽容）。
        let cfg = hs256_cfg("s6-secret");
        let claims = serde_json::json!({
            "sub": "u_alice",
            "tenant": "tenantA",
            "username": "alice",
            "nickname": "爱丽丝",
            "exp": 4_102_444_800u64,
        });
        let token = encode(
            &Header::new(jsonwebtoken::Algorithm::HS256),
            &claims,
            &EncodingKey::from_secret("s6-secret".as_bytes()),
        )
        .unwrap();
        let ctx = decode_claims(&token, &cfg).expect("应验签通过");
        assert_eq!(ctx.username.as_deref(), Some("alice"));
        assert_eq!(ctx.nickname.as_deref(), Some("爱丽丝"));

        // 旧令牌（无 username/nickname claim）。
        let legacy = sign("s6-secret", "u_old", "tenantA", &[]);
        let ctx2 = decode_claims(&legacy, &cfg).expect("应验签通过");
        assert_eq!(ctx2.username, None);
        assert_eq!(ctx2.nickname, None);
    }

    #[test]
    fn decode_claims_rejects_wrong_secret() {
        let cfg = hs256_cfg("right-secret");
        let token = sign("WRONG-secret", "u_bob", "t1", &[]);
        assert!(decode_claims(&token, &cfg).is_err(), "错密钥应验签失败");
    }

    #[test]
    fn decode_claims_rejects_expired() {
        // rule 升级超集语义的核心变更：过期令牌从放行 → 拒绝。
        let cfg = hs256_cfg("s6-secret");
        let claims = serde_json::json!({
            "sub": "u_late",
            "tenant": "t1",
            "exp": 1_000_000u64, // 1970-01-12，早已过期
        });
        let token = encode(
            &Header::new(jsonwebtoken::Algorithm::HS256),
            &claims,
            &EncodingKey::from_secret("s6-secret".as_bytes()),
        )
        .unwrap();
        assert!(decode_claims(&token, &cfg).is_err(), "过期令牌应被拒绝");
    }

    #[test]
    fn delegated_user_ctx_honors_token_over_key_tenant() {
        // 委托令牌桥核心：委托令牌带 Bearer 前缀，解出的 tenant 覆盖 API Key 绑定租户。
        let cfg = hs256_cfg("s6-secret");
        let token = sign("s6-secret", "u_carol", "tenantB", &["clerk"]);
        let req = Request::builder()
            .header("x-delegated-user-token", format!("Bearer {token}"))
            .body(axum::body::Body::empty())
            .unwrap();
        let ctx = delegated_user_ctx(&req, &cfg).expect("应解出委托用户");
        assert_eq!(ctx.tenant, "tenantB"); // 取委托令牌的 claim，非 key 绑定租户
        assert_eq!(ctx.user.as_deref(), Some("u_carol"));
        assert_eq!(ctx.roles, vec!["clerk".to_string()]);
    }

    #[test]
    fn delegated_user_ctx_accepts_bare_token_without_bearer() {
        // 容忍无 Bearer 前缀（宿主直接放裸 JWT）。
        let cfg = hs256_cfg("s6-secret");
        let token = sign("s6-secret", "u_dan", "t2", &[]);
        let req = Request::builder()
            .header("x-delegated-user-token", token)
            .body(axum::body::Body::empty())
            .unwrap();
        let ctx = delegated_user_ctx(&req, &cfg).expect("裸令牌也应解出");
        assert_eq!(ctx.user.as_deref(), Some("u_dan"));
    }

    #[test]
    fn delegated_user_ctx_none_when_absent_or_bad() {
        let cfg = hs256_cfg("s6-secret");
        // 缺头 → None（退化纯服务调用）
        let req = Request::builder().body(axum::body::Body::empty()).unwrap();
        assert!(delegated_user_ctx(&req, &cfg).is_none());
        // 坏令牌 → None（不 401，服务身份已验）
        let bad = Request::builder()
            .header("x-delegated-user-token", "Bearer not.a.jwt")
            .body(axum::body::Body::empty())
            .unwrap();
        assert!(delegated_user_ctx(&bad, &cfg).is_none());
    }

    #[test]
    fn sse_ticket_path_matches_by_suffix() {
        let spec = JwtSpec::new("flow", &["/design/collab", "/events"], None);
        assert!(is_sse_ticket_path("/api/flow/v1/x/design/collab", &spec));
        assert!(is_sse_ticket_path("/api/flow/v1/events", &spec));
        assert!(!is_sse_ticket_path("/api/flow/v1/definitions", &spec));
        let empty = JwtSpec::new("rules", &[], None);
        assert!(!is_sse_ticket_path("/anything", &empty));
    }
}
