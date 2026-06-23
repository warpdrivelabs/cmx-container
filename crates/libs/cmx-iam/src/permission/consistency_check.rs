//! 代码声明权限 vs DB 存在性一致性校验
//!
//! 启动时比对 inventory 中声明的权限码 与 DB cmx_permission 表中的权限记录。
//! 缺失时按配置 panic/warn,提示开发者手动创建 DB 记录。
//! **不自动写 DB**。

use std::collections::HashSet;

use cmx_core::model::iam::registry::all_registered_permissions;
use cmx_database::DatabaseManager;
use cmx_traits::error::TraitError;
use serde_json::Value;
use tracing::{info, warn};

/// 一致性校验报告
pub struct ConsistencyReport {
    /// 代码有、DB 无(会导致权限检查失效)
    pub missing_in_db: Vec<String>,
    /// DB 有、代码无(冗余,可清理)
    pub orphan_in_db: Vec<String>,
}

/// SQL 字符串转义(单引号替换为两个单引号)
///
/// 避免 display/description 含单引号时生成的 DDL 语法错误。
fn escape_sql(s: &str) -> String {
    s.replace('\'', "''")
}

impl ConsistencyReport {
    /// 生成待执行的 INSERT SQL
    ///
    /// 将缺失的权限生成 DDL,开发者可复制执行(由人工 review 后执行)
    pub fn to_insert_sql(&self) -> String {
        let mut sql = String::new();
        for code in &self.missing_in_db {
            if let Some(p) = all_registered_permissions().iter().find(|p| p.key == code) {
                sql.push_str(&format!(
                    "INSERT INTO cmx_permission (code, name, description, status, archived) \
                     VALUES ('{}', '{}', '{}', 1, 0);\n",
                    escape_sql(p.key),
                    escape_sql(p.display),
                    escape_sql(p.description)
                ));
            }
        }
        sql
    }
}

/// 直接查询 DB 全部权限码(含已归档,绕过 archived 过滤)
///
/// 确保一致性校验比对的是全量数据
async fn list_all_permission_codes(mm: &DatabaseManager, db_id: &str) -> Result<Vec<String>, TraitError> {
    let sql = "SELECT code FROM cmx_permission";
    let dataset = mm
        .query_sql(db_id, None, sql, "permission_codes")
        .await
        .map_err(|e| TraitError::Internal(format!("查询权限码失败: {e}")))?;

    let schema = &dataset.schema;
    let codes = dataset
        .rows
        .iter()
        .filter_map(|row| {
            row.get_by_name(schema, "code")
                .and_then(|v| match v {
                    cmx_core::model::cell::DataValue::String(s) => Some(s.clone()),
                    _ => None,
                })
        })
        .collect();

    Ok(codes)
}

/// 执行一致性校验
///
/// 比对 inventory 声明的权限码 与 DB cmx_permission 表中的权限记录。
/// 返回一致性报告,不自动写 DB。
pub async fn ensure_db_permission_consistency(
    mm: &DatabaseManager,
    db_id: &str,
) -> Result<ConsistencyReport, TraitError> {
    // 1. 收集 inventory 声明的权限码集合
    let code_perms: HashSet<String> = all_registered_permissions()
        .iter()
        .map(|p| p.key.to_string())
        .collect();

    // 2. 查询 DB 已有权限码(含已归档)
    let db_perms_raw = list_all_permission_codes(mm, db_id).await?;
    let db_perms: HashSet<String> = db_perms_raw.into_iter().collect();

    // 3. 比对
    let missing_in_db: Vec<_> = code_perms.difference(&db_perms).cloned().collect();
    let orphan_in_db: Vec<_> = db_perms.difference(&code_perms).cloned().collect();

    Ok(ConsistencyReport {
        missing_in_db,
        orphan_in_db,
    })
}

/// 启动时记录 inventory 注册的权限列表到日志
pub fn log_registered_permissions() {
    let perms = all_registered_permissions();
    info!(count = perms.len(), "已注册权限定义");
    for p in perms {
        info!(
            key = p.key,
            group = p.group,
            display = p.display,
            "权限注册"
        );
    }
}

/// 启动时统计已注解 handler 数量(辅助检查)
pub fn warn_handler_annotation_status() {
    let handlers = cmx_core::model::iam::registry::all_registered_handlers();
    let public_count = handlers.iter().filter(|h| h.is_public).count();
    let protected_count = handlers.len() - public_count;
    info!(
        total = handlers.len(),
        public = public_count,
        protected = protected_count,
        "已注解路由处理器统计(辅助检查,精确保障靠宏注入 + 中间件)"
    );
}

/// 执行完整的一致性校验流程(启动时调用)
///
/// mode: "panic" | "warn" | "off"
pub async fn run_consistency_check(
    mm: &DatabaseManager,
    db_id: &str,
    mode: &str,
) -> Result<(), TraitError> {
    if mode == "off" {
        return Ok(());
    }

    let report = ensure_db_permission_consistency(mm, db_id).await?;

    if report.missing_in_db.is_empty() && report.orphan_in_db.is_empty() {
        info!("权限一致性校验通过:代码声明与 DB 完全一致");
        return Ok(());
    }

    if !report.missing_in_db.is_empty() {
        let msg = format!(
            "权限一致性校验: {} 个代码声明的权限在 DB 中缺失\n\
             缺失: {}\n\
             建议执行的 SQL:\n{}",
            report.missing_in_db.len(),
            report.missing_in_db.join(", "),
            report.to_insert_sql()
        );

        if mode == "panic" {
            return Err(TraitError::Internal(msg));
        } else {
            warn!("{}", msg);
        }
    }

    if !report.orphan_in_db.is_empty() {
        warn!(
            "DB 中有 {} 个冗余权限(代码未声明): {}",
            report.orphan_in_db.len(),
            report.orphan_in_db.join(", ")
        );
    }

    Ok(())
}
