//! Domain 实体的自定义 Service
//!
//! 展示如何扩展 GenericCrudService 实现自定义业务逻辑

use crate::error::{Error, Result};
use crate::rest::TreeNode;
use cmx_core::model::data::dataset::DataSet;
use cmx_database::DatabaseManager;
use modql::filter::{ListOptions, OpValString, OpValsString};
use tracing::{debug, info};
use cmx_database::crud::GenericCrudService;
use super::{DomainBmc, DomainFilter, DomainForCreate, DomainTreeNodeData};

/// Domain 自定义服务
///
/// 继承 GenericCrudService 并添加自定义业务方法
pub struct DomainService;

impl DomainService {


    /// 扩展方法：搜索域名
    ///
    /// 支持模糊搜索和分页
    pub async fn search(
        mm: &DatabaseManager,
        db_id: &str,
        keyword: &str,
        page: i64,
        page_size: i64,
    ) -> Result<(DataSet, i64)> {
        debug!(
            "{:<12} - DomainService::search - keyword: {}",
            "SERVICE", keyword
        );

        let filter = DomainFilter {
            code: None,
            name: Some(OpValsString(vec![OpValString::Contains(
                keyword.to_string(),
            )])),
            r#type: None,
            status: None,
            archived: None,
        };

        let list_options = ListOptions {
            limit: Some(page_size),
            offset: Some((page - 1) * page_size),
            order_bys: Some("name".into()),
        };

        GenericCrudService::<DomainBmc, DomainFilter>::page(mm, db_id, Some(filter), list_options)
            .await
            .map_err(Error::from)
    }

    /// 查询域-应用-模块树形数据
    ///
    /// 执行 tree.sql 查询获取扁平数据，然后构建为树形结构。
    /// 返回按 域→应用→模块 层级组织的树，同级按 sort_order 排序。
    pub async fn get_tree(
        mm: &DatabaseManager,
        db_id: &str,
    ) -> Result<Vec<TreeNode<DomainTreeNodeData>>> {
        debug!("{:<12} - DomainService::get_tree", "SERVICE");

        let sql = include_str!("tree.sql");
        let dataset = mm
            .query_sql(db_id, None, sql, "domain_tree")
            .await
            .map_err(|e| Error::internal_error(format!("查询域树形数据失败: {}", e)))?;

        let items: Vec<DomainTreeNodeData> = dataset
            .iter()
            .map(|row| Self::row_to_tree_node(row, &dataset.schema))
            .collect::<Result<Vec<_>>>()?;

        Ok(TreeNode::from_list(items))
    }

    /// 将 DataSet 的一行数据转换为 DomainTreeNodeData
    ///
    /// 从 Row 中按字段名提取值，处理 Option 类型和类型转换
    fn row_to_tree_node(
        row: &cmx_core::model::data::dataset::Row,
        schema: &cmx_core::model::data::dataset::Schema,
    ) -> Result<DomainTreeNodeData> {
        Ok(DomainTreeNodeData {
            parent_id: Self::get_string_opt(row, schema, "parent_id"),
            code: Self::get_string_required(row, schema, "code")?,
            name: Self::get_string_required(row, schema, "name")?,
            description: Self::get_string_opt(row, schema, "description"),
            r#type: Self::get_string_opt(row, schema, "type"),
            tags: Self::get_string_opt(row, schema, "tags"),
            node_type: Self::get_string_required(row, schema, "node_type")?,
            level: Self::get_i32(row, schema, "level"),
            domain_code: Self::get_string_opt(row, schema, "domain_code"),
            application_code: Self::get_string_opt(row, schema, "application_code"),
            module_code: Self::get_string_opt(row, schema, "module_code"),
            sort_order: Self::get_i32_opt(row, schema, "sort_order"),
            status: Self::get_i32_opt(row, schema, "status"),
            archived: Self::get_i32_opt(row, schema, "archived"),
            create_time: Self::get_string_opt(row, schema, "create_time"),
            update_time: Self::get_string_opt(row, schema, "update_time"),
            create_by: Self::get_string_opt(row, schema, "create_by"),
            create_name: Self::get_string_opt(row, schema, "create_name"),
            update_by: Self::get_string_opt(row, schema, "update_by"),
            update_name: Self::get_string_opt(row, schema, "update_name"),
        })
    }

    /// 从 Row 中按字段名获取可选字符串值
    fn get_string_opt(
        row: &cmx_core::model::data::dataset::Row,
        schema: &cmx_core::model::data::dataset::Schema,
        name: &str,
    ) -> Option<String> {
        row.get_by_name_as(schema, name)
    }

    /// 从 Row 中按字段名获取必填字符串值
    fn get_string_required(
        row: &cmx_core::model::data::dataset::Row,
        schema: &cmx_core::model::data::dataset::Schema,
        name: &str,
    ) -> Result<String> {
        row.get_by_name_as(schema, name)
            .ok_or_else(|| Error::internal_error(format!("缺少必填字段: {}", name)))
    }

    /// 从 Row 中按字段名获取 i32 值，默认 0
    fn get_i32(
        row: &cmx_core::model::data::dataset::Row,
        schema: &cmx_core::model::data::dataset::Schema,
        name: &str,
    ) -> i32 {
        row.get_by_name_as(schema, name).unwrap_or(0)
    }

    /// 从 Row 中按字段名获取可选 i32 值
    fn get_i32_opt(
        row: &cmx_core::model::data::dataset::Row,
        schema: &cmx_core::model::data::dataset::Schema,
        name: &str,
    ) -> Option<i32> {
        row.get_by_name_as(schema, name)
    }
}