/// PostgreSQL CRUD 仓库实现

use futures::future::LocalBoxFuture;
use cmx_core::model::data::dataset::DataSet;
use cmx_core::model::cell::DataValue;
use crate::error::Result;
use crate::transaction::{execute_sql_by_ids, query_sql_by_ids};
use super::{
    build_where_clause, data_value_to_sql_literal, extract_count_from_dataset,
    Condition, CrudRepository, PageResult,
};

/// PostgreSQL CRUD 仓库
///
/// 实现了 [`CrudRepository`] trait，所有操作均通过 db_id 查找 PostgreSQL 连接池执行。
#[derive(Debug, Clone, Default)]
pub struct PostgresCrudRepository;

// ─────────────────────────────────────────────────────────────────────────────
// 私有 SQL 构建辅助函数
// ─────────────────────────────────────────────────────────────────────────────

fn build_select(table: &str, conditions: &[Condition]) -> String {
    let where_clause = build_where_clause(conditions);
    if where_clause.is_empty() {
        format!("SELECT * FROM {}", table)
    } else {
        format!("SELECT * FROM {} WHERE {}", table, where_clause)
    }
}

fn build_select_by_id(table: &str, id_col: &str, id: &DataValue) -> String {
    format!(
        "SELECT * FROM {} WHERE {} = {}",
        table, id_col, data_value_to_sql_literal(id)
    )
}

fn build_insert(table: &str, values: &[(String, DataValue)]) -> String {
    let cols: Vec<&str> = values.iter().map(|(c, _)| c.as_str()).collect();
    let vals: Vec<String> = values.iter().map(|(_, v)| data_value_to_sql_literal(v)).collect();
    format!(
        "INSERT INTO {} ({}) VALUES ({})",
        table,
        cols.join(", "),
        vals.join(", ")
    )
}

fn build_update_by_id(
    table: &str,
    id_col: &str,
    id: &DataValue,
    values: &[(String, DataValue)],
) -> String {
    let set_clause: Vec<String> = values
        .iter()
        .map(|(c, v)| format!("{} = {}", c, data_value_to_sql_literal(v)))
        .collect();
    format!(
        "UPDATE {} SET {} WHERE {} = {}",
        table,
        set_clause.join(", "),
        id_col,
        data_value_to_sql_literal(id)
    )
}

fn build_update_by_condition(
    table: &str,
    conditions: &[Condition],
    values: &[(String, DataValue)],
) -> String {
    let set_clause: Vec<String> = values
        .iter()
        .map(|(c, v)| format!("{} = {}", c, data_value_to_sql_literal(v)))
        .collect();
    let where_clause = build_where_clause(conditions);
    if where_clause.is_empty() {
        format!("UPDATE {} SET {}", table, set_clause.join(", "))
    } else {
        format!("UPDATE {} SET {} WHERE {}", table, set_clause.join(", "), where_clause)
    }
}

fn build_delete_by_id(table: &str, id_col: &str, id: &DataValue) -> String {
    format!(
        "DELETE FROM {} WHERE {} = {}",
        table, id_col, data_value_to_sql_literal(id)
    )
}

fn build_delete_by_condition(table: &str, conditions: &[Condition]) -> String {
    let where_clause = build_where_clause(conditions);
    if where_clause.is_empty() {
        format!("DELETE FROM {}", table)
    } else {
        format!("DELETE FROM {} WHERE {}", table, where_clause)
    }
}

fn build_count(table: &str, conditions: &[Condition]) -> String {
    let where_clause = build_where_clause(conditions);
    if where_clause.is_empty() {
        format!("SELECT COUNT(*) AS count FROM {}", table)
    } else {
        format!("SELECT COUNT(*) AS count FROM {} WHERE {}", table, where_clause)
    }
}

