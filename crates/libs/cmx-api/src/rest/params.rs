// //! REST 参数解析
// //!
// //! 提供分页查询等参数的解析。
//
// use modql::filter::ListOptions;
// use serde::Deserialize;
// use serde_json::Value;
//
// /// 列表查询的默认限制数量
// pub const LIST_LIMIT_DEFAULT: i64 = 1000;
//
// /// 列表查询的最大限制数量
// pub const LIST_LIMIT_MAX: i64 = 5000;
//
// /// 分页默认每页条数
// pub const PAGE_SIZE_DEFAULT: i64 = 20;
//
// /// 分页最大每页条数
// pub const PAGE_SIZE_MAX: i64 = 500;
//
// /// 获取单条记录的查询参数
// ///
// /// 用于通过 id 查询单条记录。
// #[derive(Debug, Deserialize, Clone)]
// pub struct GetParams {
//     /// 主键值
//     pub id: String
// }
//
//
// /// 更新请求 Payload
// #[derive(Debug, Clone, serde::Deserialize)]
// pub struct UpdatePayload<E> {
//     /// 主键 ID
//     pub id: Value,
//     /// 更新数据
//     pub data: E,
// }
//
// /// 删除请求 Payload
// #[derive(Debug, Clone, serde::Deserialize)]
// pub struct DeletePayload {
//     /// 主键 ID 列表（单个删除传一个元素）
//     pub ids: Vec<Value>,
// }
//
//
// /// 列表查询参数
// ///
// /// 用于列表查询的通用参数结构。
// #[derive(Debug, Deserialize, Clone)]
// pub struct ListParams<F> {
//     /// 过滤条件
//     pub filter: Option<F>,
//     /// 排序字段（支持多个，用逗号分隔，前缀 - 表示降序）
//     pub order_bys: Option<String>,
// }
//
// impl<F> ListParams<F> {
//     /// 转换为 ListOptions
//     pub fn to_list_options(&self) -> ListOptions {
//         ListOptions {
//             limit: Some(LIST_LIMIT_DEFAULT),
//             offset: None,
//             order_bys: self.order_bys.as_ref().map(|s| s.as_str().into()),
//         }
//     }
// }
//
// /// 分页查询参数
// ///
// /// 用于列表和分页查询的通用参数结构。
// #[derive(Debug, Deserialize, Clone)]
// pub struct PageParams<F> {
//     /// 过滤条件
//     pub filter: Option<F>,
//     /// 页码（从 1 开始）
//     #[serde(default = "default_page")]
//     pub current: Option<i64>,
//     /// 每页条数
//     #[serde(default = "default_size")]
//     pub size: Option<i64>,
//     /// 排序字段（支持多个，用逗号分隔，前缀 - 表示降序）
//     pub order_bys: Option<String>,
//     // /// 数据库 ID（可选）
//     // #[serde(default)]
//     // pub db_id: Option<String>,
// }
//
// fn default_page() -> Option<i64> {
//     Some(1)
// }
//
// fn default_size() -> Option<i64> {
//     Some(PAGE_SIZE_DEFAULT)
// }
//
// impl<F> PageParams<F> {
//     /// 获取页码，默认为 1
//     pub fn get_page(&self) -> i64 {
//         let page = self.current.unwrap_or(1);
//         if page < 1 { 1 } else { page }
//     }
//
//     /// 获取每页条数，默认为 20
//     pub fn get_size(&self) -> i64 {
//         let size = self.size.unwrap_or(PAGE_SIZE_DEFAULT);
//         if size < 1 {
//             PAGE_SIZE_DEFAULT
//         } else if size > PAGE_SIZE_MAX {
//             PAGE_SIZE_MAX
//         } else {
//             size
//         }
//     }
//
//     /// 计算偏移量
//     pub fn get_offset(&self) -> i64 {
//         (self.get_page() - 1) * self.get_size()
//     }
//
//     /// 转换为 ListOptions
//     pub fn to_list_options(&self) -> ListOptions {
//         let size = self.get_size();
//
//         ListOptions {
//             limit: Some(size),
//             offset: Some(self.get_offset()),
//             order_bys: self.order_bys.as_ref().map(|s| s.as_str().into()),
//         }
//     }
//
//     // /// 获取数据库 ID
//     // pub async fn get_db_id(&self) -> String {
//     //     if self.db_id.is_some() {
//     //         return self.db_id.clone().unwrap();
//     //     }
//     //     get_default_db_manager().get_default_db_id().await
//     // }
// }
//
// #[cfg(test)]
// mod tests {
//     use super::*;
//     use crate::crud::utils::FIELD_CREATE_TIME;
//
//     #[test]
//     fn test_list_params_to_list_options() {
//         let params: ListParams<()> = ListParams {
//             filter: None,
//             order_bys: Some(FIELD_CREATE_TIME.to_string())
//         };
//         let options = params.to_list_options();
//         assert_eq!(options.limit, Some(LIST_LIMIT_DEFAULT));
//         assert_eq!(options.offset, None);
//     }
//
//     #[test]
//     fn test_page_params_default_values() {
//         let params: PageParams<()> = PageParams {
//             filter: None,
//             current: None,
//             size: None,
//             order_bys: None,
//             // db_id: None,
//         };
//         assert_eq!(params.get_page(), 1);
//         assert_eq!(params.get_size(), PAGE_SIZE_DEFAULT);
//         assert_eq!(params.get_offset(), 0);
//     }
//
//     #[test]
//     fn test_page_params_custom_values() {
//         let params: PageParams<()> = PageParams {
//             filter: None,
//             current: Some(3),
//             size: Some(50),
//             order_bys: None,
//             // db_id: None,
//         };
//         assert_eq!(params.get_page(), 3);
//         assert_eq!(params.get_size(), 50);
//         assert_eq!(params.get_offset(), 100);
//     }
//
//     #[test]
//     fn test_page_params_max_size() {
//         let params: PageParams<()> = PageParams {
//             filter: None,
//             current: Some(1),
//             size: Some(10000),
//             order_bys: None,
//             // db_id: None,
//         };
//         assert_eq!(params.get_size(), PAGE_SIZE_MAX);
//     }
//
//     #[test]
//     fn test_page_params_invalid_page() {
//         let params: PageParams<()> = PageParams {
//             filter: None,
//             current: Some(0),
//             size: Some(20),
//             order_bys: None,
//             // db_id: None,
//         };
//         assert_eq!(params.get_page(), 1);
//         assert_eq!(params.get_offset(), 0);
//     }
//
//     #[test]
//     fn test_page_params_to_list_options() {
//         let params: PageParams<()> = PageParams {
//             filter: None,
//             current: Some(3),
//             size: Some(50),
//             order_bys: Some("-create_time".to_string()),
//             // db_id: Some("tenant1".to_string()),
//         };
//         let options = params.to_list_options();
//         assert_eq!(options.limit, Some(50));
//         assert_eq!(options.offset, Some(100));
//     }
//
//     #[test]
//     fn test_deserialize_get_params() {
//         let json = r#"{"id":"test123","db_id":"tenant1"}"#;
//         let params: GetParams = serde_json::from_str(json).unwrap();
//         assert_eq!(params.id, "test123");
//     }
//
//     #[test]
//     fn test_deserialize_page_params() {
//         let json = r#"{"page":2,"size":30,"order_bys":"name"}"#;
//         let params: PageParams<()> = serde_json::from_str(json).unwrap();
//         assert_eq!(params.current, Some(2));
//         assert_eq!(params.size, Some(30));
//         assert_eq!(params.order_bys, Some("name".to_string()));
//     }
//
//     #[test]
//     fn test_deserialize_page_params_missing_values() {
//         let json = r#"{}"#;
//         let params: PageParams<()> = serde_json::from_str(json).unwrap();
//         assert_eq!(params.get_page(), 1);
//         assert_eq!(params.get_size(), PAGE_SIZE_DEFAULT);
//     }
// }
