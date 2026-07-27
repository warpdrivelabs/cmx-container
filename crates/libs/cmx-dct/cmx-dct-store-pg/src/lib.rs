//! cmx-dct-store-pg —— 数据字典（DCT）模块的 PostgreSQL 持久化/服务层。
//!
//! - `resolve_dict`：从定义 JSON 解析目标字典表 → 强类型 `DictView`（合并 base 字段集 +
//!   构建/缓存落库校验规范 TableSpec）。
//! - `resolve_db_id`：字典操作的 db_id 路由（显式 header 优先，缺失回退业务库）。
//! - 服务函数：`search`（分页读）/ `search_zmc`（零拷贝列式二进制）/ `upsert`（merge 回存 +
//!   铸号 + 列校验）/ `delete` / `save`（changeset 事务回存：inserted 铸号 / updated 乐观锁 /
//!   deleted）。每个返回纯数据 / 语义化结果枚举，HTTP 信封由 cmx-dct-api 薄 handler 包装。
//!
//! SQL 全部来自 cmx-dct-model；本层接 cmx-database-pg 全局 manager 执行 + 事务编排。

use serde_json::{Value, json};

use cmx_api_types::Result;
use cmx_core::model::cell::DataValue;
use cmx_database_pg::{DatabaseManager, Error as DbError, get_default_pg_db_manager};

use cmx_dct_model::{
    DctQuery, DictColumn, DictView, SERVER_FILLED_COLS, SERVER_REPLACED_COLS, base_fieldset,
    build_search_sql, build_upsert_sql_dv, is_server_managed_col,
    mint_ids_for_inserts, pk_is_generated, row_fields, to_dv_by_col, valid_col,
};

// ============================================================================
// 错误助手（与迁移前完全一致的 HTTP 语义）
// ============================================================================

/// 普通业务错误 → cmx_api_types::Error（BusinessError，code!=0/HTTP 200）。
pub fn api_err(msg: &str) -> cmx_api_types::Error {
    cmx_biz::BizError::business(msg.to_string()).into()
}

/// DB 原始错误 → 已翻译的优雅错误（稳定错误码 + 中文），不再暴露 PG 英文原文。
pub fn api_err_db(raw: &str) -> cmx_api_types::Error {
    cmx_biz::BizError::from_db_error(raw).into()
}

/// 从 `cmx_database_pg::Error` 抽出 **PostgreSQL 真实错误明细**（SQLSTATE 文案 + DETAIL + 约束名）。
///
/// 背景：tokio-postgres 的 `Error` 顶层 `Display` 恒为无信息的 `db error`——真正的
/// message/detail/constraint 藏在 `as_db_error()` 里。若直接 `format!("{e}")` 会把
/// 「唯一键冲突」这类可翻译错误塌缩成 `db error`，前端无从判断。
///
/// 把三段拼成一个完整串，交给 [`cmx_biz::BizError::from_db_error`] 归类成
/// `CmxErrCode` + 优雅中文。拼接保证含 `unique constraint "..."` / `foreign key` 等稳定
/// 子串，令 `classify_db_error` 命中；`brief_db_detail` 再从中抽约束名脱敏展示。
/// 非 PG 错误（连接/池/事务）回退顶层 Display。
///
/// 与 `cmx-rpt-store-pg::pg_detail` 实现一致，保持 DCT/RPT 落库错误翻译口径统一。
pub(crate) fn pg_detail(e: &DbError) -> String {
    if let DbError::Postgres(pg) = e
        && let Some(db) = pg.as_db_error()
    {
        let mut s = db.message().to_string();
        if let Some(d) = db.detail() {
            s.push(' ');
            s.push_str(d);
        }
        if let Some(c) = db.constraint() {
            s.push_str(&format!(" constraint \"{c}\""));
        }
        return s;
    }
    e.to_string()
}

/// 统一包装 save 路径（upsert/delete/save_apply）的 DB 执行错误：翻译为优雅错误（稳定码 +
/// 中文）+ 结构化日志。
///
/// - `error` 级：只留阶段（phase）+ dict_code + table + row_index + 真实 PG 明细（pg_detail
///   抽出的 message/detail/constraint），便于日志侧反查；不打印 SQL 全文（避免日志膨胀）。
/// - `debug` 级：记录失败 SQL 全文，排查时按需开启。
///
/// 仅用于**原本就走 api_err_db** 的 save 路径（语义不变，只是统一日志结构 + 抽真实 PG 错误）。
/// search/count/search_zmc 路径仍用 api_err（business 错误），不在本函数范围。
///
/// # Arguments
///
/// - `e`：cmx_database_pg 错误（必传具体类型，以便 `as_db_error()` 取 PG 明细）
/// - `phase`：阶段标签（`upsert` / `insert` / `update` / `delete` / `select_parent_id` / `recompute_is_leaf` / `export` / `import_*`）
/// - `view`：字典表视图
/// - `row_index`：行索引（删除/插入单行用 `Some(i)`，批/不限用 `None`）
/// - `sql`：执行的 SQL（仅 debug 级别打印）
fn map_db_err(
    e: DbError,
    phase: &str,
    view: &DictView,
    row_index: Option<usize>,
    sql: &str,
) -> cmx_api_types::Error {
    let detail = pg_detail(&e);
    tracing::error!(
        target: "cmx_dct::db",
        phase = phase,
        dict_code = %view.dict_code,
        table = %view.table_name,
        row_index = ?row_index,
        error = %e,
        pg_detail = %detail,
        sql = sql,
        "db exec failed"
    );
    api_err_db(&detail)
}

// ============================================================================
// db_id 路由
// ============================================================================

/// 解析字典操作的 db_id：前端显式传 `db_id` header 时用它，缺失时回退到业务库（source_type=biz）。
/// 字典数据通常建在业务库（如 fico-db），而非默认的主控库（primary）。
/// 前端字典兜底数据源（cmx-dict-select 的 createRestDictDataSource）不带 db_id，
/// 这里经 get_biz_db_id() 自动路由到业务库，免去前端手填。
pub async fn resolve_db_id(db_id_header: Option<&str>) -> String {
    if let Some(v) = db_id_header {
        let s = v.trim();
        if !s.is_empty() {
            return s.to_string();
        }
    }
    get_default_pg_db_manager().get_biz_db_id().await
}

// ============================================================================
// 元数据解析：从定义 JSON 找到目标字典表 + 合并列
// ============================================================================

// ----------------------------------------------------------------------------
// resolve_dict 的子函数（纯重构拆分，对外仅 resolve_dict 一个 pub 入口）
// ----------------------------------------------------------------------------

/// file 自动解析 + doc 加载 + base 字段集加载。
///
/// file 缺失时由 `resolve_dict_file` 在 domain/app/module 下扫描含 dictCode 的 DCT 文件
/// （前端运行时只持 dictCode + domain/app/module，无 file 坐标）。返回 (doc, base, file)。
async fn resolve_doc(q: &DctQuery) -> Result<(Value, Value, String)> {
    // file 缺失时自动解析：在该 domain/app/module 下扫描含 dictCode 的 DCT 文件。
    // 前端运行时只持有 dictCode + domain/app/module（host 无 file 坐标），故 file 由后端兜底。
    let file = match &q.file {
        Some(f) if !f.is_empty() => f.clone(),
        _ => {
            cmx_model::definitions::resolve::resolve_dict_file(
                &q.domain,
                &q.application,
                &q.module,
                &q.dict,
            )
            .await?
        }
    };
    let doc_ref = cmx_model::definitions::store::DefRef {
        domain: Some(q.domain.clone()),
        application: Some(q.application.clone()),
        app: Some(q.application.clone()),
        module: Some(q.module.clone()),
        file: Some(file.clone()),
        id: None,
        kind: None,
    };
    let doc = cmx_model::definitions::store::get_definition(&doc_ref).await?;
    let base = load_base(&doc).await;
    Ok((doc, base, file))
}

