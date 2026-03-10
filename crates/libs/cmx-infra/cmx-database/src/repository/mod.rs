/// 通用 CRUD 仓库模块
///
/// 该模块定义了通用的增删改查操作 trait，并针对不同数据库类型分别实现。
/// 所有操作均通过 db_id（数据库标识）和可选的 txn_id（事务标识）进行路由，
/// 查询结果统一返回 cmx-core 的 DataSet 类型。

mod postgres;
mod mysql;
mod sqlite;

use futures::future::LocalBoxFuture;
use cmx_core::model::data::dataset::DataSet;
use cmx_core::model::cell::DataValue;
use crate::error::Result;

pub use postgres::PostgresCrudRepository;
pub use mysql::MySqlCrudRepository;
pub use sqlite::SqliteCrudRepository;

// ─────────────────────────────────────────────────────────────────────────────
// Condition 类型
// ─────────────────────────────────────────────────────────────────────────────

/// 条件操作符
#[derive(Debug, Clone)]
pub enum ConditionOp {
    /// 等于
    Eq,
    /// 不等于
    Ne,
    /// 大于
    Gt,
    /// 大于等于
    Ge,
    /// 小于
    Lt,
    /// 小于等于
    Le,
    /// 模糊匹配（LIKE）
    Like,
    /// IN 操作
    In,
    /// IS NULL
    IsNull,
    /// IS NOT NULL
    IsNotNull,
}

/// 查询条件
#[derive(Debug, Clone)]
pub struct Condition {
    /// 列名
    pub column: String,
    /// 操作符
    pub op: ConditionOp,
    /// 比较值（IsNull / IsNotNull 时为 None）
    pub value: Option<DataValue>,
    /// IN 操作时使用的值列表
    pub in_values: Vec<DataValue>,
}

impl Condition {
    /// 等于条件
    pub fn eq(column: impl Into<String>, value: DataValue) -> Self {
        Self { column: column.into(), op: ConditionOp::Eq, value: Some(value), in_values: vec![] }
    }
    /// 不等于条件
    pub fn ne(column: impl Into<String>, value: DataValue) -> Self {
        Self { column: column.into(), op: ConditionOp::Ne, value: Some(value), in_values: vec![] }
    }
    /// 大于条件
    pub fn gt(column: impl Into<String>, value: DataValue) -> Self {
        Self { column: column.into(), op: ConditionOp::Gt, value: Some(value), in_values: vec![] }
    }
    /// 大于等于条件
    pub fn ge(column: impl Into<String>, value: DataValue) -> Self {
        Self { column: column.into(), op: ConditionOp::Ge, value: Some(value), in_values: vec![] }
    }
    /// 小于条件
    pub fn lt(column: impl Into<String>, value: DataValue) -> Self {
        Self { column: column.into(), op: ConditionOp::Lt, value: Some(value), in_values: vec![] }
    }
    /// 小于等于条件
    pub fn le(column: impl Into<String>, value: DataValue) -> Self {
        Self { column: column.into(), op: ConditionOp::Le, value: Some(value), in_values: vec![] }
    }
    /// LIKE 条件
    pub fn like(column: impl Into<String>, pattern: impl Into<String>) -> Self {
        Self { column: column.into(), op: ConditionOp::Like, value: Some(DataValue::String(pattern.into())), in_values: vec![] }
    }
    /// IN 条件
    pub fn in_values(column: impl Into<String>, values: Vec<DataValue>) -> Self {
        Self { column: column.into(), op: ConditionOp::In, value: None, in_values: values }
    }
    /// IS NULL 条件
    pub fn is_null(column: impl Into<String>) -> Self {
        Self { column: column.into(), op: ConditionOp::IsNull, value: None, in_values: vec![] }
    }
    /// IS NOT NULL 条件
    pub fn is_not_null(column: impl Into<String>) -> Self {
        Self { column: column.into(), op: ConditionOp::IsNotNull, value: None, in_values: vec![] }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 分页结果
// ─────────────────────────────────────────────────────────────────────────────

/// 分页查询结果
#[derive(Debug)]
pub struct PageResult {
    /// 当前页数据集
    pub data: DataSet,
    /// 总记录数
    pub total: u64,
    /// 当前页码（从 1 开始）
    pub page: u64,
    /// 每页大小
    pub page_size: u64,
}

// ─────────────────────────────────────────────────────────────────────────────
// CrudRepository trait
// ─────────────────────────────────────────────────────────────────────────────

/// 通用 CRUD 操作 trait
///
/// 所有方法均接受 `db_id`（数据库 ID）和可选的 `txn_id`（事务 ID）作为路由参数。
/// 查询操作返回 `DataSet`，变更操作返回受影响行数。
///
/// 实现者：
/// - [`PostgresCrudRepository`] — PostgreSQL 实现
/// - [`MySqlCrudRepository`]   — MySQL 实现
/// - [`SqliteCrudRepository`]  — SQLite 实现
///
/// # 示例
/// ```rust
/// use cmx_database::repository::{SqliteCrudRepository, CrudRepository, Condition};
/// use cmx_core::model::cell::DataValue;
///
/// async fn example() -> cmx_database::Result<()> {
///     let repo = SqliteCrudRepository;
///     // 插入一行
///     repo.insert("mydb", None, "users", &[
///         ("name".to_string(), DataValue::String("Alice".to_string())),
///         ("age".to_string(),  DataValue::Int(30)),
///     ]).await?;
///     // 查询
///     let ds = repo.find_list("mydb", None, "users",
///         &[Condition::eq("age", DataValue::Int(30))], "ds1").await?;
///     Ok(())
/// }
/// ```
pub trait CrudRepository {
    // ── 查询 ────────────────────────────────────────────────────────────────

    /// 根据主键字段查询单条记录
    ///
    /// # 参数
    /// * `db_id`      - 数据库 ID
    /// * `txn_id`     - 事务 ID（None 表示非事务执行）
    /// * `table`      - 表名
    /// * `id_col`     - 主键列名
    /// * `id`         - 主键值
    /// * `dataset_id` - 结果 DataSet 的标识
    fn find_by_id<'a>(
        &'a self,
        db_id: &'a str,
        txn_id: Option<&'a str>,
        table: &'a str,
        id_col: &'a str,
        id: &'a DataValue,
        dataset_id: &'a str,
    ) -> LocalBoxFuture<'a, Result<DataSet>>;

    /// 查询表中所有记录
    fn find_all<'a>(
        &'a self,
        db_id: &'a str,
        txn_id: Option<&'a str>,
        table: &'a str,
        dataset_id: &'a str,
    ) -> LocalBoxFuture<'a, Result<DataSet>>;

