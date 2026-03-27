use serde::Serialize;
use utoipa::ToSchema;

/// API 统一响应结构
///
/// # 响应格式
/// ```json
/// {
///     "code": 0,
///     "msg": "success",
///     "data": { ... }
/// }
/// ```
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ApiResp<T> {
    pub code: u16,
    pub msg: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pagination: Option<Pagination>,
}

impl<T> ApiResp<T> {
    pub fn ok(data: T) -> Self {
        Self {
            code: 0,
            msg: "success".to_string(),
            data: Some(data),
            pagination: None,
        }
    }

    pub fn ok_no_data() -> Self {
        Self {
            code: 0,
            msg: "success".to_string(),
            data: None,
            pagination: None,
        }
    }

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

    pub fn fail(code: u16, msg: impl Into<String>) -> Self {
        Self {
            code,
            msg: msg.into(),
            data: None,
            pagination: None,
        }
    }

    pub fn is_success(&self) -> bool {
        self.code == 0
    }
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Pagination {
    pub page: u64,
    pub page_size: u64,
    pub total: u64,
    pub total_pages: u64,
}

impl Pagination {
    pub fn new(page: u64, page_size: u64, total: u64) -> Self {
        let total_pages = if page_size == 0 { 0 } else { (total as f64 / page_size as f64).ceil() as u64 };
        Self { page, page_size, total, total_pages }
    }

    pub fn offset(&self) -> u64 {
        (self.page.saturating_sub(1)) * self.page_size
    }

    pub fn has_prev(&self) -> bool {
        self.page > 1
    }

    pub fn has_next(&self) -> bool {
        self.page < self.total_pages
    }
}

impl<T> ApiResp<Vec<T>> {
    pub fn list(data: Vec<T>) -> Self {
        Self::ok(data)
    }

    pub fn empty_list() -> Self {
        Self::ok(vec![])
    }
}

pub type UnitResp = ApiResp<()>;

impl ApiResp<()> {
    pub fn msg(msg: impl Into<String>) -> Self {
        Self {
            code: 0,
            msg: msg.into(),
            data: None,
            pagination: None,
        }
    }
}
