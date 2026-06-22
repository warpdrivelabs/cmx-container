//! IAM 配置定义

use serde::{Deserialize, Serialize};

/// IAM 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IamConfig {
    /// 认证库 db_id（默认 default_db_id）
    #[serde(default)]
    pub auth_db_id: Option<String>,

    /// 密码最小长度
    #[serde(default = "default_password_min_length")]
    pub password_min_length: usize,

    /// 内置角色编码列表（不可删除/修改 code）
    #[serde(default = "default_builtin_role_codes")]
    pub builtin_role_codes: Vec<String>,

    /// 权限缓存 TTL（秒）— 预留配置，当前权限检查依赖 AuthContext 内存查询
    /// 未来若引入 IamChecker 本地缓存（moka），此配置控制缓存过期时间
    #[serde(default = "default_permission_cache_ttl")]
    pub permission_cache_ttl_secs: u64,

    /// 临时授权清理任务执行间隔（秒，默认 3600）
    #[serde(default = "default_cleanup_interval")]
    pub assignment_cleanup_interval_secs: u64,

    /// 审计日志批量阈值（默认 100，超过则聚合为统计记录）
    #[serde(default = "default_audit_batch_size")]
    pub audit_batch_size: u32,

    /// 是否启用 SoD 规则校验（默认 false）
    #[serde(default)]
    pub enable_sod_check: bool,

    /// SoD 校验范围（默认 All，校验用户合并后所有权限；Incremental 仅校验本次新增）
    #[serde(default = "default_sod_check_scope")]
    pub sod_check_scope: SodCheckScope,

    /// 角色层级最大深度（默认 10，0 表示不限制）
    #[serde(default = "default_role_max_depth")]
    pub role_max_depth: u32,

    /// 故障降级策略（默认 FailClose）
    #[serde(default = "default_failure_mode")]
    pub failure_mode: FailureMode,

    /// 熔断阈值：连续失败次数（默认 5）
    #[serde(default = "default_circuit_breaker_threshold")]
    pub circuit_breaker_threshold: u32,

    /// 熔断恢复时间（秒，默认 60）
    #[serde(default = "default_circuit_breaker_reset_secs")]
    pub circuit_breaker_reset_secs: u64,
}

/// 故障降级策略
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum FailureMode {
    /// 故障开放：缓存/DB 故障时，仅放行 system:all 用户
    FailOpen,
    /// 故障封闭：缓存/DB 故障时，全部拒绝
    FailClose,
}

/// SoD 校验范围
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SodCheckScope {
    /// 校验用户合并后的所有权限（含已有权限）
    All,
    /// 仅校验本次新增的权限
    Incremental,
}

fn default_password_min_length() -> usize {
    8
}

fn default_builtin_role_codes() -> Vec<String> {
    vec!["admin".to_string()]
}

fn default_permission_cache_ttl() -> u64 {
    300
}

fn default_cleanup_interval() -> u64 {
    3600
}

fn default_audit_batch_size() -> u32 {
    100
}

fn default_sod_check_scope() -> SodCheckScope {
    SodCheckScope::All
}

fn default_role_max_depth() -> u32 {
    10
}

fn default_failure_mode() -> FailureMode {
    FailureMode::FailClose
}

fn default_circuit_breaker_threshold() -> u32 {
    5
}

fn default_circuit_breaker_reset_secs() -> u64 {
    60
}

impl Default for IamConfig {
    fn default() -> Self {
        Self {
            auth_db_id: None,
            password_min_length: default_password_min_length(),
            builtin_role_codes: default_builtin_role_codes(),
            permission_cache_ttl_secs: default_permission_cache_ttl(),
            assignment_cleanup_interval_secs: default_cleanup_interval(),
            audit_batch_size: default_audit_batch_size(),
            enable_sod_check: false,
            sod_check_scope: default_sod_check_scope(),
            role_max_depth: default_role_max_depth(),
            failure_mode: default_failure_mode(),
            circuit_breaker_threshold: default_circuit_breaker_threshold(),
            circuit_breaker_reset_secs: default_circuit_breaker_reset_secs(),
        }
    }
}
