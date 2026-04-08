//! 表元数据服务
//!
//! 封装 cmx_meta_table_define 和 cmx_meta_table_define_version 两个表的增删改查操作

use chrono::Utc;
use cmx_core::model::data::dataset::DataSet;
use cmx_database::DatabaseManager;
use cmx_database::crud::{DbBmc, GenericCrudService};
use cmx_utils::snowflake_id_str;
use modql::field::{HasSeaFields, SeaField, SeaFields};
use modql::filter::{FilterGroups, ListOptions};
use sea_query::{Asterisk, Condition, Expr, PostgresQueryBuilder, Query, SelectStatement};
use sea_query_binder::SqlxBinder;
use serde_json::Value;
use tracing::{debug, info, warn};

use super::bmc::{TableMetadataBmc, TableMetadataVersionBmc};
use super::entity::{TableMetadataForCreate, TableMetadataForUpdate};
use super::filter::{TableMetadataFilter, TableMetadataVersionFilter};
use crate::error::{PluginError, PluginResult};
use crate::infrastructure::database::table_metadata::TableMetadataDetail;

/// 表元数据服务
pub struct TableMetadataService;

impl TableMetadataService {
    /// 创建表元数据
    ///
    /// 同时写入 cmx_meta_table_define 和 cmx_meta_table_define_version
    pub async fn create(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: Option<&str>,
        data: TableMetadataForCreate,
    ) -> PluginResult<DataSet> {
        info!(
            "{:<12} - TableMetadataService::create - table_name: {}, db_id: {}",
            "SERVICE", data.table_name, db_id
        );

        let id = snowflake_id_str();
        let now = Utc::now();
        let archived = 0;

        let version_id = snowflake_id_str();
        let mut version_fields = SeaFields::new(vec![]);
        version_fields.push(SeaField::new("id", version_id));
        version_fields.push(SeaField::new("table_name", data.table_name.clone()));
        version_fields.push(SeaField::new("db_id", data.db_id.clone()));
        version_fields.push(SeaField::new("plugin_id", data.plugin_id.clone()));
        version_fields.push(SeaField::new("version", data.version.clone()));
        version_fields.push(SeaField::new("domain_code", data.domain_code.clone()));
        version_fields.push(SeaField::new(
            "application_code",
            data.application_code.clone(),
        ));
        version_fields.push(SeaField::new("module_code", data.module_code.clone()));
        version_fields.push(SeaField::new("metadata", data.metadata.clone()));
        version_fields.push(SeaField::new("archived", archived));
        version_fields.push(SeaField::new("create_time", now));
        version_fields.push(SeaField::new("update_time", now));

        let (version_columns, version_values) = version_fields.for_sea_insert();
        let mut version_query = Query::insert();
        version_query
            .into_table(TableMetadataVersionBmc::table_ref())
            .columns(version_columns)
            .values(version_values)
            .map_err(|e| PluginError::Database(format!("构建版本表插入语句失败: {}", e)))?;

        let (version_sql, version_sql_values) = version_query.build_sqlx(PostgresQueryBuilder);
        debug!("{:<12} - SQL: {}", "SERVICE", version_sql);

        mm.execute_sql_with_sqlxvalues(db_id, txn_id, &version_sql, version_sql_values)
            .await
            .map_err(|e| {
                warn!(
                    "{:<12} - 创建版本表记录失败: {}, table_name: {}",
                    "SERVICE", e, data.table_name
                );
                PluginError::Database(format!("创建版本表记录失败: {}", e))
            })?;

        let mut main_fields = SeaFields::new(vec![]);
        main_fields.push(SeaField::new("id", id.clone()));
        main_fields.push(SeaField::new("table_name", data.table_name.clone()));
        main_fields.push(SeaField::new("db_id", data.db_id.clone()));
        main_fields.push(SeaField::new("plugin_id", data.plugin_id.clone()));
        main_fields.push(SeaField::new("version", data.version.clone()));
        main_fields.push(SeaField::new("domain_code", data.domain_code.clone()));
        main_fields.push(SeaField::new(
            "application_code",
            data.application_code.clone(),
        ));
        main_fields.push(SeaField::new("module_code", data.module_code.clone()));
        main_fields.push(SeaField::new("archived", archived));
        main_fields.push(SeaField::new("create_time", now));
        main_fields.push(SeaField::new("update_time", now));

        let (main_columns, main_values) = main_fields.for_sea_insert();
        let mut main_query = Query::insert();
        main_query
            .into_table(TableMetadataBmc::table_ref())
            .columns(main_columns)
            .values(main_values)
            .map_err(|e| PluginError::Database(format!("构建主表插入语句失败: {}", e)))?;

        let (main_sql, main_sql_values) = main_query.build_sqlx(PostgresQueryBuilder);
        debug!("{:<12} - SQL: {}", "SERVICE", main_sql);

        mm.execute_sql_with_sqlxvalues(db_id, txn_id, &main_sql, main_sql_values)
            .await
            .map_err(|e| {
                warn!(
                    "{:<12} - 创建主表记录失败: {}, table_name: {}",
                    "SERVICE", e, data.table_name
                );
                PluginError::Database(format!("创建主表记录失败: {}", e))
            })?;

        info!(
            "{:<12} - 创建成功, id: {}, table_name: {}",
            "SERVICE", id, data.table_name
        );

        Self::get_by_id(mm, db_id, &id).await
    }

