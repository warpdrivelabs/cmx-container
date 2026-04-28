//! COUNT 查询优化器
//!
//! 参考 MyBatis-Plus 的分页插件策略，优化带有 LEFT JOIN 的 COUNT SQL 生成。
//!
//! 核心机制：智能 Join 优化
//! - 如果 LEFT JOIN 的表没有在 WHERE 条件中被使用，则在生成 COUNT SQL 时移除该 JOIN
//! - 如果 WHERE 条件中使用了 LEFT JOIN 的表，则必须保留该 JOIN

use sqlparser::ast::{
    Expr, Join, JoinOperator, Query, Select, SelectItem, SetExpr, Statement, TableFactor,
    BinaryOperator,
};
use sqlparser::dialect::PostgreSqlDialect;
use sqlparser::parser::Parser;
use std::collections::HashSet;
use tracing::warn;

/// COUNT 查询优化器配置
#[derive(Debug, Clone)]
pub struct CountOptimizerConfig {
    /// 是否启用 JOIN 优化（默认 true）
    pub optimize_join: bool,
    /// 是否启用整体 COUNT 优化（默认 true）
    pub optimize_count: bool,
}

impl Default for CountOptimizerConfig {
    fn default() -> Self {
        Self {
            optimize_join: true,
            optimize_count: true,
        }
    }
}

/// 生成优化的 COUNT SQL
///
/// # 参数
/// - `original_sql`: 原始 SELECT SQL
/// - `where_clause`: 额外的 WHERE 条件（可选）
/// - `config`: 优化器配置
///
/// # 返回
/// 优化后的 COUNT SQL 字符串
pub fn generate_count_sql(
    original_sql: &str,
    where_clause: Option<&str>,
    config: &CountOptimizerConfig,
) -> String {
    if !config.optimize_count {
        return wrap_count_sql(original_sql);
    }

    match try_generate_optimized_count_sql(original_sql, where_clause, config) {
        Ok(sql) => sql,
        Err(e) => {
            warn!("COUNT SQL 优化失败，使用原始方式: {}", e);
            wrap_count_sql(original_sql)
        }
    }
}

/// 尝试生成优化的 COUNT SQL
fn try_generate_optimized_count_sql(
    original_sql: &str,
    where_clause: Option<&str>,
    config: &CountOptimizerConfig,
) -> Result<String, String> {
    let dialect = PostgreSqlDialect {};

    let statements = Parser::parse_sql(&dialect, original_sql)
        .map_err(|e| format!("SQL 解析失败: {}", e))?;

    if statements.len() != 1 {
        return Err("只支持单条 SQL 语句".to_string());
    }

    let statement = statements.into_iter().next().unwrap();

    let query = match statement {
        Statement::Query(query) => query,
        _ => return Err("只支持 SELECT 查询".to_string()),
    };

    let where_aliases = extract_aliases_from_where(where_clause);

    let optimized_query = optimize_query(query, where_clause, &where_aliases, config);

    let count_sql = format!("{}", optimized_query);

    Ok(count_sql)
}

/// 使用子查询包装的 COUNT SQL（兜底方案）
fn wrap_count_sql(original_sql: &str) -> String {
    format!("SELECT COUNT(*) FROM ({}) AS count_subquery", original_sql)
}

/// 优化查询
fn optimize_query(
    mut query: Box<Query>,
    where_clause: Option<&str>,
    where_aliases: &HashSet<String>,
    config: &CountOptimizerConfig,
) -> Box<Query> {
    query.order_by = None;
    query.limit = None;
    query.offset = None;

    if let SetExpr::Select(select) = &mut *query.body {
        optimize_select(select, where_clause, where_aliases, config);
    }

    query
}

/// 优化 SELECT 子句
fn optimize_select(
    select: &mut Select,
    where_clause: Option<&str>,
    where_aliases: &HashSet<String>,
    config: &CountOptimizerConfig,
) {
    select.projection = vec![SelectItem::UnnamedExpr(Expr::Identifier(
        sqlparser::ast::Ident::new("COUNT(*)"),
    ))];

    if let Some(additional_where) = where_clause {
        append_where_condition(select, additional_where);
    }

    if config.optimize_join {
        optimize_joins(select, where_aliases);
    }
}

