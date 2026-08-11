//! cmx-dct-store-pg 元数据解析——从定义 JSON 找到目标字典表 + 合并列 + 主键 + 校验规范。
//!
//! 对外入口：[`resolve_dict`]（DctQuery → DictView）。
//! 其余函数均为 `resolve_dict` 的纯重构子步骤，模块内私有。
//! （db_id 路由已上提到 `cmx_api::db_id`，供所有 API crate 共用。）

use cmx_api_types::Result;
use cmx_dct_model::{DctQuery, DictColumn, DictView, base_fieldset};
use serde_json::{Value, json};

use crate::error::api_err;
// 元数据解析：从定义 JSON 找到目标字典表 + 合并列
// ============================================================================

/// file 自动解析 + doc 加载 + base 字段集加载。
///
/// file 缺失时由 `resolve_dict_file` 在 domain/app/module 下扫描含 dictCode 的 DCT 文件
/// （前端运行时只持 dictCode + domain/app/module，无 file 坐标）。返回 (doc, base, file)。
async fn resolve_doc(
    domain: &str,
    app: &str,
    module: &str,
    file: Option<&str>,
    dict: &str,
) -> Result<(Value, Value, String)> {
    let file = match file {
        Some(f) if !f.is_empty() => f.to_string(),
        _ => {
            cmx_model_meta::definitions::resolve::resolve_dict_file(domain, app, module, dict)
                .await?
        }
    };
    let doc_ref = cmx_model_meta::definitions::store::DefRef {
        domain: Some(domain.to_string()),
        application: Some(app.to_string()),
        app: Some(app.to_string()),
        module: Some(module.to_string()),
        file: Some(file.clone()),
        id: None,
        kind: None,
    };
    let doc = cmx_model_meta::definitions::store::get_definition(&doc_ref).await?;
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
    // 收集本表声明的引用字段集名，供默认顺序与兜底补尾使用。
    // 按键名后缀 "FieldSet" 动态识别（任何 xxxFieldSet 键的字符串值都视为字段集引用），
    // 不再维护键名清单——新增通用字段集约定时无需改代码。
    // 段序 = 定义文件里 *FieldSet 键的书写序（serde_json preserve_order 保证）。
    let declared_sets: Vec<String> = {
        let mut out = Vec::new();
        if let Some(obj) = t.as_object() {
            for (key, val) in obj {
                if key.ends_with("FieldSet")
                    && let Some(set_name) = val.as_str()
                {
                    let s = set_name.to_string();
                    if !s.is_empty() && !out.contains(&s) {
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
#[allow(clippy::too_many_arguments)]
fn resolve_or_build_spec(
    domain: &str,
    app: &str,
    module: &str,
    generation: u64,
    file: &str,
    table_name: &str,
    pk: &str,
    raw_fields: &[Value],
    version: u64,
) -> std::sync::Arc<cmx_biz::validation::TableSpec> {
    // 落库前列级校验规范：进程内缓存（键含版本 + 定义树代数）。
    // 拼代数：手动改定义字段但不升 version 的带外变更也会让 spec 陈旧，随代数收敛。
    let spec_key = format!(
        "{}#g{}",
        cmx_biz::validation::spec_key(domain, app, module, file, table_name, version),
        generation
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
    // 0) 坐标归一化：DAM 缺失/部分时按 dict 全局反查补全（三段齐全 → 快路径直通）。
    use cmx_model_meta::definitions::coord;
    let partial = coord::DamPartial {
        domain: coord::clean_opt(q.domain.clone()),
        application: coord::clean_opt(q.application.clone()),
        module: coord::clean_opt(q.module.clone()),
    };
    let (domain, application, module) = if let (Some(d), Some(a), Some(m)) = (
        partial.domain.clone(),
        partial.application.clone(),
        partial.module.clone(),
    ) {
        (d, a, m)
    } else {
        let c = coord::resolve_dam_by_code("DCT", &q.dict, &partial).await?;
        (c.domain, c.application, c.module)
    };
    // 定义树代数：spec 缓存键用（带外变更感知）。
    let generation = coord::definitions_generation().await;

    // 1) file 解析 + doc/base 加载。
    let (doc, base, file) =
        resolve_doc(&domain, &application, &module, q.file.as_deref(), &q.dict).await?;

    // 2) 定位目标字典表定义。
    let tables = doc
        .get("dictionaryTables")
        .and_then(|v| v.as_array())
        .ok_or_else(|| api_err("定义缺少 dictionaryTables"))?;
    let t = tables
        .iter()
        .find(|t| cmx_model_meta::definitions::resolve::dict_matches(t, &q.dict))
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
    let spec = resolve_or_build_spec(
        &domain, &application, &module, generation, &file, &table_name, &pk, &raw_fields, version,
    );

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
        code_rule: dm.get("codeRule").cloned(),
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
    let base_ref = cmx_model_meta::definitions::store::DefRef {
        domain: Some("base".into()),
        application: None,
        app: None,
        module: None,
        file: Some(file.to_string()),
        id: None,
        kind: None,
    };
    cmx_model_meta::definitions::store::get_definition(&base_ref)
        .await
        .unwrap_or_else(|_| json!({}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cmx_dct_model::DictColumn;

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
