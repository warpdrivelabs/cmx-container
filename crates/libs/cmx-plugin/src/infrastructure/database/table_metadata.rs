//! 表元数据存储模块
//!
//! 提供插件表元数据的增删改查操作，包括：
//! - cmx_meta_table_define_version: 表元数据版本表
//! - cmx_meta_table_define: 表元数据主表
//!
//! 使用 sea_query 构建 SQL，使用 cmx-database 执行查询

use chrono::{DateTime, Utc};
use sea_query::{Expr, PostgresQueryBuilder, Query};
use sea_query_binder::SqlxBinder;
use std::sync::Arc;

use cmx_core::model::data::dataset::DataSet;
use cmx_database::DatabaseManager;

use crate::error::{PluginError, PluginResult};

/// cmx_meta_table_define_version 记录
#[derive(Debug, Clone)]
pub struct TableMetadataVersionRecord {
    pub id: String,
    pub table_name: String,
    pub db_id: String,
    pub plugin_id: String,
    pub version: String,
    pub metadata: serde_json::Value,
    pub archived: i32,
    pub create_time: DateTime<Utc>,
    pub update_time: DateTime<Utc>,
    pub create_by: Option<String>,
    pub create_name: Option<String>,
    pub update_by: Option<String>,
    pub update_name: Option<String>,
}

/// cmx_meta_table_define + metadata 联查结果
#[derive(Debug, Clone)]
pub struct TableMetadataRecord {
    pub id: String,
    pub table_name: String,
    pub db_id: String,
    pub plugin_id: String,
    pub version: String,
    pub metadata: serde_json::Value,
    pub archived: i32,
    pub create_time: DateTime<Utc>,
    pub update_time: DateTime<Utc>,
    pub create_by: Option<String>,
    pub create_name: Option<String>,
    pub update_by: Option<String>,
    pub update_name: Option<String>,
}

/// 查询条件结构
#[derive(Debug, Clone, Default)]
pub struct TableMetadataQuery {
    pub table_name: String,
    pub db_id: Option<String>,
    pub plugin_id: Option<String>,
}

/// 表元数据仓库
pub struct TableMetadataRepository {
    db_manager: Arc<DatabaseManager>,
    default_db_id: String,
}

impl TableMetadataRepository {
    /// 创建新的表元数据仓库
    pub fn new(db_manager: Arc<DatabaseManager>, default_db_id: String) -> Self {
        Self {
            db_manager,
            default_db_id,
        }
    }

    /// 获取默认数据库ID
    pub fn default_db_id(&self) -> &str {
        &self.default_db_id
    }

    /// 插入或更新版本元数据（存在则更新metadata，不存在则插入）
    pub async fn insert_or_update_version(
        &self,
        record: &TableMetadataVersionRecord,
        txn_id: Option<&str>,
    ) -> PluginResult<()> {
        let existing = self
            .find_version(
                &record.table_name,
                &record.version,
                Some(&record.db_id),
                Some(&record.plugin_id),
            )
            .await?;

        if existing.is_some() {
            let mut query = Query::update();
            query
                .table("cmx_meta_table_define_version")
                .values(vec![
                    ("metadata", record.metadata.clone().into()),
                    ("update_time", record.update_time.into()),
                    ("update_by", record.update_by.clone().into()),
                    ("update_name", record.update_name.clone().into()),
                ])
                .and_where(Expr::col("table_name").eq(&record.table_name))
                .and_where(Expr::col("db_id").eq(&record.db_id))
                .and_where(Expr::col("version").eq(&record.version));

            let (sql, sql_values) = query.build_sqlx(PostgresQueryBuilder);

            self.db_manager
                .execute_sql_with_sqlxvalues(&self.default_db_id, txn_id, &sql, sql_values)
                .await
                .map_err(|e| PluginError::Database(format!("更新版本元数据失败: {}", e)))?;
        } else {
            let mut query = Query::insert();
            query
                .into_table("cmx_meta_table_define_version")
                .columns(vec![
                    "id",
                    "table_name",
                    "db_id",
                    "plugin_id",
                    "version",
                    "metadata",
                    "archived",
                    "create_time",
                    "update_time",
                    "create_by",
                    "create_name",
                ])
                .values(vec![
                    record.id.clone().into(),
                    record.table_name.clone().into(),
                    record.db_id.clone().into(),
                    record.plugin_id.clone().into(),
                    record.version.clone().into(),
                    record.metadata.clone().into(),
                    record.archived.into(),
                    record.create_time.into(),
                    record.update_time.into(),
                    record.create_by.clone().into(),
                    record.create_name.clone().into(),
                ])
                .map_err(|e| PluginError::Database(format!("构建插入语句失败: {}", e)))?;

            let (sql, sql_values) = query.build_sqlx(PostgresQueryBuilder);

            self.db_manager
                .execute_sql_with_sqlxvalues(&self.default_db_id, txn_id, &sql, sql_values)
                .await
                .map_err(|e| PluginError::Database(format!("插入版本元数据失败: {}", e)))?;
        }

        Ok(())
    }

