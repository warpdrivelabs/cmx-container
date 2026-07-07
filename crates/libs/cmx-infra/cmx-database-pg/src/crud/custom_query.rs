//! 自定义查询服务
//!
//! 提供自定义 SQL 查询功能，支持动态过滤、分页和排序

use modql::filter::{FilterGroups, IntoFilterNodes, ListOptions};
use sea_query::{Alias, Condition, PostgresQueryBuilder, Query};
use std::sync::Arc;
use tracing::debug;

use crate::DatabaseManager;
use crate::crud::count_optimizer::{CountOptimizerConfig, generate_count_sql};
use crate::crud::error::{Result, ServiceError};
use cmx_core::model::data::dataset::{DataSet, Schema};

/// 自定义查询服务
///
/// 提供自定义 SQL 的分页查询功能
pub struct CustomQueryService;

impl CustomQueryService {
    /// 自定义分页查询
    ///
    /// 传入自定义 SQL，使用 filters 生成 WHERE 条件，使用 list_options 生成分页参数和排序条件。
    /// 先查询 count，如果 count > 0 再继续查询数据。
    ///
    /// # 参数
    /// * `mm` - 数据库管理器
    /// * `db_id` - 数据库 ID
    /// * `txn_id` - 事务 ID（可选）
    /// * `filters` - 过滤条件列表（可选）
    /// * `list_options` - 列表选项（包含分页和排序）
    /// * `sql` - 自定义 SQL（支持复杂 JOIN 查询）
    /// * `dataset_id` - 数据集 ID（用于标识查询结果集，如 "application_custom_page"）
    ///
    /// # 返回值
    /// 返回包含查询结果的 DataSet 和总数
    ///
    /// # 示例
    /// ```rust
    /// let sql = "select a.*, b.name from a left join b on a.name = b.name";
    /// let (dataset, total) = CustomQueryService::page_custom::<SomeFilter>(
    ///     &mm, "db_id", None, Some(filters), list_options, sql, "my_custom_query"
    /// ).await?;
    /// ```
    pub async fn page_custom<F>(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: Option<&str>,
        filters: Option<Vec<F>>,
        list_options: ListOptions,
        sql: &str,
        dataset_id: &str,
    ) -> Result<(DataSet, i64)>
    where
        F: Into<FilterGroups> + Clone + IntoFilterNodes,
    {
        debug!(
            "{:<12} - CustomQueryService::page_custom - db_id: {}, dataset_id: {}",
            "CUSTOM_QUERY", db_id, dataset_id
        );

        let where_clause = Self::build_where_clause(filters.clone())?;

        let (order_by, limit_offset) = Self::build_order_and_pagination(&list_options);

        let count_sql = generate_count_sql(
            sql,
            where_clause.as_deref(),
            &CountOptimizerConfig::default(),
        );

        let total = Self::execute_count_query(mm, db_id, txn_id, &count_sql, dataset_id).await?;

        debug!("{:<12} - COUNT 查询结果: {}", "CUSTOM_QUERY", total);

        let dataset = if total > 0 {
            let data_sql = Self::build_final_sql(
                sql,
                where_clause.as_deref(),
                order_by.as_deref(),
                limit_offset.as_deref(),
            );

            Self::execute_data_query(mm, db_id, txn_id, &data_sql, dataset_id).await?
        } else {
            Self::empty_dataset(dataset_id)
        };

        let row_count = dataset.iter().count();
        debug!(
            "{:<12} - 分页查询返回 {} 行, 总数: {}",
            "CUSTOM_QUERY", row_count, total
        );

        Ok((dataset, total))
    }

    /// 构建 WHERE 条件子句
    ///
    /// # 参数
    /// * `filters` - 过滤条件列表
    ///
    /// # 返回值
    /// 返回 WHERE 子句（不包含 WHERE 关键字），参数值已内联到 SQL 中
    fn build_where_clause<F>(filters: Option<Vec<F>>) -> Result<Option<String>>
    where
        F: Into<FilterGroups> + Clone + IntoFilterNodes,
    {
        if let Some(filters) = filters {
            let mut temp_query = Query::select();
            temp_query.from(Alias::new("temp_table"));

            let filters: FilterGroups = Vec::into(filters);
            let cond: Condition = filters
                .try_into()
                .map_err(|e| ServiceError::bad_request(format!("过滤条件错误: {}", e)))?;

            temp_query.cond_where(cond);

            let sql = temp_query.to_string(PostgresQueryBuilder);

            let where_clause = sql
                .find("WHERE")
                .map(|pos| sql[pos + 5..].trim().to_string());

            Ok(where_clause)
        } else {
            Ok(None)
        }
    }

