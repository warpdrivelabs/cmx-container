//! 表差异报告：基于 DdlDiff 引擎生成部署计划报告。
//!
//! 从 lib.rs 拆出：原 diff/report 相关函数。

use cmx_core::model::cell::{ColumnDefine, FieldType, IndexDefine, IndexKind, TableDefine};
use cmx_metadata::{ColumnChange, DdlDiff, DdlDialect, IndexChange, PgTableDefineExecutor, PostgresDdlDialect, TableChange};
use serde_json::{Value, json};
use tracing::debug;

use cmx_api_types::Result;

use crate::db_err;
use crate::ledger::table_exists;

pub(crate) fn table_action_label(change: &Value) -> &'static str {
    match change.get("action").and_then(|v| v.as_str()) {
        Some("create_table") => "创建表",
        Some("upgrade_table") => "升级表",
        Some("no_change") => "校验表",
        _ => "处理表",
    }
}

/// 变更报告中的索引改名提示文案：`addedIndexes` 里带 `renamedFrom` 的条目
/// （内容一致仅名字不同的系统命名索引，将 DROP 旧名 + CREATE 新名）。
/// 供部署计划与执行两条 SSE 流以 progress 行直读，明细卡片中另有标记。
pub(crate) fn rename_hints(change: &Value) -> Vec<String> {
    change
        .get("addedIndexes")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|i| {
                    let from = i.get("renamedFrom")?.as_str()?;
                    let to = i.get("name").and_then(|v| v.as_str()).unwrap_or("");
                    Some(format!(
                        "⚠ 索引改名重建: {from} → {to}（内容未变仅变名，仍执行 DROP+CREATE）"
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// 列类型 → 前端展示的 PG 类型字符串，与建表 DDL 基准（`PostgresDdlDialect::map_column_type`）一致。
///
/// 统一从这里取类型字符串，保证「计划报告」与「实际建表/升级」使用同一套类型映射，
/// 避免出现 `integer` vs `bigint`、`timestamp without time zone` vs `timestamp with time zone`
/// 这类永久假阳性。
pub(crate) fn pg_display_type(c: &ColumnDefine) -> String {
    PostgresDdlDialect::default().map_column_type(c)
}

/// 单个索引定义 → 前端展示 JSON（含 name/columns/unique）。
fn index_to_json(idx: &IndexDefine) -> Value {
    json!({
        "name": idx.name,
        "columns": idx.columns,
        "unique": matches!(idx.kind, IndexKind::Unique),
    })
}

/// 将单列 `old → new` 的实质差异映射为前端消费的 `{field, from, to}` 变更明细。
///
/// 依赖 [`DdlDiff::column_changed`] 的判定逻辑（精度/标度仅对 Decimal 比较），保证这里产出的
/// `changes` 与执行路径（`DdlDiff::diff_to_ddl`）完全一致：报告说要改的列，执行时确实会改；
/// 报告说不改的列，执行时确实不改。
fn column_change_detail(old: &ColumnDefine, new: &ColumnDefine) -> Vec<Value> {
    let mut diffs = Vec::new();
    let mut push = |field: &str, from: String, to: String| {
        if from != to {
            diffs.push(json!({ "field": field, "from": from, "to": to }));
        }
    };
    // dataType 比较基于渲染后的类型字符串（与建表/执行基准 map_column_type 一致），
    // 而非 field_type 枚举：同为 String 时，TEXT 与 VARCHAR(255) 是不同 PG 类型，必须报告。
    {
        let old_t = pg_display_type(old);
        let new_t = pg_display_type(new);
        if old_t != new_t {
            push("dataType", old_t, new_t);
        }
    }
    push(
        "length",
        old.length.map(|n| n.to_string()).unwrap_or_default(),
        new.length.map(|n| n.to_string()).unwrap_or_default(),
    );
    // 精度/标度仅 Decimal 类型展示（与 DdlDiff::column_changed 一致，非 Decimal 不比较）
    if matches!(new.field_type, FieldType::Decimal) {
        push(
            "precision",
            old.precision.map(|n| n.to_string()).unwrap_or_default(),
            new.precision.map(|n| n.to_string()).unwrap_or_default(),
        );
        push(
            "scale",
            old.scale.map(|n| n.to_string()).unwrap_or_default(),
            new.scale.map(|n| n.to_string()).unwrap_or_default(),
        );
    }
    push(
        "nullable",
        old.is_nullable.to_string(),
        new.is_nullable.to_string(),
    );
    push(
        "default",
        old.default_value.clone().unwrap_or_default(),
        new.default_value.clone().unwrap_or_default(),
    );
    diffs
}

/// 把 DdlDiff 产出的「新表/建表」变更映射为前端 JSON（`addedColumns` 为全列）。
pub(crate) fn report_create_table(def: &TableDefine) -> Value {
    json!({
        "table": def.table_name,
        "displayName": def.display_name,
        "action": "create_table",
        "created": true,
        "addedColumns": def.columns.iter().map(|c| json!({
            "name": c.name,
            "label": c.label,
            "dataType": pg_display_type(c),
            "nullable": c.is_nullable,
        })).collect::<Vec<Value>>(),
        "modifiedColumns": [],
        "unchangedColumns": [],
        "columnCount": def.columns.len(),
        "addedIndexes": def.indexes.iter().map(index_to_json).collect::<Vec<_>>(),
        "droppedIndexes": [],
        "preservedIndexes": [],
        "commentChange": null,
        "modifiedColumnComments": [],
    })
}

/// 纯函数：基于「数据库还原定义 current」与「设计期定义 desired」生成单表的部署计划报告。
///
/// 复用执行路径已验证可靠的 [`DdlDiff::diff`] 比较引擎，消除「报告」与「执行」两套类型基准
/// 不一致导致的假阳性。返回 JSON 结构与旧实现保持一致，前端零改动。
///
/// - 无变更 → `action: "no_change"`，`unchangedColumns` 为设计期全部列名。
/// - 有新增/修改列 → `action: "upgrade_table"`。
/// - 删列（设计期不再有某列）遵循 additive-only 约束，**不报告、不执行**，仅忽略。
///
/// 无变更报告（`[]` 和兜底 `_` 分支共用）。
fn no_change_report(desired: &TableDefine) -> Value {
    json!({
        "table": desired.table_name,
        "displayName": desired.display_name,
        "action": "no_change",
        "created": false,
        "addedColumns": [],
        "modifiedColumns": [],
        "unchangedColumns": desired.columns.iter().map(|c| c.name.clone()).collect::<Vec<_>>(),
        "columnCount": desired.columns.len(),
        "addedIndexes": [],
        "droppedIndexes": [],
        "preservedIndexes": [],
        "commentChange": null,
        "modifiedColumnComments": [],
    })
}

pub(crate) fn diff_table_to_report(current: &TableDefine, desired: &TableDefine) -> Value {
    let changes = DdlDiff::diff(std::slice::from_ref(current), std::slice::from_ref(desired));
    match changes.as_slice() {
        // 表无实质变更（DdlDiff 未产出任何 TableChange）
        [] => no_change_report(desired),
        // 仅可能是 AlterTable（CreateTable/DropTable 不会出现，因为表已存在且两边表名相同）
        [
            TableChange::AlterTable {
                column_changes,
                index_changes,
                comment_change,
                column_comment_changes,
                ..
            },
            ..,
        ] => {
            let mut added = Vec::new();
            let mut modified = Vec::new();
            for cc in column_changes {
                match cc {
                    ColumnChange::AddColumn(c) => added.push(json!({
                        "name": c.name,
                        "label": c.label,
                        "dataType": pg_display_type(c),
                        "nullable": c.is_nullable,
                    })),
                    ColumnChange::AlterColumn { old, new } => {
                        let detail = column_change_detail(old, new);
                        if !detail.is_empty() {
                            modified.push(json!({
                                "name": new.name,
                                "label": new.label,
                                "changes": detail,
                            }));
                        }
                    }
                    // DropColumn：additive-only，忽略（不报删列）
                    ColumnChange::DropColumn(_) => {}
                }
            }
            // 索引变更：AddIndex 用设计期名，DropIndex 用 DB 真实名（均为 IndexDefine）。
            // RenameIndex：内容一致仅改名的系统命名索引（DROP 旧 + CREATE 新），双侧分别
            // 带 renamedFrom / renamedTo 提示，前端计划展示「改名自 xxx」。
            // PreservedManualIndex：手工创建的索引（非系统命名且不在定义中），保留不删，
            // 单独成组提示用户「不会被删除，如不需要请手工清理」。
            let mut added_idx = Vec::new();
            let mut dropped_idx = Vec::new();
            let mut preserved_idx = Vec::new();
            for ic in index_changes {
                match ic {
                    IndexChange::AddIndex(i) => added_idx.push(index_to_json(i)),
                    IndexChange::DropIndex(i) => dropped_idx.push(index_to_json(i)),
                    IndexChange::RenameIndex { old, new } => {
                        let mut a = index_to_json(new);
                        if let Some(obj) = a.as_object_mut() {
                            obj.insert("renamedFrom".to_string(), json!(old.name));
                        }
                        added_idx.push(a);
                        let mut d = index_to_json(old);
                        if let Some(obj) = d.as_object_mut() {
                            obj.insert("renamedTo".to_string(), json!(new.name));
                        }
                        dropped_idx.push(d);
                    }
                    IndexChange::PreservedManualIndex(i) => {
                        let mut v = index_to_json(i);
                        if let Some(obj) = v.as_object_mut() {
                            obj.insert(
                                "message".to_string(),
                                json!("用户手工创建的索引，部署不会删除；如不再需要请手工 DROP"),
                            );
                        }
                        preserved_idx.push(v);
                    }
                }
            }
            // 表注释变更：DdlDiff 的 comment_change 只存新值，from 需从 current 取。
            let comment_change_json =
                if comment_change.is_some() || current.comment != desired.comment {
                    Some(json!({
                        "from": current.comment.clone().unwrap_or_default(),
                        "to": desired.comment.clone().unwrap_or_default(),
                    }))
                } else {
                    None
                };
            // 列注释变更：label 不一致的列（old 来自 DB col_description，new 来自设计期 caption）。
            let modified_col_comments: Vec<Value> = column_comment_changes
                .iter()
                .map(|cc| {
                    json!({
                        "name": cc.column,
                        "from": cc.old_label,
                        "to": cc.new_label,
                    })
                })
                .collect();
            // 已变更列名集合（新增 + 修改）
            let changed_names: std::collections::HashSet<&str> = added
                .iter()
                .filter_map(|v| v.get("name").and_then(|n| n.as_str()))
                .chain(
                    modified
                        .iter()
                        .filter_map(|v| v.get("name").and_then(|n| n.as_str())),
                )
                .collect();
            let unchanged: Vec<String> = desired
                .columns
                .iter()
                .map(|c| c.name.clone())
                .filter(|n| !changed_names.contains(n.as_str()))
                .collect();
            json!({
                "table": desired.table_name,
                "displayName": desired.display_name,
                "action": "upgrade_table",
                "created": false,
                "addedColumns": added,
                "modifiedColumns": modified,
                "unchangedColumns": unchanged,
                "columnCount": desired.columns.len(),
                "addedIndexes": added_idx,
                "droppedIndexes": dropped_idx,
                "preservedIndexes": preserved_idx,
                "commentChange": comment_change_json,
                "modifiedColumnComments": modified_col_comments,
            })
        }
        // 兜底：理论上不可达（表已存在，DdlDiff 只可能产出 AlterTable 或空）
        _ => no_change_report(desired),
    }
}

/// 生成单张表的部署计划报告（含建表/升级/无变化判定）。
///
/// 与执行路径共用同一套 DdlDiff 比较引擎：表不存在 → 建表报告；表已存在 → 通过
/// [`PgTableDefineExecutor::query_current_table_define`] 内省还原当前结构，再调用
/// [`diff_table_to_report`] 生成与执行结果一致的变更明细。
pub(crate) async fn table_change_plan(db_id: &str, def: &TableDefine) -> Result<Value> {
    if !table_exists(db_id, &def.table_name).await? {
        return Ok(report_create_table(def));
    }
    let executor = PgTableDefineExecutor::new(db_id.to_string(), None);
    let current = executor
        .query_current_table_define(def)
        .await
        .map_err(db_err("内省当前表结构失败"))?;
    let report = diff_table_to_report(&current, def);
    // 判定为升级表时，打印 DB vs 设计期的逐列差异（结构化日志，便于排查假阳性）
    if report.get("action").and_then(|v| v.as_str()) == Some("upgrade_table") {
        let cur_map: std::collections::HashMap<&str, &ColumnDefine> = current
            .columns
            .iter()
            .map(|c| (c.name.as_str(), c))
            .collect();
        for d in &def.columns {
            if let Some(c) = cur_map.get(d.name.as_str()) {
                let old_t = pg_display_type(c);
                let new_t = pg_display_type(d);
                let diff = (old_t != new_t)
                    .then(|| format!("dataType({old_t}→{new_t})"))
                    .or_else(|| {
                        (c.is_nullable != d.is_nullable)
                            .then(|| format!("nullable({}→{})", c.is_nullable, d.is_nullable))
                    })
                    .or_else(|| {
                        (c.label != d.label).then(|| format!("label({:?}→{:?})", c.label, d.label))
                    })
                    .or_else(|| {
                        (c.default_value != d.default_value).then(|| {
                            format!("default({:?}→{:?})", c.default_value, d.default_value)
                        })
                    });
                if let Some(what) = diff {
                    debug!(
                        table = %def.table_name,
                        column = %d.name,
                        diff = %what,
                        "列差异"
                    );
                }
            }
        }
    }
    Ok(report)
}