/// 追加额外的 WHERE 条件
fn append_where_condition(select: &mut Select, additional_where: &str) {
    let dialect = PostgreSqlDialect {};
    let expr_str = format!("SELECT 1 WHERE {}", additional_where);

    if let Ok(statements) = Parser::parse_sql(&dialect, &expr_str)
        && let Some(Statement::Query(query)) = statements.into_iter().next()
            && let SetExpr::Select(inner_select) = *query.body
                && let Some(selection) = inner_select.selection {
                    select.selection = Some(match &select.selection {
                        Some(existing) => Expr::BinaryOp {
                            left: Box::new(existing.clone()),
                            op: BinaryOperator::And,
                            right: Box::new(selection),
                        },
                        None => selection,
                    });
                }
}

/// 优化 JOIN 子句（移除未使用的 LEFT JOIN）
fn optimize_joins(select: &mut Select, where_aliases: &HashSet<String>) {
    if select.from.len() != 1 {
        return;
    }

    let from = &mut select.from[0];
    if from.joins.is_empty() {
        return;
    }

    from.joins.retain(|join| {
        if matches!(join.join_operator, JoinOperator::LeftOuter(_)) {
            let join_alias = get_table_alias_from_join(join);
            if let Some(alias) = join_alias {
                !where_aliases.contains(&alias.to_lowercase())
            } else {
                true
            }
        } else {
            true
        }
    });
}

/// 从 JOIN 中获取表别名
fn get_table_alias_from_join(join: &Join) -> Option<String> {
    match &join.relation {
        TableFactor::Table { alias, .. } => alias.as_ref().map(|a| a.to_string()),
        TableFactor::Derived { alias, .. } => alias.as_ref().map(|a| a.to_string()),
        _ => None,
    }
}

/// 从 WHERE 条件中提取表别名
fn extract_aliases_from_where(where_clause: Option<&str>) -> HashSet<String> {
    let mut aliases = HashSet::new();

    if let Some(where_str) = where_clause {
        let pattern = regex::Regex::new(r"\b([a-zA-Z_][a-zA-Z0-9_]*)\s*\.").unwrap();

        for cap in pattern.captures_iter(where_str) {
            if let Some(alias) = cap.get(1) {
                aliases.insert(alias.as_str().to_lowercase());
            }
        }
    }

    aliases
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_count_sql() {
        let sql = "SELECT u.id, u.name FROM user u WHERE u.age > 18";
        let count_sql = generate_count_sql(sql, None, &CountOptimizerConfig::default());
        assert!(count_sql.contains("COUNT(*)"));
        assert!(count_sql.contains("u.age > 18"));
    }

    #[test]
    fn test_left_join_optimization() {
        let sql = "SELECT u.id, u.name, o.order_no FROM user u LEFT JOIN orders o ON u.id = o.user_id WHERE u.age > 18";
        let count_sql = generate_count_sql(sql, None, &CountOptimizerConfig::default());
        assert!(count_sql.contains("COUNT(*)"));
        assert!(count_sql.contains("u.age > 18"));
        assert!(!count_sql.to_lowercase().contains("left join orders"));
    }

    #[test]
    fn test_left_join_preserved_when_used_in_where() {
        let sql = "SELECT u.id, u.name, o.order_no FROM user u LEFT JOIN orders o ON u.id = o.user_id WHERE o.status = 'paid'";
        let count_sql = generate_count_sql(sql, None, &CountOptimizerConfig::default());
        assert!(count_sql.contains("COUNT(*)"));
        assert!(count_sql.to_lowercase().contains("left join orders"));
    }

    #[test]
    fn test_with_additional_where() {
        let sql = "SELECT u.id, u.name FROM user u WHERE u.age > 18";
        let count_sql = generate_count_sql(sql, Some("u.status = 'active'"), &CountOptimizerConfig::default());
        assert!(count_sql.contains("COUNT(*)"));
        assert!(count_sql.contains("u.age > 18"));
        assert!(count_sql.contains("u.status = 'active'"));
    }
}
