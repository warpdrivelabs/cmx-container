//! REST 参数解析
//!
//! 提供分页查询等参数的解析。

use modql::filter::ListOptions;
use serde::Deserialize;

/// 列表查询的默认限制数量
pub const LIST_LIMIT_DEFAULT: i64 = 1000;

/// 列表查询的最大限制数量
pub const LIST_LIMIT_MAX: i64 = 5000;

/// 获取单条记录的查询参数
///
/// 用于通过 id 查询单条记录。
#[derive(Debug, Deserialize)]
pub struct GetParams {
    /// 主键值
    pub id: String,
}

/// 删除记录的查询参数
///
/// 用于通过 id 删除记录。
#[derive(Debug, Deserialize)]
pub struct DeleteParams {
    /// 主键值
    pub id: String,
}

/// 列表查询参数
///
/// 用于列表查询的通用参数结构。
#[derive(Debug, Deserialize)]
pub struct ListParams<F> {
    /// 过滤条件
    pub filter: Option<F>,
    /// 排序字段（支持多个，用逗号分隔，前缀 - 表示降序）
    pub order_bys: Option<String>,
}

impl<F> ListParams<F> {
    /// 转换为 ListOptions
    pub fn to_list_options(&self) -> ListOptions {
        ListOptions {
            limit: Some(LIST_LIMIT_DEFAULT),
            offset: None,
            order_bys: self.order_bys.as_ref().map(|s| s.as_str().into()),
        }
    }
}

/// 分页查询参数
///
/// 用于列表和分页查询的通用参数结构。
#[derive(Debug, Deserialize)]
pub struct PageParams<F> {
    /// 过滤条件
    pub filter: Option<F>,
    /// 偏移量（从 0 开始）
    pub offset: Option<i64>,
    /// 每页数量
    pub limit: Option<i64>,
    /// 排序字段（支持多个，用逗号分隔，前缀 - 表示降序）
    pub order_bys: Option<String>,
}

impl<F> PageParams<F> {
    /// 获取 limit 值，如果没有设置则返回默认值
    pub fn get_limit(&self) -> i64 {
        self.limit.unwrap_or(20)
    }

    /// 转换为 ListOptions
    pub fn to_list_options(&self) -> ListOptions {
        let limit = self.limit.unwrap_or(20);
        let limit = if limit > LIST_LIMIT_MAX {
            LIST_LIMIT_MAX
        } else {
            limit
        };
        
        ListOptions {
            limit: Some(limit),
            offset: self.offset,
            order_bys: self.order_bys.as_ref().map(|s| s.as_str().into()),
        }
    }
}