    /// 按条件查询记录列表
    ///
    /// # 参数
    /// * `conditions` - 查询条件列表，多条件之间为 AND 关系
    fn find_list<'a>(
        &'a self,
        db_id: &'a str,
        txn_id: Option<&'a str>,
        table: &'a str,
        conditions: &'a [Condition],
        dataset_id: &'a str,
    ) -> LocalBoxFuture<'a, Result<DataSet>>;

    /// 分页查询
    ///
    /// # 参数
    /// * `page`      - 页码（从 1 开始）
    /// * `page_size` - 每页记录数
    fn find_page<'a>(
        &'a self,
        db_id: &'a str,
        txn_id: Option<&'a str>,
        table: &'a str,
        conditions: &'a [Condition],
        page: u64,
        page_size: u64,
        dataset_id: &'a str,
    ) -> LocalBoxFuture<'a, Result<PageResult>>;

    /// 统计记录数
    fn count<'a>(
        &'a self,
        db_id: &'a str,
        txn_id: Option<&'a str>,
        table: &'a str,
        conditions: &'a [Condition],
    ) -> LocalBoxFuture<'a, Result<u64>>;

    // ── 变更 ────────────────────────────────────────────────────────────────

    /// 插入一条记录
    ///
    /// # 参数
    /// * `values` - 字段名与值的列表，例如 `[("name", DataValue::String("Alice"))]`
    ///
    /// # 返回值
    /// * `Result<u64>` - 受影响的行数
    fn insert<'a>(
        &'a self,
        db_id: &'a str,
        txn_id: Option<&'a str>,
        table: &'a str,
        values: &'a [(String, DataValue)],
    ) -> LocalBoxFuture<'a, Result<u64>>;

    /// 根据主键字段更新一条记录
    ///
    /// # 参数
    /// * `id_col` - 主键列名
    /// * `id`     - 主键值
    /// * `values` - 要更新的字段名与值列表
    fn update_by_id<'a>(
        &'a self,
        db_id: &'a str,
        txn_id: Option<&'a str>,
        table: &'a str,
        id_col: &'a str,
        id: &'a DataValue,
        values: &'a [(String, DataValue)],
    ) -> LocalBoxFuture<'a, Result<u64>>;

    /// 按条件更新记录
    ///
    /// # 参数
    /// * `conditions` - 更新条件列表（多条件 AND）
    /// * `values`     - 要更新的字段名与值列表
    fn update_by_condition<'a>(
        &'a self,
        db_id: &'a str,
        txn_id: Option<&'a str>,
        table: &'a str,
        conditions: &'a [Condition],
        values: &'a [(String, DataValue)],
    ) -> LocalBoxFuture<'a, Result<u64>>;

    /// 根据主键字段删除一条记录
    fn delete_by_id<'a>(
        &'a self,
        db_id: &'a str,
        txn_id: Option<&'a str>,
        table: &'a str,
        id_col: &'a str,
        id: &'a DataValue,
    ) -> LocalBoxFuture<'a, Result<u64>>;

    /// 按条件删除记录
    fn delete_by_condition<'a>(
        &'a self,
        db_id: &'a str,
        txn_id: Option<&'a str>,
        table: &'a str,
        conditions: &'a [Condition],
    ) -> LocalBoxFuture<'a, Result<u64>>;
}

