//! REST 参数解析
//!
//! 提供分页查询等参数的解析 文档，。

use modql::filter::ListOptions;
use serde::Deserialize;
use serde_json::Value;
use utoipa::ToSchema;
use cmx_core::LIST_LIMIT_DEFAULT;

/// 分页默认每页条数
pub const PAGE_SIZE_DEFAULT: i64 = 20;

/// 分页最大每页条数
pub const PAGE_SIZE_MAX: i64 = 500;


/// 获取单条记录的查询参数
///
/// 用于通过 id 查询单条记录。
#[derive(Debug, Deserialize, Clone)]
pub struct GetParamsDoc {
    /// 主键值
    pub id: String
}


/// 更新请求 Payload
#[derive(Debug, Clone, serde::Deserialize,ToSchema)]
pub struct UpdatePayloadDoc<E>  {
    /// 主键 ID
    pub id: Value,
    /// 更新数据
    pub data: E,
}

/// 删除请求 Payload
#[derive(Debug, Clone, serde::Deserialize,ToSchema)]
pub struct DeletePayloadDoc {
    /// 主键 ID 列表（单个删除传一个元素）
    pub ids: Vec<Value>,
}


/// 列表查询参数
///
/// 用于列表查询的通用参数结构。
#[derive(Debug, Deserialize, Clone,ToSchema)]
pub struct ListParamsDoc<F> {
    /// 过滤条件
    pub filter: Option<F>,
    /// 排序字段（支持多个，用逗号分隔，前缀 - 表示降序）
    pub order_bys: Option<String>,
}

impl<F> ListParamsDoc<F> {
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
#[derive(Debug, Deserialize, Clone, ToSchema)]
pub struct PageParamsDoc<F> {
    /// 过滤条件
    pub filter: Option<F>,
    /// 页码（从 1 开始）
    #[serde(default = "default_page")]
    pub current: Option<i64>,
    /// 每页条数
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
    /// 获取页码，默认为 1
    pub fn get_page(&self) -> i64 {
        let page = self.current.unwrap_or(1);
        if page < 1 { 1 } else { page }
    }

    /// 获取每页条数，默认为 20
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

    /// 计算偏移量
    pub fn get_offset(&self) -> i64 {
        (self.get_page() - 1) * self.get_size()
    }
}
