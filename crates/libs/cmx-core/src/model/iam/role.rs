//! 角色基础数据模型（WASM 可见）

use serde::{Deserialize, Serialize};
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

/// 角色基础数据模型（WASM 可见）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct Role {
    pub id: String,
    pub code: String,
    pub name: String,
    #[serde(default)]
    pub data_scope: Option<i64>,
    #[serde(default)]
    pub sort_order: Option<i64>,
    #[serde(default)]
    pub status: Option<i64>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub archived: Option<i64>,
    #[serde(default)]
    pub create_time: Option<String>,
    #[serde(default)]
    pub update_time: Option<String>,
    #[serde(default)]
    pub create_by: Option<String>,
    #[serde(default)]
    pub create_name: Option<String>,
    #[serde(default)]
    pub update_by: Option<String>,
    #[serde(default)]
    pub update_name: Option<String>,
}
