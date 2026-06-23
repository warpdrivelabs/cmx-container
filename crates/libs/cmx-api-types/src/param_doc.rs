//! OpenAPI 文档参数结构定义。
//!
//! 该模块提供了 REST API 接口中常用的请求参数结构，用于自动生成 OpenAPI 文档。
//! 包括单条查询、更新/删除载荷、列表过滤和分页查询等场景的参数类型。

use modql::filter::ListOptions;
use serde::Deserialize;
use serde_json::Value;
use utoipa::ToSchema;
use cmx_core::LIST_LIMIT_DEFAULT;

/// 默认每页条数。
pub const PAGE_SIZE_DEFAULT: i64 = 20;

/// 每页最大条数。
pub const PAGE_SIZE_MAX: i64 = 500;

/// 单条记录查询参数。
///
/// 用于根据 ID 获取单条记录的接口。
#[derive(Debug, Deserialize, Clone)]
pub struct GetParamsDoc {
    /// 记录唯一标识。
    pub id: String,
}

/// 更新操作载荷。
///
/// 包含待更新记录的 ID 和更新数据。
#[derive(Debug, Clone, serde::Deserialize, ToSchema)]
pub struct UpdatePayloadDoc<E> {
    /// 待更新记录的唯一标识。
    pub id: Value,
    /// 更新数据。
    pub data: E,
}

/// 批量删除操作载荷。
///
/// 包含待删除记录的 ID 列表。
#[derive(Debug, Clone, serde::Deserialize, ToSchema)]
pub struct DeletePayloadDoc {
    /// 待删除记录的唯一标识列表。
    pub ids: Vec<Value>,
}

/// 列表查询参数。
///
/// 支持过滤条件和排序，不带分页信息，适用于全量列表查询。
#[derive(Debug, Deserialize, Clone, ToSchema)]
pub struct ListParamsDoc<F> {
    /// 单个过滤条件。
    pub filter: Option<F>,
    /// 多个过滤条件列表。
    pub filters: Option<Vec<F>>,
    /// 排序字段，格式为 `"field1:asc,field2:desc"`。
    pub order_bys: Option<String>,
}

impl<F> ListParamsDoc<F> {
    /// 将列表查询参数转换为 modql 的 [`ListOptions`]。
    ///
    /// # Returns
    ///
    /// 返回使用默认 limit 和当前 `order_bys` 构造的 `ListOptions`。
    pub fn to_list_options(&self) -> ListOptions {
        ListOptions {
            limit: Some(LIST_LIMIT_DEFAULT),
            offset: None,
            order_bys: self.order_bys.as_ref().map(|s| s.as_str().into()),
        }
    }
}

/// 分页查询参数。
///
/// 支持过滤条件、排序和分页信息，适用于分页列表查询。
#[derive(Debug, Deserialize, Clone, ToSchema)]
pub struct PageParamsDoc<F> {
    /// 多个过滤条件列表。
    pub filters: Option<Vec<F>>,
    /// 当前页码，默认为 1。
    #[serde(default = "default_page")]
    pub current: Option<i64>,
    /// 每页条数，默认为 [`PAGE_SIZE_DEFAULT`]。
    #[serde(default = "default_size")]
    pub size: Option<i64>,
}

fn default_page() -> Option<i64> {
    Some(1)
}

fn default_size() -> Option<i64> {
    Some(PAGE_SIZE_DEFAULT)
}

impl<F> PageParamsDoc<F> {
    /// 获取当前页码。
    ///
    /// 当 `current` 为 `None` 或小于 1 时，返回 1。
    ///
    /// # Returns
    ///
    /// 不小于 1 的页码值。
    pub fn get_page(&self) -> i64 {
        let page = self.current.unwrap_or(1);
        if page < 1 { 1 } else { page }
    }

    /// 获取每页条数。
    ///
    /// 当 `size` 为 `None` 或小于 1 时，返回 [`PAGE_SIZE_DEFAULT`]；
    /// 当 `size` 超过 [`PAGE_SIZE_MAX`] 时，返回 [`PAGE_SIZE_MAX`]。
    ///
    /// # Returns
    ///
    /// 在 1 到 [`PAGE_SIZE_MAX`] 范围内的每页条数。
    pub fn get_size(&self) -> i64 {
        let size = self.size.unwrap_or(PAGE_SIZE_DEFAULT);
        if size < 1 {
            PAGE_SIZE_DEFAULT
        } else if size > PAGE_SIZE_MAX {
            PAGE_SIZE_MAX
        } else {
            size
        }
    }

    /// 计算数据库查询偏移量。
    ///
    /// # Returns
    ///
    /// 返回 `(page - 1) * size` 的值，用于 SQL 的 `OFFSET` 子句。
    pub fn get_offset(&self) -> i64 {
        (self.get_page() - 1) * self.get_size()
    }
}
