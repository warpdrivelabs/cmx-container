//! 认证中间件
//!
//! 从 Authorization: Bearer <token> 头或 X-API-Key 头解析认证信息，
//! 调用 AuthService::validate_token / AuthService::validate_api_key 验证，
//! 将 AuthContext 注入 CmxSvrContext。
//! 支持路由白名单（login/refresh/docs 等无需认证）。
//! 白名单由内置默认白名单 + TOML `[auth].whitelist` 合并构成，
//! 通过 `GlobalAuthService::initialize_whitelist` 在启动时注入。
//!
//! ## 白名单匹配规则
//!
//! - **普通规则**（不含通配符）：前缀匹配，如 `/api/auth` 匹配 `/api/auth/login`、`/api/auth/refresh`
//! - **`*` 通配符**：匹配单层路径段（不含 `/`），如 `/api/biz/*` 匹配 `/api/biz/users` 但不匹配 `/api/biz/users/123`
//! - **`**` 通配符**：匹配多层路径（含 `/`），如 `/api/auth/**` 匹配 `/api/auth/`、`/api/auth/oauth2/token`、`/api/auth/a/b/c`
//! - **锚定匹配**：规则从路径开头匹配，普通规则等价于隐式 `**` 后缀（向后兼容历史前缀匹配语义）

use std::sync::{Arc, OnceLock, RwLock};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use regex::Regex;
use tracing::{debug, info, warn};

use crate::middleware::CmxSvrContext;
use cmx_traits::auth::AuthService;

/// 全局动态认证白名单（已编译为正则表达式列表）
///
/// 启动时由 `GlobalAuthService::initialize_whitelist` 注入，
/// 内容为内置白名单（`cmx_auth::config::BUILTIN_WHITELIST`）
/// 与 TOML `[auth].whitelist` 合并去重、统一编译为正则的结果。
/// 使用 `RwLock` 包裹以支持运行时热更新（可选）。
static GLOBAL_AUTH_WHITELIST: OnceLock<RwLock<Vec<CompiledRule>>> = OnceLock::new();

/// 全局 AuthService 实例
static GLOBAL_AUTH_SERVICE: OnceLock<Arc<dyn AuthService>> = OnceLock::new();

/// 全局 OAuth2 策略实例
static GLOBAL_OAUTH2_POLICY: OnceLock<Arc<cmx_auth::policy::OAuth2Policy>> = OnceLock::new();

/// 全局第三方 OAuth2 Provider 注册表
static GLOBAL_OAUTH2_PROVIDER_REGISTRY: OnceLock<cmx_auth::oauth2::OAuth2ProviderRegistry> = OnceLock::new();

/// 已编译的白名单规则
///
/// - `Prefix`：普通前缀匹配（不含通配符的历史规则，等价于隐式 `**` 后缀）
/// - `Regex`：含通配符的规则，已预编译为正则表达式
#[derive(Debug)]
enum CompiledRule {
    /// 普通前缀匹配（向后兼容）
    Prefix(String),
    /// 通配符规则（已编译为正则）
    Regex(Regex),
}

impl CompiledRule {
    /// 判断路径是否匹配此规则
    fn matches(&self, path: &str) -> bool {
        match self {
            CompiledRule::Prefix(prefix) => path.starts_with(prefix),
            CompiledRule::Regex(re) => re.is_match(path),
        }
    }
}

/// 判断规则是否包含通配符
fn has_wildcard(pattern: &str) -> bool {
    pattern.contains('*') || pattern.contains('?') || pattern.contains('[')
}