/// 合并列：own fields + 全部 *FieldSet 引用（按 fieldSetOrder 或默认序）。
///
/// 返回 (columns, raw_fields)：columns 是去重保序的 DictColumn；raw_fields 是合并后的
/// 原始字段（带 fieldLength/decimalDigits），供构建校验规范 TableSpec。
fn merge_columns(t: &Value, base: &Value, with_props: bool) -> (Vec<DictColumn>, Vec<Value>) {
    // 合并列：own fields + 全部 *FieldSet 引用（与 compile_dct 对齐）。
    let mut columns: Vec<DictColumn> = Vec::new();
    // 合并后的原始字段（带 fieldLength/decimalDigits），供构建校验规范 TableSpec。
    let mut raw_fields: Vec<Value> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let push = |fields: &Vec<Value>,
                columns: &mut Vec<DictColumn>,
                raw_fields: &mut Vec<Value>,
                seen: &mut std::collections::HashSet<String>,
                with_props: bool| {
        for f in fields {
            let name = match f.get("name").and_then(|v| v.as_str()) {
                Some(n) if !n.is_empty() => n.to_string(),
                _ => continue,
            };
            if !seen.insert(name.clone()) {
                continue;
            }
            raw_fields.push(f.clone());
            let caption = f
                .get("caption")
                .and_then(|c| {
                    c.get("zh_CN")
                        .and_then(|v| v.as_str())
                        .or_else(|| c.as_str())
                })
                .unwrap_or(&name)
                .to_string();
            // 录入控件/编辑设置/显示属性/维度类型/字典引用/物理字段：原样透传，
            // 供前端 DCT→列模型转换时派生 cmx-dict-select 录入控件与字典回显。
            let edit = f.get("edit").filter(|v| v.is_object()).cloned();
            let edit_settings = f.get("editSettings").filter(|v| v.is_object()).cloned();
            let display = f.get("display").filter(|v| v.is_object()).cloned();
            // 扁平属性（字段定义顶层键）：仅在 with_props=true 时收集，按白名单取规范键
            // （field-edit-display-modes.md §四 所列：列布局/基本/约束/治理的扁平键）。
            // handler 投影时铺到列对象顶层，与字段定义 JSON 存储形态一致，前端可直接展开。
            let extra = if with_props {
                let mut x = serde_json::Map::new();
                for k in [
                    "width", "frozen", "visible", "required", "align", "intDigits", "decimalDigits",
                    "pattern", "enumValues", "defaultValue", "agg", "unique", "maxlength", "min",
                    "max", "placeholder", "label", "i18n", "searchable", "filterable", "sensitive",
                ] {
                    if let Some(v) = f.get(k) {
                        x.insert(k.to_string(), v.clone());
                    }
                }
                if x.is_empty() {
                    None
                } else {
                    Some(Value::Object(x))
                }
            } else {
                None
            };
            columns.push(DictColumn {
                caption,
                data_type: f
                    .get("dataType")
                    .and_then(|v| v.as_str())
                    .unwrap_or("VARCHAR")
                    .to_string(),
                is_pk: f
                    .get("isPrimaryKey")
                    .and_then(|v| v.as_i64())
                    .map(|n| n != 0)
                    .unwrap_or(false),
                nullable: f.get("nullable").and_then(|v| v.as_bool()).unwrap_or(true),
                dim_type: f
                    .get("dimType")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                ref_dict: f
                    .get("refDict")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                display_field: f
                    .get("displayField")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                ref_field: f
                    .get("refField")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                physical_field: f
                    .get("physicalField")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                edit,
                edit_settings,
                display,
                extra,
                name,
            });
        }
    };
    // 分组（段）合并顺序：若本表声明了 fieldSetOrder（设计期「分组排序」产出），按它决定
    // 各组先后——'own' = 本表 fields，其余 = 引用字段集名（按名从 base 取字段）。
    // 无 fieldSetOrder 时默认「本表 fields 在前 → 各 *FieldSet 引用按固定键序」，向后兼容。
    // push 闭包内置 seen 去重，故段顺序变化不会导致同名字段重复。
    let own_fields: Option<&Vec<Value>> = t.get("fields").and_then(|v| v.as_array());
    // 收集本表声明的引用字段集名（按固定键序，去重），供默认顺序与兜底补尾使用。
    let declared_sets: Vec<String> = {
        let mut out = Vec::new();
        if let Some(obj) = t.as_object() {
            for key in [
                "baseFieldSet",
                "hierarchyFieldSet",
                "scopeFieldSet",
                "effectiveFieldSet",
                "disableFieldSet",
                "auditFieldSet",
                "systemFieldSet",
            ] {
                if let Some(set_name) = obj.get(key).and_then(|v| v.as_str()) {
                    let s = set_name.to_string();
                    if !out.contains(&s) {
                        out.push(s);
                    }
                }
            }
        }
        out
    };

    let have_own = own_fields.is_some();
    let ordered_segs: Vec<String> = if let Some(order) = t.get("fieldSetOrder").and_then(|v| v.as_array()) {
        // 仅保留实际存在的段：'own'（本表有 fields 时）或已声明的引用字段集名。悬空项忽略。
        let have_sets: std::collections::HashSet<&String> = declared_sets.iter().collect();
        let mut segs: Vec<String> = order
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .filter(|s| s == "own" && have_own || have_sets.contains(s))
            .collect();
        // 去重（保首个位置）。
        let mut seen_seg = std::collections::HashSet::new();
        segs.retain(|s| seen_seg.insert(s.clone()));
        if segs.is_empty() {
            // fieldSetOrder 全悬空 → 回退默认。
            let mut def: Vec<String> = declared_sets.clone();
            if have_own {
                def.push("own".to_string());
            }
            def
        } else {
            // 清单外漏掉的段按默认相对序补尾（不丢分组）。
            let used: std::collections::HashSet<String> = segs.iter().cloned().collect();
            for s in &declared_sets {
                if !used.contains(s) {
                    segs.push(s.clone());
                }
            }
            if have_own && !used.contains("own") {
                segs.push("own".to_string());
            }
            segs
        }
    } else {
        // 无 fieldSetOrder：默认「引用组在前（固定键序）→ 本表组在后」。
        // 注：历史上此处是「本表 fields 先 push、再引用」，与下方 reorder_columns 配合产出
        // Common 头/Audit 尾的显示序。为保持向后兼容，默认段序仍按 [引用..., own]，
        // 但实际 push 顺序见下方——以 fieldSetOrder 是否存在分两路，确保旧表行为不变。
        let mut def = declared_sets.clone();
        if have_own {
            def.push("own".to_string());
        }
        def
    };

    if t.get("fieldSetOrder").and_then(|v| v.as_array()).is_some() {
        // 自定义段序：严格按 ordered_segs push（own → 本表 fields；其余 → base 字段集）。
        for seg in &ordered_segs {
            if seg == "own" {
                if let Some(own) = own_fields {
                    push(own, &mut columns, &mut raw_fields, &mut seen, with_props);
                }
            } else if let Some(fields) = base_fieldset(base, seg) {
                push(fields, &mut columns, &mut raw_fields, &mut seen, with_props);
            }
        }
    } else {
        // 默认（向后兼容）：本表 fields 在前 → 各引用按固定键序。
        if let Some(own) = own_fields {
            push(own, &mut columns, &mut raw_fields, &mut seen, with_props);
        }
        for set_name in &declared_sets {
            if let Some(fields) = base_fieldset(base, set_name) {
                push(fields, &mut columns, &mut raw_fields, &mut seen, with_props);
            }
        }
    }

    // 显示列序：Common 字段集（baseFieldSet）在前、Audit 字段集（auditFieldSet）置尾，
    // 其余居中保持合并相对顺序。仅影响 /dct/meta 投影，不影响物理表 DDL。
    reorder_columns(&mut columns, base, t);

    (columns, raw_fields)
}