    /// 通过主键ID获取详情（联查版本表获取 metadata）
    pub async fn get_by_id(mm: &DatabaseManager, db_id: &str, id: &str) -> PluginResult<DataSet> {
        debug!(
            "{:<12} - TableMetadataService::get_by_id - id: {}",
            "SERVICE", id
        );

        let mut select = Query::select();
        select.from(TableMetadataBmc::table_ref()).columns(vec![
            ("cmx_meta_table_define", "id"),
            ("cmx_meta_table_define", "table_name"),
            ("cmx_meta_table_define", "db_id"),
            ("cmx_meta_table_define", "plugin_id"),
            ("cmx_meta_table_define", "version"),
            ("cmx_meta_table_define", "domain_code"),
            ("cmx_meta_table_define", "application_code"),
            ("cmx_meta_table_define", "module_code"),
            ("cmx_meta_table_define", "archived"),
            ("cmx_meta_table_define", "create_time"),
            ("cmx_meta_table_define", "update_time"),
            ("cmx_meta_table_define", "create_by"),
            ("cmx_meta_table_define", "create_name"),
            ("cmx_meta_table_define", "update_by"),
            ("cmx_meta_table_define", "update_name"),
        ]);

        select.expr_as(
            Expr::col(("cmx_meta_table_define_version", "metadata")),
            "metadata",
        );

        select.join(
            sea_query::JoinType::LeftJoin,
            "cmx_meta_table_define_version",
            Condition::all()
                .add(
                    Expr::col(("cmx_meta_table_define", "table_name"))
                        .equals(("cmx_meta_table_define_version", "table_name")),
                )
                .add(
                    Expr::col(("cmx_meta_table_define", "version"))
                        .equals(("cmx_meta_table_define_version", "version")),
                )
                .add(
                    Expr::col(("cmx_meta_table_define", "db_id"))
                        .equals(("cmx_meta_table_define_version", "db_id")),
                ),
        );

        select.and_where(Expr::col(("cmx_meta_table_define", "id")).eq(id));

        let (sql, sql_values) = select.build_sqlx(PostgresQueryBuilder);
        debug!("{:<12} - SQL: {}", "SERVICE", sql);

        let dataset = mm
            .query_sql_with_sqlxvalues(db_id, None, &sql, sql_values, "table_metadata_detail")
            .await
            .map_err(|e| PluginError::Database(format!("查询表元数据详情失败: {}", e)))?;

        Ok(dataset)
    }