// ─────────────────────────────────────────────────────────────────────────────
// SQL 构建工具函数（模块内共享）
// ─────────────────────────────────────────────────────────────────────────────

/// 将 DataValue 转为 SQL 字面量字符串（用于 SQL 语句内联）
pub(crate) fn data_value_to_sql_literal(value: &DataValue) -> String {
    match value {
        DataValue::Null => "NULL".to_string(),
        DataValue::Bool(b) => if *b { "TRUE".to_string() } else { "FALSE".to_string() },
        DataValue::Int(i) => i.to_string(),
        DataValue::Float(f) => f.to_string(),
        DataValue::String(s) => format!("'{}'", s.replace('\'', "''")),
        DataValue::Decimal(d) => d.to_string(),
        DataValue::DateTime(dt) => format!("'{}'", dt.format("%Y-%m-%d %H:%M:%S")),
        DataValue::Date(d) => format!("'{}'", d.format("%Y-%m-%d")),
        DataValue::ShortStr(s) => format!("'{}'", s.replace('\'', "''")),
        DataValue::LongStr(s) => format!("'{}'", s.replace('\'', "''")),
    }
}

/// 将条件列表构建为 WHERE 子句（不含 WHERE 关键字），条件为空时返回空字符串
pub(crate) fn build_where_clause(conditions: &[Condition]) -> String {
    if conditions.is_empty() {
        return String::new();
    }
    let parts: Vec<String> = conditions.iter().map(|c| {
        let col = &c.column;
        match &c.op {
            ConditionOp::Eq => format!("{} = {}", col, data_value_to_sql_literal(c.value.as_ref().unwrap_or(&DataValue::Null))),
            ConditionOp::Ne => format!("{} != {}", col, data_value_to_sql_literal(c.value.as_ref().unwrap_or(&DataValue::Null))),
            ConditionOp::Gt => format!("{} > {}", col, data_value_to_sql_literal(c.value.as_ref().unwrap_or(&DataValue::Null))),
            ConditionOp::Ge => format!("{} >= {}", col, data_value_to_sql_literal(c.value.as_ref().unwrap_or(&DataValue::Null))),
            ConditionOp::Lt => format!("{} < {}", col, data_value_to_sql_literal(c.value.as_ref().unwrap_or(&DataValue::Null))),
            ConditionOp::Le => format!("{} <= {}", col, data_value_to_sql_literal(c.value.as_ref().unwrap_or(&DataValue::Null))),
            ConditionOp::Like => format!("{} LIKE {}", col, data_value_to_sql_literal(c.value.as_ref().unwrap_or(&DataValue::Null))),
            ConditionOp::In => {
                if c.in_values.is_empty() {
                    "1=0".to_string() // 空 IN 列表永远为 false
                } else {
                    let vals: Vec<String> = c.in_values.iter().map(data_value_to_sql_literal).collect();
                    format!("{} IN ({})", col, vals.join(", "))
                }
            },
            ConditionOp::IsNull => format!("{} IS NULL", col),
            ConditionOp::IsNotNull => format!("{} IS NOT NULL", col),
        }
    }).collect();
    parts.join(" AND ")
}

/// 从 DataSet 的 COUNT 结果中提取数值
pub(crate) fn extract_count_from_dataset(ds: &DataSet) -> u64 {
    ds.rows.first()
        .and_then(|r| r.get(0))
        .map(|v| match v {
            DataValue::Int(n) => *n as u64,
            DataValue::String(s) => s.parse().unwrap_or(0),
            _ => 0,
        })
        .unwrap_or(0)
}
