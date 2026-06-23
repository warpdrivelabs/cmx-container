//! Prometheus 认证指标。
//!
//! 定义 7 个指标：
//! - `login_total`: 登录总数（含 `method` 标签）
//! - `login_failed`: 登录失败数（含 `reason` 标签）
//! - `token_validate_duration`: Token 验证耗时
//! - `active_sessions`: 活跃会话数
//! - `online_users`: 在线用户数
//! - `token_revoked`: Token 撤销数（含 `type` 标签）
//! - `api_key_validations_total`: API Key 验证总数

use prometheus::{HistogramOpts, HistogramVec, IntCounter, IntCounterVec, IntGauge, Opts, Registry};

use crate::error::Result;

lazy_static::lazy_static! {
    /// 全局指标注册表。
    pub static ref AUTH_REGISTRY: Registry = Registry::new();

    /// 登录总数。
    pub static ref LOGIN_TOTAL: IntCounterVec = IntCounterVec::new(
        Opts::new("auth_login_total", "Total login attempts")
            .namespace("cmx"),
        &["method"]
    ).unwrap();

    /// 登录失败数。
    pub static ref LOGIN_FAILED: IntCounterVec = IntCounterVec::new(
        Opts::new("auth_login_failed_total", "Failed login attempts")
            .namespace("cmx"),
        &["reason"]
    ).unwrap();

    /// Token 验证耗时。
    pub static ref TOKEN_VALIDATE_DURATION: HistogramVec = HistogramVec::new(
        HistogramOpts::new("auth_token_validate_duration_seconds", "Token validation duration")
            .namespace("cmx"),
        &["method"]
    ).unwrap();

    /// 活跃会话数。
    pub static ref ACTIVE_SESSIONS: IntGauge = IntGauge::new(
        "auth_active_sessions", "Number of active sessions"
    ).unwrap();

    /// 在线用户数。
    pub static ref ONLINE_USERS: IntGauge = IntGauge::new(
        "auth_online_users", "Number of online users"
    ).unwrap();

    /// Token 撤销数。
    pub static ref TOKEN_REVOKED: IntCounterVec = IntCounterVec::new(
        Opts::new("auth_token_revoked_total", "Total revoked tokens")
            .namespace("cmx"),
        &["type"]
    ).unwrap();

    /// API Key 验证总数（M2M 场景，不计入 `LOGIN_TOTAL`）。
    pub static ref API_KEY_VALIDATIONS_TOTAL: IntCounter = IntCounter::new(
        "cmx_auth_api_key_validations_total", "Total API Key validations"
    ).unwrap();
}

/// 初始化指标（注册到 `AUTH_REGISTRY` 和 Prometheus 全局默认注册表）。
///
/// # Errors
///
/// 当指标注册失败时返回 `AuthInfraError`。
pub fn init_metrics() -> Result<()> {
    AUTH_REGISTRY.register(Box::new(LOGIN_TOTAL.clone()))?;
    AUTH_REGISTRY.register(Box::new(LOGIN_FAILED.clone()))?;
    AUTH_REGISTRY.register(Box::new(TOKEN_VALIDATE_DURATION.clone()))?;
    AUTH_REGISTRY.register(Box::new(ACTIVE_SESSIONS.clone()))?;
    AUTH_REGISTRY.register(Box::new(ONLINE_USERS.clone()))?;
    AUTH_REGISTRY.register(Box::new(TOKEN_REVOKED.clone()))?;
    AUTH_REGISTRY.register(Box::new(API_KEY_VALIDATIONS_TOTAL.clone()))?;

    // 同时注册到 Prometheus 全局默认注册表，确保 /metrics 端点可收集
    let default_registry = prometheus::default_registry();
    default_registry.register(Box::new(LOGIN_TOTAL.clone()))?;
    default_registry.register(Box::new(LOGIN_FAILED.clone()))?;
    default_registry.register(Box::new(TOKEN_VALIDATE_DURATION.clone()))?;
    default_registry.register(Box::new(ACTIVE_SESSIONS.clone()))?;
    default_registry.register(Box::new(ONLINE_USERS.clone()))?;
    default_registry.register(Box::new(TOKEN_REVOKED.clone()))?;
    default_registry.register(Box::new(API_KEY_VALIDATIONS_TOTAL.clone()))?;

    Ok(())
}

/// 记录登录成功。
///
/// # Arguments
///
/// * `method` - 登录方式（如 `password`、`oauth2`）。
pub fn record_login_success(method: &str) {
    LOGIN_TOTAL.with_label_values(&[method]).inc();
}

/// 记录登录失败。
///
/// # Arguments
///
/// * `reason` - 失败原因（如 `invalid_credentials`、`user_disabled`）。
pub fn record_login_failure(reason: &str) {
    LOGIN_FAILED.with_label_values(&[reason]).inc();
}

/// 记录 Token 撤销。
///
/// # Arguments
///
/// * `token_type` - Token 类型（如 `access`、`refresh`）。
pub fn record_token_revoked(token_type: &str) {
    TOKEN_REVOKED.with_label_values(&[token_type]).inc();
}

/// 记录 Token 验证耗时。
///
/// # Arguments
///
/// * `method` - 验证方式（如 `jwt_bearer`、`api_key`）。
/// * `elapsed_secs` - 验证耗时（秒）。
pub fn record_validate_duration(method: &str, elapsed_secs: f64) {
    TOKEN_VALIDATE_DURATION
        .with_label_values(&[method])
        .observe(elapsed_secs);
}

/// 活跃会话数 +1。
pub fn inc_active_sessions() {
    ACTIVE_SESSIONS.inc();
}

/// 活跃会话数 -1。
pub fn dec_active_sessions() {
    ACTIVE_SESSIONS.dec();
}

/// 设置在线用户数。
///
/// # Arguments
///
/// * `count` - 在线用户数。
pub fn set_online_users(count: i64) {
    ONLINE_USERS.set(count);
}

/// 记录 API Key 验证（M2M 场景，不计入 `LOGIN_TOTAL`）。
pub fn record_api_key_validation() {
    API_KEY_VALIDATIONS_TOTAL.inc();
}