    /// 查询版本元数据
    pub async fn find_version(
        &self,
        table_name: &str,
        version: &str,
        db_id: Option<&str>,
        plugin_id: Option<&str>,
    ) -> PluginResult<Option<TableMetadataVersionRecord>> {
        let mut query = Query::select();
        query.from("cmx_meta_table_define_version")
            .columns(vec![
                "id",
                "table_name",
                "db_id",
                "plugin_id",
                "version",
                "metadata",
                "archived",
                "create_time",
                "update_time",
                "create_by",
                "create_name",
                "update_by",
                "update_name",
            ])
            .and_where(Expr::col("table_name").eq(table_name))
            .and_where(Expr::col("version").eq(version));

        if let Some(db_id) = db_id {
            query.and_where(Expr::col("db_id").eq(db_id));
        }
        if let Some(plugin_id) = plugin_id {
            query.and_where(Expr::col("plugin_id").eq(plugin_id));
        }

        let (sql, sql_values) = query.build_sqlx(PostgresQueryBuilder);

        let result = self.db_manager
            .query_sql_with_sqlxvalues(
                &self.default_db_id,
                None,
                &sql,
                sql_values,
                "table_metadata_version_query",
            )
            .await
            .map_err(|e| PluginError::Database(format!("查询版本元数据失败: {}", e)))?;

        let records = Self::parse_version_record(&result)?;
        Ok(records.into_iter().next())
    }

    /// 查询表的所有版本元数据
    pub async fn find_versions_by_table(
        &self,
        table_name: &str,
        db_id: Option<&str>,
        plugin_id: Option<&str>,
    ) -> PluginResult<Vec<TableMetadataVersionRecord>> {
        let mut query = Query::select();
        query.from("cmx_meta_table_define_version")
            .columns(vec![
                "id",
                "table_name",
                "db_id",
                "plugin_id",
                "version",
                "metadata",
                "archived",
                "create_time",
                "update_time",
                "create_by",
                "create_name",
                "update_by",
                "update_name",
            ])
            .and_where(Expr::col("table_name").eq(table_name));

        if let Some(db_id) = db_id {
            query.and_where(Expr::col("db_id").eq(db_id));
        }
        if let Some(plugin_id) = plugin_id {
            query.and_where(Expr::col("plugin_id").eq(plugin_id));
        }

        let (sql, sql_values) = query.build_sqlx(PostgresQueryBuilder);

        let result = self.db_manager
            .query_sql_with_sqlxvalues(
                &self.default_db_id,
                None,
                &sql,
                sql_values,
                "table_metadata_versions_query",
            )
            .await
            .map_err(|e| {
                PluginError::Database(format!("查询版本元数据列表失败: {}", e))
            })?;

        Self::parse_version_record(&result)
    }

    /// 删除版本元数据
    pub async fn delete_version(
        &self,
        table_name: &str,
        version: &str,
        db_id: Option<&str>,
        plugin_id: Option<&str>,
        txn_id: Option<&str>,
    ) -> PluginResult<()> {
        let mut query = Query::delete();
        query.from_table("cmx_meta_table_define_version")
            .and_where(Expr::col("table_name").eq(table_name))
            .and_where(Expr::col("version").eq(version));

        if let Some(db_id) = db_id {
            query.and_where(Expr::col("db_id").eq(db_id));
        }
        if let Some(plugin_id) = plugin_id {
            query.and_where(Expr::col("plugin_id").eq(plugin_id));
        }

        let (sql, sql_values) = query.build_sqlx(PostgresQueryBuilder);

        self.db_manager
            .execute_sql_with_sqlxvalues(&self.default_db_id, txn_id, &sql, sql_values)
            .await
            .map_err(|e| PluginError::Database(format!("删除版本元数据失败: {}", e)))?;

        Ok(())
    }

