//! Menu Service
//!
//! 封装菜单的 CRUD 与列表/分页查询逻辑。
//! create 计算树形字段(full_path/is_leaf/level/parent_code)后写入，
//! 参照 cmx-iam/src/permission/service/crud.rs 的路径计算逻辑。

use cmx_core::model::cell::DataValue;
use cmx_core::model::data::dataset::DataSet;
use cmx_database::crud::GenericCrudService;
use cmx_database::DatabaseManager;
use modql::filter::ListOptions;
use serde_json::Value;
use tracing::instrument;

use crate::error::{BizError, Result};
use crate::menu::{MenuBmc, MenuFilter, MenuForCreate, MenuForUpdate};

/// 菜单服务
pub struct MenuService;

impl MenuService {
    /// 创建菜单：计算 full_path/parent_code/level/is_leaf 后事务内写入，
    /// 并更新父节点 is_leaf=0。
    ///
    /// # Errors
    /// 父菜单不存在、数据库写入失败时返回错误
    #[instrument(skip(mm, data))]
    pub async fn create(mm: &DatabaseManager, db_id: &str, data: MenuForCreate) -> Result<DataSet> {
        // 计算树形字段(参照 cmx-iam permission crud.rs:55-65)
        let (parent_code, full_path, level) = match &data.parent_id {
            Some(pid) => {
                // 查父节点
                let parent = GenericCrudService::<MenuBmc>::get(
                    mm,
                    db_id,
                    None,
                    Value::String(pid.clone()),
                )
                .await?;
                let _row = parent.iter().next().ok_or_else(|| {
                    BizError::business(format!("父菜单不存在: {pid}"))
                })?;
                // 通过序列化取父节点字段(schema 索引不便直接取，用 DataSet 的 Serialize)
                let p_json = serde_json::to_value(&parent)?;
                let p_path = p_json
                    .get("rows")
                    .and_then(|r| r.as_array())
                    .and_then(|rows| rows.first())
                    .and_then(|row| row.get("full_path"))
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| BizError::business("父菜单缺少 full_path 字段"))?
                    .to_string();
                let p_level = p_json
                    .get("rows")
                    .and_then(|r| r.as_array())
                    .and_then(|rows| rows.first())
                    .and_then(|row| row.get("level"))
                    .and_then(|v| v.as_i64())
                    .unwrap_or(1) as i32;
                let p_code = p_json
                    .get("rows")
                    .and_then(|r| r.as_array())
                    .and_then(|rows| rows.first())
                    .and_then(|row| row.get("code"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                (p_code, format!("{p_path}/{}", data.code), p_level + 1)
            }
            None => (None, format!("/{}", data.code), 1),
        };

        // 开事务:INSERT 新节点 + 更新父 is_leaf=0
        let txn_ctx = mm.get_transaction_context();
        let guard = txn_ctx
            .begin_with_guard(db_id)
            .await
            .map_err(|e| BizError::business(format!("开启事务失败: {e}")))?;
        let txn_id = guard.txn_id();

        let id = uuid::Uuid::new_v4().to_string();
        let sql = "INSERT INTO cmx_menu \
                   (id, code, name, parent_id, parent_code, full_path, is_leaf, level, \
                    description, path, icon, component, sort_order, visible, extension, \
                    domain_code, application_code, module_code, status, archived) \
                   VALUES ($1, $2, $3, $4, $5, $6, 1, $7, NULL, $8, $9, $10, $11, $12, $13, \
                           $14, $15, $16, 1, 0) \
                   RETURNING *";
        let params: Vec<DataValue> = vec![
            DataValue::String(id),
            DataValue::String(data.code.clone()),
            DataValue::String(data.name.clone()),
            data.parent_id.clone().into(),
            parent_code.clone().into(),
            DataValue::String(full_path),
            DataValue::Int(level as i64),
            data.path.clone().into(),
            data.icon.clone().into(),
            data.component.clone().into(),
            DataValue::Int(data.sort_order as i64),
            DataValue::Int(data.visible as i64),
            data.extension.clone().into(),
            DataValue::String(data.domain_code.clone()),
            DataValue::String(data.application_code.clone()),
            DataValue::String(data.module_code.clone()),
        ];
        let dataset = mm
            .query_sql_with_datavalues(db_id, Some(txn_id), sql, params, "create_menu")
            .await
            .map_err(|e| BizError::business(format!("新增菜单失败: {e}")))?;

        // 父节点 is_leaf = 0
        if let Some(pid) = &data.parent_id {
            let upd_sql = "UPDATE cmx_menu SET is_leaf = 0 WHERE id = $1";
            let _ = mm
                .execute_sql_with_datavalues(
                    db_id,
                    Some(txn_id),
                    upd_sql,
                    vec![DataValue::String(pid.clone())],
                )
                .await;
        }

        // 提交事务
        guard
            .commit()
            .await
            .map_err(|e| BizError::business(format!("事务提交失败: {e}")))?;

        Ok(dataset)
    }

    /// 查询单个菜单
    pub async fn get(mm: &DatabaseManager, db_id: &str, id: &str) -> Result<DataSet> {
        GenericCrudService::<MenuBmc>::get(mm, db_id, None, Value::String(id.to_string()))
            .await
            .map_err(Into::into)
    }

    /// 更新菜单
    pub async fn update(
        mm: &DatabaseManager,
        db_id: &str,
        id: Value,
        data: MenuForUpdate,
    ) -> Result<DataSet> {
        GenericCrudService::<MenuBmc>::update(mm, db_id, None, id, data)
            .await
            .map_err(Into::into)
    }

    /// 删除菜单
    pub async fn delete(mm: &DatabaseManager, db_id: &str, ids: Vec<Value>) -> Result<DataSet> {
        GenericCrudService::<MenuBmc>::delete(mm, db_id, None, ids)
            .await
            .map_err(Into::into)
    }

    /// 按 code 删除菜单(幂等安装用,不存在时静默成功)
    ///
    /// # Errors
    /// 数据库执行失败时返回错误
    pub async fn delete_by_code(mm: &DatabaseManager, db_id: &str, code: &str) -> Result<()> {
        use cmx_core::model::cell::DataValue;
        mm.execute_sql_with_datavalues(
            db_id,
            None,
            "DELETE FROM cmx_menu WHERE code = $1",
            vec![DataValue::String(code.to_string())],
        )
        .await
        .map_err(|e| crate::error::BizError::business(format!("按 code 删除菜单失败: {e}")))?;
        Ok(())
    }

    /// 列表查询
    pub async fn list(
        mm: &DatabaseManager,
        db_id: &str,
        filters: Option<Vec<MenuFilter>>,
        list_options: Option<ListOptions>,
    ) -> Result<DataSet> {
        GenericCrudService::<MenuBmc, MenuFilter>::list(mm, db_id, None, filters, list_options)
            .await
            .map_err(Into::into)
    }

    /// 分页查询
    pub async fn page(
        mm: &DatabaseManager,
        db_id: &str,
        filters: Option<Vec<MenuFilter>>,
        list_options: ListOptions,
    ) -> Result<(DataSet, i64)> {
        GenericCrudService::<MenuBmc, MenuFilter>::page(mm, db_id, None, filters, list_options)
            .await
            .map_err(Into::into)
    }
}