    /// 构建排序和分页子句
    ///
    /// # 参数
    /// * `list_options` - 列表选项
    ///
    /// # 返回值
    /// 返回 (order_by, limit_offset) 元组
    fn build_order_and_pagination(list_options: &ListOptions) -> (Option<String>, Option<String>) {
        let mut temp_query = Query::select();
        temp_query.from(Alias::new("temp_table"));

        list_options.clone().apply_to_sea_query(&mut temp_query);

        let sql = temp_query.to_string(PostgresQueryBuilder);

        let order_by = if let Some(pos) = sql.find("ORDER BY") {
            let order_part = if let Some(limit_pos) = sql.find("LIMIT") {
                sql[pos..limit_pos].trim().to_string()
            } else if let Some(offset_pos) = sql.find("OFFSET") {
                sql[pos..offset_pos].trim().to_string()
            } else {
                sql[pos..].trim().to_string()
            };
            Some(order_part)
        } else {
            None
        };

        let limit_offset = if sql.contains("LIMIT") || sql.contains("OFFSET") {
            let mut parts = vec![];
            if let Some(limit_pos) = sql.find("LIMIT") {
                if let Some(offset_pos) = sql.find("OFFSET") {
                    parts.push(sql[limit_pos..offset_pos].trim().to_string());
                    parts.push(sql[offset_pos..].trim().to_string());
                } else {
                    parts.push(sql[limit_pos..].trim().to_string());
                }
            } else if let Some(offset_pos) = sql.find("OFFSET") {
                parts.push(sql[offset_pos..].trim().to_string());
            }
            if !parts.is_empty() {
                Some(parts.join(" "))
            } else {
                None
            }
        } else {
            None
        };

        (order_by, limit_offset)
    }

    /// 构建最终 SQL
    ///
    /// # 参数
    /// * `base_sql` - 基础 SQL
    /// * `where_clause` - WHERE 子句
    /// * `order_by` - ORDER BY 子句
    /// * `limit_offset` - LIMIT/OFFSET 子句
    ///
    /// # 返回值
    /// 返回完整的 SQL
    fn build_final_sql(
        base_sql: &str,
        where_clause: Option<&str>,
        order_by: Option<&str>,
        limit_offset: Option<&str>,
    ) -> String {
        let mut sql = base_sql.to_string();

        if let Some(where_sql) = where_clause {
            if base_sql.to_uppercase().contains(" WHERE ") {
                sql = format!("{} AND {}", sql, where_sql);
            } else {
                sql = format!("{} WHERE {}", sql, where_sql);
            }
        }

        if let Some(ob) = order_by {
            sql = format!("{} {}", sql, ob);
        }

        if let Some(lo) = limit_offset {
            sql = format!("{} {}", sql, lo);
        }

        sql
    }

    /// 执行 COUNT 查询
    ///
    /// # 参数
    /// * `mm` - 数据库管理器
    /// * `db_id` - 数据库 ID
    /// * `txn_id` - 事务 ID
    /// * `sql` - SQL 语句
    /// * `dataset_id` - 数据集 ID
    ///
    /// # 返回值
    /// 返回记录总数
    async fn execute_count_query(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: Option<&str>,
        sql: &str,
        dataset_id: &str,
    ) -> Result<i64> {
        let count_dataset_id = format!("{}_count", dataset_id);
        let dataset = mm
            .query_sql(db_id, txn_id, sql, &count_dataset_id)
            .await
            .map_err(|e| ServiceError::internal_error(format!("COUNT 查询失败: {}", e)))?;

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

    /// 执行数据查询
    ///
    /// # 参数
    /// * `mm` - 数据库管理器
    /// * `db_id` - 数据库 ID
    /// * `txn_id` - 事务 ID
    /// * `sql` - SQL 语句
    /// * `dataset_id` - 数据集 ID
    ///
    /// # 返回值
    /// 返回查询结果 DataSet
    async fn execute_data_query(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: Option<&str>,
        sql: &str,
        dataset_id: &str,
    ) -> Result<DataSet> {
        let dataset = mm
            .query_sql(db_id, txn_id, sql, dataset_id)
            .await
            .map_err(|e| ServiceError::internal_error(format!("数据查询失败: {}", e)))?;

        Ok(dataset)
    }

    /// 创建一个空的 DataSet（用于返回操作结果）
    fn empty_dataset(dataset_id: &str) -> DataSet {
        let schema = Arc::new(Schema::new_unchecked(dataset_id, vec![]));
        DataSet::empty(dataset_id, schema)
    }
}
