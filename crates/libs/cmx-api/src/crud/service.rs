//! 通用 CRUD 服务
//!
//! 提供通用的 CRUD 操作，使用 cmx-database 执行 SQL，返回 DataSet。

use std::marker::PhantomData;

use cmx_core::model::data::dataset::DataSet;
use cmx_database::DatabaseManager;
use modql::filter::{FilterGroups, ListOptions};
use modql::{SIden, StringIden};
use sea_query::{Asterisk, Expr, IntoIden, PostgresQueryBuilder, Query};
use sea_query_binder::SqlxBinder;
use serde_json::Value;
use tracing::debug;

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
    pub async fn create(
        mm: &DatabaseManager,
        db_id: &str,
        mut data: Value,
    ) -> Result<DataSet> {
        debug!("{:<12} - GenericCrudService::create", "CRUD");

        prep_fields_for_create::<MC>(&mut data, None);

        let obj = data
            .as_object()
            .ok_or_else(|| Error::bad_request("创建数据必须是 JSON 对象"))?;

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

        mm.execute_sql_with_sqlxvalues(db_id, None, &sql, sql_values)
            .await
            .map_err(|e| Error::internal_error(format!("创建失败: {}", e)))?;

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
    pub async fn get(
        mm: &DatabaseManager,
        db_id: &str,
        id: Value,
    ) -> Result<DataSet> {
        debug!("{:<12} - GenericCrudService::get", "CRUD");

        let mut query = Query::select();
        query.from(MC::table_ref());
        query.column(Asterisk);

        let pk_col = SIden(MC::PK_COLUMN).into_iden();
        query.and_where(Expr::col(pk_col).eq(json_value_to_sea_query(id)));

        let (sql, sql_values) = query.build_sqlx(PostgresQueryBuilder);

        let dataset = mm
            .query_sql_with_sqlxvalues(db_id, None, &sql, sql_values, MC::TABLE)
            .await
            .map_err(|e| Error::internal_error(format!("查询失败: {}", e)))?;

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
    pub async fn update(
        mm: &DatabaseManager,
        db_id: &str,
        id: Value,
        mut data: Value,
    ) -> Result<DataSet> {
        debug!("{:<12} - GenericCrudService::update", "CRUD");

        prep_fields_for_update::<MC>(&mut data, None);

        let obj = data
            .as_object()
            .ok_or_else(|| Error::bad_request("更新数据必须是 JSON 对象"))?;

        let mut query = Query::update();
        query.table(MC::table_ref());

        for (key, val) in obj {
            if key != MC::PK_COLUMN {
                let col = StringIden(key.clone()).into_iden();
                query.value(col, json_value_to_sea_query(val.clone()));
            }
        }

        let pk_col = SIden(MC::PK_COLUMN).into_iden();
        query.and_where(Expr::col(pk_col).eq(json_value_to_sea_query(id.clone())));

        let (sql, sql_values) = query.build_sqlx(PostgresQueryBuilder);

        mm.execute_sql_with_sqlxvalues(db_id, None, &sql, sql_values)
            .await
            .map_err(|e| Error::internal_error(format!("更新失败: {}", e)))?;

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
    pub async fn delete(
        mm: &DatabaseManager,
        db_id: &str,
        id: Value,
    ) -> Result<DataSet> {
        debug!("{:<12} - GenericCrudService::delete", "CRUD");

        let dataset_before = Self::get(mm, db_id, id.clone()).await?;

        let mut query = Query::delete();
        query.from_table(MC::table_ref());

        let pk_col = SIden(MC::PK_COLUMN).into_iden();
        query.and_where(Expr::col(pk_col).eq(json_value_to_sea_query(id)));

        let (sql, sql_values) = query.build_sqlx(PostgresQueryBuilder);

        mm.execute_sql_with_sqlxvalues(db_id, None, &sql, sql_values)
            .await
            .map_err(|e| Error::internal_error(format!("删除失败: {}", e)))?;

        Ok(dataset_before)
    }
}

impl<MC, F> GenericCrudService<MC, F>
where
    MC: DbBmc,
    F: Into<FilterGroups>,
{
    /// 列表查询（带过滤）
    ///
    /// # 参数
    /// * `mm` - 数据库管理器
    /// * `db_id` - 数据库 ID
    /// * `filter` - 过滤条件（可选）
    /// * `list_options` - 列表选项（可选）
    ///
    /// # 返回值
    /// 返回包含查询结果的 DataSet
    pub async fn list(
        mm: &DatabaseManager,
        db_id: &str,
        _filter: Option<F>,
        list_options: Option<ListOptions>,
    ) -> Result<DataSet> {
        debug!("{:<12} - GenericCrudService::list", "CRUD");

        let mut query = Query::select();
        query.from(MC::table_ref());
        query.column(Asterisk);

        if let Some(lo) = list_options {
            if let Some(limit) = lo.limit {
                query.limit(limit as u64);
            }
            if let Some(offset) = lo.offset {
                query.offset(offset as u64);
            }
        }

        let (sql, sql_values) = query.build_sqlx(PostgresQueryBuilder);

        let dataset = mm
            .query_sql_with_sqlxvalues(db_id, None, &sql, sql_values, MC::TABLE)
            .await
            .map_err(|e| Error::internal_error(format!("列表查询失败: {}", e)))?;

        Ok(dataset)
    }

    /// 分页查询（带过滤）
    ///
    /// # 参数
    /// * `mm` - 数据库管理器
    /// * `db_id` - 数据库 ID
    /// * `filter` - 过滤条件（可选）
    /// * `list_options` - 列表选项
    ///
    /// # 返回值
    /// 返回包含查询结果的 DataSet 和总数
    pub async fn page(
        mm: &DatabaseManager,
        db_id: &str,
        _filter: Option<F>,
        list_options: ListOptions,
    ) -> Result<(DataSet, i64)> {
        debug!("{:<12} - GenericCrudService::page", "CRUD");

        let mut query = Query::select();
        query.from(MC::table_ref());
        query.column(Asterisk);

        let limit = list_options.limit.unwrap_or(20);
        let offset = list_options.offset.unwrap_or(0);

        query.limit(limit as u64);
        query.offset(offset as u64);

        let (sql, sql_values) = query.build_sqlx(PostgresQueryBuilder);

        let dataset = mm
            .query_sql_with_sqlxvalues(db_id, None, &sql, sql_values, MC::TABLE)
            .await
            .map_err(|e| Error::internal_error(format!("分页查询失败: {}", e)))?;

        let total = Self::count(mm, db_id, None).await?;

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
    pub async fn count(
        mm: &DatabaseManager,
        db_id: &str,
        _filter: Option<F>,
    ) -> Result<i64> {
        debug!("{:<12} - GenericCrudService::count", "CRUD");

        let mut query = Query::select();
        query.from(MC::table_ref());
        query.expr(Expr::col(Asterisk).count());

        let (sql, sql_values) = query.build_sqlx(PostgresQueryBuilder);

        let dataset = mm
            .query_sql_with_sqlxvalues(db_id, None, &sql, sql_values, "count")
            .await
            .map_err(|e| Error::internal_error(format!("统计失败: {}", e)))?;

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
