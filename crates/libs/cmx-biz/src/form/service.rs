//! Form Service
//!
//! 封装表单的 CRUD 与列表/分页查询逻辑，复用 GenericCrudService

use cmx_core::model::data::dataset::DataSet;
use cmx_database::crud::GenericCrudService;
use cmx_database::DatabaseManager;
use modql::filter::ListOptions;
use serde_json::Value;

use crate::error::Result;
use crate::form::{FormBmc, FormFilter, FormForCreate, FormForUpdate};

/// 表单服务
pub struct FormService;

impl FormService {
    /// 创建表单
    pub async fn create(mm: &DatabaseManager, db_id: &str, data: FormForCreate) -> Result<DataSet> {
        GenericCrudService::<FormBmc>::create(mm, db_id, None, data)
            .await
            .map_err(Into::into)
    }

    /// 查询单个表单
    pub async fn get(mm: &DatabaseManager, db_id: &str, id: &str) -> Result<DataSet> {
        GenericCrudService::<FormBmc>::get(mm, db_id, None, Value::String(id.to_string()))
            .await
            .map_err(Into::into)
    }

    /// 更新表单
    pub async fn update(
        mm: &DatabaseManager,
        db_id: &str,
        id: Value,
        data: FormForUpdate,
    ) -> Result<DataSet> {
        GenericCrudService::<FormBmc>::update(mm, db_id, None, id, data)
            .await
            .map_err(Into::into)
    }

    /// 删除表单
    pub async fn delete(mm: &DatabaseManager, db_id: &str, ids: Vec<Value>) -> Result<DataSet> {
        GenericCrudService::<FormBmc>::delete(mm, db_id, None, ids)
            .await
            .map_err(Into::into)
    }

    /// 按 code 删除表单(幂等安装用,不存在时静默成功)
    ///
    /// # Errors
    /// 数据库执行失败时返回错误
    pub async fn delete_by_code(mm: &DatabaseManager, db_id: &str, code: &str) -> Result<()> {
        use cmx_core::model::cell::DataValue;
        mm.execute_sql_with_datavalues(
            db_id,
            None,
            "DELETE FROM cmx_form WHERE code = $1",
            vec![DataValue::String(code.to_string())],
        )
        .await
        .map_err(|e| crate::error::BizError::business(format!("按 code 删除表单失败: {e}")))?;
        Ok(())
    }

    /// 列表查询
    ///
    /// - `filters`：多组过滤器，组与组之间 OR，组内字段 AND
    /// - `list_options`：分页与排序（None 表示默认 limit=20）
    pub async fn list(
        mm: &DatabaseManager,
        db_id: &str,
        filters: Option<Vec<FormFilter>>,
        list_options: Option<ListOptions>,
    ) -> Result<DataSet> {
        GenericCrudService::<FormBmc, FormFilter>::list(mm, db_id, None, filters, list_options)
            .await
            .map_err(Into::into)
    }

    /// 分页查询
    ///
    /// 返回 `(DataSet, total)`
    pub async fn page(
        mm: &DatabaseManager,
        db_id: &str,
        filters: Option<Vec<FormFilter>>,
        list_options: ListOptions,
    ) -> Result<(DataSet, i64)> {
        GenericCrudService::<FormBmc, FormFilter>::page(mm, db_id, None, filters, list_options)
            .await
            .map_err(Into::into)
    }
}