/// 解析主键列名并标记 columns 中的 pk 列。
///
/// 优先序：isPrimaryKey 标记列 → idField（若存在于列中）→ codeField。
/// 返回 (pk, id_field, code_field)。
fn resolve_pk(dm: &Value, columns: &mut [DictColumn]) -> (String, String, String) {
    // 主键：优先 isPrimaryKey 标记列；否则 idField（若存在于列中）；再否则 codeField。
    let id_field = dm
        .get("idField")
        .and_then(|v| v.as_str())
        .unwrap_or("id")
        .to_string();
    let code_field = dm
        .get("codeField")
        .and_then(|v| v.as_str())
        .unwrap_or("code")
        .to_string();
    let pk = columns
        .iter()
        .find(|c| c.is_pk)
        .map(|c| c.name.clone())
        .or_else(|| {
            columns
                .iter()
                .find(|c| c.name == id_field)
                .map(|c| c.name.clone())
        })
        .unwrap_or_else(|| code_field.clone());
    // 标记 pk 列（供元数据投影）。
    for c in columns.iter_mut() {
        if c.name == pk {
            c.is_pk = true;
        }
    }
    (pk, id_field, code_field)
}

/// 落库前列级校验规范：查缓存，未命中则从 raw_fields 构建并缓存。
///
/// 缓存键含 version，定义改版本即换键，旧条目自然作废（免主动失效）。
fn resolve_or_build_spec(
    q: &DctQuery,
    file: &str,
    table_name: &str,
    pk: &str,
    raw_fields: &[Value],
    version: u64,
) -> std::sync::Arc<cmx_biz::validation::TableSpec> {
    // 落库前列级校验规范：从合并后的原始字段构建 TableSpec，进程内缓存（键含版本，免失效）。
    let spec_key = cmx_biz::validation::spec_key(
        &q.domain,
        &q.application,
        &q.module,
        file,
        table_name,
        version,
    );
    match cmx_biz::validation::get_spec(&spec_key) {
        Some(s) => s,
        None => {
            let built = std::sync::Arc::new(cmx_biz::validation::build_table_spec(
                table_name.to_string(),
                pk,
                raw_fields,
            ));
            cmx_biz::validation::put_spec(spec_key, built.clone());
            built
        }
    }
}

/// 解析 `DctQuery` → 强类型 `DictView`（合并列 + base 字段集 + 校验规范缓存）。
///
/// `with_props`：是否把字段定义里的扁平属性（width/visible/pattern/enumValues/required/
/// intDigits/decimalDigits 等）收集到 `DictColumn.extra`。仅 `/dct/meta` 在 `with_props=true`
/// 时需要（供前端字典维护页构建完整列模型）；数据装载/回存场景传 false，保持 payload 精简。
pub async fn resolve_dict(q: &DctQuery, with_props: bool) -> Result<DictView> {
    // 1) file 解析 + doc/base 加载。
    let (doc, base, file) = resolve_doc(q).await?;

    // 2) 定位目标字典表定义。
    let tables = doc
        .get("dictionaryTables")
        .and_then(|v| v.as_array())
        .ok_or_else(|| api_err("定义缺少 dictionaryTables"))?;
    let t = tables
        .iter()
        .find(|t| cmx_model::definitions::resolve::dict_matches(t, &q.dict))
        .ok_or_else(|| api_err(&format!("未找到字典 {}", q.dict)))?;
    let dm = t.get("dictMeta").cloned().unwrap_or_else(|| json!({}));
    let table_name = dm
        .get("tableName")
        .and_then(|v| v.as_str())
        .ok_or_else(|| api_err("dictMeta 缺少 tableName"))?
        .to_string();

    // 3) 合并列（own + *FieldSet 引用，按 fieldSetOrder 或默认序）+ 显示序重排。
    let (mut columns, raw_fields) = merge_columns(t, &base, with_props);

    // 4) 解析主键列 + 标记。
    let (pk, id_field, code_field) = resolve_pk(&dm, &mut columns);

    // 5) 落库校验规范（缓存查/建）。
    let version = doc
        .get("moduleMeta")
        .and_then(|m| m.get("version"))
        .and_then(|v| v.as_u64())
        .or_else(|| doc.get("version").and_then(|v| v.as_u64()))
        .unwrap_or(0);
    let spec = resolve_or_build_spec(q, &file, &table_name, &pk, &raw_fields, version);

    // 6) 组装 DictView。
    Ok(DictView {
        dict_code: dm
            .get("dictCode")
            .and_then(|v| v.as_str())
            .unwrap_or(&q.dict)
            .to_string(),
        dict_name: dm
            .get("dictName")
            .and_then(|v| v.as_str())
            .unwrap_or(&table_name)
            .to_string(),
        self_hierarchy: dm
            .get("selfHierarchy")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        parent_field: dm
            .get("parentField")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        label_field: dm
            .get("labelField")
            .and_then(|v| v.as_str())
            .unwrap_or("name")
            .to_string(),
        table_name,
        id_field,
        code_field,
        columns,
        pk,
        spec,
    })
}

/// 按显示约定重排列顺序：baseFieldSet（Common 字段集）置前、auditFieldSet（Audit 字段集）
/// 置尾，其余列居中保持合并相对顺序。仅影响 `/dct/meta` 投影的显示列序，不影响物理表 DDL
/// 与校验规范（后者按字段名查，与顺序无关）。
fn reorder_columns(columns: &mut Vec<DictColumn>, base: &Value, table_def: &Value) {
    // 已声明 fieldSetOrder 时，合并阶段已按用户自定义段序 push，此处不再做 Common/Audit
    // 三分组重排（否则会打散用户排好的分组顺序）。无 fieldSetOrder 时走原默认逻辑。
    if table_def.get("fieldSetOrder").and_then(|v| v.as_array()).is_some() {
        return;
    }
    /// 取 table_def 上某 `*FieldSet` 引用（值=base 字段集名）的字段名集合。
    fn names_of(base: &Value, table_def: &Value, key: &str) -> std::collections::HashSet<String> {
        table_def
            .get(key)
            .and_then(|v| v.as_str())
            .and_then(|set_name| base_fieldset(base, set_name))
            .map(|fields| {
                fields
                    .iter()
                    .filter_map(|f| f.get("name").and_then(|v| v.as_str()).map(String::from))
                    .collect()
            })
            .unwrap_or_default()
    }
    let common = names_of(base, table_def, "baseFieldSet");
    let audit = names_of(base, table_def, "auditFieldSet");
    if common.is_empty() && audit.is_empty() {
        return;
    }
    // 三分组，组内保持原合并相对顺序（drain 顺序遍历）。
    let (mut head, mut mid, mut tail) = (Vec::new(), Vec::new(), Vec::new());
    for c in columns.drain(..) {
        if common.contains(&c.name) {
            head.push(c);
        } else if audit.contains(&c.name) {
            tail.push(c);
        } else {
            mid.push(c);
        }
    }
    columns.extend(head);
    columns.extend(mid);
    columns.extend(tail);
}

/// 从 baseDctMetaRef.file 读 base 字段集定义（无则空对象）。
async fn load_base(doc: &Value) -> Value {
    let file = doc
        .get("baseDctMetaRef")
        .and_then(|r| r.get("file"))
        .and_then(|v| v.as_str());
    let file = match file {
        Some(f) => f,
        None => return json!({}),
    };
    let base_ref = cmx_model::definitions::store::DefRef {
        domain: Some("base".into()),
        application: None,
        app: None,
        module: None,
        file: Some(file.to_string()),
        id: None,
        kind: None,
    };
    cmx_model::definitions::store::get_definition(&base_ref)
        .await
        .unwrap_or_else(|_| json!({}))
}

