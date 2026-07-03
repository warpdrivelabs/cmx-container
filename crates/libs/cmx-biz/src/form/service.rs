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

    /// 按模块编码查询表单定义列表(供模块导出复用,返回结构化 FormDefinition)。
    ///
    /// 封装原 module_export 的内联 SQL,消除导出与导入的不对称。
    ///
    /// # Errors
    /// 数据库查询失败时返回错误
    pub async fn list_by_module(
        mm: &DatabaseManager,
        db_id: &str,
        module_code: &str,
    ) -> Result<Vec<cmx_core::model::module::FormDefinition>> {
        use cmx_core::model::cell::DataValue;
        let sql = "SELECT code, name, description, definition, domain_code, application_code, module_code \
                   FROM cmx_form WHERE module_code = $1 AND archived = 0";
        let ds = mm
            .query_sql_with_datavalues(
                db_id,
                None,
                sql,
                vec![DataValue::String(module_code.to_string())],
                "form_list_by_module",
            )
            .await
            .map_err(|e| crate::error::BizError::internal(format!("按模块查询表单失败: {e}")))?;
        let schema = ds.schema.as_ref();
        let mut result = Vec::new();
        for row in ds.iter() {
            let code = row.get_by_name_as::<String>(schema, "code").unwrap_or_default();
            let name = row.get_by_name_as::<String>(schema, "name").unwrap_or_default();
            let description = row.get_by_name_as::<String>(schema, "description");
            // definition 是 JSONB,可能以 Value 或 String 形式返回,统一归一化
            let definition = row
                .get_by_name_as::<serde_json::Value>(schema, "definition")
                .or_else(|| {
                    row.get_by_name_as::<String>(schema, "definition")
                        .and_then(|s| serde_json::from_str(&s).ok())
                })
                .map(cmx_utils::json::coerce_to_object)
                .unwrap_or_default();
            let domain_code = row
                .get_by_name_as::<String>(schema, "domain_code")
                .unwrap_or_default();
            let application_code = row
                .get_by_name_as::<String>(schema, "application_code")
                .unwrap_or_default();
            let module_code = row
                .get_by_name_as::<String>(schema, "module_code")
                .unwrap_or_default();
            result.push(cmx_core::model::module::FormDefinition {
                code,
                name,
                description,
                definition,
                domain_code,
                application_code,
                module_code,
            });
        }
        Ok(result)
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