/// PostgreSQL 分页使用 LIMIT ... OFFSET ...
fn build_select_page(
    table: &str,
    conditions: &[Condition],
    page: u64,
    page_size: u64,
) -> String {
    let where_clause = build_where_clause(conditions);
    let offset = (page.saturating_sub(1)) * page_size;
    if where_clause.is_empty() {
        format!(
            "SELECT * FROM {} LIMIT {} OFFSET {}",
            table, page_size, offset
        )
    } else {
        format!(
            "SELECT * FROM {} WHERE {} LIMIT {} OFFSET {}",
            table, where_clause, page_size, offset
        )
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// CrudRepository 实现
// ─────────────────────────────────────────────────────────────────────────────

impl CrudRepository for PostgresCrudRepository {
    fn find_by_id<'a>(
        &'a self,
        db_id: &'a str,
        txn_id: Option<&'a str>,
        table: &'a str,
        id_col: &'a str,
        id: &'a DataValue,
        dataset_id: &'a str,
    ) -> LocalBoxFuture<'a, Result<DataSet>> {
        Box::pin(async move {
            let sql = build_select_by_id(table, id_col, id);
            query_sql_by_ids(db_id, txn_id, &sql, dataset_id).await
        })
    }

    fn find_all<'a>(
        &'a self,
        db_id: &'a str,
        txn_id: Option<&'a str>,
        table: &'a str,
        dataset_id: &'a str,
    ) -> LocalBoxFuture<'a, Result<DataSet>> {
        Box::pin(async move {
            let sql = format!("SELECT * FROM {}", table);
            query_sql_by_ids(db_id, txn_id, &sql, dataset_id).await
        })
    }

    fn find_list<'a>(
        &'a self,
        db_id: &'a str,
        txn_id: Option<&'a str>,
        table: &'a str,
        conditions: &'a [Condition],
        dataset_id: &'a str,
    ) -> LocalBoxFuture<'a, Result<DataSet>> {
        Box::pin(async move {
            let sql = build_select(table, conditions);
            query_sql_by_ids(db_id, txn_id, &sql, dataset_id).await
        })
    }

    fn find_page<'a>(
        &'a self,
        db_id: &'a str,
        txn_id: Option<&'a str>,
        table: &'a str,
        conditions: &'a [Condition],
        page: u64,
        page_size: u64,
        dataset_id: &'a str,
    ) -> LocalBoxFuture<'a, Result<PageResult>> {
        Box::pin(async move {
            let count_sql = build_count(table, conditions);
            let count_ds = query_sql_by_ids(db_id, txn_id, &count_sql, "__count__").await?;
            let total = extract_count_from_dataset(&count_ds);

            let page_sql = build_select_page(table, conditions, page, page_size);
            let data = query_sql_by_ids(db_id, txn_id, &page_sql, dataset_id).await?;

            Ok(PageResult { data, total, page, page_size })
        })
    }

    fn count<'a>(
        &'a self,
        db_id: &'a str,
        txn_id: Option<&'a str>,
        table: &'a str,
        conditions: &'a [Condition],
    ) -> LocalBoxFuture<'a, Result<u64>> {
        Box::pin(async move {
            let sql = build_count(table, conditions);
            let ds = query_sql_by_ids(db_id, txn_id, &sql, "__count__").await?;
            Ok(extract_count_from_dataset(&ds))
        })
    }

    fn insert<'a>(
        &'a self,
        db_id: &'a str,
        txn_id: Option<&'a str>,
        table: &'a str,
        values: &'a [(String, DataValue)],
    ) -> LocalBoxFuture<'a, Result<u64>> {
        Box::pin(async move {
            let sql = build_insert(table, values);
            execute_sql_by_ids(db_id, txn_id, &sql).await
        })
    }

    fn update_by_id<'a>(
        &'a self,
        db_id: &'a str,
        txn_id: Option<&'a str>,
        table: &'a str,
        id_col: &'a str,
        id: &'a DataValue,
        values: &'a [(String, DataValue)],
    ) -> LocalBoxFuture<'a, Result<u64>> {
        Box::pin(async move {
            let sql = build_update_by_id(table, id_col, id, values);
            execute_sql_by_ids(db_id, txn_id, &sql).await
        })
    }

    fn update_by_condition<'a>(
        &'a self,
        db_id: &'a str,
        txn_id: Option<&'a str>,
        table: &'a str,
        conditions: &'a [Condition],
        values: &'a [(String, DataValue)],
    ) -> LocalBoxFuture<'a, Result<u64>> {
        Box::pin(async move {
            let sql = build_update_by_condition(table, conditions, values);
            execute_sql_by_ids(db_id, txn_id, &sql).await
        })
    }

    fn delete_by_id<'a>(
        &'a self,
        db_id: &'a str,
        txn_id: Option<&'a str>,
        table: &'a str,
        id_col: &'a str,
        id: &'a DataValue,
    ) -> LocalBoxFuture<'a, Result<u64>> {
        Box::pin(async move {
            let sql = build_delete_by_id(table, id_col, id);
            execute_sql_by_ids(db_id, txn_id, &sql).await
        })
    }

    fn delete_by_condition<'a>(
        &'a self,
        db_id: &'a str,
        txn_id: Option<&'a str>,
        table: &'a str,
        conditions: &'a [Condition],
    ) -> LocalBoxFuture<'a, Result<u64>> {
        Box::pin(async move {
            let sql = build_delete_by_condition(table, conditions);
            execute_sql_by_ids(db_id, txn_id, &sql).await
        })
    }
}