// ============================================================================
// 服务函数：装载
// ============================================================================

/// 构造 search 的 (sql, count_sql, params)，附带结构化 debug 日志（脱敏：不打印 raw 全文）。
///
/// `search`（执行 data + count）与 `search_zmc`（只执行 data）共用此前置，
/// 避免 SQL 构造 + 日志重复。zmc 不执行 count（流式列式导出不分页计数）。
fn build_search(view: &DictView, raw: &Value) -> (String, String, Vec<DataValue>) {
    let (sql, count_sql, params) = build_search_sql(view, raw);
    tracing::debug!(
        target: "cmx_dct::search",
        dict_code = %view.dict_code, table = %view.table_name,
        sql_len = sql.len(), params_len = params.len(),
        "build search sql"
    );
    (sql, count_sql, params)
}

/// 装载字典数据（分页 + 计数）。返回 `{rows,total,page,pageSize}`。
pub async fn search(view: &DictView, raw: &Value, db_id: &str) -> Result<Value> {
    let (sql, count_sql, params) = build_search(view, raw);

    let mm = get_default_pg_db_manager();
    let ds = mm
        .query_sql_with_datavalues(
            db_id,
            None,
            &sql,
            params.clone(),
            &view.dict_code,
        )
        .await
        .map_err(|e| {
            tracing::error!(
                target: "cmx_dct::search",
                dict_code = %view.dict_code, table = %view.table_name, error = %e,
                "search data query failed"
            );
            tracing::debug!(target: "cmx_dct::search", sql = %sql, "failed sql");
            api_err(&format!("字典查询失败: {e}"))
        })?;
    let total_ds = mm
        .query_sql_with_datavalues(db_id, None, &count_sql, params, "cnt")
        .await
        .map_err(|e| {
            tracing::error!(
                target: "cmx_dct::search",
                dict_code = %view.dict_code, table = %view.table_name, error = %e,
                "search count query failed"
            );
            tracing::debug!(target: "cmx_dct::search", sql = %count_sql, "failed sql");
            api_err(&format!("字典计数失败: {e}"))
        })?;

    // DataSet → rows JSON。
    let rows_val = serde_json::to_value(&ds).map_err(|e| api_err(&format!("序列化失败: {e}")))?;
    let rows = rows_val.get("rows").cloned().unwrap_or_else(|| json!([]));
    let total = serde_json::to_value(&total_ds)
        .ok()
        .and_then(|v| {
            v.get("rows")
                .and_then(|r| r.get(0))
                .and_then(|r0| r0.get("cnt"))
                .cloned()
        })
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    let (page, page_size) = cmx_dct_model::parse_paging(raw);
    Ok(json!({
        "rows": rows,
        "total": total,
        "page": page,
        "pageSize": page_size,
    }))
}

/// 零拷贝装载：tokio-postgres + ZmcDataSet + 列式二进制。返回列式包字节（handler 包 msgpack 信封）。
pub async fn search_zmc(view: &DictView, raw: &Value, db_id: &str) -> Result<Vec<u8>> {
    let (sql, _count_sql, params) = build_search(view, raw);

    let mm = get_default_pg_db_manager();
    // 零拷贝：ZmcDataSet 持有原始 tokio-postgres Row，惰性列式二进制编码。
    let zmc = mm
        .query_sql_zmc_with_datavalues(db_id, &sql, params, &view.dict_code)
        .await
        .map_err(|e| {
            tracing::error!(
                target: "cmx_dct::search",
                dict_code = %view.dict_code, table = %view.table_name, error = %e,
                "search_zmc query failed"
            );
            tracing::debug!(target: "cmx_dct::search", sql = %sql, "failed sql");
            api_err(&format!("字典零拷贝查询失败: {e}"))
        })?;
    let mut buf = Vec::new();
    zmc.encode_columnar_binary(&mut buf);
    Ok(buf)
}

// ============================================================================
// 服务函数：回存（upsert merge）
// ============================================================================

/// upsert 结果：校验未过（携带 violations）或落库成功（affected + idMap）。
pub enum UpsertOutcome {
    /// 落库前列级校验未通过：结构化 violations（一次回报全部）。
    Invalid(Vec<cmx_biz::errcode::Violation>),
    /// 落库成功：`{count, idMap}`。
    Ok {
        affected: u64,
        id_map: serde_json::Map<String, Value>,
    },
}

/// 回存（upsert，merge 语义）。body：数组或单对象。
pub async fn upsert(view: &DictView, body: Value, db_id: &str) -> Result<UpsertOutcome> {
    // body：数组或单对象。
    let items: Vec<Value> = match body {
        Value::Array(a) => a,
        v => vec![v],
    };
    // 取出可改写的行对象。
    let mut rows: Vec<serde_json::Map<String, Value>> = items
        .into_iter()
        .filter_map(|v| v.as_object().cloned())
        .collect();

    // 主键为服务端生成的 bigint 列时：为「临时 id」行铸真号 + 回填自分级 parent_id。
    // 回传 idMap 供前端把临时行 id 换成真号。NoID(code PK)字典 pk_is_generated=false，跳过铸号。
    let id_map = if pk_is_generated(view) {
        mint_ids_for_inserts(view, &mut rows)
    } else {
        serde_json::Map::new()
    };

    // 落库前列级校验：类型/长度/精度/非空（NOT NULL 跳过服务端 backfill 列）。一次回报全部。
    let vopts = cmx_biz::validation::ValidateOptions::insert(SERVER_FILLED_COLS, SERVER_REPLACED_COLS);
    let mut violations = Vec::new();
    for (i, obj) in rows.iter().enumerate() {
        violations.extend(cmx_biz::validation::validate_insert_row(
            &view.spec,
            obj,
            Some(i),
            &vopts,
        ));
    }
    if !violations.is_empty() {
        return Ok(UpsertOutcome::Invalid(violations));
    }

    let mm = get_default_pg_db_manager();
    let mut affected = 0u64;
    for (i, obj) in rows.iter().enumerate() {
        if let Some((sql, params)) = build_upsert_sql_dv(view, obj) {
            let n = mm
                .execute_sql_with_datavalues(db_id, None, &sql, params)
                .await
                .map_err(|e| map_db_err(e, "upsert", view, Some(i), &sql))?;
            affected += n;
        }
    }

    Ok(UpsertOutcome::Ok { affected, id_map })
}

// ============================================================================
// 服务函数：删除
// ============================================================================

/// 删除一行（按 pk）。返回 `{ok, deleted}`。
pub async fn delete(view: &DictView, id: &str, db_id: &str) -> Result<Value> {
    let sql = cmx_dct_model::build_delete_sql(view);
    // 按 pk 列类型构造 DataValue（整型列的字符串 id 转 Int），走 datavalues 绑定。
    let params = vec![to_dv_by_col(view, &view.pk, &json!(id))];

    let mm = get_default_pg_db_manager();
    let n = mm
        .execute_sql_with_datavalues(db_id, None, &sql, params)
        .await
        .map_err(|e| map_db_err(e, "delete", view, None, &sql))?;

    Ok(json!({ "ok": n > 0, "deleted": n }))
}

// ============================================================================
// 服务函数：changeset 回存（事务）
// ============================================================================