    /// 插入或更新表元数据（先查后写）
    pub async fn upsert_metadata(
        &self,
        record: &TableMetadataRecord,
        txn_id: Option<&str>,
    ) -> PluginResult<()> {
        let query = TableMetadataQuery {
            table_name: record.table_name.clone(),
            db_id: Some(record.db_id.clone()),
            plugin_id: None,
        };
        let existing = self.find_metadata(&query).await?;

        if existing.is_some() {
            let mut update_query = Query::update();
            update_query
                .table("cmx_meta_table_define")
                .values(vec![
                    ("plugin_id", record.plugin_id.clone().into()),
                    ("version", record.version.clone().into()),
                    ("update_time", record.update_time.into()),
                    ("update_by", record.update_by.clone().into()),
                    ("update_name", record.update_name.clone().into()),
                ])
                .and_where(Expr::col("table_name").eq(&record.table_name))
                .and_where(Expr::col("db_id").eq(&record.db_id));

            let (sql, sql_values) = update_query.build_sqlx(PostgresQueryBuilder);

            self.db_manager
                .execute_sql_with_sqlxvalues(&self.default_db_id, txn_id, &sql, sql_values)
                .await
                .map_err(|e| PluginError::Database(format!("更新表元数据失败: {}", e)))?;
        } else {
            let mut insert_query = Query::insert();
            insert_query
                .into_table("cmx_meta_table_define")
                .columns(vec![
                    "id",
                    "table_name",
                    "db_id",
                    "plugin_id",
                    "version",
                    "archived",
                    "create_time",
                    "update_time",
                    "create_by",
                    "create_name",
                ])
                .values(vec![
                    record.id.clone().into(),
                    record.table_name.clone().into(),
                    record.db_id.clone().into(),
                    record.plugin_id.clone().into(),
                    record.version.clone().into(),
                    record.archived.into(),
                    record.create_time.into(),
                    record.update_time.into(),
                    record.create_by.clone().into(),
                    record.create_name.clone().into(),
                ])
                .map_err(|e| PluginError::Database(format!("构建插入语句失败: {}", e)))?;

            let (sql, sql_values) = insert_query.build_sqlx(PostgresQueryBuilder);

            self.db_manager
                .execute_sql_with_sqlxvalues(&self.default_db_id, txn_id, &sql, sql_values)
                .await
                .map_err(|e| PluginError::Database(format!("插入表元数据失败: {}", e)))?;
        }

        Ok(())
    }

    /// 查询表元数据（联查 version 表获取 metadata）
    pub async fn find_metadata(
        &self,
        query: &TableMetadataQuery,
    ) -> PluginResult<Option<TableMetadataRecord>> {
        let mut select = Query::select();
        select
            .from("cmx_meta_table_define")
            .columns(vec![
               ( "cmx_meta_table_define","id"),
               ( "cmx_meta_table_define","table_name"),
               ( "cmx_meta_table_define","db_id"),
               ( "cmx_meta_table_define","plugin_id"),
               ( "cmx_meta_table_define","version"),
               ( "cmx_meta_table_define","archived"),
               ( "cmx_meta_table_define","create_time"),
               ( "cmx_meta_table_define","update_time"),
               ( "cmx_meta_table_define","create_by"),
               ( "cmx_meta_table_define","create_name"),
               ( "cmx_meta_table_define","update_by"),
               ( "cmx_meta_table_define","update_name"),
            ])
            .expr_as(
                sea_query::Expr::col(("cmx_meta_table_define_version", "metadata")),
                "metadata",
            )
            .join(
                sea_query::JoinType::LeftJoin,
                "cmx_meta_table_define_version",
                sea_query::Condition::all()
                    .add(sea_query::Expr::col(("cmx_meta_table_define","table_name")).equals(("cmx_meta_table_define_version", "table_name")))
                    .add(sea_query::Expr::col(("cmx_meta_table_define","version")).equals(("cmx_meta_table_define_version", "version")))
                    .add(sea_query::Expr::col(("cmx_meta_table_define","db_id")).equals(("cmx_meta_table_define_version", "db_id"))),
            )
            .and_where(sea_query::Expr::col(("cmx_meta_table_define","table_name")).eq(&query.table_name));

        if let Some(ref db_id) = query.db_id {
            select.and_where(sea_query::Expr::col(("cmx_meta_table_define","db_id")).eq(db_id));
        }
        if let Some(ref plugin_id) = query.plugin_id {
            select.and_where(sea_query::Expr::col(("cmx_meta_table_define","plugin_id")).eq(plugin_id));
        }

        let (sql, sql_values) = select.build_sqlx(PostgresQueryBuilder);

        let result = self.db_manager
            .query_sql_with_sqlxvalues(
                &self.default_db_id,
                None,
                &sql,
                sql_values,
                "table_metadata_query",
            )
            .await
            .map_err(|e| PluginError::Database(format!("查询表元数据失败: {}", e)))?;

        let records = Self::parse_metadata_record(&result)?;
        Ok(records.into_iter().next())
    }