    /// 通过 table_name + db_id 获取详情
    pub async fn get_by_table_name(
        mm: &DatabaseManager,
        db_id: &str,
        table_name: &str,
        target_db_id: &str,
    ) -> PluginResult<DataSet> {
        debug!(
            "{:<12} - TableMetadataService::get_by_table_name - table_name: {}, db_id: {}",
            "SERVICE", table_name, target_db_id
        );

        let mut select = Query::select();
        select.from(TableMetadataBmc::table_ref()).columns(vec![
            ("cmx_meta_table_define", "id"),
            ("cmx_meta_table_define", "table_name"),
            ("cmx_meta_table_define", "db_id"),
            ("cmx_meta_table_define", "plugin_id"),
            ("cmx_meta_table_define", "version"),
            ("cmx_meta_table_define", "domain_code"),
            ("cmx_meta_table_define", "application_code"),
            ("cmx_meta_table_define", "module_code"),
            ("cmx_meta_table_define", "archived"),
            ("cmx_meta_table_define", "create_time"),
            ("cmx_meta_table_define", "update_time"),
            ("cmx_meta_table_define", "create_by"),
            ("cmx_meta_table_define", "create_name"),
            ("cmx_meta_table_define", "update_by"),
            ("cmx_meta_table_define", "update_name"),
        ]);

        select.expr_as(
            Expr::col(("cmx_meta_table_define_version", "metadata")),
            "metadata",
        );

        select.join(
            sea_query::JoinType::LeftJoin,
            "cmx_meta_table_define_version",
            Condition::all()
                .add(
                    Expr::col(("cmx_meta_table_define", "table_name"))
                        .equals(("cmx_meta_table_define_version", "table_name")),
                )
                .add(
                    Expr::col(("cmx_meta_table_define", "version"))
                        .equals(("cmx_meta_table_define_version", "version")),
                )
                .add(
                    Expr::col(("cmx_meta_table_define", "db_id"))
                        .equals(("cmx_meta_table_define_version", "db_id")),
                ),
        );

        select.and_where(Expr::col(("cmx_meta_table_define", "table_name")).eq(table_name));
        select.and_where(Expr::col(("cmx_meta_table_define", "db_id")).eq(target_db_id));

        let (sql, sql_values) = select.build_sqlx(PostgresQueryBuilder);
        debug!("{:<12} - SQL: {}", "SERVICE", sql);

        let dataset = mm
            .query_sql_with_sqlxvalues(db_id, None, &sql, sql_values, "table_metadata_detail")
            .await
            .map_err(|e| PluginError::Database(format!("查询表元数据详情失败: {}", e)))?;

        Ok(dataset)
    }

    /// 列表查询（联查版本表 + 域/应用/模块表获取 name 字段）
    ///
    /// LEFT JOIN cmx_meta_table_define_version 获取 metadata，
    /// LEFT JOIN cmx_domain / cmx_application / cmx_module 获取 domain_name / application_name / module_name
    pub async fn list(
        mm: &DatabaseManager,
        db_id: &str,
        filters: Option<Vec<TableMetadataFilter>>,
        list_options: Option<ListOptions>,
    ) -> PluginResult<DataSet> {
        debug!(
            "{:<12} - TableMetadataService::list - db_id: {}",
            "SERVICE", db_id
        );

        let mut select = Self::build_join_select(false);

        if let Some(filters) = filters {
            let filter_groups: FilterGroups = Vec::into(filters);
            let cond: Condition = filter_groups.try_into().map_err(|e| {
                PluginError::Database(format!("过滤条件错误: {}", e))
            })?;
            select.cond_where(cond);
        }

        if let Some(lo) = list_options {
            lo.apply_to_sea_query(&mut select);
        }

        let (sql, sql_values) = select.build_sqlx(PostgresQueryBuilder);
        debug!("{:<12} - SQL: {}", "SERVICE", sql);

        let dataset = mm
            .query_sql_with_sqlxvalues(db_id, None, &sql, sql_values, "cmx_meta_table_define")
            .await
            .map_err(|e| PluginError::Database(format!("列表查询失败: {}", e)))?;

        Ok(dataset)
    }

    /// 分页查询（联查版本表 + 域/应用/模块表获取 name 字段）
    ///
    /// LEFT JOIN cmx_meta_table_define_version 获取 metadata，
    /// LEFT JOIN cmx_domain / cmx_application / cmx_module 获取 domain_name / application_name / module_name，
    /// 并返回总记录数
    pub async fn page(
        mm: &DatabaseManager,
        db_id: &str,
        filters: Option<Vec<TableMetadataFilter>>,
        list_options: ListOptions,
    ) -> PluginResult<(DataSet, i64)> {
        debug!(
            "{:<12} - TableMetadataService::page - db_id: {}",
            "SERVICE", db_id
        );

        let mut select = Self::build_join_select(false);

        if let Some(filters) = filters.clone() {
            let filter_groups: FilterGroups = Vec::into(filters);
            let cond: Condition = filter_groups.try_into().map_err(|e| {
                PluginError::Database(format!("过滤条件错误: {}", e))
            })?;
            select.cond_where(cond);
        }

        list_options.clone().apply_to_sea_query(&mut select);

        let (sql, sql_values) = select.build_sqlx(PostgresQueryBuilder);
        debug!("{:<12} - SQL: {}", "SERVICE", sql);

        let dataset = mm
            .query_sql_with_sqlxvalues(db_id, None, &sql, sql_values, "cmx_meta_table_define")
            .await
            .map_err(|e| PluginError::Database(format!("分页查询失败: {}", e)))?;

        let total = Self::count(mm, db_id, filters).await?;

        Ok((dataset, total))
    }

