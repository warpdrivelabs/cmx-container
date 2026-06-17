//! API Key 实体

use serde::{Deserialize, Serialize};

/// API Key 实体（对应 `cmx_auth_api_key` 数据库表）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyEntity {
    /// Key 前缀（前 8 位，唯一标识）。
    pub key_prefix: String,

    /// Key 的 SHA256 哈希（用于校验明文 Key）。
    pub key_hash: String,

    /// 关联用户 ID（纯服务间调用时为空）。
    pub user_id: Option<String>,

    /// 关联服务名称（如 `billing-service`）。
    pub service_name: Option<String>,

    /// 允许的 scope 列表。
    pub scopes: Vec<String>,

    /// 描述/备注。
    pub description: Option<String>,

    /// 状态：`0` 禁用，`1` 启用。
    pub status: i64,
}