/// save 结果：校验未过 / 乐观锁冲突 / 成功。
pub enum SaveOutcome {
    /// 落库前列级校验未通过：结构化 violations。
    Invalid(Vec<cmx_biz::errcode::Violation>),
    /// 乐观锁冲突（updated baseline 不匹配）→ handler 返回 409。
    Conflict,
    /// 成功：`{affected, updatedAt, idMap}`（handler 再补 ok/mode）。
    Ok {
        affected: u64,
        updated_at: Vec<Value>,
        id_map: serde_json::Map<String, Value>,
    },
}

/// 基于 changeset 的回存（对标 doc 的 ChangeSetCollector/DocSaver）。
/// body: `{ saveMode, changes: { <tableName|dict>: { inserted, updated, deleted } } }`。
/// 事务内执行；updated 带 update_time baseline 做乐观锁（冲突→Conflict）。
pub async fn save(view: &DictView, body: &Value, db_id: &str) -> Result<SaveOutcome> {
    // changes：按 path 分桶。字典是单表，只认与本 dict 的 tableName/dictCode 匹配的那个桶
    // （前端 ChangeSetCollector 的 path 是 root dataset id = dictCode 或 tableName）。
    let changes = body.get("changes").and_then(|v| v.as_object());
    let bucket = changes.and_then(|m| {
        m.get(&view.dict_code)
            .or_else(|| m.get(&view.table_name))
            // 单桶时直接取第一个（前端 root path 可能用别名）
            .or_else(|| m.values().next())
    });
    let bucket = match bucket {
        Some(b) => b,
        None => {
            tracing::info!(
                target: "cmx_dct::save",
                dict_code = %view.dict_code, table = %view.table_name, db_id = db_id,
                "empty_changeset"
            );
            return Ok(SaveOutcome::Ok {
                affected: 0,
                updated_at: Vec::new(),
                id_map: serde_json::Map::new(),
            });
        }
    };
    // 预统计各分支行数，便于日志中区分事务内失败阶段（不打印全字段，避免日志爆炸）。
    let ins_n = bucket
        .get("inserted")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    let upd_n = bucket
        .get("updated")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    let del_n = bucket
        .get("deleted")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    tracing::info!(
        target: "cmx_dct::save",
        dict_code = %view.dict_code, table = %view.table_name,
        pk = %view.pk, db_id = db_id,
        inserted = ins_n, updated = upd_n, deleted = del_n,
        "enter"
    );

    // 落库前列级校验（开事务前，一次回报全部）。inserted 走整行校验（含 NOT NULL，跳过
    // 服务端 backfill 列）；updated 只校验其 fields（不做整表 NOT NULL）。
    let violations = validate_bucket(view, bucket);
    if !violations.is_empty() {
        // 校验未通过：聚合首条违规定位到表+列+行号，便于日志侧反查前端表单字段。
        let first = violations.first();
        tracing::warn!(
            target: "cmx_dct::save",
            dict_code = %view.dict_code, table = %view.table_name,
            count = violations.len(),
            first_row = ?first.and_then(|v| v.row),
            first_column = ?first.and_then(|v| v.column.clone()),
            first_code = first.map(|v| v.code).unwrap_or(""),
            first_message = ?first.map(|v| v.message.as_str()).unwrap_or(""),
            "validation_failed"
        );
        return Ok(SaveOutcome::Invalid(violations));
    }

    let mm = get_default_pg_db_manager();
    let tx = mm.get_transaction_context();
    let txn_id = tx.begin(db_id).await.map_err(|e| {
        tracing::error!(
            target: "cmx_dct::save",
            dict_code = %view.dict_code, table = %view.table_name, db_id = db_id, error = %e,
            "tx_begin_failed"
        );
        api_err(&format!("开启事务失败: {e}"))
    })?;

    let result = save_apply(mm, db_id, &txn_id, view, bucket).await;

    match result {
        Ok((affected, updated_at, conflict, id_map)) => {
            if conflict {
                tracing::warn!(
                    target: "cmx_dct::save",
                    dict_code = %view.dict_code, table = %view.table_name, db_id = db_id,
                    "optimistic_lock_conflict rolling_back=true"
                );
                let _ = tx.rollback(&txn_id).await;
                return Ok(SaveOutcome::Conflict);
            }
            tx.commit(&txn_id).await.map_err(|e| {
                tracing::error!(
                    target: "cmx_dct::save",
                    dict_code = %view.dict_code, table = %view.table_name,
                    db_id = db_id, affected = affected, error = %e,
                    "tx_commit_failed"
                );
                api_err(&format!("提交事务失败: {e}"))
            })?;
            tracing::info!(
                target: "cmx_dct::save",
                dict_code = %view.dict_code, table = %view.table_name,
                db_id = db_id, affected = affected,
                updated_rows = updated_at.len(), idmap_size = id_map.len(),
                "success"
            );
            Ok(SaveOutcome::Ok {
                affected,
                updated_at,
                id_map,
            })
        }
        Err(e) => {
            let _ = tx.rollback(&txn_id).await;
            // 已被 map_db_err 记录过 SQL/原始错误，此处只补充阶段 + 表级上下文。
            tracing::error!(
                target: "cmx_dct::save",
                dict_code = %view.dict_code, table = %view.table_name, db_id = db_id, error = %e,
                "save_apply_failed"
            );
            Err(e)
        }
    }
}

/// 校验一个 changeset 桶（inserted 整行 + updated 部分字段）。返回违规列表（空=通过）。
fn validate_bucket(view: &DictView, bucket: &Value) -> Vec<cmx_biz::errcode::Violation> {
    let vopts_insert = cmx_biz::validation::ValidateOptions::insert(SERVER_FILLED_COLS, SERVER_REPLACED_COLS);
    let vopts_update = cmx_biz::validation::ValidateOptions::update(SERVER_FILLED_COLS, SERVER_REPLACED_COLS);
    let mut violations = Vec::new();

    if let Some(ins) = bucket.get("inserted").and_then(|v| v.as_array()) {
        for (i, row) in ins.iter().enumerate() {
            if let Some(obj) = row_fields(row) {
                violations.extend(cmx_biz::validation::validate_insert_row(
                    &view.spec,
                    &obj,
                    Some(i),
                    &vopts_insert,
                ));
            }
        }
    }
    if let Some(ups) = bucket.get("updated").and_then(|v| v.as_array()) {
        for (i, row) in ups.iter().enumerate() {
            if let Some(fields) = row.get("fields").and_then(|v| v.as_object()) {
                violations.extend(cmx_biz::validation::validate_update_fields(
                    &view.spec,
                    fields,
                    Some(i),
                    &vopts_update,
                ));
            }
        }
    }
    violations
}

// ============================================================================
// is_leaf 级联维护（分级字典）：仿 cmx-iam recompute_parent_is_leaf
//
// 分级字典（self_hierarchy=true + parent_field + is_leaf 列齐全）保存时需维护父子 is_leaf：
//   - 新增子节点 / 节点移入新父 → 新父 is_leaf=0
//   - 删除子节点 / 节点移出旧父 → 旧父若无其他子节点则 is_leaf=1
// 与 cmx-iam 的差异：iam 在事务提交后用独立连接重算（规避删除可见性竞态）；
// 此处 dct 在 save_apply 同事务内重算（dct 三段 apply 顺序 deleted→inserted→updated，
// 重算在最后统一执行，此时所有增删改已落地，同事务内可见性一致，无竞态）。
// ============================================================================

/// 分级字典的 parent 列名。仅当 self_hierarchy=true + parent_field 非空 + is_leaf 是合法列时返回。
/// 不满足任一条件（非分级字典或缺列）返回 None，调用方据此跳过所有 is_leaf 维护。
fn hierarchy_parent_field(view: &DictView) -> Option<String> {
    if !view.self_hierarchy {
        return None;
    }
    let pf = view.parent_field.as_ref()?;
    if !valid_col(view, pf) || !valid_col(view, "is_leaf") {
        return None;
    }
    Some(pf.clone())
}