/// 将通配符模式编译为正则表达式
///
/// 支持的通配符：
/// - `**`：匹配任意字符（包括路径分隔符 `/`）
/// - `*`：匹配单层路径段（不含 `/`）
/// - 其他正则元字符会被转义
fn compile_wildcard_to_regex(pattern: &str) -> Result<Regex, regex::Error> {
    let mut regex_str = String::with_capacity(pattern.len() * 2);
    regex_str.push('^');

    // 逐字符扫描，处理 `**`、`*` 和普通字符
    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '*' => {
                // 检查是否是 `**`
                if i + 1 < chars.len() && chars[i + 1] == '*' {
                    // `**` 匹配任意字符（含 `/`）
                    regex_str.push_str(".*");
                    i += 2;
                } else {
                    // `*` 匹配单层路径段（不含 `/`）
                    regex_str.push_str("[^/]*");
                    i += 1;
                }
            }
            '?' => {
                // `?` 匹配单个非 `/` 字符
                regex_str.push_str("[^/]");
                i += 1;
            }
            c => {
                // 转义正则元字符（. + * ? ^ $ ( ) [ ] { } | \）
                if ".+*?^$()[]{}|\\".contains(c) {
                    regex_str.push('\\');
                }
                regex_str.push(c);
                i += 1;
            }
        }
    }

    regex_str.push('$');
    debug!(pattern = %pattern, regex = %regex_str, "白名单规则编译为正则");
    Regex::new(&regex_str)
}

/// 将原始规则字符串编译为 CompiledRule
fn compile_rule(pattern: &str) -> Result<CompiledRule, String> {
    let trimmed = pattern.trim().to_string();
    if trimmed.is_empty() {
        return Err("规则不能为空".to_string());
    }

    if has_wildcard(&trimmed) {
        let re = compile_wildcard_to_regex(&trimmed)
            .map_err(|e| format!("规则 '{}' 编译失败: {}", trimmed, e))?;
        Ok(CompiledRule::Regex(re))
    } else {
        Ok(CompiledRule::Prefix(trimmed))
    }
}

/// 全局认证服务管理器
pub struct GlobalAuthService;

impl GlobalAuthService {
    /// 初始化全局认证服务
    pub fn initialize(auth_service: Arc<dyn AuthService>) -> Result<(), String> {
        GLOBAL_AUTH_SERVICE
            .set(auth_service)
            .map_err(|_| "全局认证服务已初始化".to_string())
    }

    /// 初始化全局 OAuth2 策略
    pub fn initialize_oauth2(policy: Arc<cmx_auth::policy::OAuth2Policy>) -> Result<(), String> {
        GLOBAL_OAUTH2_POLICY
            .set(policy)
            .map_err(|_| "全局 OAuth2 策略已初始化".to_string())
    }

