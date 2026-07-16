//! 模型中心 · 数据库初始化与模块部署（真实落库）。
//!
//! 职责：
//! - `db_state`：读目标库台账（cmx_model_meta / cmx_model_module），组合出每模块每 kind 的 scenario。
//! - `init_db`：在目标库建 5 张台账系统表 + 写 cmx_model_meta + 历史（真实建表）。
//! - `deploy`：把选中的 DCT/DOC 定义编译成 TableDefine，用 PgTableDefineExecutor 建到目标库，
//!   写对象台账 + cmx_model_module + cmx_model_source（源 JSON 留档）+ 历史。
//!
//! 关键约束（见 docs/模型中心-…设计.md）：
//! - 建表现状对比只用数据库内省（PgTableDefineExecutor 内部走 information_schema，不读台账）。
//! - DDL 用 txn_id=None（PG DDL 自动提交）；台账 DML 在事务内；失败经 deploy_history 状态可对账。
//! - additive-only：create_or_upgrade_table 只加列/加索引，不 DROP。

use cmx_core::model::cell::{
    ColumnDefine, DataValue, FieldType, IndexDefine, IndexKind, TableDefine,
};
use cmx_database::get_default_db_manager;
use cmx_metadata::{
    ColumnChange, DdlDiff, DdlDialect, PgTableDefineExecutor, PostgresDdlDialect, TableChange,
    TableDefineDbExecutor,
};
use cmx_utils::snowflake_id_str;
use serde_json::{Value, json};
use std::cmp::Reverse;

use crate::Result;
use cmx_api_types::Error;

const META_VERSION: i32 = 1;
const ENGINE_VERSION: &str = "1.0.0";
/// 台账系统表清单（初始化时建入目标库）。
const LEDGER_TABLES: &[&str] = &[
    "cmx_model_meta",
    "cmx_model_module",
    "cmx_model_deploy_history",
    "cmx_model_source",
];

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}
fn db_err<E: std::fmt::Display>(ctx: &str) -> impl Fn(E) -> Error + '_ {
    move |e| Error::InternalError(format!("{ctx}: {e}"))
}

