//! Domain 实体的自定义 Service
//!
//! 展示如何扩展 GenericCrudService 实现自定义业务逻辑

use super::{DomainBmc, DomainFilter, DomainForCreate, DomainForUpdate, DomainTreeNodeData};
use crate::dam_asset_service::DamAssetService;
use crate::{BizError, Result};
use cmx_api_types::TreeNode;
use cmx_core::model::data::dataset::DataSet;
use cmx_database::DatabaseManager;
use cmx_database::crud::GenericCrudService;
use modql::filter::{ListOptions, OpValString, OpValsString};
use serde_json::Value;
use tracing::{debug, info};

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

        GenericCrudService::<DomainBmc, DomainFilter>::page(
            mm,
            db_id,
            None,
            Some(vec![filter]),
            list_options,
        )
        .await
        .map_err(BizError::from)
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
            .map_err(|e| BizError::internal(format!("查询域树形数据失败: {}", e)))?;

        let items: Vec<DomainTreeNodeData> = dataset
            .iter()
            .map(|row| Self::row_to_tree_node(row, &dataset.schema))
            .collect::<Result<Vec<_>>>()?;

        Ok(TreeNode::from_list(items))
    }

    /// 创建域
    ///
    /// 域无文件副作用（域级目录在创建应用/模块时才创建），直接写库。
    pub async fn create(
        mm: &DatabaseManager,
        db_id: &str,
        data: DomainForCreate,
    ) -> Result<DataSet> {
        info!(
            "{:<12} - DomainService::create - code: {}",
            "SERVICE", data.code
        );
        GenericCrudService::<DomainBmc>::create(mm, db_id, None, data)
            .await
            .map_err(BizError::from)
    }

    /// 更新域
    ///
    /// # 流程
    /// 1. 开启事务
    /// 2. 读取旧 code
    /// 3. 执行 DB 更新
    /// 4. 若 code 变更 → 搬移域级目录 + 重写 module/application 的列
    /// 5. 提交事务
    pub async fn update(
        mm: &DatabaseManager,
        db_id: &str,
        id: Value,
        data: DomainForUpdate,
    ) -> Result<DataSet> {
        info!(
            "{:<12} - DomainService::update - id: {}",
            "SERVICE", id
        );

        let tx = mm
            .get_transaction_context()
            .begin_with_guard(db_id)
            .await
            .map_err(|e| BizError::internal(format!("开启事务失败: {}", e)))?;

        // 读取旧 code
        let old_ds = GenericCrudService::<DomainBmc>::get(
            mm,
            db_id,
            Some(tx.txn_id()),
            id.clone(),
        )
        .await?;
        let old_code = Self::get_field(&old_ds, "code").unwrap_or_default();

        let result = GenericCrudService::<DomainBmc>::update(
            mm,
            db_id,
            Some(tx.txn_id()),
            id.clone(),
            data,
        )
        .await?;

        // 检查 code 是否变更（注意：code 不可改，DomainForUpdate 无 code 字段，
        // 但为防御性编程仍检查——若 id 与 code 不同则触发级联）
        let new_code = Self::get_field(&result, "code").unwrap_or_else(|| old_code.clone());

        if old_code != new_code && !old_code.is_empty() && !new_code.is_empty() {
            if let Err(e) = DamAssetService::on_domain_renamed(
                mm,
                db_id,
                Some(tx.txn_id()),
                &old_code,
                &new_code,
            )
            .await
            {
                tx.rollback()
                    .await
                    .map_err(|e| BizError::internal(format!("回滚事务失败: {}", e)))?;
                return Err(e);
            }
        }

        tx.commit()
            .await
            .map_err(|e| BizError::internal(format!("提交事务失败: {}", e)))?;

        Ok(result)
    }

    /// 删除域
    ///
    /// 删前校验：拒绝域下仍有 application 或 module。
    pub async fn delete(mm: &DatabaseManager, db_id: &str, ids: Vec<Value>) -> Result<DataSet> {
        info!(
            "{:<12} - DomainService::delete - count: {}",
            "SERVICE",
            ids.len()
        );

        // 逐个校验引用完整性
        for id in &ids {
            // 先查 code（id 即 code，但防御性查询）
            let ds = GenericCrudService::<DomainBmc>::get(
                mm,
                db_id,
                None,
                id.clone(),
            )
            .await?;
            let code = Self::get_field(&ds, "code").unwrap_or_else(|| id.as_str().unwrap_or("").to_string());
            DamAssetService::check_domain_deletable(mm, db_id, &code).await?;
        }

        let ids_value: Vec<Value> = ids;
        GenericCrudService::<DomainBmc>::delete(mm, db_id, None, ids_value)
            .await
            .map_err(BizError::from)
    }

    /// 从 DataSet 提取字符串字段值（第一行）。
    fn get_field(ds: &DataSet, field: &str) -> Option<String> {
        let row = ds.iter().next()?;
        let value = row.get_by_name(&ds.schema, field)?;
        String::try_from(value.clone()).ok()
    }

    /// 将 DataSet 的一行数据转换为 DomainTreeNodeData
    ///
    /// 从 Row 中按字段名提取值，处理 Option 类型和类型转换
    fn row_to_tree_node(
        row: &cmx_core::model::data::dataset::Row,
        schema: &cmx_core::model::data::dataset::Schema,
    ) -> Result<DomainTreeNodeData> {
        Ok(DomainTreeNodeData {
            id: Self::get_string_opt(row, schema, "id").unwrap_or("".to_string()),
            parent_code: Self::get_string_opt(row, schema, "parent_code"),
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
            .ok_or_else(|| BizError::internal(format!("缺少必填字段: {}", name)))
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
