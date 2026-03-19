//! REST 参数解析
//!
//! 提供分页查询等参数的解析。

use modql::filter::ListOptions;
use serde::Deserialize;
use cmx_database::get_default_db_manager;

/// 列表查询的默认限制数量
pub const LIST_LIMIT_DEFAULT: i64 = 1000;

/// 列表查询的最大限制数量
pub const LIST_LIMIT_MAX: i64 = 5000;

// /// 默认数据库 ID
// pub const DB_ID_DEFAULT: &str = "primary";

// /// 数据库 ID 参数
// ///
// /// 用于指定操作哪个数据库（多租户场景）。
// #[derive(Debug, Deserialize, Clone)]
// pub struct DbIdParams {
//     /// 数据库 ID
//     #[serde(default = "default_db_id")]
//     pub db_id: String,
// }

// fn default_db_id() -> String {
//     DB_ID_DEFAULT.to_string()
// }

/// 获取单条记录的查询参数
///
/// 用于通过 id 查询单条记录。
#[derive(Debug, Deserialize, Clone)]
pub struct GetParams {
    /// 主键值
    pub id: String,
    /// 数据库 ID（可选）
    #[serde(default)]
    pub db_id: Option<String>,
}

impl GetParams {
    /// 获取数据库 ID
    pub async fn get_db_id(&self) -> String {
        if self.db_id.is_some() {
            return self.db_id.clone().unwrap();
        }
        get_default_db_manager().get_default_db_id().await
    }
}

/// 删除记录的查询参数
///
/// 用于通过 id 删除记录。
#[derive(Debug, Deserialize, Clone)]
pub struct DeleteParams {
    /// 主键值
    pub id: String,
    /// 数据库 ID（可选）
    #[serde(default)]
    pub db_id: Option<String>,
}

impl DeleteParams {
    /// 获取数据库 ID
    pub async fn get_db_id(&self) -> String {
        if self.db_id.is_some() {
            return self.db_id.clone().unwrap();
        }
        get_default_db_manager().get_default_db_id().await
    }
}

/// 列表查询参数
///
/// 用于列表查询的通用参数结构。
#[derive(Debug, Deserialize, Clone)]
pub struct ListParams<F> {
    /// 过滤条件
    pub filter: Option<F>,
    /// 排序字段（支持多个，用逗号分隔，前缀 - 表示降序）
    pub order_bys: Option<String>,
    /// 数据库 ID（可选）
    #[serde(default)]
    pub db_id: Option<String>,
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

    /// 获取数据库 ID
    pub async fn get_db_id(&self) -> String {
        if self.db_id.is_some() {
            return self.db_id.clone().unwrap();
        }
        get_default_db_manager().get_default_db_id().await
    }
}

/// 分页查询参数
///
/// 用于列表和分页查询的通用参数结构。
#[derive(Debug, Deserialize, Clone)]
// #[serde(rename_all = "camelCase")]  序列化时会支持改成字段小驼峰

pub struct PageParams<F> {
    /// 过滤条件
    pub filter: Option<F>,
    /// 偏移量（从 0 开始）
    pub offset: Option<i64>,
    /// 每页数量
    pub limit: Option<i64>,
    /// 排序字段（支持多个，用逗号分隔，前缀 - 表示降序）
    pub order_bys: Option<String>,
    /// 数据库 ID（可选）
    #[serde(default)]
    pub db_id: Option<String>,
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

    /// 获取数据库 ID
    pub async fn get_db_id(&self) -> String {
        if self.db_id.is_some() {
            return self.db_id.clone().unwrap();
        }
        get_default_db_manager().get_default_db_id().await
    }
}



#[cfg(test)]
mod tests {
    use super::*;



    #[test]
    fn test_list_params_to_list_options() {
        let params: ListParams<()> = ListParams {
            filter: None,
            order_bys: Some("name".to_string()),
            db_id: None,
        };
        let options = params.to_list_options();
        assert_eq!(options.limit, Some(LIST_LIMIT_DEFAULT));
        assert_eq!(options.offset, None);
    }

    #[test]
    fn test_page_params_default_limit() {
        let params: PageParams<()> = PageParams {
            filter: None,
            offset: None,
            limit: None,
            order_bys: None,
            db_id: None,
        };
        assert_eq!(params.get_limit(), 20);
    }

    #[test]
    fn test_page_params_custom_limit() {
        let params: PageParams<()> = PageParams {
            filter: None,
            offset: None,
            limit: Some(50),
            order_bys: None,
            db_id: None,
        };
        assert_eq!(params.get_limit(), 50);
    }

    #[test]
    fn test_page_params_max_limit() {
        let params: PageParams<()> = PageParams {
            filter: None,
            offset: None,
            limit: Some(10000),
            order_bys: None,
            db_id: None,
        };
        let options = params.to_list_options();
        assert_eq!(options.limit, Some(LIST_LIMIT_MAX));
    }

    #[test]
    fn test_page_params_to_list_options() {
        let params: PageParams<()> = PageParams {
            filter: None,
            offset: Some(100),
            limit: Some(50),
            order_bys: Some("-create_time".to_string()),
            db_id: Some("tenant1".to_string()),
        };
        let options = params.to_list_options();
        assert_eq!(options.limit, Some(50));
        assert_eq!(options.offset, Some(100));
    }

    #[test]
    fn test_deserialize_get_params() {
        let json = r#"{"id":"test123","db_id":"tenant1"}"#;
        let params: GetParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.id, "test123");
        assert_eq!(params.db_id, Some("tenant1".to_string()));
    }

    #[test]
    fn test_deserialize_page_params() {
        let json = r#"{"offset":10,"limit":30,"order_bys":"name"}"#;
        let params: PageParams<()> = serde_json::from_str(json).unwrap();
        assert_eq!(params.offset, Some(10));
        assert_eq!(params.limit, Some(30));
        assert_eq!(params.order_bys, Some("name".to_string()));
    }
}