fn data_value_string(v: &DataValue) -> Option<String> {
    match v {
        DataValue::String(s) => Some(s.clone()),
        DataValue::ShortStr(s) => Some(s.to_string()),
        DataValue::LongStr(s) => Some(s.to_string()),
        DataValue::Int(i) => Some(i.to_string()),
        DataValue::Float(f) => Some(f.to_string()),
        DataValue::Decimal(d) => Some(d.to_string()),
        DataValue::DateTime(dt) => Some(dt.to_rfc3339()),
        DataValue::Date(d) => Some(d.to_string()),
        DataValue::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

// ════════════════════════════════════════════════════════════════════════
//  一、编译器：DCT/DOC 定义 JSON → cmx-core TableDefine
// ════════════════════════════════════════════════════════════════════════

/// DCT dataType 词元 → FieldType（大小写不敏感）。
fn map_field_type(data_type: &str) -> FieldType {
    match data_type.to_ascii_uppercase().as_str() {
        "VARCHAR" | "CHAR" | "STRING" => FieldType::String,
        "TEXT" | "CLOB" => FieldType::Text,
        "INT" | "INTEGER" | "BIGINT" | "TINYINT" | "SMALLINT" | "LONG" => FieldType::Int,
        "DECIMAL" | "NUMERIC" | "NUMBER" => FieldType::Decimal,
        "FLOAT" | "DOUBLE" | "REAL" => FieldType::Float,
        "DATE" => FieldType::Date,
        "DATETIME" | "TIMESTAMP" => FieldType::DateTime,
        "BOOL" | "BOOLEAN" => FieldType::Bool,
        "JSON" | "JSONB" => FieldType::Json,
        "UUID" => FieldType::Uuid,
        "BINARY" | "BLOB" | "BYTEA" => FieldType::Binary,
        _ => FieldType::String,
    }
}

/// 取字段中文标题（caption.zh_CN / caption 字符串 / 空）。
fn field_caption(f: &Value) -> String {
    match f.get("caption") {
        Some(Value::Object(o)) => o
            .get("zh_CN")
            .or_else(|| o.get("en"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        Some(Value::String(s)) => s.clone(),
        _ => String::new(),
    }
}

/// 表/汇总表标题（caption.zh_CN / caption 字符串 / tableAlias / name）。
fn table_caption(t: &Value, fallback: &str) -> String {
    t.get("tableAlias")
        .and_then(|v| v.as_str())
        .or_else(|| match t.get("caption") {
            Some(Value::Object(o)) => o
                .get("zh_CN")
                .or_else(|| o.get("en"))
                .and_then(|v| v.as_str()),
            Some(Value::String(s)) => Some(s.as_str()),
            _ => None,
        })
        .or_else(|| t.get("name").and_then(|v| v.as_str()))
        .unwrap_or(fallback)
        .to_string()
}

/// 单个字段对象 → ColumnDefine。id_field 命中则标记主键。
fn field_to_column(f: &Value, id_field: &str, ordinal: u32) -> Option<ColumnDefine> {
    let name = f.get("name").and_then(|v| v.as_str())?.to_string();
    if name.is_empty() {
        return None;
    }
    let data_type = f
        .get("dataType")
        .and_then(|v| v.as_str())
        .unwrap_or("VARCHAR");
    let ft = map_field_type(data_type);
    let nullable = f.get("nullable").and_then(|v| v.as_bool()).unwrap_or(true);
    let field_len = f
        .get("fieldLength")
        .and_then(|v| v.as_u64())
        .map(|n| n as u32);
    let dec = f
        .get("decimalDigits")
        .and_then(|v| v.as_u64())
        .map(|n| n as u32);
    let is_pk = (!id_field.is_empty() && name == id_field)
        || f.get("isPrimaryKey")
            .and_then(|v| v.as_i64())
            .map(|n| n != 0)
            .unwrap_or(false)
        || f.get("isPrimaryKey")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

    // 长度 / 精度：VARCHAR 用 length；DECIMAL 用 precision(=fieldLength)+scale(=decimalDigits)。
    let (length, precision, scale) = match ft {
        FieldType::String => (field_len, None, None),
        FieldType::Decimal => (None, field_len, dec.or(Some(0))),
        _ => (None, None, None),
    };

    Some(ColumnDefine {
        name,
        label: field_caption(f),
        field_type: ft,
        is_primary_key: is_pk,
        is_nullable: if is_pk { false } else { nullable },
        default_value: None,
        i18n: false,
        length,
        precision,
        scale,
        db_type: None,
        ordinal: Some(ordinal),
        create_time: None,
        update_time: None,
        is_foreign_key: false,
        foreign_key_table: None,
        foreign_key_column: None,
        extensions: Default::default(),
    })
}

/// uniqueKeys = [[col,...], ...] → 唯一索引定义。
fn unique_indexes(t: &Value, table_name: &str) -> Vec<IndexDefine> {
    let mut indexes = Vec::new();
    if let Some(uks) = t.get("uniqueKeys").and_then(|v| v.as_array()) {
        for (i, uk) in uks.iter().enumerate() {
            if let Some(cols) = uk.as_array() {
                let cnames: Vec<String> = cols
                    .iter()
                    .filter_map(|c| c.as_str().map(|s| s.to_string()))
                    .collect();
                if !cnames.is_empty() {
                    indexes.push(IndexDefine {
                        name: format!("uk_{}_{}", table_name, i + 1),
                        columns: cnames,
                        kind: IndexKind::Unique,
                    });
                }
            }
        }
    }
    indexes
}

/// 从 base fieldSets 对象里取某字段集的 fields 数组。
fn base_fieldset<'a>(base: &'a Value, set_name: &str) -> Option<&'a Vec<Value>> {
    base.get("fieldSets")?
        .get(set_name)?
        .get("fields")?
        .as_array()
}

/// DCT 定义 doc + 其 base fieldset doc → Vec<TableDefine>（每个 dictionaryTable 一张表）。
/// 列 = 本表 fields + baseFieldSet + auditFieldSet + systemFieldSet（跳过 null），按列名去重。
fn compile_dct(doc: &Value, base: &Value) -> Vec<TableDefine> {
    let tables = match doc.get("dictionaryTables").and_then(|v| v.as_array()) {
        Some(t) => t,
        None => return vec![],
    };
    let mut out = Vec::new();
    for t in tables {
        let dm = t.get("dictMeta").cloned().unwrap_or(json!({}));
        let table_name = dm
            .get("tableName")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if table_name.is_empty() {
            continue;
        }
        let id_field = dm.get("idField").and_then(|v| v.as_str()).unwrap_or("id");
        let display = dm
            .get("dictName")
            .and_then(|v| v.as_str())
            .unwrap_or(&table_name)
            .to_string();
        let comment = dm
            .get("remark")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // 合并字段来源：本表 + 三个引用字段集（跳过 null / 缺失）。
        let mut columns: Vec<ColumnDefine> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut ord: u32 = 0;
        let push_fields = |fields: &Vec<Value>,
                           columns: &mut Vec<ColumnDefine>,
                           seen: &mut std::collections::HashSet<String>,
                           ord: &mut u32| {
            for f in fields {
                *ord += 1;
                if let Some(c) = field_to_column(f, id_field, *ord)
                    && seen.insert(c.name.clone()) {
                        columns.push(c);
                    }
            }
        };
        if let Some(own) = t.get("fields").and_then(|v| v.as_array()) {
            push_fields(own, &mut columns, &mut seen, &mut ord);
        }
        // 合并全部 *FieldSet 引用（base/hierarchy/audit/effective/disable/scope/system…）。
        // 固定顺序保证列序稳定；hierarchyFieldSet 提供自分级字典的 parent_id/full_path/level_no/is_leaf。
        for set_key in [
            "baseFieldSet",
            "hierarchyFieldSet",
            "scopeFieldSet",
            "effectiveFieldSet",
            "disableFieldSet",
            "auditFieldSet",
            "systemFieldSet",
        ] {
            if let Some(set_name) = t.get(set_key).and_then(|v| v.as_str())
                && let Some(fields) = base_fieldset(base, set_name) {
                    push_fields(fields, &mut columns, &mut seen, &mut ord);
                }
        }
        // 兜底：捕获上面未列出的任何 `*FieldSet` 键（前向兼容新增字段集）。
        if let Some(obj) = t.as_object() {
            for (k, v) in obj {
                if k.ends_with("FieldSet")
                    && !matches!(
                        k.as_str(),
                        "baseFieldSet"
                            | "hierarchyFieldSet"
                            | "scopeFieldSet"
                            | "effectiveFieldSet"
                            | "disableFieldSet"
                            | "auditFieldSet"
                            | "systemFieldSet"
                    )
                    && let Some(set_name) = v.as_str()
                    && let Some(fields) = base_fieldset(base, set_name)
                {
                    push_fields(fields, &mut columns, &mut seen, &mut ord);
                }
            }
        }

        let primary_keys: Vec<String> = columns
            .iter()
            .filter(|c| c.is_primary_key)
            .map(|c| c.name.clone())
            .collect();

        out.push(TableDefine {
            indexes: unique_indexes(t, &table_name),
            table_name,
            display_name: display,
            columns,
            primary_keys,
            version: 1,
            create_time: None,
            update_time: None,
            i18n: false,
            comment,
            schema: None,
            tablespace: None,
            is_partitioned: false,
            partition_type: None,
            partition_columns: vec![],
            extensions: Default::default(),
        });
    }
    out
}

fn compile_doc_table(t: &Value, base: &Value) -> Option<TableDefine> {
    let table_name = t
        .get("tableName")
        .or_else(|| t.get("name"))
        .or_else(|| t.get("id"))
        .and_then(|v| v.as_str())?
        .to_string();
    if table_name.is_empty() {
        return None;
    }
    let display = table_caption(t, &table_name);
    let comment = t
        .get("remark")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let mut columns: Vec<ColumnDefine> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut ord: u32 = 0;
    // DOC 主键约定：id（若存在）。sum/summaries 表也沿用该约定。
    let id_field = "id";
    if let Some(own) = t.get("fields").and_then(|v| v.as_array()) {
        for f in own {
            ord += 1;
            if let Some(c) = field_to_column(f, id_field, ord)
                && seen.insert(c.name.clone()) {
                    columns.push(c);
                }
        }
    }
    // documentFieldSets: [ "voucherCommonFields", ... ] 引用 base。汇总表通常不配，
    // 但保留同样展开能力，便于后续把通用审计字段抽到 base。
    if let Some(sets) = t.get("documentFieldSets").and_then(|v| v.as_array()) {
        for s in sets {
            if let Some(set_name) = s.as_str()
                && let Some(fields) = base_fieldset(base, set_name) {
                    for f in fields {
                        ord += 1;
                        if let Some(c) = field_to_column(f, id_field, ord)
                            && seen.insert(c.name.clone()) {
                                columns.push(c);
                            }
                    }
                }
        }
    }

    let primary_keys: Vec<String> = columns
        .iter()
        .filter(|c| c.is_primary_key)
        .map(|c| c.name.clone())
        .collect();

    Some(TableDefine {
        table_name: table_name.clone(),
        display_name: display,
        columns,
        primary_keys,
        indexes: unique_indexes(t, &table_name),
        version: 1,
        create_time: None,
        update_time: None,
        i18n: false,
        comment,
        schema: None,
        tablespace: None,
        is_partitioned: false,
        partition_type: None,
        partition_columns: vec![],
        extensions: Default::default(),
    })
}

/// DOC 定义 doc → Vec<TableDefine>（每个 voucherTable 一张表）。
/// DOC 列 = 本表 fields + documentFieldSets 引用的 base 字段集；每层表下的
/// summaries/sum 汇总表也按同一 TableDefine 链路编译，以复用创建与升级执行器。
fn compile_doc(doc: &Value, base: &Value) -> Vec<TableDefine> {
    let tables = match doc.get("voucherTables").and_then(|v| v.as_array()) {
        Some(t) => t,
        None => return vec![],
    };
    let mut out = Vec::new();
    let mut seen_tables: std::collections::HashSet<String> = std::collections::HashSet::new();
    for t in tables {
        if let Some(def) = compile_doc_table(t, base)
            && seen_tables.insert(def.table_name.clone()) {
                out.push(def);
            }
        for key in ["summaries", "sum"] {
            if let Some(summaries) = t.get(key).and_then(|v| v.as_array()) {
                for summary in summaries {
                    if let Some(def) = compile_doc_table(summary, base)
                        && seen_tables.insert(def.table_name.clone()) {
                            out.push(def);
                        }
                }
            }
        }
    }
    out
}

// 台账系统表本身的 TableDefine（初始化时建入目标库）。此处直接给最小结构，
// 完整列由迁移 SQL 保证；这里仅用于「目标库没有该表时」由引擎补齐 + 未来 diff 升级。
// 为稳妥，实际初始化改为直接执行 CREATE TABLE IF NOT EXISTS（见 init_db）。

// ════════════════════════════════════════════════════════════════════════
//  二、内省辅助：读定义文件（复用 definitions store）
// ════════════════════════════════════════════════════════════════════════

/// 读某定义文件全文（domain/application/module/file）。
async fn read_def(domain: &str, app: &str, module: &str, file: &str) -> Result<Value> {
    let r = cmx_portal::definitions::store::DefRef {
        domain: Some(domain.to_string()),
        application: Some(app.to_string()),
        app: Some(app.to_string()),
        module: Some(module.to_string()),
        file: Some(file.to_string()),
        id: None,
    };
    cmx_portal::definitions::store::get_definition(&r)
        .await
        .map_err(|e| Error::BadRequest(format!("读取定义失败 {file}: {e}")))
}

/// 读 base 字段集文件（domain=base）。
async fn read_base(file: &str) -> Result<Value> {
    let r = cmx_portal::definitions::store::DefRef {
        domain: Some("base".to_string()),
        application: None,
        app: None,
        module: None,
        file: Some(file.to_string()),
        id: None,
    };
    cmx_portal::definitions::store::get_definition(&r)
        .await
        .map_err(db_err("读取 base 字段集失败"))
}

/// 编译一个定义（kind=DCT/DOC）→ (TableDefine 列表, 源 JSON)。
async fn compile_definition(
    kind: &str,
    domain: &str,
    app: &str,
    module: &str,
    file: &str,
) -> Result<(Vec<TableDefine>, Value)> {
    let doc = read_def(domain, app, module, file).await?;
    let base_ref_key = if kind == "DOC" {
        "baseDocMetaRef"
    } else {
        "baseDctMetaRef"
    };
    let base_file = doc
        .get(base_ref_key)
        .and_then(|r| r.get("file"))
        .and_then(|v| v.as_str())
        .unwrap_or(if kind == "DOC" {
            "base_doc_meta_v1.json"
        } else {
            "base_dct_meta_v1.json"
        })
        .to_string();
    let base = read_base(&base_file)
        .await
        .unwrap_or(json!({ "fieldSets": {} }));
    let defs = if kind == "DOC" {
        compile_doc(&doc, &base)
    } else {
        compile_dct(&doc, &base)
    };
    Ok((defs, doc))
}

// ════════════════════════════════════════════════════════════════════════
//  三、DB 读取辅助（针对任意 db_id）
// ════════════════════════════════════════════════════════════════════════

/// 表是否存在（information_schema，内省）。
async fn table_exists(db_id: &str, table: &str) -> Result<bool> {
    let mm = get_default_db_manager();
    let sql = "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = current_schema() AND table_name = $1";
    let ds = mm
        .query_sql_with_datavalues(
            db_id,
            None,
            sql,
            vec![DataValue::String(table.to_string())],
            "mc_exists",
        )
        .await
        .map_err(db_err("查询表存在性失败"))?;
    let n = ds
        .iter()
        .next()
        .and_then(|r| r.get(0))
        .and_then(|v| match v {
            DataValue::Int(i) => Some(*i),
            _ => None,
        })
        .unwrap_or(0);
    Ok(n > 0)
}

fn table_action_label(change: &Value) -> &'static str {
    match change.get("action").and_then(|v| v.as_str()) {
        Some("create_table") => "创建表",
        Some("upgrade_table") => "升级表",
        Some("no_change") => "校验表",
        _ => "处理表",
    }
}

/// 列类型 → 前端展示的 PG 类型字符串，与建表 DDL 基准（`PostgresDdlDialect::map_column_type`）一致。
///
/// 统一从这里取类型字符串，保证「计划报告」与「实际建表/升级」使用同一套类型映射，
/// 避免出现 `integer` vs `bigint`、`timestamp without time zone` vs `timestamp with time zone`
/// 这类永久假阳性。
fn pg_display_type(c: &ColumnDefine) -> String {
    PostgresDdlDialect::default().map_column_type(c)
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
    if old.field_type != new.field_type {
        push("dataType", pg_display_type(old), pg_display_type(new));
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
fn report_create_table(def: &TableDefine) -> Value {
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
fn diff_table_to_report(current: &TableDefine, desired: &TableDefine) -> Value {
    let changes = DdlDiff::diff(std::slice::from_ref(current), std::slice::from_ref(desired));
    match changes.as_slice() {
        // 表无实质变更（DdlDiff 未产出任何 TableChange）
        [] => json!({
            "table": desired.table_name,
            "displayName": desired.display_name,
            "action": "no_change",
            "created": false,
            "addedColumns": [],
            "modifiedColumns": [],
            "unchangedColumns": desired.columns.iter().map(|c| c.name.clone()).collect::<Vec<_>>(),
            "columnCount": desired.columns.len(),
        }),
        // 仅可能是 AlterTable（CreateTable/DropTable 不会出现，因为表已存在且两边表名相同）
        [TableChange::AlterTable { column_changes, .. }, ..] => {
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
            })
        }
        // 兜底：理论上不可达（表已存在，DdlDiff 只可能产出 AlterTable 或空）
        _ => json!({
            "table": desired.table_name,
            "displayName": desired.display_name,
            "action": "no_change",
            "created": false,
            "addedColumns": [],
            "modifiedColumns": [],
            "unchangedColumns": desired.columns.iter().map(|c| c.name.clone()).collect::<Vec<_>>(),
            "columnCount": desired.columns.len(),
        }),
    }
}

/// 生成单张表的部署计划报告（含建表/升级/无变化判定）。
///
/// 与执行路径共用同一套 DdlDiff 比较引擎：表不存在 → 建表报告；表已存在 → 通过
/// [`PgTableDefineExecutor::query_current_table_define`] 内省还原当前结构，再调用
/// [`diff_table_to_report`] 生成与执行结果一致的变更明细。
async fn table_change_plan(db_id: &str, def: &TableDefine) -> Result<Value> {
    if !table_exists(db_id, &def.table_name).await? {
        return Ok(report_create_table(def));
    }
    let executor = PgTableDefineExecutor::new(db_id.to_string(), None);
    let current = executor
        .query_current_table_define(def)
        .await
        .map_err(db_err("内省当前表结构失败"))?;
    Ok(diff_table_to_report(&current, def))
}

/// 读 cmx_model_meta 单行（未初始化返回 None）。
async fn read_meta(db_id: &str) -> Result<Option<Value>> {
    if !table_exists(db_id, "cmx_model_meta").await? {
        return Ok(None);
    }
    let mm = get_default_db_manager();
    let ds = mm
        .query_sql(
            db_id,
            None,
            "SELECT meta_version, status, initialized_at FROM cmx_model_meta LIMIT 1",
            "mc_meta",
        )
        .await
        .map_err(db_err("读取 cmx_model_meta 失败"))?;
    let schema = ds.schema.clone();
    if let Some(row) = ds.iter().next() {
        let mv = row
            .get_by_name(schema.as_ref(), "meta_version")
            .and_then(|v| match v {
                DataValue::Int(i) => Some(*i),
                _ => None,
            })
            .unwrap_or(1);
        let st = row
            .get_by_name(schema.as_ref(), "status")
            .and_then(|v| match v {
                DataValue::String(s) => Some(s.clone()),
                DataValue::ShortStr(s) => Some(s.to_string()),
                _ => None,
            })
            .unwrap_or_default();
        Ok(Some(json!({ "meta_version": mv, "status": st })))
    } else {
        Ok(None)
    }
}

/// 读 cmx_model_module 全部行 → map: "domain/app/module" -> 已部署模块台账详情。
async fn read_modules(db_id: &str) -> Result<std::collections::HashMap<String, Value>> {
    let mut map = std::collections::HashMap::new();
    if !table_exists(db_id, "cmx_model_module").await? {
        return Ok(map);
    }
    let mm = get_default_db_manager();
    let sql = "SELECT domain_code, application_code, module_code, module_name, dct_version, dct_status, doc_version, doc_status, seed_version, seed_status, table_count, def_source, first_deployed_at, current_deployed_at, deployed_by, deployed_name, create_time, update_time FROM cmx_model_module WHERE archived = 0";
    let ds = mm
        .query_sql(db_id, None, sql, "mc_modules")
        .await
        .map_err(db_err("读取模块台账失败"))?;
    let schema = ds.schema.clone();
    let sv = |row: &cmx_core::model::data::dataset::Row, name: &str| -> Option<String> {
        row.get_by_name(schema.as_ref(), name)
            .and_then(data_value_string)
    };
    for row in ds.iter() {
        let d = sv(row, "domain_code").unwrap_or_default();
        let a = sv(row, "application_code").unwrap_or_default();
        let m = sv(row, "module_code").unwrap_or_default();
        let key = format!("{d}/{a}/{m}");
        map.insert(key.clone(), json!({
            "key": key,
            "domain": d,
            "application": a,
            "module": m,
            "module_name": sv(row, "module_name"),
            "table_count": sv(row, "table_count").and_then(|s| s.parse::<i64>().ok()).unwrap_or(0),
            "def_source": sv(row, "def_source"),
            "first_deployed_at": sv(row, "first_deployed_at"),
            "current_deployed_at": sv(row, "current_deployed_at"),
            "deployed_by": sv(row, "deployed_by"),
            "deployed_name": sv(row, "deployed_name"),
            "create_time": sv(row, "create_time"),
            "update_time": sv(row, "update_time"),
            "dct": { "version": sv(row, "dct_version"), "status": sv(row, "dct_status").unwrap_or_else(|| "none".into()) },
            "doc": { "version": sv(row, "doc_version"), "status": sv(row, "doc_status").unwrap_or_else(|| "none".into()) },
            "seed": { "version": sv(row, "seed_version"), "status": sv(row, "seed_status").unwrap_or_else(|| "none".into()) },
        }));
    }
    Ok(map)
}

/// 由 applied/latest/status 推导 scenario（镜像前端 mcScenario）。
fn scenario_of(applied: Option<&str>, latest: Option<&str>, status: &str) -> &'static str {
    if status == "failed" {
        return "retry";
    }
    match (applied, latest) {
        (None, Some(_)) => "create",
        (None, None) => "none",
        (Some(_), None) => "current",
        (Some(a), Some(l)) => {
            let (na, nl) = (a.parse::<i64>().unwrap_or(0), l.parse::<i64>().unwrap_or(0));
            if na < nl {
                "upgrade"
            } else if na > nl {
                "downgrade"
            } else if status == "drift" {
                "drift"
            } else {
                "current"
            }
        }
    }
}

// ════════════════════════════════════════════════════════════════════════
//  四、对外 API：db_state / init_db / deploy
// ════════════════════════════════════════════════════════════════════════

/// 组合 db-state：库门闸 + 每模块每 kind scenario（真实读台账 + 定义列表）。
pub async fn db_state(db_id: &str) -> Result<Value> {
    let meta = read_meta(db_id).await?;
    let initialized = meta.is_some();
    let meta_version = meta
        .as_ref()
        .and_then(|m| m.get("meta_version"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0) as i32;
    let applied_modules = read_modules(db_id).await?;

    // 定义列表：每模块每 kind 的默认版本 = latest。
    let dct_list = cmx_portal::definitions::store::list_definitions(Some("DCT"), None, None, None)
        .await
        .map_err(db_err("列出 DCT 失败"))?;
    let doc_list = cmx_portal::definitions::store::list_definitions(Some("DOC"), None, None, None)
        .await
        .map_err(db_err("列出 DOC 失败"))?;

    // 归并成 module -> { DCT: {ver,file,title}, DOC: {...} }（取默认/最大版本）
    #[derive(Default, Clone)]
    struct KindDef {
        ver: i64,
        file: String,
        title: String,
        is_default: bool,
        tables: i64,
        summary: String,
    }
    // module -> (domain, application, module_code, display_name, kinds)
    type ModuleMap = std::collections::BTreeMap<
        String,
        (
            String,
            String,
            String,
            String,
            std::collections::HashMap<&'static str, KindDef>,
        ),
    >;
    let mut mods: ModuleMap = std::collections::BTreeMap::new();
    let mut versions: std::collections::BTreeMap<
        String,
        std::collections::HashMap<&'static str, Vec<KindDef>>,
    > = std::collections::BTreeMap::new();
    let mut ingest = |items: &Vec<Value>, kind: &'static str| {
        for it in items {
            let domain = it
                .get("domain")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let app = it
                .get("application")
                .or_else(|| it.get("app"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let module = it
                .get("module")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let key = format!("{domain}/{app}/{module}");
            let ver = it.get("version").and_then(|v| v.as_i64()).unwrap_or(1);
            let is_def = it
                .get("isDefault")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let file = it
                .get("file")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let title = it
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let tables = it.get("tableCount").and_then(|v| v.as_i64()).unwrap_or(0);
            let summary = it
                .get("summary")
                .or_else(|| it.get("details"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let e = mods.entry(key).or_insert_with(|| {
                (
                    domain.clone(),
                    app.clone(),
                    module.clone(),
                    title.clone(),
                    std::collections::HashMap::new(),
                )
            });
            let cur = e.4.entry(kind).or_default();
            if file.is_empty() {
                continue;
            }
            let kd = KindDef {
                ver,
                file: file.clone(),
                title: title.clone(),
                is_default: is_def,
                tables,
                summary: summary.clone(),
            };
            versions
                .entry(format!("{domain}/{app}/{module}"))
                .or_default()
                .entry(kind)
                .or_default()
                .push(kd.clone());
            if cur.file.is_empty()
                || (is_def && !cur.is_default)
                || (is_def == cur.is_default && ver > cur.ver)
            {
                *cur = kd;
            }
            if e.3.is_empty() && !title.is_empty() {
                e.3 = title.clone();
            }
        }
    };
    ingest(&dct_list, "DCT");
    ingest(&doc_list, "DOC");

    let mut modules = Vec::new();
    let mut counts = json!({ "create": 0, "upgrade": 0, "current": 0, "retry": 0, "drift": 0 });
    let bump = |counts: &mut Value, sc: &str| {
        if let Some(n) = counts.get_mut(sc).and_then(|v| v.as_i64()) {
            counts[sc] = json!(n + 1);
        }
    };

    for (key, (domain, app, module, title, kinds)) in &mods {
        let applied = applied_modules.get(key);
        let mut cell = |kind: &str| -> Value {
            let def = kinds.get(kind);
            let mut vers = versions
                .get(key)
                .and_then(|m| m.get(kind))
                .cloned()
                .unwrap_or_default();
            vers.sort_by_key(|b| Reverse(b.ver));
            let latest = def.map(|k| k.ver.to_string());
            let (app_ver, status) = applied
                .and_then(|a| a.get(kind.to_lowercase()))
                .map(|k| {
                    (
                        k.get("version")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string()),
                        k.get("status")
                            .and_then(|v| v.as_str())
                            .unwrap_or("none")
                            .to_string(),
                    )
                })
                .unwrap_or((None, "none".to_string()));
            let sc = scenario_of(app_ver.as_deref(), latest.as_deref(), &status);
            bump(&mut counts, sc);
            json!({
                "applied": app_ver,
                "latest": latest,
                "status": status,
                "scenario": sc,
                "file": def.map(|k| k.file.clone()).unwrap_or_default(),
                "title": def.map(|k| k.title.clone()).unwrap_or_default(),
                "summary": def.map(|k| k.summary.clone()).unwrap_or_default(),
                "table_count": def.map(|k| k.tables).unwrap_or(0),
                "is_default": def.map(|k| k.is_default).unwrap_or(false),
                "versions": vers.iter().map(|v| json!({
                    "version": v.ver.to_string(),
                    "file": v.file,
                    "title": v.title,
                    "summary": v.summary,
                    "table_count": v.tables,
                    "is_default": v.is_default,
                })).collect::<Vec<Value>>(),
            })
        };
        let tblc: i64 = kinds.values().map(|k| k.tables).sum();
        modules.push(json!({
            "domain": domain, "application": app, "module": module,
            "module_name": applied.and_then(|a| a.get("module_name")).and_then(|v| v.as_str()).unwrap_or(if title.is_empty() { module.as_str() } else { title.as_str() }),
            "installed": applied.is_some(),
            "created_at": applied.and_then(|a| a.get("first_deployed_at")).and_then(|v| v.as_str()),
            "updated_at": applied.and_then(|a| a.get("current_deployed_at")).and_then(|v| v.as_str()).or_else(|| applied.and_then(|a| a.get("update_time")).and_then(|v| v.as_str())),
            "deployed_by": applied.and_then(|a| a.get("deployed_by")).and_then(|v| v.as_str()),
            "deployed_name": applied.and_then(|a| a.get("deployed_name")).and_then(|v| v.as_str()),
            "table_count": tblc,
            "dct": cell("DCT"), "doc": cell("DOC"),
            "seed": json!({ "applied": null, "latest": null, "status": "none", "scenario": "none", "file": "" }),
        }));
    }

    let mut installed_modules = Vec::new();
    for (key, applied) in &applied_modules {
        let parts: Vec<&str> = key.split('/').collect();
        let domain = applied
            .get("domain")
            .and_then(|v| v.as_str())
            .unwrap_or_else(|| parts.first().copied().unwrap_or(""));
        let app = applied
            .get("application")
            .and_then(|v| v.as_str())
            .unwrap_or_else(|| parts.get(1).copied().unwrap_or(""));
        let module = applied
            .get("module")
            .and_then(|v| v.as_str())
            .unwrap_or_else(|| parts.get(2).copied().unwrap_or(""));
        let defined = mods.get(key);
        let def_cell = |kind: &str| -> Value {
            let def = defined.and_then(|(_, _, _, _, kinds)| kinds.get(kind));
            let mut vers = versions
                .get(key)
                .and_then(|m| m.get(kind))
                .cloned()
                .unwrap_or_default();
            vers.sort_by_key(|b| Reverse(b.ver));
            let k = applied
                .get(kind.to_lowercase())
                .cloned()
                .unwrap_or_else(|| json!({}));
            json!({
                "applied": k.get("version").and_then(|v| v.as_str()),
                "status": k.get("status").and_then(|v| v.as_str()).unwrap_or("none"),
                "latest": def.map(|d| d.ver.to_string()),
                "file": def.map(|d| d.file.clone()).unwrap_or_default(),
                "title": def.map(|d| d.title.clone()).unwrap_or_default(),
                "summary": def.map(|d| d.summary.clone()).unwrap_or_default(),
                "table_count": def.map(|d| d.tables).unwrap_or(0),
                "versions": vers.iter().map(|v| json!({
                    "version": v.ver.to_string(),
                    "file": v.file,
                    "title": v.title,
                    "summary": v.summary,
                    "table_count": v.tables,
                    "is_default": v.is_default,
                })).collect::<Vec<Value>>(),
            })
        };
        installed_modules.push(json!({
            "key": key,
            "domain": domain,
            "application": app,
            "module": module,
            "module_name": applied.get("module_name").and_then(|v| v.as_str()).unwrap_or(module),
            "table_count": applied.get("table_count").and_then(|v| v.as_i64()).unwrap_or(0),
            "created_at": applied.get("first_deployed_at").and_then(|v| v.as_str()).or_else(|| applied.get("create_time").and_then(|v| v.as_str())),
            "updated_at": applied.get("current_deployed_at").and_then(|v| v.as_str()).or_else(|| applied.get("update_time").and_then(|v| v.as_str())),
            "deployed_by": applied.get("deployed_by").and_then(|v| v.as_str()),
            "deployed_name": applied.get("deployed_name").and_then(|v| v.as_str()),
            "dct": def_cell("DCT"),
            "doc": def_cell("DOC"),
            "seed": def_cell("SEED"),
        }));
    }

    let page_mode = if !initialized {
        "init"
    } else if meta_version < META_VERSION {
        "meta_upgrade"
    } else {
        "normal"
    };

    Ok(json!({
        "db_id": db_id,
        "initialized": initialized,
        "meta_version": meta_version,
        "expected_meta_version": META_VERSION,
        "db_status": if initialized { "CURRENT" } else { "UNINITIALIZED" },
        "page_mode": page_mode,
        "scenario_counts": counts,
        "installed_modules": installed_modules,
        "modules": modules,
    }))
}

/// 台账系统表 DDL（初始化时执行；幂等 IF NOT EXISTS，与迁移文件同源）。
const INIT_DDL: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS cmx_model_meta (id VARCHAR(64) NOT NULL, db_id VARCHAR(100), meta_version INT4 NOT NULL DEFAULT 1, app_id VARCHAR(64) NOT NULL DEFAULT 'default', engine_version VARCHAR(50), portal_version VARCHAR(50), status VARCHAR(20) NOT NULL DEFAULT 'ready', initialized_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP, initialized_by VARCHAR(100), initialized_name VARCHAR(100), last_upgraded_at TIMESTAMP, last_upgraded_by VARCHAR(100), remark VARCHAR(500), create_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP, update_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP, PRIMARY KEY (id))",
    "CREATE UNIQUE INDEX IF NOT EXISTS uk_model_meta_db_app ON cmx_model_meta (db_id, app_id)",
    "CREATE TABLE IF NOT EXISTS cmx_model_module (id VARCHAR(64) NOT NULL, db_id VARCHAR(100), app_id VARCHAR(64) NOT NULL DEFAULT 'default', domain_code VARCHAR(100), application_code VARCHAR(100), module_code VARCHAR(100), module_name VARCHAR(200), dct_version VARCHAR(50), dct_status VARCHAR(20) DEFAULT 'none', doc_version VARCHAR(50), doc_status VARCHAR(20) DEFAULT 'none', seed_version VARCHAR(50), seed_status VARCHAR(20) DEFAULT 'none', overall_status VARCHAR(20) DEFAULT 'active', table_count INT4 DEFAULT 0, def_source VARCHAR(300), def_checksum VARCHAR(64), first_deployed_at TIMESTAMP, current_deployed_at TIMESTAMP, deployed_by VARCHAR(100), deployed_name VARCHAR(100), archived INT4 DEFAULT 0, create_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP, update_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP, PRIMARY KEY (id))",
    "CREATE UNIQUE INDEX IF NOT EXISTS uk_model_module_key ON cmx_model_module (db_id, app_id, domain_code, application_code, module_code)",
    "CREATE TABLE IF NOT EXISTS cmx_model_deploy_history (id VARCHAR(64) NOT NULL, batch_id VARCHAR(64), db_id VARCHAR(100), app_id VARCHAR(64) NOT NULL DEFAULT 'default', domain_code VARCHAR(100), application_code VARCHAR(100), module_code VARCHAR(100), module_name VARCHAR(200), kind VARCHAR(20), action VARCHAR(20), from_version VARCHAR(50), to_version VARCHAR(50), status VARCHAR(20), ddl_summary JSONB, object_count INT4 DEFAULT 0, seed_rows INT4 DEFAULT 0, def_ref VARCHAR(300), def_version VARCHAR(50), engine_version VARCHAR(50), error_message TEXT, started_at TIMESTAMP, finished_at TIMESTAMP, duration_ms INT8, operator_id VARCHAR(100), operator_name VARCHAR(100), create_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP, PRIMARY KEY (id))",
    "CREATE INDEX IF NOT EXISTS idx_model_history_module ON cmx_model_deploy_history (db_id, domain_code, application_code, module_code)",
    "CREATE TABLE IF NOT EXISTS cmx_model_source (id VARCHAR(64) NOT NULL, db_id VARCHAR(100), app_id VARCHAR(64) NOT NULL DEFAULT 'default', domain_code VARCHAR(100), application_code VARCHAR(100), module_code VARCHAR(100), module_name VARCHAR(200), kind VARCHAR(20), version VARCHAR(50), source_file VARCHAR(300), source_json JSONB, compiled_json JSONB, checksum VARCHAR(64), table_count INT4 DEFAULT 0, seed_row_count INT4 DEFAULT 0, is_current INT4 DEFAULT 1, engine_version VARCHAR(50), imported_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP, imported_by VARCHAR(100), imported_name VARCHAR(100), remark VARCHAR(500), create_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP, PRIMARY KEY (id))",
    "CREATE UNIQUE INDEX IF NOT EXISTS uk_model_source_ver ON cmx_model_source (db_id, app_id, domain_code, application_code, module_code, kind, version)",
];

/// 初始化目标库：建 4 张台账系统表 + 写 cmx_model_meta + 记 INIT 历史。
pub async fn init_db(db_id: &str, operator_id: &str, operator_name: &str) -> Result<Value> {
    let mm = get_default_db_manager();
    // 幂等：已初始化则直接返回 db-state。
    if read_meta(db_id).await?.is_some() {
        return db_state(db_id).await;
    }
    // 1) 建系统表（DDL 自动提交，txn_id=None）
    for ddl in INIT_DDL {
        mm.execute_sql(db_id, None, ddl)
            .await
            .map_err(db_err("建台账系统表失败"))?;
    }
    // 2) 写 meta + 历史（同一事务）
    let ctx = mm.get_transaction_context();
    let guard = ctx
        .begin_with_guard(db_id)
        .await
        .map_err(db_err("开启事务失败"))?;
    let txn = guard.txn_id().to_string();

    let meta_sql = "INSERT INTO cmx_model_meta (id, db_id, meta_version, engine_version, status, initialized_by, initialized_name) VALUES ($1,$2,$3,$4,'ready',$5,$6)";
    mm.execute_sql_with_datavalues(
        db_id,
        Some(&txn),
        meta_sql,
        vec![
            DataValue::String(snowflake_id_str()),
            DataValue::String(db_id.to_string()),
            DataValue::Int(META_VERSION as i64),
            DataValue::String(ENGINE_VERSION.to_string()),
            DataValue::String(operator_id.to_string()),
            DataValue::String(operator_name.to_string()),
        ],
    )
    .await
    .map_err(db_err("写 cmx_model_meta 失败"))?;

    let hist_sql = "INSERT INTO cmx_model_deploy_history (id, db_id, kind, action, to_version, status, object_count, engine_version, operator_id, operator_name, finished_at) VALUES ($1,$2,'INIT','create',$3,'success',$4,$5,$6,$7,CURRENT_TIMESTAMP)";
    mm.execute_sql_with_datavalues(
        db_id,
        Some(&txn),
        hist_sql,
        vec![
            DataValue::String(snowflake_id_str()),
            DataValue::String(db_id.to_string()),
            DataValue::String(META_VERSION.to_string()),
            DataValue::Int(LEDGER_TABLES.len() as i64),
            DataValue::String(ENGINE_VERSION.to_string()),
            DataValue::String(operator_id.to_string()),
            DataValue::String(operator_name.to_string()),
        ],
    )
    .await
    .map_err(db_err("写 INIT 历史失败"))?;

    guard.commit().await.map_err(db_err("提交事务失败"))?;
    db_state(db_id).await
}

/// 初始化进度事件（推给 SSE 通道）。kind: connect/step/progress/done/error。
#[derive(Clone)]
pub struct InitEvent {
    pub kind: String,
    pub data: Value,
}
fn ev(kind: &str, data: Value) -> InitEvent {
    InitEvent {
        kind: kind.to_string(),
        data,
    }
}

/// 流式生成初始化/系统表升级计划：只读探测，不执行 DDL、不写台账。
pub async fn init_plan_stream(db_id: &str, tx: &tokio::sync::mpsc::UnboundedSender<InitEvent>) {
    let send = |e: InitEvent| {
        let _ = tx.send(e);
    };
    send(ev(
        "connect",
        json!({ "message": format!("准备生成目标数据库 {db_id} 的系统表执行计划 …"), "db_id": db_id }),
    ));

    let mm = get_default_db_manager();
    match mm.query_sql(db_id, None, "SELECT 1", "mc_plan_ping").await {
        Ok(_) => send(ev(
            "connect",
            json!({ "ok": true, "message": "数据库连接成功，开始只读检查" }),
        )),
        Err(e) => {
            send(ev(
                "error",
                json!({ "stage": "connect", "message": format!("数据库连接失败: {e}") }),
            ));
            return;
        }
    }

    let meta = match read_meta(db_id).await {
        Ok(v) => v,
        Err(e) => {
            send(ev(
                "error",
                json!({ "stage": "state", "message": format!("读取模型中心状态失败: {e}") }),
            ));
            return;
        }
    };
    let reinit = meta.is_some();
    send(ev(
        "step",
        json!({
            "message": if reinit { "该库已初始化：计划执行加性校验/升级，不删除任何数据" } else { "该库尚未初始化：计划创建模型中心台账系统表" },
            "reinit": reinit,
            "meta_version": meta.as_ref().and_then(|m| m.get("meta_version")).and_then(|v| v.as_i64()).unwrap_or(0),
            "expected_meta_version": META_VERSION,
        }),
    ));

    let total = INIT_DDL.len();
    send(ev(
        "step",
        json!({ "message": format!("计划检查/执行 {total} 条幂等 DDL（CREATE IF NOT EXISTS / INDEX IF NOT EXISTS）"), "total": total }),
    ));
    for (i, ddl) in INIT_DDL.iter().enumerate() {
        let obj = ddl
            .split_whitespace()
            .skip_while(|w| !w.eq_ignore_ascii_case("EXISTS"))
            .nth(1)
            .unwrap_or("")
            .to_string();
        send(ev(
            "progress",
            json!({
                "index": i + 1,
                "total": total,
                "object": obj,
                "message": format!("[{}/{}] {}", i + 1, total, if obj.is_empty() { "执行幂等 DDL" } else { obj.as_str() })
            }),
        ));
    }
    send(ev(
        "step",
        json!({ "message": "计划写入/更新 cmx_model_meta，并记录 INIT 执行历史" }),
    ));
    send(ev(
        "done",
        json!({
            "message": "执行计划已生成，请审核后确认是否执行",
            "action": if reinit { "upgrade" } else { "create" },
            "tables": LEDGER_TABLES,
            "ddl_count": total,
            "db_id": db_id,
        }),
    ));
}

/// 流式初始化：逐步把「连接建立 / 建表进度 / 写台账 / 完成 / 错误」推给 tx。
/// 每一步都发事件，前端实时展示；失败发 error 事件后中止（不 panic）。
pub async fn init_db_stream(
    db_id: &str,
    operator_id: &str,
    operator_name: &str,
    tx: &tokio::sync::mpsc::UnboundedSender<InitEvent>,
) {
    let started = std::time::Instant::now();
    let send = |e: InitEvent| {
        let _ = tx.send(e);
    };

    send(ev(
        "connect",
        json!({ "message": format!("连接目标数据库 {db_id} …"), "db_id": db_id }),
    ));

    let mm = get_default_db_manager();

    // 0) 探测连接（一次轻量查询即建立/复用连接池）——把连接失败与建表失败区分开。
    match mm.query_sql(db_id, None, "SELECT 1", "mc_ping").await {
        Ok(_) => send(ev(
            "connect",
            json!({ "ok": true, "message": "数据库连接成功" }),
        )),
        Err(e) => {
            send(ev(
                "error",
                json!({ "stage": "connect", "message": format!("数据库连接失败: {e}") }),
            ));
            return;
        }
    }

    // 幂等 + 可重复：已初始化不再跳过，而是重跑「加性 DDL」(CREATE TABLE IF NOT EXISTS，绝不删表/删列)，
    // 并把 meta 标记为「已校验/升级」。满足"允许多次初始化，但不删数据，只做变化性升级"。
    let reinit = matches!(read_meta(db_id).await, Ok(Some(_)));
    if reinit {
        send(ev(
            "step",
            json!({ "message": "检测到该库已初始化 → 执行加性校验/升级（不删除任何数据）" }),
        ));
    }

    // 1) 逐条建系统表（DDL 自动提交，每条发进度）。CREATE TABLE IF NOT EXISTS 天然加性、可重复。
    let total = INIT_DDL.len();
    send(ev(
        "step",
        json!({ "message": format!("{}台账系统表（{total} 条 DDL）", if reinit { "校验" } else { "创建" }), "total": total }),
    ));
    for (i, ddl) in INIT_DDL.iter().enumerate() {
        // 从 DDL 里粗提对象名用于展示
        let obj = ddl
            .split_whitespace()
            .skip_while(|w| !w.eq_ignore_ascii_case("EXISTS"))
            .nth(1)
            .unwrap_or("")
            .to_string();
        match mm.execute_sql(db_id, None, ddl).await {
            Ok(_) => send(ev(
                "progress",
                json!({
                    "index": i + 1, "total": total, "object": obj,
                    "message": format!("[{}/{}] {}", i + 1, total, if obj.is_empty() { "执行 DDL" } else { obj.as_str() })
                }),
            )),
            Err(e) => {
                send(ev(
                    "error",
                    json!({ "stage": "ddl", "index": i + 1, "object": obj, "message": format!("建表失败: {e}") }),
                ));
                return;
            }
        }
    }

    // 2) 写/更新 meta + 记历史（事务）。UPSERT：首次插入，重复初始化则更新 last_upgraded_at（不动 initialized_*）。
    send(ev("step", json!({ "message": "写入台账元信息与历史 …" })));
    let ctx = mm.get_transaction_context();
    let guard = match ctx.begin_with_guard(db_id).await {
        Ok(g) => g,
        Err(e) => {
            send(ev(
                "error",
                json!({ "stage": "txn", "message": format!("开启事务失败: {e}") }),
            ));
            return;
        }
    };
    let txn = guard.txn_id().to_string();

    let meta_sql = "INSERT INTO cmx_model_meta (id, db_id, meta_version, engine_version, status, initialized_by, initialized_name) VALUES ($1,$2,$3,$4,'ready',$5,$6) \
        ON CONFLICT (db_id, app_id) DO UPDATE SET meta_version = EXCLUDED.meta_version, engine_version = EXCLUDED.engine_version, status = 'ready', last_upgraded_at = CURRENT_TIMESTAMP, last_upgraded_by = EXCLUDED.initialized_by, update_time = CURRENT_TIMESTAMP";
    if let Err(e) = mm
        .execute_sql_with_datavalues(
            db_id,
            Some(&txn),
            meta_sql,
            vec![
                DataValue::String(snowflake_id_str()),
                DataValue::String(db_id.to_string()),
                DataValue::Int(META_VERSION as i64),
                DataValue::String(ENGINE_VERSION.to_string()),
                DataValue::String(operator_id.to_string()),
                DataValue::String(operator_name.to_string()),
            ],
        )
        .await
    {
        send(ev(
            "error",
            json!({ "stage": "meta", "message": format!("写元信息失败: {e}") }),
        ));
        return;
    }
    let action = if reinit { "upgrade" } else { "create" };
    let hist_sql = "INSERT INTO cmx_model_deploy_history (id, db_id, kind, action, to_version, status, object_count, engine_version, operator_id, operator_name, finished_at) VALUES ($1,$2,'INIT',$3,$4,'success',$5,$6,$7,$8,CURRENT_TIMESTAMP)";
    if let Err(e) = mm
        .execute_sql_with_datavalues(
            db_id,
            Some(&txn),
            hist_sql,
            vec![
                DataValue::String(snowflake_id_str()),
                DataValue::String(db_id.to_string()),
                DataValue::String(action.to_string()),
                DataValue::String(META_VERSION.to_string()),
                DataValue::Int(LEDGER_TABLES.len() as i64),
                DataValue::String(ENGINE_VERSION.to_string()),
                DataValue::String(operator_id.to_string()),
                DataValue::String(operator_name.to_string()),
            ],
        )
        .await
    {
        send(ev(
            "error",
            json!({ "stage": "history", "message": format!("写历史失败: {e}") }),
        ));
        return;
    }
    if let Err(e) = guard.commit().await {
        send(ev(
            "error",
            json!({ "stage": "commit", "message": format!("提交事务失败: {e}") }),
        ));
        return;
    }

    // 3) 完成：附最新 db-state，前端直接刷新工作台。
    match db_state(db_id).await {
        Ok(st) => send(ev(
            "done",
            json!({
                "message": if reinit { "校验/升级完成（未删除任何数据）" } else { "初始化完成" },
                "reinit": reinit,
                "tables": total,
                "db_state": st,
                "duration_ms": started.elapsed().as_millis() as i64,
            }),
        )),
        Err(e) => send(ev(
            "error",
            json!({ "stage": "state", "message": e.to_string() }),
        )),
    }
}

/// 部署一批定义（create/upgrade）到目标库：编译→建表→写台账+源JSON+历史。
/// items: [{ kind, domain, application, module, file }]
pub async fn deploy(
    db_id: &str,
    items: &[Value],
    operator_id: &str,
    operator_name: &str,
) -> Result<Value> {
    deploy_with_events(db_id, items, operator_id, operator_name, None).await
}

/// 流式部署：把后端每个阶段的提示、进度、错误、最终结果推给 SSE。
pub async fn deploy_stream(
    db_id: &str,
    items: &[Value],
    operator_id: &str,
    operator_name: &str,
    tx: &tokio::sync::mpsc::UnboundedSender<InitEvent>,
) {
    match deploy_with_events(db_id, items, operator_id, operator_name, Some(tx)).await {
        Ok(v) => {
            let _ = tx.send(ev(
                "done",
                json!({
                    "message": "部署执行完成",
                    "results": v.get("results").cloned().unwrap_or_else(|| json!([])),
                    "batch_id": v.get("batch_id").cloned().unwrap_or(Value::Null),
                    "db_state": v.get("db_state").cloned().unwrap_or(Value::Null),
                }),
            ));
        }
        Err(e) => {
            let _ = tx.send(ev(
                "error",
                json!({ "stage": "deploy", "message": e.to_string() }),
            ));
        }
    }
}

/// 流式生成部署计划：编译定义并汇总将处理的表，不执行 DDL、不写台账。
pub async fn deploy_plan_stream(
    db_id: &str,
    items: &[Value],
    tx: &tokio::sync::mpsc::UnboundedSender<InitEvent>,
) {
    let send = |kind: &str, data: Value| {
        let _ = tx.send(ev(kind, data));
    };
    send(
        "connect",
        json!({ "message": format!("检查目标数据库 {db_id} 的模型中心状态 …"), "db_id": db_id }),
    );
    match read_meta(db_id).await {
        Ok(Some(_)) => send(
            "connect",
            json!({ "ok": true, "message": "模型中心已初始化，可生成部署计划" }),
        ),
        Ok(None) => {
            send(
                "error",
                json!({ "stage": "state", "message": "数据库尚未初始化，请先初始化模型中心" }),
            );
            return;
        }
        Err(e) => {
            send(
                "error",
                json!({ "stage": "state", "message": format!("读取模型中心状态失败: {e}") }),
            );
            return;
        }
    }

    send(
        "step",
        json!({ "message": format!("开始生成 {} 个模块定义的部署计划", items.len()), "total": items.len() }),
    );
    let mut results = Vec::new();
    let mut total_tables = 0usize;
    for (idx, it) in items.iter().enumerate() {
        let kind = it
            .get("kind")
            .and_then(|v| v.as_str())
            .unwrap_or("DCT")
            .to_uppercase();
        let domain = it.get("domain").and_then(|v| v.as_str()).unwrap_or("");
        let app = it
            .get("application")
            .or_else(|| it.get("app"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let module = it.get("module").and_then(|v| v.as_str()).unwrap_or("");
        let file = it.get("file").and_then(|v| v.as_str()).unwrap_or("");
        send(
            "step",
            json!({
                "message": format!("[{}/{}] 分析 {}/{}/{} · {} · {}", idx + 1, items.len(), domain, app, module, kind, file),
                "index": idx + 1,
                "total": items.len(),
                "module": module,
                "kind": kind,
                "file": file,
            }),
        );
        if kind == "SEED" {
            send(
                "progress",
                json!({ "message": format!("{module} · SEED 暂未实现，计划中将跳过"), "module": module, "kind": kind, "status": "skipped" }),
            );
            results.push(json!({ "module": module, "kind": kind, "status": "skipped", "note": "SEED 暂未实现" }));
            continue;
        }
        let (defs, src) = match compile_definition(&kind, domain, app, module, file).await {
            Ok(x) => x,
            Err(e) => {
                send(
                    "error",
                    json!({ "stage": "compile", "module": module, "kind": kind, "message": e.to_string() }),
                );
                results.push(json!({ "module": module, "kind": kind, "status": "failed", "error": e.to_string() }));
                continue;
            }
        };
        let version = src
            .get("docMeta")
            .or_else(|| src.get("dctMeta"))
            .or_else(|| src.get("seedMeta"))
            .and_then(|m| m.get("version"))
            .and_then(|v| v.as_i64())
            .unwrap_or(1);
        total_tables += defs.len();
        let mut changes = Vec::new();
        let mut inspect_error: Option<String> = None;
        for def in &defs {
            match table_change_plan(db_id, def).await {
                Ok(ch) => changes.push(ch),
                Err(e) => {
                    let msg = e.to_string();
                    send(
                        "error",
                        json!({ "stage": "inspect", "module": module, "kind": kind, "table": def.table_name, "message": msg }),
                    );
                    inspect_error = Some(msg);
                    break;
                }
            }
        }
        if let Some(e) = inspect_error {
            results.push(json!({ "module": module, "kind": kind, "status": "failed", "error": e }));
            continue;
        }
        send(
            "progress",
            json!({
                "message": format!("{module} · {kind} 计划建表/升级 {} 张表，定义版本 v{}", defs.len(), version),
                "module": module,
                "kind": kind,
                "version": version,
                "tables": defs.len(),
                "changes": changes,
            }),
        );
        results.push(json!({
            "module": module,
            "kind": kind,
            "status": "planned",
            "version": version,
            "tables": defs.len(),
            "table_names": defs.iter().map(|d| d.table_name.clone()).collect::<Vec<String>>(),
            "changes": changes,
        }));
    }

    let failed = results
        .iter()
        .filter(|r| r.get("status").and_then(|v| v.as_str()) == Some("failed"))
        .count();
    if failed > 0 {
        send(
            "error",
            json!({ "stage": "plan", "message": format!("计划生成完成，但有 {failed} 项编译失败，请返回调整选择") }),
        );
        return;
    }
    send(
        "done",
        json!({
            "message": "部署执行计划已生成，请审核后确认是否执行",
            "results": results,
            "total": items.len(),
            "tables": total_tables,
            "db_id": db_id,
        }),
    );
}

async fn deploy_with_events(
    db_id: &str,
    items: &[Value],
    operator_id: &str,
    operator_name: &str,
    tx: Option<&tokio::sync::mpsc::UnboundedSender<InitEvent>>,
) -> Result<Value> {
    let send = |kind: &str, data: Value| {
        if let Some(tx) = tx {
            let _ = tx.send(ev(kind, data));
        }
    };

    // 前置：库必须已初始化
    send(
        "connect",
        json!({ "message": format!("检查目标数据库 {db_id} 的模型中心状态 …"), "db_id": db_id }),
    );
    if read_meta(db_id).await?.is_none() {
        return Err(Error::BadRequest(
            "数据库尚未初始化，请先初始化模型中心".into(),
        ));
    }
    let mm = get_default_db_manager();
    let batch_id = snowflake_id_str();
    let mut results = Vec::new();
    send(
        "step",
        json!({ "message": format!("开始部署 {} 个模块定义", items.len()), "batch_id": batch_id, "total": items.len() }),
    );

    for (idx, it) in items.iter().enumerate() {
        let kind = it
            .get("kind")
            .and_then(|v| v.as_str())
            .unwrap_or("DCT")
            .to_uppercase();
        let domain = it.get("domain").and_then(|v| v.as_str()).unwrap_or("");
        let app = it
            .get("application")
            .or_else(|| it.get("app"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let module = it.get("module").and_then(|v| v.as_str()).unwrap_or("");
        let file = it.get("file").and_then(|v| v.as_str()).unwrap_or("");
        send(
            "step",
            json!({
                "message": format!("[{}/{}] 准备部署 {}/{}/{} · {} · {}", idx + 1, items.len(), domain, app, module, kind, file),
                "index": idx + 1,
                "total": items.len(),
                "module": module,
                "kind": kind,
                "file": file,
            }),
        );
        if kind == "SEED" {
            send(
                "progress",
                json!({ "message": format!("{module} · SEED 暂未实现，已跳过"), "module": module, "kind": kind, "status": "skipped" }),
            );
            results.push(json!({ "module": module, "kind": kind, "status": "skipped", "note": "SEED 暂未实现" }));
            continue;
        }
        let started = std::time::Instant::now();
        let hist_id = snowflake_id_str();
        // 历史：pending→executing（对账锚点）
        send(
            "progress",
            json!({ "message": "写入执行历史锚点 …", "module": module, "kind": kind }),
        );
        let _ = mm.execute_sql_with_datavalues(db_id, None,
            "INSERT INTO cmx_model_deploy_history (id, batch_id, db_id, domain_code, application_code, module_code, kind, action, status, def_ref, engine_version, operator_id, operator_name, started_at) VALUES ($1,$2,$3,$4,$5,$6,$7,'create','executing',$8,$9,$10,$11,CURRENT_TIMESTAMP)",
            vec![
                DataValue::String(hist_id.clone()), DataValue::String(batch_id.clone()), DataValue::String(db_id.to_string()),
                DataValue::String(domain.to_string()), DataValue::String(app.to_string()), DataValue::String(module.to_string()),
                DataValue::String(kind.clone()), DataValue::String(file.to_string()), DataValue::String(ENGINE_VERSION.to_string()),
                DataValue::String(operator_id.to_string()), DataValue::String(operator_name.to_string()),
            ]).await;

        // 编译
        send(
            "progress",
            json!({ "message": "编译定义 JSON 为数据库表结构 …", "module": module, "kind": kind, "file": file }),
        );
        let (defs, src) = match compile_definition(&kind, domain, app, module, file).await {
            Ok(x) => x,
            Err(e) => {
                let _ = fail_history(
                    &hist_id,
                    db_id,
                    &e.to_string(),
                    started.elapsed().as_millis() as i64,
                )
                .await;
                send(
                    "error",
                    json!({ "stage": "compile", "module": module, "kind": kind, "message": e.to_string() }),
                );
                results.push(json!({ "module": module, "kind": kind, "status": "failed", "error": e.to_string() }));
                continue;
            }
        };
        let version = src
            .get("docMeta")
            .or_else(|| src.get("dctMeta"))
            .or_else(|| src.get("seedMeta"))
            .and_then(|m| m.get("version"))
            .and_then(|v| v.as_i64())
            .unwrap_or(1);
        let module_name = src
            .get("docMeta")
            .or_else(|| src.get("dctMeta"))
            .or_else(|| src.get("seedMeta"))
            .and_then(|m| m.get("metaName"))
            .and_then(|v| v.as_str())
            .unwrap_or(module)
            .to_string();
        send(
            "progress",
            json!({ "message": format!("编译完成：{} 张表，定义版本 v{}", defs.len(), version), "module": module, "kind": kind, "tables": defs.len(), "version": version }),
        );

        // 建表（DDL 自动提交，txn_id=None；内省 diff，additive-only）
        let executor = PgTableDefineExecutor::new(db_id.to_string(), None);
        let mut created = 0i64;
        let mut changes = Vec::new();
        let mut ddl_err: Option<String> = None;
        for (table_idx, def) in defs.iter().enumerate() {
            let change = match table_change_plan(db_id, def).await {
                Ok(ch) => ch,
                Err(e) => {
                    ddl_err = Some(format!("内省表 {} 失败: {e}", def.table_name));
                    break;
                }
            };
            send(
                "progress",
                json!({
                    "message": format!("[{}/{}] {} {}", table_idx + 1, defs.len(), table_action_label(&change), def.table_name),
                    "module": module,
                    "kind": kind,
                    "table": def.table_name,
                    "index": table_idx + 1,
                    "total": defs.len(),
                    "change": change,
                }),
            );
            if let Err(e) = executor.create_or_upgrade_table(def).await {
                ddl_err = Some(format!("建表 {} 失败: {e}", def.table_name));
                break;
            }
            changes.push(change);
            created += 1;
        }
        if let Some(e) = ddl_err {
            let _ = fail_history(&hist_id, db_id, &e, started.elapsed().as_millis() as i64).await;
            send(
                "error",
                json!({ "stage": "ddl", "module": module, "kind": kind, "message": e }),
            );
            results.push(json!({ "module": module, "kind": kind, "status": "failed", "error": e, "changes": changes }));
            continue;
        }

        // 台账 DML（事务）：源 JSON 留档 + 模块态 UPSERT + 对象台账 + 历史 success
        send(
            "progress",
            json!({ "message": "写入台账事务：源 JSON、模块版本、部署历史 …", "module": module, "kind": kind }),
        );
        let ctx = mm.get_transaction_context();
        let guard = match ctx.begin_with_guard(db_id).await {
            Ok(g) => g,
            Err(e) => {
                results.push(json!({ "module": module, "kind": kind, "status": "failed", "error": format!("事务失败: {e}") }));
                continue;
            }
        };
        let txn = guard.txn_id().to_string();
        let kind_lc = kind.to_lowercase();
        let has_table_define_version = table_exists(db_id, "cmx_meta_table_define_version")
            .await
            .unwrap_or(false);

        let tx_result: Result<()> = async {
            // 4-a 源 JSON 留档：翻旧版 is_current=0，插新版
            mm.execute_sql_with_datavalues(db_id, Some(&txn),
            "UPDATE cmx_model_source SET is_current = 0 WHERE db_id=$1 AND domain_code=$2 AND application_code=$3 AND module_code=$4 AND kind=$5",
            vec![DataValue::String(db_id.into()), DataValue::String(domain.into()), DataValue::String(app.into()), DataValue::String(module.into()), DataValue::String(kind.clone())])
                .await.map_err(db_err("更新源 JSON 当前标记失败"))?;

            let compiled = serde_json::to_value(&defs).unwrap_or(Value::Null);
            mm.execute_sql_with_datavalues(db_id, Some(&txn),
            "INSERT INTO cmx_model_source (id, db_id, domain_code, application_code, module_code, module_name, kind, version, source_file, source_json, compiled_json, table_count, is_current, engine_version, imported_by, imported_name) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10::jsonb,$11::jsonb,$12,1,$13,$14,$15) ON CONFLICT (db_id, app_id, domain_code, application_code, module_code, kind, version) DO UPDATE SET source_json=EXCLUDED.source_json, compiled_json=EXCLUDED.compiled_json, is_current=1, imported_at=CURRENT_TIMESTAMP",
            vec![
                DataValue::String(snowflake_id_str()), DataValue::String(db_id.into()), DataValue::String(domain.into()), DataValue::String(app.into()), DataValue::String(module.into()), DataValue::String(module_name.clone()),
                DataValue::String(kind.clone()), DataValue::String(version.to_string()), DataValue::String(file.into()),
                DataValue::Json(serde_json::to_string(&src).unwrap_or_default()), DataValue::Json(serde_json::to_string(&compiled).unwrap_or_default()),
                DataValue::Int(created), DataValue::String(ENGINE_VERSION.into()), DataValue::String(operator_id.into()), DataValue::String(operator_name.into()),
            ]).await.map_err(db_err("写源 JSON 失败"))?;

            // 4-b 模块态 UPSERT（该 kind 版本+状态=current）
            let set_ver = format!("{kind_lc}_version");
            let set_st = format!("{kind_lc}_status");
            let upsert_sql = format!(
                "INSERT INTO cmx_model_module (id, db_id, domain_code, application_code, module_code, module_name, {set_ver}, {set_st}, table_count, first_deployed_at, current_deployed_at, deployed_by, deployed_name) \
                 VALUES ($1,$2,$3,$4,$5,$6,$7,'current',$8,CURRENT_TIMESTAMP,CURRENT_TIMESTAMP,$9,$10) \
                 ON CONFLICT (db_id, app_id, domain_code, application_code, module_code) DO UPDATE SET {set_ver}=EXCLUDED.{set_ver}, {set_st}='current', table_count=cmx_model_module.table_count, current_deployed_at=CURRENT_TIMESTAMP, deployed_by=EXCLUDED.deployed_by, deployed_name=EXCLUDED.deployed_name, update_time=CURRENT_TIMESTAMP"
            );
            mm.execute_sql_with_datavalues(db_id, Some(&txn), &upsert_sql, vec![
            DataValue::String(snowflake_id_str()), DataValue::String(db_id.into()), DataValue::String(domain.into()), DataValue::String(app.into()), DataValue::String(module.into()), DataValue::String(module_name.clone()),
            DataValue::String(version.to_string()), DataValue::Int(created), DataValue::String(operator_id.into()), DataValue::String(operator_name.into()),
            ]).await.map_err(db_err("写模块台账失败"))?;

            // 4-c 对象台账：主表 + 版本表都需登记(check-then-upsert,避免重复部署累积重复行)
            // 目标库存在核心元数据表时才写;模型中心库只初始化自身台账表,不强依赖此表。
            // app_id 用当前服务隔离标识(与模块安装守卫一致:app_id ≡ module_code)
            let current_app_id = cmx_utils::ConfigManager::global().get_app_id();
            let version_str = version.to_string();
            let has_meta_table = table_exists(db_id, "cmx_meta_table_define")
                .await
                .unwrap_or(false);
            if has_meta_table && has_table_define_version {
                for def in &defs {
                    let meta_json = serde_json::to_string(&serde_json::to_value(def).unwrap_or(Value::Null)).unwrap_or_default();

                    // (1) 主表 cmx_meta_table_define:check-then-upsert(无 table_name 唯一约束)
                    let main_check = mm.query_sql_with_datavalues(db_id, Some(&txn),
                        "SELECT id FROM cmx_meta_table_define WHERE table_name = $1 AND archived = 0",
                        vec![DataValue::String(def.table_name.clone())],
                        "deploy_meta_check")
                        .await
                        .map_err(db_err("查询表定义主表失败"))?;
                    let existing_main_id = serde_json::to_value(&main_check).ok()
                        .and_then(|j| j.get("rows").and_then(|r| r.as_array()).and_then(|rows| rows.first()).cloned())
                        .and_then(|row| row.get("id").and_then(|v| v.as_str()).map(String::from));
                    if let Some(eid) = existing_main_id {
                        // 已存在 → UPDATE
                        mm.execute_sql_with_datavalues(db_id, Some(&txn),
                            "UPDATE cmx_meta_table_define SET display_name = $1, domain_code = $2, application_code = $3, module_code = $4, db_id = $5, version = $6, ddl_status = 'completed', update_time = CURRENT_TIMESTAMP WHERE id = $7",
                            vec![
                                DataValue::String(def.display_name.clone()),
                                DataValue::String(domain.into()), DataValue::String(app.into()), DataValue::String(module.into()),
                                DataValue::String(db_id.into()), DataValue::String(version_str.clone()),
                                DataValue::String(eid),
                            ]).await.map_err(db_err("更新表定义主表失败"))?;
                    } else {
                        // 不存在 → INSERT(完整 12 列,含 app_id/plugin_id=NULL/archived=0)
                        let main_id = snowflake_id_str();
                        mm.execute_sql_with_datavalues(db_id, Some(&txn),
                            "INSERT INTO cmx_meta_table_define (id, table_name, display_name, db_id, plugin_id, version, app_id, ddl_status, domain_code, application_code, module_code, archived) VALUES ($1,$2,$3,$4,NULL,$5,$6,'completed',$7,$8,$9,0)",
                            vec![
                                DataValue::String(main_id), DataValue::String(def.table_name.clone()), DataValue::String(def.display_name.clone()),
                                DataValue::String(db_id.into()), DataValue::String(version_str.clone()), DataValue::String(current_app_id.clone()),
                                DataValue::String(domain.into()), DataValue::String(app.into()), DataValue::String(module.into()),
                            ]).await.map_err(db_err("写表定义主表失败"))?;
                    }

                    // (2) 版本表 cmx_meta_table_define_version:check-then-upsert
                    //     按 (table_name, version, app_id) 去重,避免重复部署累积重复行
                    let ver_check = mm.query_sql_with_datavalues(db_id, Some(&txn),
                        "SELECT id FROM cmx_meta_table_define_version WHERE table_name = $1 AND version = $2 AND app_id = $3 AND archived = 0",
                        vec![DataValue::String(def.table_name.clone()), DataValue::String(version_str.clone()), DataValue::String(current_app_id.clone())],
                        "deploy_meta_ver_check")
                        .await
                        .map_err(db_err("查询表定义版本表失败"))?;
                    let existing_ver_id = serde_json::to_value(&ver_check).ok()
                        .and_then(|j| j.get("rows").and_then(|r| r.as_array()).and_then(|rows| rows.first()).cloned())
                        .and_then(|row| row.get("id").and_then(|v| v.as_str()).map(String::from));
                    if let Some(vid) = existing_ver_id {
                        // 已存在 → UPDATE metadata + 基础字段
                        mm.execute_sql_with_datavalues(db_id, Some(&txn),
                            "UPDATE cmx_meta_table_define_version SET display_name = $1, db_id = $2, domain_code = $3, application_code = $4, module_code = $5, metadata = $6::jsonb, update_time = CURRENT_TIMESTAMP WHERE id = $7",
                            vec![
                                DataValue::String(def.display_name.clone()),
                                DataValue::String(db_id.into()),
                                DataValue::String(domain.into()), DataValue::String(app.into()), DataValue::String(module.into()),
                                DataValue::Json(meta_json),
                                DataValue::String(vid),
                            ]).await.map_err(db_err("更新表定义版本快照失败"))?;
                    } else {
                        // 不存在 → INSERT 完整 12 列
                        let vid = snowflake_id_str();
                        mm.execute_sql_with_datavalues(db_id, Some(&txn),
                            "INSERT INTO cmx_meta_table_define_version (id, table_name, display_name, db_id, plugin_id, version, app_id, domain_code, application_code, module_code, metadata, archived) VALUES ($1,$2,$3,$4,NULL,$5,$6,$7,$8,$9,$10::jsonb,0)",
                            vec![
                                DataValue::String(vid), DataValue::String(def.table_name.clone()), DataValue::String(def.display_name.clone()),
                                DataValue::String(db_id.into()), DataValue::String(version_str.clone()), DataValue::String(current_app_id.clone()),
                                DataValue::String(domain.into()), DataValue::String(app.into()), DataValue::String(module.into()),
                                DataValue::Json(meta_json),
                            ]).await.map_err(db_err("写表定义版本快照失败"))?;
                    }
                }
            }

            // 4-d 历史 → success
            mm.execute_sql_with_datavalues(db_id, Some(&txn),
                "UPDATE cmx_model_deploy_history SET status='success', to_version=$2, object_count=$3, ddl_summary=$4::jsonb, finished_at=CURRENT_TIMESTAMP, duration_ms=$5 WHERE id=$1",
                vec![
                    DataValue::String(hist_id.clone()),
                    DataValue::String(version.to_string()),
                    DataValue::Int(created),
                    DataValue::Json(serde_json::to_string(&changes).unwrap_or_else(|_| "[]".to_string())),
                    DataValue::Int(started.elapsed().as_millis() as i64),
                ]
            ).await.map_err(db_err("更新历史失败"))?;
            Ok(())
        }.await;

        if let Err(e) = tx_result {
            let msg = e.to_string();
            let _ = guard.rollback().await;
            let _ = fail_history(&hist_id, db_id, &msg, started.elapsed().as_millis() as i64).await;
            send(
                "error",
                json!({ "stage": "ledger", "module": module, "kind": kind, "message": msg }),
            );
            results
                .push(json!({ "module": module, "kind": kind, "status": "failed", "error": msg }));
            continue;
        }

        guard.commit().await.map_err(db_err("提交部署事务失败"))?;
        send(
            "progress",
            json!({ "message": format!("{module} · {kind} 部署成功：v{version}，{} 张表", created), "module": module, "kind": kind, "status": "success", "version": version, "tables": created, "changes": changes }),
        );
        results.push(json!({ "module": module, "kind": kind, "status": "success", "version": version, "tables": created, "changes": changes }));
    }

    Ok(
        json!({ "ok": true, "batch_id": batch_id, "results": results, "db_state": db_state(db_id).await? }),
    )
}

/// 把某历史行标记为 failed（DDL 失败等）。
async fn fail_history(hist_id: &str, db_id: &str, err: &str, dur_ms: i64) -> Result<()> {
    let mm = get_default_db_manager();
    mm.execute_sql_with_datavalues(db_id, None,
        "UPDATE cmx_model_deploy_history SET status='failed', error_message=$2, finished_at=CURRENT_TIMESTAMP, duration_ms=$3 WHERE id=$1",
        vec![DataValue::String(hist_id.into()), DataValue::String(err.into()), DataValue::Int(dur_ms)]
    ).await.map_err(db_err("写失败历史失败"))?;
    Ok(())
}

// 时间戳保留（供未来 last_sync 等使用）
#[allow(dead_code)]
fn _keep_now() -> String {
    now_iso()
}

// ════════════════════════════════════════════════════════════════════════
//  单元测试：编译器对真实 DCT JSON 的正确性（不依赖数据库）
// ════════════════════════════════════════════════════════════════════════
#[cfg(test)]
mod tests {
    use super::*;

    fn load(p: &str) -> Value {
        let root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../data/meta/definitions");
        serde_json::from_str(&std::fs::read_to_string(root.join(p)).unwrap()).unwrap()
    }

    #[test]
    fn field_type_mapping() {
        assert!(matches!(map_field_type("VARCHAR"), FieldType::String));
        assert!(matches!(map_field_type("varchar"), FieldType::String));
        assert!(matches!(map_field_type("BIGINT"), FieldType::Int));
        assert!(matches!(map_field_type("TINYINT"), FieldType::Int));
        assert!(matches!(map_field_type("DECIMAL"), FieldType::Decimal));
        assert!(matches!(map_field_type("DATE"), FieldType::Date));
    }

    #[test]
    fn compile_real_dct_cf_client() {
        let doc = load("fi/cmxfico/gl/cmxfico_dct_meta_v1.json");
        let base = load("base/base_dct_meta_v1.json");
        let defs = compile_dct(&doc, &base);
        assert!(!defs.is_empty(), "应编译出多张字典表");

        // 找 cf_client（第一张：3 本表字段 + 4 base + 4 audit + 1 system）
        let t = defs
            .iter()
            .find(|d| d.table_name == "cf_client")
            .expect("应有 cf_client");
        let cols: Vec<&str> = t.columns.iter().map(|c| c.name.as_str()).collect();
        // 本表字段
        for c in ["logical_system", "system_id", "client_role"] {
            assert!(cols.contains(&c), "缺列 {c}: {:?}", cols);
        }
        // base 字段集展开
        for c in ["code", "name", "sort_no", "status"] {
            assert!(cols.contains(&c), "缺 base 列 {c}: {:?}", cols);
        }
        // audit + system 字段集
        assert!(
            cols.contains(&"create_by") && cols.contains(&"is_system"),
            "缺 audit/system 列: {:?}",
            cols
        );
        // 唯一键 uniqueKeys=[["code"]]
        assert!(
            t.indexes
                .iter()
                .any(|i| i.columns == vec!["code".to_string()]
                    && matches!(i.kind, IndexKind::Unique)),
            "应有 code 唯一索引"
        );
        // 无重复列
        let mut seen = std::collections::HashSet::new();
        for c in &t.columns {
            assert!(seen.insert(c.name.clone()), "列 {} 重复", c.name);
        }
    }

    #[test]
    fn compile_all_tables_have_name_and_columns() {
        let doc = load("fi/cmxfico/gl/cmxfico_dct_meta_v1.json");
        let base = load("base/base_dct_meta_v1.json");
        let defs = compile_dct(&doc, &base);
        for d in &defs {
            assert!(!d.table_name.is_empty(), "表名不能为空");
            assert!(!d.columns.is_empty(), "表 {} 应有列", d.table_name);
        }
        // DOC 也能编译
        let doc2 = load("fi/cmxfico/gl/cmxfico_doc_meta_v1.json");
        let base2 = load("base/base_doc_meta_v1.json");
        let vdefs = compile_doc(&doc2, &base2);
        assert!(!vdefs.is_empty(), "DOC 应编译出单据表");
        for d in &vdefs {
            assert!(!d.columns.is_empty(), "DOC 表 {} 应有列", d.table_name);
        }
    }

    #[test]
    fn compile_doc_includes_summary_tables() {
        let doc = load("fi/cmxfico/gl/cmxfico_doc_meta_v1.json");
        let base = load("base/base_doc_meta_v1.json");
        let defs = compile_doc(&doc, &base);
        let main_count = doc
            .get("voucherTables")
            .and_then(|v| v.as_array())
            .map(|a| a.len())
            .unwrap_or(0);
        assert!(
            defs.len() > main_count,
            "DOC 编译应包含 voucherTables 下的 summaries 汇总表"
        );
        let sum = defs
            .iter()
            .find(|d| d.table_name == "cv_aux_line_sum")
            .expect("应编译出凭证辅助核算汇总表 cv_aux_line_sum");
        let cols: Vec<&str> = sum.columns.iter().map(|c| c.name.as_str()).collect();
        assert!(cols.contains(&"id"), "汇总表应包含主键 id");
        assert!(cols.contains(&"upper_id"), "汇总表应包含上级主键 upper_id");
        assert!(
            sum.primary_keys == vec!["id".to_string()],
            "汇总表 id 应作为主键"
        );
    }

    // ── diff_table_to_report：复用 DdlDiff 引擎后的假阳性修复 ──────────────

    /// 构造最小可用的 ColumnDefine（仅 name/field_type/nullable，其余默认）。
    fn mk_col(name: &str, ft: FieldType, nullable: bool) -> ColumnDefine {
        ColumnDefine {
            name: name.to_string(),
            label: name.to_string(),
            field_type: ft,
            is_primary_key: false,
            is_nullable: nullable,
            default_value: None,
            i18n: false,
            length: None,
            precision: None,
            scale: None,
            db_type: None,
            ordinal: None,
            create_time: None,
            update_time: None,
            is_foreign_key: false,
            foreign_key_table: None,
            foreign_key_column: None,
            extensions: std::collections::HashMap::new(),
        }
    }

    /// 构造最小可用的 TableDefine。
    fn mk_table(name: &str, cols: Vec<ColumnDefine>) -> TableDefine {
        TableDefine {
            table_name: name.to_string(),
            display_name: name.to_string(),
            columns: cols,
            primary_keys: vec![],
            indexes: vec![],
            version: 1,
            create_time: None,
            update_time: None,
            i18n: false,
            comment: None,
            schema: None,
            tablespace: None,
            is_partitioned: false,
            partition_type: None,
            partition_columns: vec![],
            extensions: std::collections::HashMap::new(),
        }
    }

    /// 核心场景：PG 内省还原的 bigint 列（带派生 precision=64/scale=0、db_type=BIGINT）
    /// 与设计期 Int 列（precision/scale=None）对比，不应再误报「升级表/修改列」。
    /// 这是用户报告的 `cf_client` sort_no/status/create_by 等字段的典型情况。
    #[test]
    fn diff_table_report_no_false_positive_for_bigint() {
        // 模拟 query_current_table_define 还原出的 bigint 列
        let mut db_sort_no = mk_col("sort_no", FieldType::Int, true);
        db_sort_no.precision = Some(64);
        db_sort_no.scale = Some(0);
        db_sort_no.db_type = Some("BIGINT".to_string());
        let mut db_create_time = mk_col("create_time", FieldType::DateTime, true);
        db_create_time.db_type = Some("TIMESTAMP WITH TIME ZONE".to_string());

        let current = mk_table(
            "cf_client",
            vec![db_sort_no, db_create_time],
        );
        // 设计期定义：同类型，但 precision/scale/db_type 未设置
        let desired = mk_table(
            "cf_client",
            vec![
                mk_col("sort_no", FieldType::Int, true),
                mk_col("create_time", FieldType::DateTime, true),
            ],
        );

        let report = diff_table_to_report(&current, &desired);
        assert_eq!(
            report["action"].as_str(),
            Some("no_change"),
            "bigint/timestamptz 与设计期 Int/DateTime 不应报变更: {report}"
        );
        assert!(
            report["modifiedColumns"].as_array().unwrap().is_empty(),
            "modifiedColumns 应为空: {report}"
        );
        assert_eq!(
            report["unchangedColumns"]
                .as_array()
                .map(|a| a.len())
                .unwrap_or(0),
            2,
            "两列都应归入 unchangedColumns: {report}"
        );
    }

    /// 真实差异：设计期新增一列，应报 upgrade_table + addedColumns。
    #[test]
    fn diff_table_report_detects_added_column() {
        let current = mk_table("t", vec![mk_col("id", FieldType::Int, false)]);
        let desired = mk_table(
            "t",
            vec![
                mk_col("id", FieldType::Int, false),
                mk_col("name", FieldType::String, true),
            ],
        );
        let report = diff_table_to_report(&current, &desired);
        assert_eq!(report["action"].as_str(), Some("upgrade_table"));
        let added = report["addedColumns"].as_array().unwrap();
        assert_eq!(added.len(), 1, "应报 1 个新增列: {report}");
        assert_eq!(added[0]["name"].as_str(), Some("name"));
        // dataType 应使用建表基准（String → VARCHAR/TEXT），不再是旧的 "character varying"
        assert!(
            added[0]["dataType"]
                .as_str()
                .map(|s| s != "character varying" && !s.is_empty())
                .unwrap_or(false),
            "新增列 dataType 应为建表基准类型而非 information_schema 小写名: {report}"
        );
    }

    /// 真实差异：列类型 Int → Decimal，应报 upgrade_table + modifiedColumns。
    #[test]
    fn diff_table_report_detects_type_change() {
        let current = mk_table("t", vec![mk_col("amount", FieldType::Int, true)]);
        let desired = mk_table("t", vec![mk_col("amount", FieldType::Decimal, true)]);
        let report = diff_table_to_report(&current, &desired);
        assert_eq!(report["action"].as_str(), Some("upgrade_table"));
        let modified = report["modifiedColumns"].as_array().unwrap();
        assert_eq!(modified.len(), 1, "应报 1 个修改列: {report}");
        let changes = modified[0]["changes"].as_array().unwrap();
        assert!(
            changes
                .iter()
                .any(|c| c["field"].as_str() == Some("dataType")),
            "修改明细应含 dataType 变更: {report}"
        );
    }

    /// nullable 变更应被检出（如 NOT NULL → NULL）。
    #[test]
    fn diff_table_report_detects_nullable_change() {
        let current = mk_table("t", vec![mk_col("code", FieldType::String, false)]);
        let desired = mk_table("t", vec![mk_col("code", FieldType::String, true)]);
        let report = diff_table_to_report(&current, &desired);
        assert_eq!(report["action"].as_str(), Some("upgrade_table"));
        assert!(
            !report["modifiedColumns"]
                .as_array()
                .unwrap()
                .is_empty(),
            "nullable 变更应报修改列: {report}"
        );
    }
}
