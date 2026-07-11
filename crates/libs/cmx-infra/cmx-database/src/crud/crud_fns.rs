//! 通用 CRUD 服务
//!
//! 提供通用的 CRUD 操作，使用 cmx-database 执行 SQL，返回 DataSet。

use std::convert::TryInto;
use std::marker::PhantomData;
use std::sync::Arc;

use crate::crud::error::{Result, ServiceError};
use crate::crud::{
    DbBmc, decrypt_dataset_fields, encrypt_sea_fields, prep_fields_for_create,
    prep_fields_for_update,
};
use crate::{DatabaseManager, Error};
use cmx_core::UpdatePayload;
use cmx_core::model::data::dataset::{DataSet, Schema};
use modql::SIden;
use modql::field::HasSeaFields;
use modql::filter::{FilterGroups, IntoFilterNodes, ListOptions};
use sea_query::{Asterisk, Condition, Expr, ExprTrait, IntoIden, PostgresQueryBuilder, Query};
use sea_query_sqlx::SqlxBinder;
use serde_json::Value;
use tracing::{debug, error, info, warn};

/// 创建一个空的 DataSet（用于返回操作结果）
fn empty_dataset() -> DataSet {
    let schema = Arc::new(Schema::new_unchecked("empty", vec![]));
    DataSet::empty("result", schema)
}

/// 将 serde_json::Value 转换为 sea_query::SimpleExpr
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
                sea_query::Value::String(Some(n.to_string()))
            }
        }
        Value::String(s) => sea_query::Value::String(Some(s)),
        Value::Array(arr) => sea_query::Value::String(Some(serde_json::to_string(&arr).unwrap())),
        Value::Object(obj) => sea_query::Value::String(Some(serde_json::to_string(&obj).unwrap())),
    };
    sea_query::SimpleExpr::Value(sea_value)
}

