//! 通用 CRUD 服务
//!
//! 提供通用的 CRUD 操作，使用 cmx-database 执行 SQL，返回 DataSet。

use std::convert::TryInto;
use std::marker::PhantomData;

use cmx_core::model::data::dataset::DataSet;
use cmx_database::DatabaseManager;
use modql::filter::{FilterGroups, ListOptions};
use modql::{SIden, StringIden};
use sea_query::{Asterisk, Condition, Expr, IntoIden, PostgresQueryBuilder, Query};
use sea_query_binder::SqlxBinder;
use serde_json::Value;
use tracing::{debug, info, warn};

use crate::crud::traits::DbBmc;
use crate::crud::utils::{prep_fields_for_create, prep_fields_for_update};
use crate::error::{Error, Result};

/// 将 serde_json::Value 转换为 sea_query::SimpleExpr
///
/// # 参数
/// * `value` - serde_json::Value 值
///
/// # 返回值
/// 转换后的 sea_query::SimpleExpr
fn json_value_to_sea_query(value: Value) -> sea_query::SimpleExpr {
    let sea_value = match value {
        Value::Null => sea_query::Value::Bool(None),
        Value::Bool(b) => sea_query::Value::Bool(Some(b)),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                sea_query::Value::BigInt(Some(i))
            } else if let Some(f) = n.as_f64() {
                sea_query::Value::Double(Some(f))
            } else {
                sea_query::Value::String(Some(n.to_string().into()))
            }
        }
        Value::String(s) => sea_query::Value::String(Some(s.into())),
        Value::Array(arr) => sea_query::Value::String(Some(serde_json::to_string(&arr).unwrap().into())),
        Value::Object(obj) => sea_query::Value::String(Some(serde_json::to_string(&obj).unwrap().into())),
    };
    sea_query::SimpleExpr::Value(sea_value)
}

/// 通用 CRUD 服务
///
/// # 类型参数
/// * `MC` - 实现了 DbBmc trait 的模型控制器类型
/// * `F` - 实现了 Into<FilterGroups> 的过滤器类型（可选，仅用于查询）
pub struct GenericCrudService<MC, F = ()>
where
    MC: DbBmc,
{
    _marker: PhantomData<(MC, F)>,
}