    /// 通过 ID 查询详情（联查版本表 + 域/应用/模块表获取 name 字段）
    ///
    /// SQL 与分页查询一致，额外增加 id 等值过滤条件
    pub async fn get_detail_by_id(
        mm: &DatabaseManager,
        db_id: &str,
        id: &str,
    ) -> PluginResult<DataSet> {
        debug!(
            "{:<12} - TableMetadataService::get_detail_by_id - db_id: {}, id: {}",
            "SERVICE", db_id, id
        );

        let mut select = Self::build_join_select(true);
        select.and_where(Expr::col(("cmx_meta_table_define", "id")).eq(id));

        let (sql, sql_values) = select.build_sqlx(PostgresQueryBuilder);
        debug!("{:<12} - SQL: {}", "SERVICE", sql);

        let dataset = mm
            .query_sql_with_sqlxvalues(db_id, None, &sql, sql_values, "cmx_meta_table_define")
            .await
            .map_err(|e| PluginError::Database(format!("查询详情失败: {}", e)))?;

        Ok(dataset)
    }

    /// 构建联查 SELECT 查询（公共基础）
    ///
    /// 主表 cmx_meta_table_define LEFT JOIN 四张表：
    /// - cmx_meta_table_define_version: 通过 table_name + version + db_id 关联，取 metadata
    /// - cmx_domain: 通过 domain_code = code 关联，取 name AS domain_name
    /// - cmx_application: 通过 application_code = code 关联，取 name AS application_name
    /// - cmx_module: 通过 module_code = code 关联，取 name AS module_name
    fn build_join_select(with_metadata: bool) -> SelectStatement {
        let mut select = Query::select();
        select.from(TableMetadataBmc::table_ref()).columns(vec![
            ("cmx_meta_table_define", "id"),
            ("cmx_meta_table_define", "table_name"),
            ("cmx_meta_table_define", "db_id"),
            ("cmx_meta_table_define", "plugin_id"),
            ("cmx_meta_table_define", "version"),
            ("cmx_meta_table_define", "domain_code"),
            ("cmx_meta_table_define", "application_code"),
            ("cmx_meta_table_define", "module_code"),
            ("cmx_meta_table_define", "archived"),
            ("cmx_meta_table_define", "create_time"),
            ("cmx_meta_table_define", "update_time"),
            ("cmx_meta_table_define", "create_by"),
            ("cmx_meta_table_define", "create_name"),
            ("cmx_meta_table_define", "update_by"),
            ("cmx_meta_table_define", "update_name"),
        ]);
        if with_metadata {
            select.expr_as(
                Expr::col(("cmx_meta_table_define_version", "metadata")),
                "metadata",
            );
        }

        select.expr_as(
            Expr::col(("cmx_domain", "name")),
            "domain_name",
        );
        select.expr_as(
            Expr::col(("cmx_application", "name")),
            "application_name",
        );
        select.expr_as(
            Expr::col(("cmx_module", "name")),
            "module_name",
        );

        select.join(
            sea_query::JoinType::LeftJoin,
            "cmx_meta_table_define_version",
            Condition::all()
                .add(Expr::col(("cmx_meta_table_define", "table_name")).equals(("cmx_meta_table_define_version", "table_name")))
                .add(Expr::col(("cmx_meta_table_define", "version")).equals(("cmx_meta_table_define_version", "version")))
                .add(Expr::col(("cmx_meta_table_define", "db_id")).equals(("cmx_meta_table_define_version", "db_id"))),
        );

        select.join(
            sea_query::JoinType::LeftJoin,
            "cmx_domain",
            Condition::all()
                .add(Expr::col(("cmx_meta_table_define", "domain_code")).equals(("cmx_domain", "code"))),
        );

        select.join(
            sea_query::JoinType::LeftJoin,
            "cmx_application",
            Condition::all()
                .add(Expr::col(("cmx_meta_table_define", "application_code")).equals(("cmx_application", "code"))),
        );

        select.join(
            sea_query::JoinType::LeftJoin,
            "cmx_module",
            Condition::all()
                .add(Expr::col(("cmx_meta_table_define", "module_code")).equals(("cmx_module", "code"))),
        );

        select
    }

