//! API Key 实体

use serde::{Deserialize, Serialize};

/// API Key 实体（对应 cmx_auth_api_key 数据库表）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyEntity {
    /// Key 前缀（唯一标识）
    pub key_prefix: String,
    /// Key 哈希（SHA256）
    pub key_hash: String,
    /// 关联用户 ID
    pub user_id: Option<String>,
    /// 关联服务名称
    pub service_name: Option<String>,
    /// 允许的 scope（JSON 数组）
    pub scopes: Vec<String>,
    /// 描述
    pub description: Option<String>,
    /// 状态：0-禁用 1-启用
    pub status: i64,
}
