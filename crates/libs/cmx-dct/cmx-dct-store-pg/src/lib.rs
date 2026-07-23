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
use cmx_database_pg::{DatabaseManager, get_default_pg_db_manager};

use cmx_dct_model::{
    DctQuery, DictColumn, DictView, SERVER_FILLED_COLS, SERVER_REPLACED_COLS, base_fieldset,
    build_search_sql, build_upsert_sql_dv, is_server_managed_col, json_to_datavalue,
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

/// 解析 `DctQuery` → 强类型 `DictView`（合并列 + base 字段集 + 校验规范缓存）。
pub async fn resolve_dict(q: &DctQuery) -> Result<DictView> {
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

    // 合并列：own fields + 全部 *FieldSet 引用（与 compile_dct 对齐）。
    let mut columns: Vec<DictColumn> = Vec::new();
    // 合并后的原始字段（带 fieldLength/decimalDigits），供构建校验规范 TableSpec。
    let mut raw_fields: Vec<Value> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let push = |fields: &Vec<Value>,
                columns: &mut Vec<DictColumn>,
                raw_fields: &mut Vec<Value>,
                seen: &mut std::collections::HashSet<String>| {
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
                name,
            });
        }
    };
    if let Some(own) = t.get("fields").and_then(|v| v.as_array()) {
        push(own, &mut columns, &mut raw_fields, &mut seen);
    }
    if let Some(obj) = t.as_object() {
        // 固定顺序 + 兜底（与 compile_dct 一致）。
        for key in [
            "baseFieldSet",
            "hierarchyFieldSet",
            "scopeFieldSet",
            "effectiveFieldSet",
            "disableFieldSet",
            "auditFieldSet",
            "systemFieldSet",
        ] {
            if let Some(set_name) = obj.get(key).and_then(|v| v.as_str())
                && let Some(fields) = base_fieldset(&base, set_name)
            {
                push(fields, &mut columns, &mut raw_fields, &mut seen);
            }
        }
    }

    // 显示列序：Common 字段集（baseFieldSet）在前、Audit 字段集（auditFieldSet）置尾，
    // 其余居中保持合并相对顺序。仅影响 /dct/meta 投影，不影响物理表 DDL。
    reorder_columns(&mut columns, &base, t);

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

    // 落库前列级校验规范：从合并后的原始字段构建 TableSpec，进程内缓存（键含版本，免失效）。
    let version = doc
        .get("moduleMeta")
        .and_then(|m| m.get("version"))
        .and_then(|v| v.as_u64())
        .or_else(|| doc.get("version").and_then(|v| v.as_u64()))
        .unwrap_or(0);
    let spec_key = cmx_biz::validation::spec_key(
        &q.domain,
        &q.application,
        &q.module,
        &file,
        &table_name,
        version,
    );
    let spec = match cmx_biz::validation::get_spec(&spec_key) {
        Some(s) => s,
        None => {
            let built = std::sync::Arc::new(cmx_biz::validation::build_table_spec(
                table_name.clone(),
                &pk,
                &raw_fields,
            ));
            cmx_biz::validation::put_spec(spec_key, built.clone());
            built
        }
    };

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

/// 装载字典数据（分页 + 计数）。返回 `{rows,total,page,pageSize}`。
pub async fn search(view: &DictView, raw: &Value, db_id: &str) -> Result<Value> {
    tracing::info!(
        "[DCT-DEBUG] search view: dict_code={}, dict_name={}, table_name={}, id_field={}, code_field={}, label_field={}, parent_field={:?}, pk={}, columns={}, raw={}, db_id={}",
        view.dict_code, view.dict_name, view.table_name, view.id_field, view.code_field, view.label_field, view.parent_field, view.pk, view.columns.len(), raw, db_id
    );
    let (sql, count_sql, params) = build_search_sql(view, raw);
    // JSON params -> DataValue（走 datavalues 绑定，与 cmx-sql-execution 规范一致）。
    let dv_params: Vec<DataValue> = params.iter().map(json_to_datavalue).collect();
    tracing::info!("[DCT-DEBUG] search sql={}, db_id={}, table={}", sql, db_id, view.table_name);

    let mm = get_default_pg_db_manager();
    let ds = mm
        .query_sql_with_datavalues(
            db_id,
            None,
            &sql,
            dv_params.clone(),
            &view.dict_code,
        )
        .await
        .map_err(|e| { tracing::error!("[DCT-DEBUG] search failed: sql={}, err={:?}", sql, e); api_err(&format!("字典查询失败: {e}")) })?;
    let total_ds = mm
        .query_sql_with_datavalues(db_id, None, &count_sql, dv_params, "cnt")
        .await
        .map_err(|e| api_err(&format!("字典计数失败: {e}")))?;

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

    let page = raw.get("page").and_then(|v| v.as_i64()).unwrap_or(1).max(1);
    let page_size = raw
        .get("pageSize")
        .and_then(|v| v.as_i64())
        .unwrap_or(500)
        .clamp(1, 5000);
    Ok(json!({
        "rows": rows,
        "total": total,
        "page": page,
        "pageSize": page_size,
    }))
}