impl<MC> GenericCrudService<MC>
where
    MC: DbBmc,
{
    /// 创建实体
    ///
    /// # 参数
    /// * `mm` - 数据库管理器
    /// * `db_id` - 数据库 ID
    /// * `data` - 要创建的实体数据（JSON）
    ///
    /// # 返回值
    /// 返回包含创建结果的 DataSet
    ///
    /// # 错误
    /// * `BadRequest` - 数据不是 JSON 对象
    /// * `InternalError` - 数据库操作失败
    pub async fn create(
        mm: &DatabaseManager,
        db_id: &str,
        mut data: Value,
    ) -> Result<DataSet> {
        info!("{:<12} - GenericCrudService::create - table: {}, db_id: {}", "CRUD", MC::TABLE, db_id);

        prep_fields_for_create::<MC>(&mut data, None);

        let obj = data
            .as_object()
            .ok_or_else(|| Error::bad_request(format!(
                "创建数据必须是 JSON 对象, table: {}", MC::TABLE
            )))?;

        if obj.is_empty() {
            return Err(Error::bad_request(format!(
                "创建数据不能为空, table: {}", MC::TABLE
            )));
        }

        let mut query = Query::insert();
        query.into_table(MC::table_ref());

        let mut columns = Vec::new();
        let mut values = Vec::new();

        for (key, val) in obj {
            columns.push(StringIden(key.clone()).into_iden());
            values.push(json_value_to_sea_query(val.clone()));
        }

        query.columns(columns);
        query.values_panic(values);

        let (sql, sql_values) = query.build_sqlx(PostgresQueryBuilder);
        debug!("{:<12} - SQL: {}", "CRUD", sql);

        let rows_affected = mm.execute_sql_with_sqlxvalues(db_id, None, &sql, sql_values)
            .await
            .map_err(|e| {
                warn!("{:<12} - 创建失败: {}, table: {}", "CRUD", e, MC::TABLE);
                Error::internal_error(format!("创建失败 [{}]: {}", MC::TABLE, e))
            })?;

        if rows_affected == 0 {
            warn!("{:<12} - 创建未影响任何行, table: {}", "CRUD", MC::TABLE);
        } else {
            info!("{:<12} - 创建成功, 影响行数: {}", "CRUD", rows_affected);
        }

        let pk_value = obj.get(MC::PK_COLUMN).cloned().unwrap_or(Value::Null);
        Self::get(mm, db_id, pk_value).await
    }

    /// 根据主键获取单条实体
    ///
    /// # 参数
    /// * `mm` - 数据库管理器
    /// * `db_id` - 数据库 ID
    /// * `id` - 主键值
    ///
    /// # 返回值
    /// 返回包含查询结果的 DataSet
    ///
    /// # 错误
    /// * `InternalError` - 数据库操作失败
    pub async fn get(
        mm: &DatabaseManager,
        db_id: &str,
        id: Value,
    ) -> Result<DataSet> {
        debug!("{:<12} - GenericCrudService::get - table: {}, db_id: {}, id: {:?}",
            "CRUD", MC::TABLE, db_id, id);

        let mut query = Query::select();
        query.from(MC::table_ref());
        query.column(Asterisk);

        let pk_col = SIden(MC::PK_COLUMN).into_iden();
        query.and_where(Expr::col(pk_col).eq(json_value_to_sea_query(id.clone())));

        let (sql, sql_values) = query.build_sqlx(PostgresQueryBuilder);
        debug!("{:<12} - SQL: {}", "CRUD", sql);

        let dataset = mm
            .query_sql_with_sqlxvalues(db_id, None, &sql, sql_values, MC::TABLE)
            .await
            .map_err(|e| {
                warn!("{:<12} - 查询失败: {}, table: {}, id: {:?}", "CRUD", e, MC::TABLE, id);
                Error::internal_error(format!("查询失败 [{}]: {}", MC::TABLE, e))
            })?;

        let row_count = dataset.iter().count();
        debug!("{:<12} - 查询返回 {} 行", "CRUD", row_count);

        Ok(dataset)
    }

    /// 更新实体
    ///
    /// # 参数
    /// * `mm` - 数据库管理器
    /// * `db_id` - 数据库 ID
    /// * `id` - 主键值
    /// * `data` - 要更新的数据（JSON）
    ///
    /// # 返回值
    /// 返回包含更新后结果的 DataSet
    ///
    /// # 错误
    /// * `BadRequest` - 数据不是 JSON 对象
    /// * `InternalError` - 数据库操作失败
    pub async fn update(
        mm: &DatabaseManager,
        db_id: &str,
        id: Value,
        mut data: Value,
    ) -> Result<DataSet> {
        info!("{:<12} - GenericCrudService::update - table: {}, db_id: {}, id: {:?}",
            "CRUD", MC::TABLE, db_id, id);

        prep_fields_for_update::<MC>(&mut data, None);

        let obj = data
            .as_object()
            .ok_or_else(|| Error::bad_request(format!(
                "更新数据必须是 JSON 对象, table: {}", MC::TABLE
            )))?;

        if obj.is_empty() {
            return Err(Error::bad_request(format!(
                "更新数据不能为空, table: {}", MC::TABLE
            )));
        }

        let mut query = Query::update();
        query.table(MC::table_ref());

        let mut update_count = 0;
        for (key, val) in obj {
            if key != MC::PK_COLUMN {
                let col = StringIden(key.clone()).into_iden();
                query.value(col, json_value_to_sea_query(val.clone()));
                update_count += 1;
            }
        }

        if update_count == 0 {
            return Err(Error::bad_request(format!(
                "更新数据不包含有效字段, table: {}", MC::TABLE
            )));
        }

        let pk_col = SIden(MC::PK_COLUMN).into_iden();
        query.and_where(Expr::col(pk_col).eq(json_value_to_sea_query(id.clone())));

        let (sql, sql_values) = query.build_sqlx(PostgresQueryBuilder);
        debug!("{:<12} - SQL: {}", "CRUD", sql);

        let rows_affected = mm.execute_sql_with_sqlxvalues(db_id, None, &sql, sql_values)
            .await
            .map_err(|e| {
                warn!("{:<12} - 更新失败: {}, table: {}, id: {:?}", "CRUD", e, MC::TABLE, id);
                Error::internal_error(format!("更新失败 [{}]: {}", MC::TABLE, e))
            })?;

        if rows_affected == 0 {
            warn!("{:<12} - 更新未影响任何行, table: {}, id: {:?}", "CRUD", MC::TABLE, id);
        } else {
            info!("{:<12} - 更新成功, 影响行数: {}", "CRUD", rows_affected);
        }

        Self::get(mm, db_id, id).await
    }

    /// 删除实体
    ///
    /// # 参数
    /// * `mm` - 数据库管理器
    /// * `db_id` - 数据库 ID
    /// * `id` - 主键值
    ///
    /// # 返回值
    /// 返回包含删除信息的 DataSet
    ///
    /// # 错误
    /// * `InternalError` - 数据库操作失败
    pub async fn delete(
        mm: &DatabaseManager,
        db_id: &str,
        id: Value,
    ) -> Result<DataSet> {
        info!("{:<12} - GenericCrudService::delete - table: {}, db_id: {}, id: {:?}",
            "CRUD", MC::TABLE, db_id, id);

        let dataset_before = Self::get(mm, db_id, id.clone()).await?;

        let mut query = Query::delete();
        query.from_table(MC::table_ref());

        let pk_col = SIden(MC::PK_COLUMN).into_iden();
        query.and_where(Expr::col(pk_col).eq(json_value_to_sea_query(id.clone())));

        let (sql, sql_values) = query.build_sqlx(PostgresQueryBuilder);
        debug!("{:<12} - SQL: {}", "CRUD", sql);

        let rows_affected = mm.execute_sql_with_sqlxvalues(db_id, None, &sql, sql_values)
            .await
            .map_err(|e| {
                warn!("{:<12} - 删除失败: {}, table: {}, id: {:?}", "CRUD", e, MC::TABLE, id);
                Error::internal_error(format!("删除失败 [{}]: {}", MC::TABLE, e))
            })?;

        if rows_affected == 0 {
            warn!("{:<12} - 删除未影响任何行, table: {}, id: {:?}", "CRUD", MC::TABLE, id);
        } else {
            info!("{:<12} - 删除成功, 影响行数: {}", "CRUD", rows_affected);
        }

        Ok(dataset_before)
    }
}

