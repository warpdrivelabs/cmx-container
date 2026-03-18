//! Domain 实体定义
//!
//! 定义 Domain 实体的数据结构

use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// 领域实体
///
/// 表示系统中的一个领域/域对象
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Domain {
    /// 唯一标识码
    pub code: String,
    /// 名称
    pub name: String,
    /// 描述
    pub description: Option<String>,
    /// 类型
    pub r#type: Option<String>,
    /// 标签（JSON 格式）
    pub tags: Option<String>,
    /// 排序顺序
    pub sort_order: Option<i32>,
    /// 状态（0: 禁用, 1: 启用）
    pub status: Option<i32>,
    /// 是否归档（0: 否, 1: 是）
    pub archived: Option<i32>,
    /// 创建时间
    pub created_at: Option<OffsetDateTime>,
    /// 更新时间
    pub updated_at: Option<OffsetDateTime>,
    /// 创建者 ID
    pub created_by: Option<String>,
    /// 创建者名称
    pub create_name: Option<String>,
    /// 更新者 ID
    pub updated_by: Option<String>,
    /// 更新者名称
    pub update_name: Option<String>,
}

impl Domain {
    /// 创建新的 Domain 实体
    pub fn new(code: String, name: String) -> Self {
        Self {
            code,
            name,
            description: None,
            r#type: None,
            tags: None,
            sort_order: None,
            status: Some(1),
            archived: Some(0),
            created_at: None,
            updated_at: None,
            created_by: None,
            create_name: None,
            updated_by: None,
            update_name: None,
        }
    }
}