/// 零拷贝装载：tokio-postgres + ZmcDataSet + 列式二进制。返回列式包字节（handler 包 msgpack 信封）。
pub async fn search_zmc(view: &DictView, raw: &Value, db_id: &str) -> Result<Vec<u8>> {
    tracing::info!(
        "[DCT-DEBUG] search_zmc view: dict_code={}, dict_name={}, table_name={}, id_field={}, code_field={}, label_field={}, parent_field={:?}, pk={}, columns={}, raw={}, db_id={}",
        view.dict_code, view.dict_name, view.table_name, view.id_field, view.code_field, view.label_field, view.parent_field, view.pk, view.columns.len(), raw, db_id
    );
    let (sql, _count_sql, params) = build_search_sql(view, raw);
    let dv_params: Vec<cmx_core::model::cell::DataValue> =
        params.iter().map(json_to_datavalue).collect();

    let mm = get_default_pg_db_manager();
    // 零拷贝：ZmcDataSet 持有原始 tokio-postgres Row，惰性列式二进制编码。
    let zmc = mm
        .query_sql_zmc_with_datavalues(db_id, &sql, dv_params, &view.dict_code)
        .await
        .map_err(|e| {
            tracing::error!(
                "[DCT-DEBUG] search_zmc failed: dict_code={}, table_name={}, sql={}, db_id={}, err={:?}",
                view.dict_code, view.table_name, sql, db_id, e
            );
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
    let vopts = cmx_biz::validation::ValidateOptions {
        server_filled: SERVER_FILLED_COLS,
        server_replaced: SERVER_REPLACED_COLS,
        check_unknown: false,
        check_not_null: true,
    };
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
    for obj in &rows {
        if let Some((sql, params)) = build_upsert_sql_dv(view, obj) {
            let n = mm
                .execute_sql_with_datavalues(db_id, None, &sql, params)
                .await
                .map_err(|e| api_err_db(&e.to_string()))?;
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
    let sql = format!(
        "DELETE FROM \"{}\" WHERE \"{}\" = $1",
        view.table_name, view.pk
    );
    // 按 pk 列类型构造 DataValue（整型列的字符串 id 转 Int），走 datavalues 绑定。
    let params = vec![to_dv_by_col(view, &view.pk, &json!(id))];

    let mm = get_default_pg_db_manager();
    let n = mm
        .execute_sql_with_datavalues(db_id, None, &sql, params)
        .await
        .map_err(|e| api_err_db(&e.to_string()))?;

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
            return Ok(SaveOutcome::Ok {
                affected: 0,
                updated_at: Vec::new(),
                id_map: serde_json::Map::new(),
            });
        }
    };

    // 落库前列级校验（开事务前，一次回报全部）。inserted 走整行校验（含 NOT NULL，跳过
    // 服务端 backfill 列）；updated 只校验其 fields（不做整表 NOT NULL）。
    let violations = validate_bucket(view, bucket);
    if !violations.is_empty() {
        return Ok(SaveOutcome::Invalid(violations));
    }

    let mm = get_default_pg_db_manager();
    let tx = mm.get_transaction_context();
    let txn_id = tx
        .begin(db_id)
        .await
        .map_err(|e| api_err(&format!("开启事务失败: {e}")))?;

    let result = save_apply(mm, db_id, &txn_id, view, bucket).await;

    match result {
        Ok((affected, updated_at, conflict, id_map)) => {
            if conflict {
                let _ = tx.rollback(&txn_id).await;
                return Ok(SaveOutcome::Conflict);
            }
            tx.commit(&txn_id)
                .await
                .map_err(|e| api_err(&format!("提交事务失败: {e}")))?;
            Ok(SaveOutcome::Ok {
                affected,
                updated_at,
                id_map,
            })
        }
        Err(e) => {
            let _ = tx.rollback(&txn_id).await;
            Err(e)
        }
    }
}

/// 校验一个 changeset 桶（inserted 整行 + updated 部分字段）。返回违规列表（空=通过）。
fn validate_bucket(view: &DictView, bucket: &Value) -> Vec<cmx_biz::errcode::Violation> {
    let vopts_insert = cmx_biz::validation::ValidateOptions {
        server_filled: SERVER_FILLED_COLS,
        server_replaced: SERVER_REPLACED_COLS,
        check_unknown: false,
        check_not_null: true,
    };
    let vopts_update = cmx_biz::validation::ValidateOptions {
        server_filled: SERVER_FILLED_COLS,
        server_replaced: SERVER_REPLACED_COLS,
        check_unknown: false,
        check_not_null: false,
    };
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

/// 在事务内应用 changeset 的一个桶。返回 (affected, updatedAt, conflict, idMap)。
/// idMap：inserted 行的 临时id→新铸真id（供前端回填），NoID 字典为空。
async fn save_apply(
    mm: &DatabaseManager,
    db_id: &str,
    txn_id: &str,
    view: &DictView,
    bucket: &Value,
) -> Result<(u64, Vec<Value>, bool, serde_json::Map<String, Value>)> {
    let mut affected = 0u64;
    let mut updated_at: Vec<Value> = Vec::new();
    let mut id_map = serde_json::Map::new();

    // deleted：按 pk 删。
    if let Some(dels) = bucket.get("deleted").and_then(|v| v.as_array()) {
        for id in dels {
            let sql = format!(
                "DELETE FROM \"{}\" WHERE \"{}\" = $1",
                view.table_name, view.pk
            );
            let params = vec![to_dv_by_col(view, &view.pk, id)];
            let n = mm
                .execute_sql_with_datavalues(db_id, Some(txn_id), &sql, params)
                .await
                .map_err(|e| api_err_db(&e.to_string()))?;
            affected += n;
        }
    }

    // inserted：先铺平成可改写行对象 → 服务端生成列则铸号 + 回填 parent_id → 整行 upsert。
    if let Some(ins) = bucket.get("inserted").and_then(|v| v.as_array()) {
        let mut rows: Vec<serde_json::Map<String, Value>> =
            ins.iter().filter_map(row_fields).collect();
        if pk_is_generated(view) {
            id_map = mint_ids_for_inserts(view, &mut rows);
        }
        for o in &rows {
            if let Some((sql, params)) = build_upsert_sql_dv(view, o) {
                let n = mm
                    .execute_sql_with_datavalues(db_id, Some(txn_id), &sql, params)
                    .await
                    .map_err(|e| api_err_db(&e.to_string()))?;
                affected += n;
            }
        }
    }

    // updated：带乐观锁基线（baseline=装载时 update_time）。有 baseline 且表有 update_time 列时，
    // UPDATE ... WHERE pk=$ AND update_time=baseline；影响 0 行 = 冲突。
    if let Some(ups) = bucket.get("updated").and_then(|v| v.as_array()) {
        for row in ups {
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
            for (k, v) in fields {
                if !valid_col(view, k) || k == &view.pk || is_server_managed_col(k) {
                    continue;
                }
                i += 1;
                set_parts.push(format!("\"{}\" = ${}", k, i));
                params.push(to_dv_by_col(view, k, v));
            }
            if set_parts.is_empty() {
                continue;
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
            let baseline = row.get("baseline").filter(|b| !b.is_null()).cloned();
            let lock_clause = if let Some(b) = baseline.filter(|_| valid_col(view, "update_time")) {
                i += 1;
                params.push(to_dv_by_col(view, "update_time", &b));
                format!(" AND \"update_time\" = ${}", i)
            } else {
                String::new()
            };
            let sql = format!(
                "UPDATE \"{}\" SET {} WHERE \"{}\" = ${}{}",
                view.table_name,
                set_parts.join(", "),
                view.pk,
                pk_ph,
                lock_clause
            );
            let n = mm
                .execute_sql_with_datavalues(db_id, Some(txn_id), &sql, params)
                .await
                .map_err(|e| api_err_db(&e.to_string()))?;
            if n == 0 && !lock_clause.is_empty() {
                // 乐观锁冲突（baseline 不匹配）。
                return Ok((affected, updated_at, true, id_map));
            }
            affected += n;
            // 回传新 update_time 供前端刷新基线。
            if valid_col(view, "update_time") {
                let q = format!(
                    "SELECT \"update_time\" AS ut FROM \"{}\" WHERE \"{}\" = $1",
                    view.table_name, view.pk
                );
                let q_params = vec![to_dv_by_col(view, &view.pk, &id)];
                if let Ok(ds) = mm
                    .query_sql_with_datavalues(db_id, Some(txn_id), &q, q_params, "ut")
                    .await
                    && let Ok(v) = serde_json::to_value(&ds)
                    && let Some(ut) = v
                        .get("rows")
                        .and_then(|r| r.get(0))
                        .and_then(|r0| r0.get("ut"))
                        .cloned()
                {
                    updated_at.push(json!({ "id": id, "updateTime": ut }));
                }
            }
        }
    }

    Ok((affected, updated_at, false, id_map))
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
}