impl<MC, F> GenericCrudService<MC, F>
where
    MC: DbBmc,
    F: Into<FilterGroups> + Clone,
{
    /// 列表查询（带过滤和排序）
    ///
    /// # 参数
    /// * `mm` - 数据库管理器
    /// * `db_id` - 数据库 ID
    /// * `filter` - 过滤条件（可选）
    /// * `list_options` - 列表选项（可选，包含分页和排序）
    ///
    /// # 返回值
    /// 返回包含查询结果的 DataSet
    ///
    /// # 错误
    /// * `BadRequest` - 过滤条件格式错误
    /// * `InternalError` - 数据库操作失败
    pub async fn list(
        mm: &DatabaseManager,
        db_id: &str,
        filter: Option<F>,
        list_options: Option<ListOptions>,
    ) -> Result<DataSet> {
        debug!("{:<12} - GenericCrudService::list - table: {}, db_id: {}",
            "CRUD", MC::TABLE, db_id);

        let mut query = Query::select();
        query.from(MC::table_ref());
        query.column(Asterisk);

        // 应用过滤条件
        if let Some(filter) = filter {
            let filters: FilterGroups = filter.into();
            let cond: Condition = filters
                .try_into()
                .map_err(|e| {
                    warn!("{:<12} - 过滤条件错误: {}, table: {}", "CRUD", e, MC::TABLE);
                    Error::bad_request(format!("过滤条件错误 [{}]: {}", MC::TABLE, e))
                })?;
            query.cond_where(cond);
        }

        // 应用列表选项（分页和排序）
        if let Some(lo) = list_options {
            lo.apply_to_sea_query(&mut query);
        }

        let (sql, sql_values) = query.build_sqlx(PostgresQueryBuilder);
        debug!("{:<12} - SQL: {}", "CRUD", sql);

        let dataset = mm
            .query_sql_with_sqlxvalues(db_id, None, &sql, sql_values, MC::TABLE)
            .await
            .map_err(|e| {
                warn!("{:<12} - 列表查询失败: {}, table: {}", "CRUD", e, MC::TABLE);
                Error::internal_error(format!("列表查询失败 [{}]: {}", MC::TABLE, e))
            })?;

        let row_count = dataset.iter().count();
        debug!("{:<12} - 列表查询返回 {} 行", "CRUD", row_count);

        Ok(dataset)
    }

    /// 分页查询（带过滤和排序）
    ///
    /// # 参数
    /// * `mm` - 数据库管理器
    /// * `db_id` - 数据库 ID
    /// * `filter` - 过滤条件（可选）
    /// * `list_options` - 列表选项（包含分页和排序）
    ///
    /// # 返回值
    /// 返回包含查询结果的 DataSet 和总数
    ///
    /// # 错误
    /// * `BadRequest` - 过滤条件格式错误
    /// * `InternalError` - 数据库操作失败
    pub async fn page(
        mm: &DatabaseManager,
        db_id: &str,
        filter: Option<F>,
        list_options: ListOptions,
    ) -> Result<(DataSet, i64)> {
        debug!("{:<12} - GenericCrudService::page - table: {}, db_id: {}",
            "CRUD", MC::TABLE, db_id);

        let mut query = Query::select();
        query.from(MC::table_ref());
        query.column(Asterisk);

        // 应用过滤条件
        if let Some(filter) = filter.clone() {
            let filters: FilterGroups = filter.into();
            let cond: Condition = filters
                .try_into()
                .map_err(|e| {
                    warn!("{:<12} - 过滤条件错误: {}, table: {}", "CRUD", e, MC::TABLE);
                    Error::bad_request(format!("过滤条件错误 [{}]: {}", MC::TABLE, e))
                })?;
            query.cond_where(cond);
        }

        // 应用列表选项（分页和排序）
        list_options.clone().apply_to_sea_query(&mut query);

        let (sql, sql_values) = query.build_sqlx(PostgresQueryBuilder);
        debug!("{:<12} - SQL: {}", "CRUD", sql);

        let dataset = mm
            .query_sql_with_sqlxvalues(db_id, None, &sql, sql_values, MC::TABLE)
            .await
            .map_err(|e| {
                warn!("{:<12} - 分页查询失败: {}, table: {}", "CRUD", e, MC::TABLE);
                Error::internal_error(format!("分页查询失败 [{}]: {}", MC::TABLE, e))
            })?;

        let row_count = dataset.iter().count();
        let total = Self::count(mm, db_id, filter).await?;

        debug!("{:<12} - 分页查询返回 {} 行, 总数: {}", "CRUD", row_count, total);

        Ok((dataset, total))
    }

    /// 统计数量（带过滤）
    ///
    /// # 参数
    /// * `mm` - 数据库管理器
    /// * `db_id` - 数据库 ID
    /// * `filter` - 过滤条件（可选）
    ///
    /// # 返回值
    /// 返回记录总数
    ///
    /// # 错误
    /// * `BadRequest` - 过滤条件格式错误
    /// * `InternalError` - 数据库操作失败
    pub async fn count(
        mm: &DatabaseManager,
        db_id: &str,
        filter: Option<F>,
    ) -> Result<i64> {
        debug!("{:<12} - GenericCrudService::count - table: {}, db_id: {}",
            "CRUD", MC::TABLE, db_id);

        let mut query = Query::select();
        query.from(MC::table_ref());
        query.expr(Expr::col(Asterisk).count());

        // 应用过滤条件
        if let Some(filter) = filter {
            let filters: FilterGroups = filter.into();
            let cond: Condition = filters
                .try_into()
                .map_err(|e| {
                    warn!("{:<12} - 过滤条件错误: {}, table: {}", "CRUD", e, MC::TABLE);
                    Error::bad_request(format!("过滤条件错误 [{}]: {}", MC::TABLE, e))
                })?;
            query.cond_where(cond);
        }

        let (sql, sql_values) = query.build_sqlx(PostgresQueryBuilder);
        debug!("{:<12} - SQL: {}", "CRUD", sql);

        let dataset = mm
            .query_sql_with_sqlxvalues(db_id, None, &sql, sql_values, "count")
            .await
            .map_err(|e| {
                warn!("{:<12} - 统计失败: {}, table: {}", "CRUD", e, MC::TABLE);
                Error::internal_error(format!("统计失败 [{}]: {}", MC::TABLE, e))
            })?;

        let count = dataset
            .iter()
            .next()
            .and_then(|row| row.get(0))
            .and_then(|val| match val {
                cmx_core::model::cell::DataValue::Int(i) => Some(*i),
                _ => None,
            })
            .unwrap_or(0);

        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// 测试 json_value_to_sea_query 函数
    mod json_value_conversion {
        use super::*;

        #[test]
        fn test_null_value() {
            let result = json_value_to_sea_query(json!(null));
            assert!(matches!(result, sea_query::SimpleExpr::Value(sea_query::Value::Bool(None))));
        }

        #[test]
        fn test_bool_true() {
            let result = json_value_to_sea_query(json!(true));
            assert!(matches!(result, sea_query::SimpleExpr::Value(sea_query::Value::Bool(Some(true)))));
        }

        #[test]
        fn test_bool_false() {
            let result = json_value_to_sea_query(json!(false));
            assert!(matches!(result, sea_query::SimpleExpr::Value(sea_query::Value::Bool(Some(false)))));
        }

        #[test]
        fn test_integer_value() {
            let result = json_value_to_sea_query(json!(42));
            assert!(matches!(result, sea_query::SimpleExpr::Value(sea_query::Value::BigInt(Some(42)))));
        }

        #[test]
        fn test_negative_integer() {
            let result = json_value_to_sea_query(json!(-100));
            assert!(matches!(result, sea_query::SimpleExpr::Value(sea_query::Value::BigInt(Some(-100)))));
        }

        #[test]
        fn test_float_value() {
            let result = json_value_to_sea_query(json!(3.14));
            if let sea_query::SimpleExpr::Value(sea_query::Value::Double(Some(f))) = result {
                assert!((f - 3.14).abs() < 0.001);
            } else {
                panic!("Expected Double value");
            }
        }

        #[test]
        fn test_string_value() {
            let result = json_value_to_sea_query(json!("hello world"));
            if let sea_query::SimpleExpr::Value(sea_query::Value::String(Some(s))) = result {
                assert_eq!(s.as_str(), "hello world");
            } else {
                panic!("Expected String value");
            }
        }

        #[test]
        fn test_empty_string() {
            let result = json_value_to_sea_query(json!(""));
            if let sea_query::SimpleExpr::Value(sea_query::Value::String(Some(s))) = result {
                assert_eq!(s.as_str(), "");
            } else {
                panic!("Expected String value");
            }
        }

        #[test]
        fn test_array_value() {
            let result = json_value_to_sea_query(json!([1, 2, 3]));
            if let sea_query::SimpleExpr::Value(sea_query::Value::String(Some(s))) = result {
                let parsed: Vec<i32> = serde_json::from_str(&s).unwrap();
                assert_eq!(parsed, vec![1, 2, 3]);
            } else {
                panic!("Expected String value (JSON array)");
            }
        }

        #[test]
        fn test_object_value() {
            let result = json_value_to_sea_query(json!({"key": "value"}));
            if let sea_query::SimpleExpr::Value(sea_query::Value::String(Some(s))) = result {
                let parsed: serde_json::Value = serde_json::from_str(&s).unwrap();
                assert_eq!(parsed["key"], "value");
            } else {
                panic!("Expected String value (JSON object)");
            }
        }
    }
}
