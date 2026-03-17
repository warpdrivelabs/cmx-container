//! Domain 模型
//!
//! 领域实体模型示例，演示如何使用通用 CRUD 框架。

use crate::crud::traits::DbBmc;
use modql::filter::{FilterNodes, OpValsString, OpValsInt64};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// 领域实体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Domain {
    pub code: String,
    pub name: String,
    pub description: Option<String>,
    pub r#type: Option<String>,
    pub tags: Option<String>,
    pub sort_order: Option<i32>,
    pub status: Option<i32>,
    pub archived: Option<i32>,
    pub created_at: Option<OffsetDateTime>,
    pub updated_at: Option<OffsetDateTime>,
    pub created_by: Option<String>,
    pub create_name: Option<String>,
    pub updated_by: Option<String>,
    pub update_name: Option<String>,
}

/// Domain 模型控制器
pub struct DomainBmc;

impl DbBmc for DomainBmc {
    const TABLE: &'static str = "cmx_domain";
    const PK_COLUMN: &'static str = "code";
}

/// Domain 查询过滤器
#[derive(FilterNodes, Deserialize, Default, Debug)]
pub struct DomainFilter {
    pub code: Option<OpValsString>,
    pub name: Option<OpValsString>,
    pub r#type: Option<OpValsString>,
    pub status: Option<OpValsInt64>,
    pub archived: Option<OpValsInt64>,
}
