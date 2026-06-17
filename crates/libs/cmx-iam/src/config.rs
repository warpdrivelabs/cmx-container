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

impl Default for IamConfig {
    fn default() -> Self {
        Self {
            auth_db_id: None,
            password_min_length: default_password_min_length(),
            builtin_role_codes: default_builtin_role_codes(),
            permission_cache_ttl_secs: default_permission_cache_ttl(),
        }
    }
}
