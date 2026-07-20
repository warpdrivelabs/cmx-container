//! SEED / MENU 部署的业务编排。
//!
//! 本模块对外暴露（后续任务填充）：
//! - deploy_seed_with_events: SEED 部署主流程（Task 6/7）
//! - deploy_menu_with_events: MENU 部署主流程（Task 6/7）
//! - [`compile_all_definitions_for_module`]: 聚合编译某模块所有 DCT/DOC/RPT 定义
//! - [`infer_conflict_columns`]: 从 TableDefine 推断 UPSERT 冲突列
//!
//! 本任务（Task 4）仅落地后两项；deploy 主流程留待后续任务。

use crate::compile_definition;
use cmx_api_types::{Error, Result};
use cmx_core::model::cell::{IndexKind, TableDefine};
use cmx_model::definitions::store::list_definitions;

/// 推断 UPSERT 冲突列。
///
/// 优先级：
/// 1. 单列唯一索引（业务编码字段最常见的去重约束）
/// 2. 复合唯一索引（联合唯一，如 (client_id, coa_code)）
/// 3. 主键列
/// 4. 兜底：`["code"]`（cmxfico 数据集永远走这条 —— 文件命名约定为业务编码）
///
/// 返回的列名列表会直接作为 PostgreSQL `INSERT ... ON CONFLICT (...)` 的目标列。
pub fn infer_conflict_columns(def: &TableDefine) -> Vec<String> {
    // 1. 优先：单列唯一索引
    for idx in &def.indexes {
        if matches!(idx.kind, IndexKind::Unique) && idx.columns.len() == 1 {
            return idx.columns.clone();
        }
    }
    // 2. 次：复合唯一索引
    for idx in &def.indexes {
        if matches!(idx.kind, IndexKind::Unique) && !idx.columns.is_empty() {
            return idx.columns.clone();
        }
    }
    // 3. 主键
    if !def.primary_keys.is_empty() {
        return def.primary_keys.clone();
    }
    // 4. 兜底（cmxfico 数据集永远走这条）
    vec!["code".to_string()]
}

/// 聚合编译某模块下所有 DCT/DOC/RPT 定义 → `Vec<TableDefine>`。
///
/// 流程：
/// 1. 调 `list_definitions(None, domain, app, module)` 列出该模块所有定义文件（不限 kind）。
/// 2. 逐个调 `compile_definition` 编译成 `TableDefine` 列表。
/// 3. 顺序合并所有定义的表，返回聚合结果。
///
/// 文件项中 `kind` / `file` 缺失或为空时跳过（容错），避免脏数据中断整个模块编译。
pub async fn compile_all_definitions_for_module(
    domain: &str,
    app: &str,
    module: &str,
) -> Result<Vec<TableDefine>> {
    let mut all = Vec::new();
    // 列出该模块所有定义文件（不限 kind，DCT/DOC/RPT 全要）
    let files = list_definitions(None, Some(domain), Some(app), Some(module))
        .await
        .map_err(|e| Error::InternalError(format!("list_definitions 失败: {e}")))?;

    for f in files {
        let kind = f.get("kind").and_then(|v| v.as_str()).unwrap_or("");
        let file = f.get("file").and_then(|v| v.as_str()).unwrap_or("");
        if kind.is_empty() || file.is_empty() {
            continue;
        }
        let (defs, _src) = compile_definition(kind, domain, app, module, file).await?;
        all.extend(defs);
    }
    Ok(all)
}
