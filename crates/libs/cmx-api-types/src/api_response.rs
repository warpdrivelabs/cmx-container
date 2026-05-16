//! 统一 REST API 响应格式定义。
//!
//! 该模块提供了标准化的 API 响应结构 [`ApiResp`]，支持成功/失败响应、
//! 分页数据以及列表数据等场景，确保所有接口返回一致的 JSON 格式。

use serde::Serialize;
use utoipa::ToSchema;

/// 统一 API 响应结构。
///
/// 所有 REST API 接口应使用此结构作为返回类型，确保响应格式一致。
/// 响应包含业务状态码、消息、可选的数据和分页信息。
///
/// # Examples
///
/// ```
/// use cmx_api_types::ApiResp;
///
/// let resp = ApiResp::ok("hello");
/// assert!(resp.is_success());
/// assert_eq!(resp.data, Some("hello"));
/// ```
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ApiResp<T> {
    /// 业务状态码，0 表示成功，非 0 表示失败。
    pub code: u16,
    /// 响应消息，成功时为 "success"，失败时为错误描述。
    pub msg: String,
    /// 响应数据，成功时包含业务数据，失败时通常为 `None`。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    /// 分页信息，仅在分页查询时存在。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pagination: Option<Pagination>,
}

impl<T> ApiResp<T> {
    /// 构造成功响应，携带业务数据。
    ///
    /// # Arguments
    ///
    /// * `data` - 业务数据。
    ///
    /// # Returns
    ///
    /// 返回 `code` 为 0、`msg` 为 "success" 的成功响应。
    pub fn ok(data: T) -> Self {
        Self {
            code: 0,
            msg: "success".to_string(),
            data: Some(data),
            pagination: None,
        }
    }

    /// 构造成功响应，不携带业务数据。
    ///
    /// # Returns
    ///
    /// 返回 `code` 为 0、`msg` 为 "success"、`data` 为 `None` 的成功响应。
    pub fn ok_no_data() -> Self {
        Self {
            code: 0,
            msg: "success".to_string(),
            data: None,
            pagination: None,
        }
    }

    /// 构造带分页信息的成功响应。
    ///
    /// # Arguments
    ///
    /// * `data` - 当前页的业务数据。
    /// * `page` - 当前页码，从 1 开始。
    /// * `page_size` - 每页条数。
    /// * `total` - 总记录数。
    ///
    /// # Returns
    ///
    /// 返回包含分页信息的成功响应，`total_pages` 由 `total` 和 `page_size` 自动计算。
    pub fn ok_with_pagination(data: T, page: u64, page_size: u64, total: u64) -> Self {
        let total_pages = (total as f64 / page_size as f64).ceil() as u64;
        Self {
            code: 0,
            msg: "success".to_string(),
            data: Some(data),
            pagination: Some(Pagination {
                page,
                page_size,
                total,
                total_pages,
            }),
        }
    }

    /// 构造失败响应，携带错误码和消息。
    ///
    /// # Arguments
    ///
    /// * `code` - 业务错误码，非 0。
    /// * `msg` - 错误描述信息。
    ///
    /// # Returns
    ///
    /// 返回指定错误码和消息的失败响应，`data` 和 `pagination` 均为 `None`。
    pub fn fail(code: u16, msg: impl Into<String>) -> Self {
        Self {
            code,
            msg: msg.into(),
            data: None,
            pagination: None,
        }
    }

    /// 构造失败响应，同时携带业务数据。
    ///
    /// # Arguments
    ///
    /// * `code` - 业务错误码，非 0。
    /// * `msg` - 错误描述信息。
    /// * `data` - 附加的业务数据。
    ///
    /// # Returns
    ///
    /// 返回携带错误码、消息和数据的失败响应，`pagination` 为 `None`。
    pub fn fail_with_data(code: u16, msg: impl Into<String>, data: T) -> Self {
        Self {
            code,
            msg: msg.into(),
            data: Some(data),
            pagination: None,
        }
    }

    /// 判断响应是否为成功状态。
    ///
    /// # Returns
    ///
    /// 当 `code` 为 0 时返回 `true`，否则返回 `false`。
    pub fn is_success(&self) -> bool {
        self.code == 0
    }
}

/// 分页信息。
///
/// 描述分页查询的元数据，包括当前页码、每页条数、总记录数和总页数。
#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Pagination {
    /// 当前页码，从 1 开始。
    pub page: u64,
    /// 每页条数。
    pub page_size: u64,
    /// 总记录数。
    pub total: u64,
    /// 总页数，由 `total` 和 `page_size` 计算得出。
    pub total_pages: u64,
}

impl Pagination {
    /// 根据页码、每页条数和总记录数创建分页信息。
    ///
    /// `total_pages` 由 `total` 除以 `page_size` 向上取整自动计算。
    /// 当 `page_size` 为 0 时，`total_pages` 为 0。
    ///
    /// # Arguments
    ///
    /// * `page` - 当前页码，从 1 开始。
    /// * `page_size` - 每页条数。
    /// * `total` - 总记录数。
    ///
    /// # Returns
    ///
    /// 包含计算后 `total_pages` 的 `Pagination` 实例。
    pub fn new(page: u64, page_size: u64, total: u64) -> Self {
        let total_pages = if page_size == 0 { 0 } else { (total as f64 / page_size as f64).ceil() as u64 };
        Self { page, page_size, total, total_pages }
    }

    /// 计算当前页在数据库中的偏移量。
    ///
    /// # Returns
    ///
    /// 返回 `(page - 1) * page_size` 的值，用于 SQL 的 `OFFSET` 子句。
    pub fn offset(&self) -> u64 {
        (self.page.saturating_sub(1)) * self.page_size
    }

    /// 判断是否存在上一页。
    ///
    /// # Returns
    ///
    /// 当 `page` 大于 1 时返回 `true`。
    pub fn has_prev(&self) -> bool {
        self.page > 1
    }

    /// 判断是否存在下一页。
    ///
    /// # Returns
    ///
    /// 当 `page` 小于 `total_pages` 时返回 `true`。
    pub fn has_next(&self) -> bool {
        self.page < self.total_pages
    }
}

impl<T> ApiResp<Vec<T>> {
    /// 构造列表数据成功响应。
    ///
    /// # Arguments
    ///
    /// * `data` - 列表数据。
    ///
    /// # Returns
    ///
    /// 返回包含列表数据的成功响应。
    pub fn list(data: Vec<T>) -> Self {
        Self::ok(data)
    }

    /// 构造空列表成功响应。
    ///
    /// # Returns
    ///
    /// 返回包含空列表的成功响应。
    pub fn empty_list() -> Self {
        Self::ok(vec![])
    }
}

/// 无数据响应类型别名。
///
/// 用于不需要返回业务数据的场景，如删除、更新等操作。
pub type UnitResp = ApiResp<()>;

impl ApiResp<()> {
    /// 构造携带自定义消息的成功响应。
    ///
    /// # Arguments
    ///
    /// * `msg` - 自定义消息内容。
    ///
    /// # Returns
    ///
    /// 返回 `code` 为 0、携带自定义消息的成功响应。
    pub fn msg(msg: impl Into<String>) -> Self {
        Self {
            code: 0,
            msg: msg.into(),
            data: None,
            pagination: None,
        }
    }
}