    /// 获取全局认证服务
    pub fn get() -> Option<&'static Arc<dyn AuthService>> {
        GLOBAL_AUTH_SERVICE.get()
    }

    /// 获取全局 OAuth2 策略
    pub fn get_oauth2() -> Option<&'static Arc<cmx_auth::policy::OAuth2Policy>> {
        GLOBAL_OAUTH2_POLICY.get()
    }

    /// 初始化全局认证白名单
    ///
    /// 合并内置白名单与用户配置白名单并去重，编译为匹配规则后注入到全局状态。
    /// 必须在应用启动早期调用（建议在 `init_auth_service` 中完成）。
    ///
    /// ## 匹配规则
    /// - 普通规则（不含通配符）：前缀匹配
    /// - `*`：匹配单层路径段（不含 `/`）
    /// - `**`：匹配多层路径（含 `/`）
    pub fn initialize_whitelist(custom_whitelist: Vec<String>) -> Result<(), String> {
        let mut combined: Vec<String> = cmx_auth::config::BUILTIN_WHITELIST
            .iter()
            .map(|s| s.to_string())
            .collect();

        // 追加自定义白名单（去重）
        for path in custom_whitelist {
            let trimmed = path.trim().to_string();
            if trimmed.is_empty() {
                continue;
            }
            if !combined.contains(&trimmed) {
                combined.push(trimmed);
            }
        }

        // 编译所有规则
        let mut compiled = Vec::with_capacity(combined.len());
        for pattern in &combined {
            match compile_rule(pattern) {
                Ok(rule) => compiled.push(rule),
                Err(e) => warn!(pattern = %pattern, error = %e, "白名单规则编译失败，已跳过"),
            }
        }

        debug!(count = compiled.len(), "认证白名单初始化完成");
        GLOBAL_AUTH_WHITELIST
            .set(RwLock::new(compiled))
            .map_err(|_| "全局认证白名单已初始化".to_string())
    }

    /// 检查路径是否在白名单中
    ///
    /// 按以下顺序匹配：
    /// 1. 前缀匹配（普通规则）
    /// 2. 正则匹配（通配符规则）
    pub fn is_whitelisted(path: &str) -> bool {
        GLOBAL_AUTH_WHITELIST
            .get()
            .and_then(|rw| rw.read().ok())
            .map(|rules| rules.iter().any(|rule| rule.matches(path)))
            .unwrap_or_else(|| {
                // 未初始化时回退到内置白名单前缀匹配
                cmx_auth::config::BUILTIN_WHITELIST
                    .iter()
                    .any(|prefix| path.starts_with(prefix))
            })
    }

    /// 初始化第三方 OAuth2 Provider 注册表
    pub fn initialize_provider_registry(registry: cmx_auth::oauth2::OAuth2ProviderRegistry) -> Result<(), String> {
        // 同步到 cmx-auth 内部的全局注册表，使 AuthServiceImpl 可访问
        cmx_auth::oauth2::OAuth2ProviderRegistry::initialize_global(registry.clone())?;
        GLOBAL_OAUTH2_PROVIDER_REGISTRY
            .set(registry)
            .map_err(|_| "OAuth2 Provider 注册表已初始化".to_string())
    }

    /// 获取第三方 OAuth2 Provider 注册表
    pub fn get_provider_registry() -> Option<&'static cmx_auth::oauth2::OAuth2ProviderRegistry> {
        GLOBAL_OAUTH2_PROVIDER_REGISTRY.get()
    }
}

/// 认证中间件
///
/// 需在 `mw_context_resolver` 之后注册，确保 CmxSvrContext 已存在。
/// 从全局 GlobalAuthService 获取 AuthService，验证 Token 或 API Key 后注入 AuthContext。
pub async fn mw_auth(mut req: Request<Body>, next: Next) -> Result<Response, StatusCode> {
    let path = req.uri().path().to_string();
    let method = req.method().to_string();
    let query = req.uri().query().unwrap_or("").to_string();

    // 1. 白名单检查（支持通配符 * / ** 匹配）
    if GlobalAuthService::is_whitelisted(&path) {
        info!(path = %path, "认证白名单，跳过");
        return Ok(next.run(req).await);
    }

    // 2. 获取认证服务
    let auth_service = match GlobalAuthService::get() {
        Some(svc) => svc.clone(),
        None => {
            // 认证服务未配置，允许通过（兼容无认证场景）
            debug!("认证服务未配置，跳过认证");
            return Ok(next.run(req).await);
        }
    };

    // 3. 尝试提取认证信息（优先 X-API-Key，其次 Bearer Token）
    let auth_ctx = if let Some(api_key) = extract_api_key(&req) {
        // 2.1 修复：直接验证 API Key 返回 AuthContext（无状态，不创建会话）
        debug!(path = %path, "检测到 X-API-Key 头，使用 API Key 认证");
        auth_service.validate_api_key(&api_key).await.map_err(|e| {
            warn!(method = %method, path = %path, query = %query, error = %e, "API Key 认证失败，返回 401");
            StatusCode::UNAUTHORIZED
        })?
    } else if let Some(token) = extract_bearer_token(&req) {
        auth_service.validate_token(&token).await.map_err(|e| {
            warn!(method = %method, path = %path, query = %query, error = %e, "Token 验证失败，返回 401");
            StatusCode::UNAUTHORIZED
        })?
    } else {
        warn!(method = %method, path = %path, query = %query, "缺少 Authorization 头或 X-API-Key 头，返回 401");
        return Err(StatusCode::UNAUTHORIZED);
    };

    // 4. 注入 AuthContext 到 CmxSvrContext
    let svr_ctx = req
        .extensions_mut()
        .get_mut::<CmxSvrContext>()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    svr_ctx.0.auth_context = Some(auth_ctx);

    // info!(path = %path, "认证通过");
    Ok(next.run(req).await)
}