    /// 统计主表记录数（用于分页查询的总数计算）
    async fn count(
        mm: &DatabaseManager,
        db_id: &str,
        filters: Option<Vec<TableMetadataFilter>>,
    ) -> PluginResult<i64> {
        let mut query = Query::select();
        query.from(TableMetadataBmc::table_ref());
        query.expr(Expr::col(Asterisk).count());

        if let Some(filters) = filters {
            let filter_groups: FilterGroups = Vec::into(filters);
            let cond: Condition = filter_groups.try_into().map_err(|e| {
                PluginError::Database(format!("过滤条件错误: {}", e))
            })?;
            query.cond_where(cond);
        }

        let (sql, sql_values) = query.build_sqlx(PostgresQueryBuilder);
        debug!("{:<12} - SQL: {}", "SERVICE", sql);

        let dataset = mm
            .query_sql_with_sqlxvalues(db_id, None, &sql, sql_values, "count")
            .await
            .map_err(|e| PluginError::Database(format!("统计记录数失败: {}", e)))?;

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

    /// 更新表元数据
    ///
    /// 同时更新 cmx_meta_table_define 和 cmx_meta_table_define_version
    pub async fn update(
        mm: &DatabaseManager,
        plugin_id: &str,
        db_id: &str,
        txn_id: Option<&str>,
        id: Value,
        data: TableMetadataForUpdate,
    ) -> PluginResult<DataSet> {
        info!(
            "{:<12} - TableMetadataService::update - id: {:?}",
            "SERVICE", id
        );

        let existing = Self::get_by_id(mm, db_id, id.as_str().unwrap_or_default()).await?;

        let table_meta_defines_result = Self::parse_metadata_record(&existing);

        if table_meta_defines_result.is_ok()
            && let Some(record) = table_meta_defines_result.unwrap().iter().next()
        {
            let table_name = record.table_name.clone();
            let target_db_id = record.db_id.clone();
            let now = Utc::now();
            let mut main_fields = data.clone().not_none_sea_fields();
            main_fields.push(SeaField::new("update_time", now));
            let list = main_fields
                .into_vec() // 直接 move 出 Vec<SeaField>
                .into_iter()
                .filter(|f| f.iden.to_string() != "metadata")
                .collect::<Vec<_>>();

            let values = SeaFields::new(list).clone().for_sea_update();
            let mut main_query = Query::update();
            main_query
                .table(TableMetadataBmc::table_ref())
                .values(values)
                .and_where(Expr::col("id").eq(id.as_str().unwrap_or_default()));

            let (main_sql, main_sql_values) = main_query.build_sqlx(PostgresQueryBuilder);
            debug!("{:<12} - SQL: {}", "SERVICE", main_sql);

            mm.execute_sql_with_sqlxvalues(db_id, txn_id, &main_sql, main_sql_values)
                .await
                .map_err(|e| {
                    warn!("{:<12} - 更新主表记录失败: {}", "SERVICE", e);
                    PluginError::Database(format!("更新主表记录失败: {}", e))
                })?;

            // 查询 cmx_meta_table_define_version，根据 table_name 和 version 来判断是否有记录
            // 有记录则更新下记录，没有记录就新增一条数据
            let version_exists = Self::version_exists(
                mm,
                db_id,
                &table_name,
                &record.version,
                &target_db_id,
            )
            .await?;

            if version_exists {
                //更新
                let mut version_fields = data.clone().not_none_sea_fields();
                version_fields.push(SeaField::new("update_time", now));
                let values = version_fields.for_sea_update();
                let mut version_query = Query::update();
                version_query
                    .table(TableMetadataVersionBmc::table_ref())
                    .values(values)
                    .and_where(Expr::col("table_name").eq(&table_name))
                    .and_where(Expr::col("version").eq(&data.version.unwrap()))
                    .and_where(Expr::col("db_id").eq(&target_db_id));

                let (version_sql, version_sql_values) = version_query.build_sqlx(PostgresQueryBuilder);
                debug!("{:<12} - SQL: {}", "SERVICE", version_sql);

                mm.execute_sql_with_sqlxvalues(db_id, txn_id, &version_sql, version_sql_values)
                    .await
                    .map_err(|e| {
                        warn!("{:<12} - 更新版本表记录失败: {}", "SERVICE", e);
                        PluginError::Database(format!("更新版本表记录失败: {}", e))
                    })?;
            } else {
                //新增
                let version_id = snowflake_id_str();
                let mut version_fields =data.clone().not_none_sea_fields();
                version_fields.push(SeaField::new("id", version_id));
                version_fields.push(SeaField::new("table_name", table_name));
                version_fields.push(SeaField::new("db_id", target_db_id));
                version_fields.push(SeaField::new("plugin_id", plugin_id));



                // if let Some(ref metadata) = data.metadata {
                //     version_fields.push(SeaField::new("metadata", metadata.clone()));
                // } else {
                //     version_fields.push(SeaField::new("metadata", serde_json::Value::Null));
                // }
                version_fields.push(SeaField::new("archived", record.archived));
                version_fields.push(SeaField::new("create_time", now));
                version_fields.push(SeaField::new("update_time", now));

                let (version_columns, version_values) = version_fields.for_sea_insert();
                let mut version_insert = Query::insert();
                version_insert
                    .into_table(TableMetadataVersionBmc::table_ref())
                    .columns(version_columns)
                    .values(version_values)
                    .map_err(|e| {
                        PluginError::Database(format!("构建版本表插入语句失败: {}", e))
                    })?;

                let (version_sql, version_sql_values) = version_insert.build_sqlx(PostgresQueryBuilder);
                debug!("{:<12} - SQL: {}", "SERVICE", version_sql);

                mm.execute_sql_with_sqlxvalues(db_id, txn_id, &version_sql, version_sql_values)
                    .await
                    .map_err(|e| {
                        warn!("{:<12} - 创建版本表记录失败: {}", "SERVICE", e);
                        PluginError::Database(format!("创建版本表记录失败: {}", e))
                    })?;
            }

            Self::get_by_id(mm, db_id, id.as_str().unwrap_or_default()).await
        } else {
            Err(PluginError::NotFound(format!("表元数据不存在: {:?}", id)))
        }
    }

    /// 根据 plugin_id 更新 version 字段
    ///
    /// 更新 cmx_meta_table_define
    /// 指定 plugin_id version 字段
    pub async fn update_version_by_plugin_id(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: Option<&str>,
        plugin_id: &str,
        new_version: &str,
    ) -> PluginResult<u64> {
        info!(
            "{:<12} - TableMetadataService::update_version_by_plugin_id - plugin_id: {}, new_version: {}",
            "SERVICE", plugin_id, new_version
        );

        let now = Utc::now();

        // 更新主表 cmx_meta_table_define 的 version 字段
        let mut main_query = Query::update();
        main_query
            .table(TableMetadataBmc::table_ref())
            .value("version", new_version)
            .value("update_time", now)
            .and_where(Expr::col("plugin_id").eq(plugin_id));

        let (main_sql, main_sql_values) = main_query.build_sqlx(PostgresQueryBuilder);
        debug!("{:<12} - SQL: {}", "SERVICE", main_sql);

        mm.execute_sql_with_sqlxvalues(db_id, txn_id, &main_sql, main_sql_values)
            .await
            .map_err(|e| {
                warn!(
                    "{:<12} - 根据 plugin_id 更新主表 version 失败: {}",
                    "SERVICE", e
                );
                PluginError::Database(format!(
                    "根据 plugin_id 更新主表 version 失败: {}",
                    e
                ))
            })?;
        Ok(1)
    }

    /// 根据 plugin_id 删除表元数据
    ///
    /// 同时物理删除 cmx_meta_table_define 和 cmx_meta_table_define_version
    /// 两个表中指定 plugin_id 对应的所有记录
    pub async fn delete_by_plugin_id(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: Option<&str>,
        plugin_id: &str,
    ) -> PluginResult<u64> {
        info!(
            "{:<12} - TableMetadataService::delete_by_plugin_id - plugin_id: {}",
            "SERVICE", plugin_id
        );

        // 先删除版本表 cmx_meta_table_define_version 中对应 plugin_id 的记录
        let mut version_delete = Query::delete();
        version_delete
            .from_table("cmx_meta_table_define_version")
            .and_where(Expr::col("plugin_id").eq(plugin_id));

        let (version_sql, version_sql_values) = version_delete.build_sqlx(PostgresQueryBuilder);
        debug!("{:<12} - SQL: {}", "SERVICE", version_sql);

        mm.execute_sql_with_sqlxvalues(db_id, txn_id, &version_sql, version_sql_values)
            .await
            .map_err(|e| {
                warn!(
                    "{:<12} - 根据 plugin_id 删除版本表记录失败: {}",
                    "SERVICE", e
                );
                PluginError::Database(format!(
                    "根据 plugin_id 删除版本表记录失败: {}",
                    e
                ))
            })?;

        // 再删除主表 cmx_meta_table_define 中对应 plugin_id 的记录
        let mut main_delete = Query::delete();
        main_delete
            .from_table("cmx_meta_table_define")
            .and_where(Expr::col("plugin_id").eq(plugin_id));

        let (main_sql, main_sql_values) = main_delete.build_sqlx(PostgresQueryBuilder);
        debug!("{:<12} - SQL: {}", "SERVICE", main_sql);

        mm.execute_sql_with_sqlxvalues(db_id, txn_id, &main_sql, main_sql_values)
            .await
            .map_err(|e| {
                warn!(
                    "{:<12} - 根据 plugin_id 删除主表记录失败: {}",
                    "SERVICE", e
                );
                PluginError::Database(format!(
                    "根据 plugin_id 删除主表记录失败: {}",
                    e
                ))
            })?;

        info!(
            "{:<12} - 根据 plugin_id 删除成功, plugin_id: {}",
            "SERVICE", plugin_id
        );

        Ok(1)
    }

    /// 删除表元数据
    ///
    /// 同时删除 cmx_meta_table_define 和 cmx_meta_table_define_version 中的记录
    pub async fn delete(mm: &DatabaseManager, db_id: &str, ids: Vec<Value>) -> PluginResult<u64> {
        info!(
            "{:<12} - TableMetadataService::delete - count: {}",
            "SERVICE",
            ids.len()
        );

        if ids.is_empty() {
            return Ok(0);
        }

        let mut total_affected = 0u64;

        for id_val in &ids {
            let id_str = id_val.as_str().unwrap_or_default();
            let existing = Self::get_by_id(mm, db_id, id_str).await?;

            if let Some(record) = existing.iter().next() {
                let table_name: String = record
                    .get_by_name(existing.schema.as_ref(), "table_name")
                    .and_then(|v| match v {
                        cmx_core::model::cell::DataValue::String(s) => Some(s.clone()),
                        _ => None,
                    })
                    .unwrap_or_default();

                let target_db_id: String = record
                    .get_by_name(existing.schema.as_ref(), "db_id")
                    .and_then(|v| match v {
                        cmx_core::model::cell::DataValue::String(s) => Some(s.clone()),
                        _ => None,
                    })
                    .unwrap_or_default();

                let version: String = record
                    .get_by_name(existing.schema.as_ref(), "version")
                    .and_then(|v| match v {
                        cmx_core::model::cell::DataValue::String(s) => Some(s.clone()),
                        _ => None,
                    })
                    .unwrap_or_default();

                let mut version_delete = Query::delete();
                version_delete
                    .from_table("cmx_meta_table_define_version")
                    .and_where(Expr::col("table_name").eq(&table_name))
                    .and_where(Expr::col("db_id").eq(&target_db_id))
                    .and_where(Expr::col("version").eq(&version));

                let (version_sql, version_sql_values) =
                    version_delete.build_sqlx(PostgresQueryBuilder);
                debug!("{:<12} - SQL: {}", "SERVICE", version_sql);

                mm.execute_sql_with_sqlxvalues(db_id, None, &version_sql, version_sql_values)
                    .await
                    .map_err(|e| {
                        warn!("{:<12} - 删除版本表记录失败: {}", "SERVICE", e);
                        PluginError::Database(format!("删除版本表记录失败: {}", e))
                    })?;
            }
        }

        let result = GenericCrudService::<TableMetadataBmc>::delete(mm, db_id, None, ids.clone()).await;
        match result {
            Ok(_) => {
                total_affected = ids.len() as u64;
                info!("{:<12} - 删除成功, count: {}", "SERVICE", total_affected);
            }
            Err(e) => {
                warn!("{:<12} - 删除主表记录失败: {}", "SERVICE", e);
            }
        }

        Ok(total_affected)
    }

    /// 查询表的所有版本历史
    pub async fn list_versions(
        mm: &DatabaseManager,
        db_id: &str,
        table_name: &str,
        target_db_id: Option<&str>,
    ) -> PluginResult<DataSet> {
        debug!(
            "{:<12} - TableMetadataService::list_versions - table_name: {}",
            "SERVICE", table_name
        );

        let filter = TableMetadataVersionFilter {
            table_name: Some(modql::filter::OpValsString(vec![
                modql::filter::OpValString::Eq(table_name.to_string()),
            ])),
            db_id: target_db_id.map(|d| {
                modql::filter::OpValsString(vec![modql::filter::OpValString::Eq(d.to_string())])
            }),
            plugin_id: None,
            version: None,
        };

        GenericCrudService::<TableMetadataVersionBmc, TableMetadataVersionFilter>::list(
            mm,
            db_id,
            None,
            Some(vec![filter]),
            None,
        )
        .await
        .map_err(|e| PluginError::Database(format!("查询版本历史失败: {}", e)))
    }

    /// 查询版本表是否存在指定版本的记录
    async fn version_exists(
        mm: &DatabaseManager,
        db_id: &str,
        table_name: &str,
        version: &str,
        target_db_id: &str,
    ) -> PluginResult<bool> {
        let mut select = Query::select();
        select.from(TableMetadataVersionBmc::table_ref()).expr(Expr::col("id").count());
        select.and_where(Expr::col("table_name").eq(table_name));
        select.and_where(Expr::col("version").eq(version));
        select.and_where(Expr::col("db_id").eq(target_db_id));

        let (sql, sql_values) = select.build_sqlx(PostgresQueryBuilder);
        debug!("{:<12} - SQL: {}", "SERVICE", sql);

        let result = mm
            .query_sql_with_sqlxvalues(db_id, None, &sql, sql_values, "version_exists_check")
            .await
            .map_err(|e| PluginError::Database(format!("查询版本记录失败: {}", e)))?;

        if let Some(row) = result.iter().next() {
            if let Some(count_val) = row.get(0) {
                if let cmx_core::model::cell::DataValue::Int(count) = count_val {
                    return Ok(*count > 0);
                }
            }
        }
        Ok(false)
    }

    pub fn parse_metadata_record(dataset: &DataSet) -> PluginResult<Vec<TableMetadataDetail>> {
        let schema = dataset.schema.as_ref();
        let mut records = Vec::new();

        for row in dataset.iter() {
            let record = TableMetadataDetail {
                id: row.get_by_name_as(schema, "id").unwrap_or_default(),
                table_name: row.get_by_name_as(schema, "table_name").unwrap_or_default(),
                db_id: row.get_by_name_as(schema, "db_id").unwrap_or_default(),
                plugin_id: row.get_by_name_as(schema, "plugin_id").unwrap_or_default(),
                version: row.get_by_name_as(schema, "version").unwrap_or_default(),
                domain_code: row
                    .get_by_name_as(schema, "domain_code")
                    .unwrap_or_default(),
                application_code: row
                    .get_by_name_as(schema, "application_code")
                    .unwrap_or_default(),
                module_code: row
                    .get_by_name_as(schema, "module_code")
                    .unwrap_or_default(),
                metadata: row
                    .get_by_name_as::<serde_json::Value>(schema, "metadata")
                    .unwrap_or(serde_json::Value::Null),
                archived: row.get_by_name_as(schema, "archived").unwrap_or(0),
                create_time: row
                    .get_by_name_as(schema, "create_time")
                    .unwrap_or_else(Utc::now),
                update_time: row
                    .get_by_name_as(schema, "update_time")
                    .unwrap_or_else(Utc::now),
                create_by: row.get_by_name_as(schema, "create_by"),
                create_name: row.get_by_name_as(schema, "create_name"),
                update_by: row.get_by_name_as(schema, "update_by"),
                update_name: row.get_by_name_as(schema, "update_name"),
            };
            records.push(record);
        }

        Ok(records)
    }
}