/// 通用 CRUD 服务
///
/// 类型参数
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
    /// 创建单个实体
    ///
    /// # 参数
    /// * `mm` - 数据库管理器
    /// * `db_id` - 数据库 ID
    /// * `txn_id` - 事务 ID（可选）
    /// * `data` - 要创建的实体数据
    ///
    /// # 返回值
    /// 返回包含创建结果的 DataSet
    pub async fn create<E>(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: Option<&str>,
        data: E,
    ) -> Result<DataSet>
    where
        E: HasSeaFields,
    {
        info!(
            "{:<12} - GenericCrudService::create - table: {}, db_id: {}",
            "CRUD",
            MC::TABLE,
            db_id
        );

        let mut fields = data.not_none_sea_fields();
        //主键值
        let pk_value = prep_fields_for_create::<MC>(&mut fields, None);
        // 写入前对加密字段进行加密处理
        let fields = encrypt_sea_fields::<MC>(fields);

        let (columns, sea_values) = fields.for_sea_insert();
        let mut query = Query::insert();
        query
            .into_table(MC::table_ref())
            .columns(columns)
            .values(sea_values)
            .map_err(|e| ServiceError::internal_error(format!("构建插入语句失败: {}", e)))?;

        let (sql, sql_values) = query.build_sqlx(PostgresQueryBuilder);
        debug!("{:<12} - SQL: {}", "CRUD", sql);

        let rows_affected = mm
            .execute_sql_with_sqlxvalues(db_id, txn_id, &sql, sql_values)
            .await
            .map_err(|ref e| {
                if let Error::Sqlx(err) = e
                    && let sqlx::Error::Database(e) = err
                    && e.code() == Some("23505".into())
                {
                    return ServiceError::business_error("数据已存在");
                }

                error!("{:<12} - 创建失败: {}, table: {}", "CRUD", e, MC::TABLE);
                ServiceError::internal_error(format!("创建失败 [{}]: {}", MC::TABLE, e))
            })?;

        info!("{:<12} - 创建成功, 影响行数: {}", "CRUD", rows_affected);
        Self::get(mm, db_id, txn_id, Value::String(pk_value)).await
    }

    /// 批量创建多个实体
    ///
    /// # 参数
    /// * `mm` - 数据库管理器
    /// * `db_id` - 数据库 ID
    /// * `txn_id` - 事务 ID（可选）
    /// * `data` - 要创建的实体数据向量
    ///
    /// # 返回值
    /// 返回包含创建结果的 DataSet
    pub async fn create_many<E>(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: Option<&str>,
        data: Vec<E>,
    ) -> Result<DataSet>
    where
        E: HasSeaFields,
    {
        info!(
            "{:<12} - GenericCrudService::create_many - table: {}, count: {}",
            "CRUD",
            MC::TABLE,
            data.len()
        );

        if data.is_empty() {
            return Err(ServiceError::bad_request("创建数据不能为空"));
        }

        let mut query = Query::insert();

        for item in data {
            let mut fields = item.not_none_sea_fields();
            prep_fields_for_create::<MC>(&mut fields, None);
            // 写入前对加密字段进行加密处理
            let fields = encrypt_sea_fields::<MC>(fields);
            let (columns, sea_values) = fields.for_sea_insert();

            query
                .into_table(MC::table_ref())
                .columns(columns.clone())
                .values(sea_values)
                .map_err(|e| ServiceError::internal_error(format!("构建插入语句失败: {}", e)))?;
        }

        let (sql, sql_values) = query.build_sqlx(PostgresQueryBuilder);
        debug!("{:<12} - SQL: {}", "CRUD", sql);

        let rows_affected = mm
            .execute_sql_with_sqlxvalues(db_id, txn_id, &sql, sql_values)
            .await
            .map_err(|e| {
                warn!("{:<12} - 批量创建失败: {}, table: {}", "CRUD", e, MC::TABLE);
                ServiceError::internal_error(format!("批量创建失败 [{}]: {}", MC::TABLE, e))
            })?;

        info!("{:<12} - 批量创建成功, 影响行数: {}", "CRUD", rows_affected);

        Ok(empty_dataset())
    }

    /// 根据主键获取单条实体
    ///
    /// # 参数
    /// * `mm` - 数据库管理器
    /// * `db_id` - 数据库 ID
    /// * `txn_id` - 事务 ID（可选）
    /// * `id` - 主键值
    ///
    /// # 返回值
    /// 返回包含查询结果的 DataSet
    pub async fn get(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: Option<&str>,
        id: Value,
    ) -> Result<DataSet> {
        debug!(
            "{:<12} - GenericCrudService::get - table: {}, db_id: {}, id: {:?}",
            "CRUD",
            MC::TABLE,
            db_id,
            id
        );

        let mut query = Query::select();
        query.from(MC::table_ref());
        query.column(Asterisk);

        let pk_col = SIden(MC::PK_COLUMN).into_iden();
        query.and_where(Expr::col(pk_col).eq(json_value_to_sea_query(id.clone())));

        // 自动追加 archived = 0 过滤（物理删除表可通过 use_archived()=false 跳过）
        if MC::use_archived() {
            query.and_where(Expr::col(SIden("archived").into_iden()).eq(0));
        }

        let (sql, sql_values) = query.build_sqlx(PostgresQueryBuilder);
        debug!("{:<12} - SQL: {}", "CRUD", sql);

        let mut dataset = mm
            .query_sql_with_sqlxvalues(db_id, txn_id, &sql, sql_values, MC::TABLE)
            .await
            .map_err(|e| {
                warn!(
                    "{:<12} - 查询失败: {}, table: {}, id: {:?}",
                    "CRUD",
                    e,
                    MC::TABLE,
                    id
                );
                ServiceError::internal_error(format!("查询失败 [{}]: {}", MC::TABLE, e))
            })?;

        decrypt_dataset_fields(&mut dataset, MC::encrypted_fields());

        let row_count = dataset.iter().count();
        debug!("{:<12} - 查询返回 {} 行", "CRUD", row_count);

        Ok(dataset)
    }

    /// 更新单个实体
    ///
    /// # 参数
    /// * `mm` - 数据库管理器
    /// * `db_id` - 数据库 ID
    /// * `txn_id` - 事务 ID（可选）
    /// * `id` - 主键值
    /// * `data` - 要更新的数据
    ///
    /// # 返回值
    /// 返回包含更新后结果的 DataSet
    pub async fn update<E>(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: Option<&str>,
        id: Value,
        data: E,
    ) -> Result<DataSet>
    where
        E: HasSeaFields,
    {
        info!(
            "{:<12} - GenericCrudService::update - table: {}, db_id: {}, id: {:?}",
            "CRUD",
            MC::TABLE,
            db_id,
            id
        );

        let mut fields = data.not_none_sea_fields();
        prep_fields_for_update::<MC>(&mut fields, None);
        // 更新前对加密字段进行加密处理
        let fields = encrypt_sea_fields::<MC>(fields);

        let fields = fields.for_sea_update();
        let mut query = Query::update();
        query
            .table(MC::table_ref())
            .values(fields)
            .and_where(Expr::col(SIden(MC::PK_COLUMN)).eq(json_value_to_sea_query(id.clone())));

        let (sql, sql_values) = query.build_sqlx(PostgresQueryBuilder);
        debug!("{:<12} - SQL: {}", "CRUD", sql);

        let rows_affected = mm
            .execute_sql_with_sqlxvalues(db_id, txn_id, &sql, sql_values)
            .await
            .map_err(|e| {
                warn!(
                    "{:<12} - 更新失败: {}, table: {}, id: {:?}",
                    "CRUD",
                    e,
                    MC::TABLE,
                    id
                );
                ServiceError::internal_error(format!("更新失败 [{}]: {}", MC::TABLE, e))
            })?;

        if rows_affected == 0 {
            warn!(
                "{:<12} - 更新未影响任何行, table: {}, id: {:?}",
                "CRUD",
                MC::TABLE,
                id
            );
        } else {
            info!("{:<12} - 更新成功, 影响行数: {}", "CRUD", rows_affected);
        }

        Self::get(mm, db_id, txn_id, id).await
    }

    /// 批量更新多个实体
    ///
    /// # 参数
    /// * `mm` - 数据库管理器
    /// * `db_id` - 数据库 ID
    /// * `txn_id` - 事务 ID（可选）
    /// * `data` - 更新数据向量（包含 id 和 data）
    ///
    /// # 返回值
    /// 返回包含更新结果的 DataSet
    pub async fn update_many<E>(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: Option<&str>,
        data: Vec<UpdatePayload<E>>,
    ) -> Result<DataSet>
    where
        E: HasSeaFields,
    {
        info!(
            "{:<12} - GenericCrudService::update_many - table: {}, count: {}",
            "CRUD",
            MC::TABLE,
            data.len()
        );

        if data.is_empty() {
            return Err(ServiceError::bad_request("更新数据不能为空"));
        }

        let mut total_affected = 0u64;

        for item in data {
            let mut fields = item.data.not_none_sea_fields();
            prep_fields_for_update::<MC>(&mut fields, None);
            // 更新前对加密字段进行加密处理
            let fields = encrypt_sea_fields::<MC>(fields);

            let fields = fields.for_sea_update();
            let mut query = Query::update();
            query.table(MC::table_ref()).values(fields).and_where(
                Expr::col(SIden(MC::PK_COLUMN)).eq(json_value_to_sea_query(item.id.clone())),
            );

            let (sql, sql_values) = query.build_sqlx(PostgresQueryBuilder);

            let rows_affected = mm
                .execute_sql_with_sqlxvalues(db_id, txn_id, &sql, sql_values)
                .await
                .map_err(|e| {
                    warn!("{:<12} - 批量更新失败: {}, table: {}", "CRUD", e, MC::TABLE);
                    ServiceError::internal_error(format!("批量更新失败 [{}]: {}", MC::TABLE, e))
                })?;

            total_affected += rows_affected;
        }

        info!(
            "{:<12} - 批量更新成功, 总影响行数: {}",
            "CRUD", total_affected
        );

        Ok(empty_dataset())
    }

    /// 删除实体（支持单个和批量）
    ///
    /// # 参数
    /// * `mm` - 数据库管理器
    /// * `db_id` - 数据库 ID
    /// * `txn_id` - 事务 ID（可选）
    /// * `ids` - 主键值列表（单个删除传一个元素即可）
    ///
    /// # 返回值
    /// 返回包含删除信息的 DataSet
    pub async fn delete(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: Option<&str>,
        ids: Vec<Value>,
    ) -> Result<DataSet> {
        info!(
            "{:<12} - GenericCrudService::delete - table: {}, db_id: {}, count: {}",
            "CRUD",
            MC::TABLE,
            db_id,
            ids.len()
        );

        if ids.is_empty() {
            return Ok(empty_dataset());
        }

        let mut query = Query::delete();
        query.from_table(MC::table_ref()).and_where(
            Expr::col(SIden(MC::PK_COLUMN)).is_in(
                ids.iter()
                    .map(|v| json_value_to_sea_query(v.clone()))
                    .collect::<Vec<_>>(),
            ),
        );

        let (sql, sql_values) = query.build_sqlx(PostgresQueryBuilder);
        debug!("{:<12} - SQL: {}", "CRUD", sql);

        let rows_affected = mm
            .execute_sql_with_sqlxvalues(db_id, txn_id, &sql, sql_values)
            .await
            .map_err(|e| {
                warn!("{:<12} - 删除失败: {}, table: {}", "CRUD", e, MC::TABLE);
                ServiceError::internal_error(format!("删除失败 [{}]: {}", MC::TABLE, e))
            })?;

        info!("{:<12} - 删除成功, 影响行数: {}", "CRUD", rows_affected);

        Ok(empty_dataset())
    }
}