/// 取 JSON 值作为非空 id 字符串。null / 空串 / 空对象返回 None（表示无父或父置空）。
fn non_empty_id_str(v: &Value) -> Option<String> {
    match v {
        Value::Null => None,
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// UPDATE 成功后，把 parent 变更涉及的旧父/新父 id 推进 touched_parents。
/// old_parent 和 new_parent 都进集合——后续重算时：新父强制置 0、旧父按"是否还有子"决定。
/// 同一 id 可能重复推进（如多行同移），重算 SQL 是幂等的，重复执行结果一致。
fn collect_parent_change(
    touched: &mut Vec<String>,
    old_parent: Option<String>,
    new_parent: Option<String>,
) {
    if let Some(p) = old_parent {
        touched.push(p);
    }
    if let Some(p) = new_parent {
        touched.push(p);
    }
}

/// 在事务内查某行的 parent_id 值。返回 None 表示无父（null/空/行不存在）。
async fn select_parent_id(
    mm: &DatabaseManager,
    db_id: &str,
    txn_id: &str,
    view: &DictView,
    parent_field: &str,
    id: &Value,
) -> Result<Option<String>> {
    let sql = format!(
        "SELECT \"{}\" AS pid FROM \"{}\" WHERE \"{}\" = $1",
        parent_field, view.table_name, view.pk
    );
    let params = vec![to_dv_by_col(view, &view.pk, id)];
    let ds = mm
        .query_sql_with_datavalues(db_id, Some(txn_id), &sql, params, "pid")
        .await
        .map_err(|e| map_db_err(e, "select_parent_id", view, None, &sql))?;
    let ds_val = serde_json::to_value(&ds).ok();
    let pid = ds_val
        .as_ref()
        .and_then(|v| v.get("rows"))
        .and_then(|r| r.as_array())
        .and_then(|a| a.first())
        .and_then(|r0| r0.get("pid"))
        .and_then(non_empty_id_str);
    Ok(pid)
}

/// 重算一组 parent_id 的 is_leaf（仿 cmx-iam recompute_parent_is_leaf）。
/// 语义：被任何节点当作 parent 的，强制 is_leaf=0；否则 is_leaf=1（变回叶子）。
///
/// 用两条集合 SQL 替代逐行循环（dct 是批量保存场景，parent 数组可能较长）：
///   1. WHERE pk IN ($parents) AND EXISTS (有子) → is_leaf=0
///   2. WHERE pk IN ($parents) AND NOT EXISTS (无子) → is_leaf=1
///
/// 两条都基于 NOT EXISTS/EXISTS 子查询，幂等且无竞态。
async fn recompute_parent_is_leaf(
    mm: &DatabaseManager,
    db_id: &str,
    txn_id: &str,
    view: &DictView,
    parent_field: &str,
    parents: &[String],
) -> Result<()> {
    if parents.is_empty() {
        return Ok(());
    }
    // 去重（同一父可能被多次推进）
    let mut uniq: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for p in parents {
        if seen.insert(p.clone()) {
            uniq.push(p.clone());
        }
    }
    // 构造占位符列表 $1,$2,...（每个 parent 一个参数位）
    let placeholders: Vec<String> = (1..=uniq.len()).map(|i| format!("${i}")).collect();
    let ph_list = placeholders.join(", ");
    let params: Vec<DataValue> = uniq.iter().map(|s| DataValue::String(s.clone())).collect();
    // 1. 有子节点 → is_leaf=0
    let sql_has_children = format!(
        "UPDATE \"{tbl}\" SET \"is_leaf\" = 0 WHERE \"{pk}\" IN ({ph}) \
         AND EXISTS (SELECT 1 FROM \"{tbl}\" c WHERE c.\"{pf}\" = \"{tbl}\".\"{pk}\")",
        tbl = view.table_name,
        pk = view.pk,
        pf = parent_field,
        ph = ph_list
    );
    mm.execute_sql_with_datavalues(db_id, Some(txn_id), &sql_has_children, params.clone())
        .await
        .map_err(|e| map_db_err(e, "recompute_is_leaf", view, None, &sql_has_children))?;
    // 2. 无子节点 → is_leaf=1（变回叶子）
    let sql_no_children = format!(
        "UPDATE \"{tbl}\" SET \"is_leaf\" = 1 WHERE \"{pk}\" IN ({ph}) \
         AND NOT EXISTS (SELECT 1 FROM \"{tbl}\" c WHERE c.\"{pf}\" = \"{tbl}\".\"{pk}\")",
        tbl = view.table_name,
        pk = view.pk,
        pf = parent_field,
        ph = ph_list
    );
    mm.execute_sql_with_datavalues(db_id, Some(txn_id), &sql_no_children, params)
        .await
        .map_err(|e| map_db_err(e, "recompute_is_leaf", view, None, &sql_no_children))?;
    Ok(())
}

/// 在事务内执行 deleted 分支：按 pk 逐行删除。返回 (受影响行数, 被删行的旧 parent_id 集合)。
///
/// 分级字典（self_hierarchy + parent_field + is_leaf 列齐全）时，删之前先 SELECT 旧 parent_id，
/// 供 save_apply 末尾重算父节点 is_leaf（删子后旧父可能变回叶子）。非分级字典返回空集合。
async fn apply_deletes(
    mm: &DatabaseManager,
    db_id: &str,
    txn_id: &str,
    view: &DictView,
    bucket: &Value,
) -> Result<(u64, Vec<String>)> {
    let mut affected = 0u64;
    let mut touched_parents: Vec<String> = Vec::new();
    let pf = hierarchy_parent_field(view); // 分级字典的 parent 列名；非分级返回 None
    if let Some(dels) = bucket.get("deleted").and_then(|v| v.as_array()) {
        for (i, id) in dels.iter().enumerate() {
            // 分级字典：删之前先记下被删行的 parent（删完就查不到了），用于后续旧父重算。
            // select_parent_id 内部已用 map_db_err 翻译 DB 错误为 cmx_api_types::Error，
            // 此处直接 ? 冒泡，避免二次包装。
            if let Some(dpf) = pf.as_deref()
                && let Some(pid) = select_parent_id(mm, db_id, txn_id, view, dpf, id).await?
            {
                touched_parents.push(pid);
            }
            let sql = cmx_dct_model::build_delete_sql(view);
            let params = vec![to_dv_by_col(view, &view.pk, id)];
            let n = mm
                .execute_sql_with_datavalues(db_id, Some(txn_id), &sql, params)
                .await
                .map_err(|e| map_db_err(e, "delete", view, Some(i), &sql))?;
            affected += n;
        }
    }
    Ok((affected, touched_parents))
}

/// 在事务内执行 inserted 分支：铺平 → 铸号（服务端生成列）→ upsert。
/// 返回 (受影响行数, idMap, 新增行挂载的 parent_id 集合)。
///
/// 分级字典时，apply_inserts 跑完后所有 inserted 行的 parent_id 已是真值（经 mint_ids_for_inserts
/// 第二遍把临时 parent 重指向父行真号），可直接从 rows 收集，供 save_apply 末尾把新父 is_leaf 置 0。
async fn apply_inserts(
    mm: &DatabaseManager,
    db_id: &str,
    txn_id: &str,
    view: &DictView,
    bucket: &Value,
) -> Result<(u64, serde_json::Map<String, Value>, Vec<String>)> {
    let mut affected = 0u64;
    let mut id_map = serde_json::Map::new();
    let mut touched_parents: Vec<String> = Vec::new();
    let pf = hierarchy_parent_field(view);
    if let Some(ins) = bucket.get("inserted").and_then(|v| v.as_array()) {
        // 先铺平成可改写行对象 → 服务端生成列则铸号 + 回填 parent_id → 整行 upsert。
        let mut rows: Vec<serde_json::Map<String, Value>> =
            ins.iter().filter_map(row_fields).collect();
        if pk_is_generated(view) {
            id_map = mint_ids_for_inserts(view, &mut rows);
        }
        for (i, o) in rows.iter().enumerate() {
            if let Some((sql, params)) = build_upsert_sql_dv(view, o) {
                let n = mm
                    .execute_sql_with_datavalues(db_id, Some(txn_id), &sql, params)
                    .await
                    .map_err(|e| map_db_err(e, "insert", view, Some(i), &sql))?;
                affected += n;
            }
        }
        // 分级字典：收集新增行挂载的 parent（铸号后已是真值），新父需 is_leaf=0。
        if let Some(pf) = pf.as_deref() {
            for o in &rows {
                if let Some(pid) = o.get(pf).and_then(non_empty_id_str) {
                    touched_parents.push(pid);
                }
            }
        }
    }
    Ok((affected, id_map, touched_parents))
}

/// 在事务内执行 updated 分支：带乐观锁基线的 UPDATE。
///
/// 返回 (受影响行数, updatedAt 列表, 是否乐观锁冲突, 受影响 parent_id 集合)。
///
/// 分级字典时，若某行修改了 parent_field 字段（节点移父），UPDATE 之前先 SELECT 旧 parent，
/// UPDATE 之后收集新 parent；二者都进 touched_parents，供 save_apply 末尾重算 is_leaf
/// （新父置 0、旧父无子则置 1）。
///
/// **update_time 回查优化**：有 update_time 列时，用 `UPDATE ... RETURNING "update_time" AS ut`
/// 一次往返拿回行数 + 新时间戳（消除原 execute + 回查 SELECT 的 N+1）；无 update_time 列时
/// 退化走 execute（不加 RETURNING、不回查），与历史行为一致。
/// 乐观锁：0 行 + 有 lock_clause = 冲突（RETURNING 0 行等价于 execute 返回 0）。
async fn apply_updates(
    mm: &DatabaseManager,
    db_id: &str,
    txn_id: &str,
    view: &DictView,
    bucket: &Value,
) -> Result<(u64, Vec<Value>, bool, Vec<String>)> {
    let mut affected = 0u64;
    let mut updated_at: Vec<Value> = Vec::new();
    let mut touched_parents: Vec<String> = Vec::new();
    let pf = hierarchy_parent_field(view);
    if let Some(ups) = bucket.get("updated").and_then(|v| v.as_array()) {
        for (row_index, row) in ups.iter().enumerate() {
            let id = match row.get("id") {
                Some(v) if !v.is_null() => v.clone(),
                _ => continue,
            };
            let fields = match row.get("fields").and_then(|v| v.as_object()) {
                Some(f) if !f.is_empty() => f,
                _ => continue,
            };
            // 只更新白名单列（排除 pk 自身 + 服务端托管的时间列）。
            // null 与非 null 统一用占位符，配合 to_dv_by_col 产生的 DataValue（null→NullTyped
            // 按列类型，整型列字符串数字→Int），走 execute_sql_with_datavalues 绑定。
            let mut set_parts: Vec<String> = Vec::new();
            let mut params: Vec<DataValue> = Vec::new();
            let mut i = 0usize;
            let mut new_parent: Option<String> = None; // 分级字典：本行新 parent 值（若有变更）
            for (k, v) in fields {
                if !valid_col(view, k) || k == &view.pk || is_server_managed_col(k) {
                    continue;
                }
                i += 1;
                set_parts.push(format!("\"{}\" = ${}", k, i));
                params.push(to_dv_by_col(view, k, v));
                // 分级字典：记录新 parent 值（UPDATE 之前先 SELECT 旧 parent）
                if pf.as_deref() == Some(k.as_str()) {
                    new_parent = non_empty_id_str(v);
                }
            }
            if set_parts.is_empty() {
                continue;
            }
            // 分级字典 + 本行改了 parent：UPDATE 之前 SELECT 旧 parent（之后被覆盖查不到）。
            // 旧 parent 先暂存，等 UPDATE 确认成功后再推进 touched_parents（冲突时父未变，不重算）。
            let mut old_parent_to_touch: Option<String> = None;
            if new_parent.is_some()
                && let Some(pf) = &pf
            {
                old_parent_to_touch =
                    select_parent_id(mm, db_id, txn_id, view, pf, &id).await?;
            }
            // update_time 服务端刷新。
            if valid_col(view, "update_time") {
                set_parts.push("\"update_time\" = now()".to_string());
            }
            // pk 参数（按 pk 列类型，避免字符串 id 绑整型列报 WrongType）。
            i += 1;
            let pk_ph = i;
            params.push(to_dv_by_col(view, &view.pk, &id));
            // 乐观锁：baseline 存在 + 有 update_time 列 → 加 AND update_time = baseline。
            // baseline 在下面 if let 中会被 move，先借出 JSON 字符串副本给后续日志使用。
            let baseline = row.get("baseline").filter(|b| !b.is_null()).cloned();
            let baseline_repr = serde_json::to_string(&baseline).unwrap_or_default();
            let lock_clause = if let Some(b) = baseline.filter(|_| valid_col(view, "update_time")) {
                i += 1;
                params.push(to_dv_by_col(view, "update_time", &b));
                format!(" AND \"update_time\" = ${}", i)
            } else {
                String::new()
            };
            // 表有 update_time 列时走 RETURNING（一次往返拿行数 + 新时间戳），否则退化走 execute。
            let has_update_time = valid_col(view, "update_time");
            if has_update_time {
                let ret_sql = format!(
                    "UPDATE \"{}\" SET {} WHERE \"{}\" = ${}{} RETURNING \"update_time\" AS ut",
                    view.table_name,
                    set_parts.join(", "),
                    view.pk,
                    pk_ph,
                    lock_clause
                );
                let ds = mm
                    .query_sql_with_datavalues(db_id, Some(txn_id), &ret_sql, params, "ut")
                    .await
                    .map_err(|e| map_db_err(e, "update", view, Some(row_index), &ret_sql))?;
                // DataSet → JSON，后续行数统计与 update_time 取值共用此次序列化结果。
                let ds_val = serde_json::to_value(&ds).ok();
                let rows_arr = ds_val.as_ref().and_then(|v| v.get("rows"));
                // RETURNING 行数 = 受影响行数。
                let rows_touched = rows_arr
                    .and_then(|r| r.as_array())
                    .map(|a| a.len())
                    .unwrap_or(0);
                if rows_touched == 0 && !lock_clause.is_empty() {
                    // 乐观锁冲突（baseline 不匹配，RETURNING 0 行）。
                    tracing::warn!(
                        target: "cmx_dct::update",
                        dict_code = %view.dict_code, table = %view.table_name,
                        row_index = row_index, id = %id, baseline = %baseline_repr,
                        "optimistic_lock_conflict"
                    );
                    return Ok((affected, updated_at, true, touched_parents));
                }
                affected += rows_touched as u64;
                // 从 RETURNING 结果取新 update_time（第 0 行的 ut 列）。
                let ut = rows_arr
                    .and_then(|r| r.get(0))
                    .and_then(|r0| r0.get("ut"))
                    .cloned();
                if let Some(ut) = ut {
                    updated_at.push(json!({ "id": id, "updateTime": ut }));
                } else if rows_touched > 0 {
                    tracing::warn!(
                        target: "cmx_dct::update",
                        dict_code = %view.dict_code, table = %view.table_name,
                        row_index = row_index, id = %id,
                        "update_time_extract_failed"
                    );
                }
                // UPDATE 成功后推进 parent 变更（含 RETURNING 分支）
                if rows_touched > 0 {
                    collect_parent_change(&mut touched_parents, old_parent_to_touch.take(), new_parent.take());
                }
            } else {
                // 退化路径：表无 update_time 列，走 execute（不加 RETURNING、不回查）。
                let upd_sql = format!(
                    "UPDATE \"{}\" SET {} WHERE \"{}\" = ${}{}",
                    view.table_name,
                    set_parts.join(", "),
                    view.pk,
                    pk_ph,
                    lock_clause
                );
                let n = mm
                    .execute_sql_with_datavalues(db_id, Some(txn_id), &upd_sql, params)
                    .await
                    .map_err(|e| map_db_err(e, "update", view, Some(row_index), &upd_sql))?;
                if n == 0 && !lock_clause.is_empty() {
                    tracing::warn!(
                        target: "cmx_dct::update",
                        dict_code = %view.dict_code, table = %view.table_name,
                        row_index = row_index, id = %id, baseline = %baseline_repr,
                        "optimistic_lock_conflict"
                    );
                    return Ok((affected, updated_at, true, touched_parents));
                }
                affected += n;
                // UPDATE 成功后推进 parent 变更（execute 退化分支）
                if n > 0 {
                    collect_parent_change(&mut touched_parents, old_parent_to_touch.take(), new_parent.take());
                }
            }
        }
    }
    Ok((affected, updated_at, false, touched_parents))
}

/// 在事务内应用 changeset 的一个桶：deleted → inserted → updated。返回 (affected, updatedAt, conflict, idMap)。
///
/// idMap：inserted 行的 临时id→新铸真id（供前端回填），NoID 字典为空。
/// 任一 updated 行命中乐观锁冲突即提前返回（conflict=true）。
async fn save_apply(
    mm: &DatabaseManager,
    db_id: &str,
    txn_id: &str,
    view: &DictView,
    bucket: &Value,
) -> Result<(u64, Vec<Value>, bool, serde_json::Map<String, Value>)> {
    // deleted：按 pk 删（分级字典同时收集被删行的旧 parent_id）。
    let (del_affected, del_parents) = apply_deletes(mm, db_id, txn_id, view, bucket).await?;
    let mut affected = del_affected;

    // inserted：铸号 + upsert（分级字典同时收集新增行挂载的 parent_id）。
    let (ins_affected, id_map, ins_parents) =
        apply_inserts(mm, db_id, txn_id, view, bucket).await?;
    affected += ins_affected;

    // updated：乐观锁 UPDATE + 回查 update_time（命中冲突提前返回）。
    // 分级字典 + 改 parent 时同时收集新/旧 parent_id。
    let (upd_affected, updated_at, conflict, upd_parents) =
        apply_updates(mm, db_id, txn_id, view, bucket).await?;
    affected += upd_affected;

    // 分级字典 is_leaf 级联维护：收集三段所有受影响 parent_id，统一重算。
    // 仅 conflict=false（无乐观锁冲突）才重算——冲突时上层 save 会 rollback，无需重算。
    if !conflict {
        let pf = hierarchy_parent_field(view);
        if let Some(pf) = pf {
            let mut touched = del_parents;
            touched.extend(ins_parents);
            touched.extend(upd_parents);
            if !touched.is_empty() {
                recompute_parent_is_leaf(mm, db_id, txn_id, view, &pf, &touched).await?;
            }
        }
    }

    Ok((affected, updated_at, conflict, id_map))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// 造一个只设 name 的列（其余字段默认，足够测列序）。
    fn col(name: &str) -> DictColumn {
        DictColumn {
            name: name.to_string(),
            caption: name.to_string(),
            data_type: "VARCHAR".to_string(),
            is_pk: false,
            nullable: true,
            dim_type: String::new(),
            ref_dict: String::new(),
            display_field: String::new(),
            ref_field: String::new(),
            physical_field: String::new(),
            edit: None,
            edit_settings: None,
            display: None,
            extra: None,
        }
    }

    /// base 字段集定义（含 Common 无 ID + Audit）。
    fn base_meta() -> Value {
        json!({
            "fieldSets": {
                "dictionaryCommonNoIDFields": { "fields": [
                    {"name": "code"}, {"name": "name"}, {"name": "sort_no"}, {"name": "status"}
                ]},
                "dictionaryAuditFields": { "fields": [
                    {"name": "create_by"}, {"name": "create_time"},
                    {"name": "update_by"}, {"name": "update_time"}
                ]}
            }
        })
    }

    #[test]
    fn reorder_columns_common_first_audit_last() {
        let base = base_meta();
        let table_def = json!({
            "baseFieldSet": "dictionaryCommonNoIDFields",
            "auditFieldSet": "dictionaryAuditFields"
        });
        // 模拟 resolve_dict 合并后顺序：自定义 -> Common -> Audit。
        let mut columns = vec![
            col("custom1"), col("custom2"),
            col("code"), col("name"), col("sort_no"), col("status"),
            col("create_by"), col("create_time"), col("update_by"), col("update_time"),
        ];
        reorder_columns(&mut columns, &base, &table_def);
        let names: Vec<&str> = columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec![
            "code", "name", "sort_no", "status",
            "custom1", "custom2",
            "create_by", "create_time", "update_by", "update_time",
        ]);
    }

    #[test]
    fn reorder_columns_no_fieldset_refs_noop() {
        // 无 baseFieldSet/auditFieldSet 引用 -> 不重排。
        let base = base_meta();
        let table_def = json!({});
        let mut columns = vec![col("a"), col("b"), col("c")];
        reorder_columns(&mut columns, &base, &table_def);
        let names: Vec<&str> = columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["a", "b", "c"]);
    }

    #[test]
    fn reorder_columns_common_with_id() {
        // dictionaryCommonFields（含 id）场景：id 也排前。
        let base = json!({
            "fieldSets": {
                "dictionaryCommonFields": { "fields": [
                    {"name": "id"}, {"name": "code"}, {"name": "name"}
                ]}
            }
        });
        let table_def = json!({ "baseFieldSet": "dictionaryCommonFields" });
        let mut columns = vec![col("custom"), col("id"), col("code"), col("name"), col("create_time")];
        reorder_columns(&mut columns, &base, &table_def);
        let names: Vec<&str> = columns.iter().map(|c| c.name.as_str()).collect();
        // Common[id,code,name] -> mid[custom,create_time] -> audit[]（无 audit 引用）
        assert_eq!(names, vec!["id", "code", "name", "custom", "create_time"]);
    }

    #[test]
    fn reorder_columns_skipped_when_field_set_order_present() {
        // 声明了 fieldSetOrder 时，reorder_columns 不再做 Common/Audit 三分组重排——
        // 合并阶段已按用户自定义段序 push，此处保持原样（含系统列交叉排）。
        let base = base_meta();
        let table_def = json!({
            "baseFieldSet": "dictionaryCommonNoIDFields",
            "auditFieldSet": "dictionaryAuditFields",
            "fieldSetOrder": ["dictionaryCommonNoIDFields", "own", "dictionaryAuditFields"]
        });
        // 模拟按 fieldSetOrder 合并后的顺序（custom 本表字段穿插在 Common/Audit 之间）。
        let mut columns = vec![
            col("code"), col("custom1"), col("name"), col("create_time"), col("status"),
        ];
        reorder_columns(&mut columns, &base, &table_def);
        let names: Vec<&str> = columns.iter().map(|c| c.name.as_str()).collect();
        // 未被重排，保持传入顺序。
        assert_eq!(names, vec!["code", "custom1", "name", "create_time", "status"]);
    }
}

// ============================================================================
// 子模块：流式导入导出服务
// ============================================================================
mod import_export;
pub use import_export::*;