/// 从 Authorization 头提取 Bearer Token
fn extract_bearer_token(req: &Request<Body>) -> Option<String> {
    let auth_header = req.headers().get("authorization")?.to_str().ok()?;

    if let Some(token) = auth_header.strip_prefix("Bearer ") {
        return Some(token.to_string());
    }

    // 也支持小写的 bearer
    if let Some(token) = auth_header.strip_prefix("bearer ") {
        return Some(token.to_string());
    }

    None
}

/// 从 X-API-Key 头提取 API Key
fn extract_api_key(req: &Request<Body>) -> Option<String> {
    req.headers()
        .get("X-API-Key")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::Request;

    #[test]
    fn test_extract_bearer_token() {
        let req = Request::builder()
            .header("authorization", "Bearer abc123")
            .body(Body::empty())
            .unwrap();
        assert_eq!(extract_bearer_token(&req), Some("abc123".to_string()));
    }

    #[test]
    fn test_extract_bearer_token_lowercase() {
        let req = Request::builder()
            .header("authorization", "bearer xyz789")
            .body(Body::empty())
            .unwrap();
        assert_eq!(extract_bearer_token(&req), Some("xyz789".to_string()));
    }

    #[test]
    fn test_extract_bearer_token_missing() {
        let req = Request::builder().body(Body::empty()).unwrap();
        assert_eq!(extract_bearer_token(&req), None);
    }

    // === 白名单通配符匹配测试 ===

    #[test]
    fn test_wildcard_double_star_matches_multilevel() {
        // `/api/auth/**` 匹配多级子路径
        let rule = compile_rule("/api/auth/**").unwrap();
        assert!(rule.matches("/api/auth/"));
        assert!(rule.matches("/api/auth/oauth2/token"));
        assert!(rule.matches("/api/auth/a/b/c"));
        assert!(!rule.matches("/api/biz/users"));
    }

    #[test]
    fn test_wildcard_single_star_matches_single_segment() {
        // `/api/biz/*` 仅匹配单层路径段（* 可匹配空，故 /api/biz/ 也命中）
        let rule = compile_rule("/api/biz/*").unwrap();
        assert!(rule.matches("/api/biz/users"));
        assert!(rule.matches("/api/biz/orders"));
        // * 可匹配空字符串，故末尾斜杠也命中
        assert!(rule.matches("/api/biz/"));
        // 不匹配多层路径
        assert!(!rule.matches("/api/biz/users/123"));
    }

    #[test]
    fn test_prefix_rule_backward_compatible() {
        // 普通规则（不含通配符）保持前缀匹配
        let rule = compile_rule("/api/auth/login").unwrap();
        assert!(rule.matches("/api/auth/login"));
        assert!(rule.matches("/api/auth/login/special"));
        assert!(!rule.matches("/api/auth/refresh"));
    }

    #[test]
    fn test_wildcard_question_mark_single_char() {
        // `?` 匹配单个非 / 字符
        let rule = compile_rule("/api/v?/users").unwrap();
        assert!(rule.matches("/api/v1/users"));
        assert!(rule.matches("/api/v2/users"));
        assert!(!rule.matches("/api/v12/users"));
    }

    #[test]
    fn test_regex_meta_escaped() {
        // 含正则元字符的路径应被转义为普通字符
        let rule = compile_rule("/api/data.json").unwrap();
        assert!(rule.matches("/api/data.json"));
        assert!(!rule.matches("/api/dataXjson"));
    }
}