impl<MC, F> GenericCrudService<MC, F>
where
    MC: DbBmc,
    F: Into<FilterGroups> + Clone + IntoFilterNodes,
{
    /// 列表查询（带过滤和排序）
    ///
    /// # 参数
    /// * `mm` - 数据库管理器
    /// * `db_id` - 数据库 ID
    /// * `txn_id` - 事务 ID（可选）
    /// * `filters` - 过滤条件列表（可选）
    /// * `list_options` - 列表选项（可选，包含分页和排序）
    ///
    /// # 返回值
    /// 返回包含查询结果的 DataSet
    pub async fn list(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: Option<&str>,
        filters: Option<Vec<F>>,
        list_options: Option<ListOptions>,
    ) -> Result<DataSet> {
        debug!(
            "{:<12} - GenericCrudService::list - table: {}, db_id: {}",
            "CRUD",
            MC::TABLE,
            db_id
        );

        let mut query = Query::select();
        query.from(MC::table_ref());
        query.column(Asterisk);

        if let Some(filters) = filters {
            let filters: FilterGroups = Vec::into(filters);
            let cond: Condition = filters.try_into().map_err(|e| {
                warn!("{:<12} - 过滤条件错误: {}, table: {}", "CRUD", e, MC::TABLE);
                ServiceError::bad_request(format!("过滤条件错误 [{}]: {}", MC::TABLE, e))
            })?;
            query.cond_where(cond);
        }

        // 注意：list/page 不在此处自动追加 archived=0，因为外部 filter 可能已包含 archived 条件
        // （如管理员查归档数据 archived=1）。archived=0 默认过滤由各 Service 的 with_default_archived 注入。
        // get 方法在此层自动追加是安全的（按 PK 查，不会与 filter 冲突）。

        if let Some(lo) = list_options {
            lo.apply_to_sea_query(&mut query);
        }

        let (sql, sql_values) = query.build_sqlx(PostgresQueryBuilder);
        debug!("{:<12} - SQL: {}", "CRUD", sql);

        let mut dataset = mm
            .query_sql_with_sqlxvalues(db_id, txn_id, &sql, sql_values, MC::TABLE)
            .await
            .map_err(|e| {
                warn!("{:<12} - 列表查询失败: {}, table: {}", "CRUD", e, MC::TABLE);
                ServiceError::internal_error(format!("列表查询失败 [{}]: {}", MC::TABLE, e))
            })?;

        decrypt_dataset_fields(&mut dataset, MC::encrypted_fields());

        let row_count = dataset.iter().count();
        debug!("{:<12} - 列表查询返回 {} 行", "CRUD", row_count);

        Ok(dataset)
    }

    /// 分页查询（带过滤和排序）
    ///
    /// # 参数
    /// * `mm` - 数据库管理器
    /// * `db_id` - 数据库 ID
    /// * `txn_id` - 事务 ID（可选）
    /// * `filter` - 过滤条件（可选）
    /// * `list_options` - 列表选项（包含分页和排序）
    ///
    /// # 返回值
    /// 返回包含查询结果的 DataSet 和总数
    pub async fn page(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: Option<&str>,
        filters: Option<Vec<F>>,
        list_options: ListOptions,
    ) -> Result<(DataSet, i64)> {
        debug!(
            "{:<12} - GenericCrudService::page - table: {}, db_id: {}",
            "CRUD",
            MC::TABLE,
            db_id
        );

        let mut query = Query::select();
        query.from(MC::table_ref());
        query.column(Asterisk);

        if let Some(filters) = filters.clone() {
            let filters: FilterGroups = Vec::into(filters);
            let cond: Condition = filters.try_into().map_err(|e| {
                warn!("{:<12} - 过滤条件错误: {}, table: {}", "CRUD", e, MC::TABLE);
                ServiceError::bad_request(format!("过滤条件错误 [{}]: {}", MC::TABLE, e))
            })?;
            query.cond_where(cond);
        }

        // 注意：page 不在此处自动追加 archived=0（同 list，外部 filter 可能已包含 archived 条件）

        list_options.clone().apply_to_sea_query(&mut query);

        let (sql, sql_values) = query.build_sqlx(PostgresQueryBuilder);
        debug!("{:<12} - SQL: {}", "CRUD", sql);

        let mut dataset = mm
            .query_sql_with_sqlxvalues(db_id, txn_id, &sql, sql_values, MC::TABLE)
            .await
            .map_err(|e| {
                warn!("{:<12} - 分页查询失败: {}, table: {}", "CRUD", e, MC::TABLE);
                ServiceError::internal_error(format!("分页查询失败 [{}]: {}", MC::TABLE, e))
            })?;

        decrypt_dataset_fields(&mut dataset, MC::encrypted_fields());

        let row_count = dataset.iter().count();
        let total = Self::count(mm, db_id, txn_id, filters).await?;

        debug!(
            "{:<12} - 分页查询返回 {} 行, 总数: {}",
            "CRUD", row_count, total
        );

        Ok((dataset, total))
    }

    /// 统计数量（带过滤）
    ///
    /// # 参数
    /// * `mm` - 数据库管理器
    /// * `db_id` - 数据库 ID
    /// * `txn_id` - 事务 ID（可选）
    /// * `filter` - 过滤条件（可选）
    ///
    /// # 返回值
    /// 返回记录总数
    pub async fn count(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: Option<&str>,
        filters: Option<Vec<F>>,
    ) -> Result<i64> {
        debug!(
            "{:<12} - GenericCrudService::count - table: {}, db_id: {}",
            "CRUD",
            MC::TABLE,
            db_id
        );

        let mut query = Query::select();
        query.from(MC::table_ref());
        query.expr(Expr::col(Asterisk).count());

        if let Some(filters) = filters {
            let filters: FilterGroups = Vec::into(filters);
            let cond: Condition = filters.try_into().map_err(|e| {
                warn!("{:<12} - 过滤条件错误: {}, table: {}", "CRUD", e, MC::TABLE);
                ServiceError::bad_request(format!("过滤条件错误 [{}]: {}", MC::TABLE, e))
            })?;
            query.cond_where(cond);
        }

        let (sql, sql_values) = query.build_sqlx(PostgresQueryBuilder);
        debug!("{:<12} - SQL: {}", "CRUD", sql);

        let dataset = mm
            .query_sql_with_sqlxvalues(db_id, txn_id, &sql, sql_values, "count")
            .await
            .map_err(|e| {
                warn!("{:<12} - 统计失败: {}, table: {}", "CRUD", e, MC::TABLE);
                ServiceError::internal_error(format!("统计失败 [{}]: {}", MC::TABLE, e))
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

    /// 自定义分页查询（便捷方法）
    ///
    /// 这是 CustomQueryService::page_custom 的便捷包装
    ///
    /// # 参数
    /// * `mm` - 数据库管理器
    /// * `db_id` - 数据库 ID
    /// * `txn_id` - 事务 ID（可选）
    /// * `filters` - 过滤条件列表（可选）
    /// * `list_options` - 列表选项（包含分页和排序）
    /// * `sql` - 自定义 SQL（支持复杂 JOIN 查询）
    /// * `dataset_id` - 数据集 ID（用于标识查询结果集）
    ///
    /// # 返回值
    /// 返回包含查询结果的 DataSet 和总数
    pub async fn page_custom(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: Option<&str>,
        filters: Option<Vec<F>>,
        list_options: ListOptions,
        sql: &str,
    ) -> Result<(DataSet, i64)> {
        crate::crud::CustomQueryService::page_custom(
            mm,
            db_id,
            txn_id,
            filters,
            list_options,
            sql,
            MC::TABLE,
        )
        .await
    }
}