    /// 查询数据库下所有表的最新元数据
    pub async fn find_all_metadata(
        &self,
        db_id: Option<&str>,
    ) -> PluginResult<Vec<TableMetadataRecord>> {
        let mut select = Query::select();
        select
            .from("cmx_meta_table_define")
            .columns(vec![
                "id",
                "table_name",
                "db_id",
                "plugin_id",
                "version",
                "archived",
                "create_time",
                "update_time",
                "create_by",
                "create_name",
                "update_by",
                "update_name",
            ])
            .expr_as(
                sea_query::Expr::col(("cmx_meta_table_define_version", "metadata")),
                "metadata",
            )
            .join(
                sea_query::JoinType::LeftJoin,
                "cmx_meta_table_define_version",
                sea_query::Condition::all()
                    .add(sea_query::Expr::col(("cmx_meta_table_define","table_name")).equals(("cmx_meta_table_define_version", "table_name")))
                    .add(sea_query::Expr::col(("cmx_meta_table_define","version")).equals(("cmx_meta_table_define_version", "version")))
                    .add(sea_query::Expr::col(("cmx_meta_table_define","db_id")).equals(("cmx_meta_table_define_version", "db_id"))),
            );

        if let Some(db_id) = db_id {
            select.and_where(sea_query::Expr::col("cmx_meta_table_define.db_id").eq(db_id));
        }

        let (sql, sql_values) = select.build_sqlx(PostgresQueryBuilder);

        let result = self.db_manager
            .query_sql_with_sqlxvalues(
                &self.default_db_id,
                None,
                &sql,
                sql_values,
                "table_metadata_all_query",
            )
            .await
            .map_err(|e| PluginError::Database(format!("查询所有表元数据失败: {}", e)))?;

        Self::parse_metadata_record(&result)
    }

    /// 删除表元数据
    pub async fn delete_metadata(
        &self,
        query: &TableMetadataQuery,
        txn_id: Option<&str>,
    ) -> PluginResult<()> {
        let mut conditions = vec![Expr::col("table_name").eq(&query.table_name)];

        if let Some(ref db_id) = query.db_id {
            conditions.push(Expr::col("db_id").eq(db_id));
        }
        if let Some(ref plugin_id) = query.plugin_id {
            conditions.push(Expr::col("plugin_id").eq(plugin_id));
        }

        let mut delete_query = Query::delete();
        delete_query.from_table("cmx_meta_table_define");
        for cond in conditions {
            delete_query.and_where(cond);
        }

        let (sql, sql_values) = delete_query.build_sqlx(PostgresQueryBuilder);

        self.db_manager
            .execute_sql_with_sqlxvalues(&self.default_db_id, txn_id, &sql, sql_values)
            .await
            .map_err(|e| PluginError::Database(format!("删除表元数据失败: {}", e)))?;

        Ok(())
    }

    fn parse_version_record(dataset: &DataSet) -> PluginResult<Vec<TableMetadataVersionRecord>> {
        let schema = dataset.schema.as_ref();
        let mut records = Vec::new();

        for row in dataset.iter() {
            let record = TableMetadataVersionRecord {
                id: row.get_by_name_as(schema, "id").unwrap_or_default(),
                table_name: row.get_by_name_as(schema, "table_name").unwrap_or_default(),
                db_id: row.get_by_name_as(schema, "db_id").unwrap_or_default(),
                plugin_id: row.get_by_name_as(schema, "plugin_id").unwrap_or_default(),
                version: row.get_by_name_as(schema, "version").unwrap_or_default(),
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

    fn parse_metadata_record(dataset: &DataSet) -> PluginResult<Vec<TableMetadataRecord>> {
        let schema = dataset.schema.as_ref();
        let mut records = Vec::new();

        for row in dataset.iter() {
            let record = TableMetadataRecord {
                id: row.get_by_name_as(schema, "id").unwrap_or_default(),
                table_name: row.get_by_name_as(schema, "table_name").unwrap_or_default(),
                db_id: row.get_by_name_as(schema, "db_id").unwrap_or_default(),
                plugin_id: row.get_by_name_as(schema, "plugin_id").unwrap_or_default(),
                version: row.get_by_name_as(schema, "version").unwrap_or_default(),
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

impl Default for TableMetadataRepository {
    fn default() -> Self {
        Self::new(
            Arc::new(DatabaseManager::new(Default::default())),
            "default".to_string(),
        )
    }
}
