//! cmx-dct-store-pg 回存服务——upsert（merge 语义）/ delete / save（changeset 事务）。
//!
//! - [`upsert`]：单行/批量 merge，含铸号 + 列校验。
//! - [`delete`]：按 pk 删单行。
//! - [`save`]：changeset（inserted/updated/deleted）事务回存，updated 带 update_time 乐观锁。
//!
//! save 内部的 `apply_deletes` / `apply_inserts` / `apply_updates` / `save_apply` 为模块私有；
//! 分级字典的 level_no/full_path/is_leaf 级联重算委托 [`crate::hierarchy`]。

use cmx_api_types::Result;
use cmx_core::model::cell::DataValue;
use cmx_database_pg::{DatabaseManager, get_default_pg_db_manager};
use cmx_dct_model::{
    DictView, SERVER_FILLED_COLS, SERVER_REPLACED_COLS, build_upsert_sql_dv,
    is_server_managed_col, mint_ids_for_inserts, pk_is_generated, row_fields, to_dv_by_col,
    valid_col,
};
use serde_json::{Value, json};

use crate::error::{api_err, map_db_err};
use crate::hierarchy::{
    hierarchy_parent_field, non_empty_id_str, recompute_hierarchy_subtree, select_parent_id,
};

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

    // 编码引擎铸号：若字典配置了 codeRule(mode=auto)，为 code_field 为空的行铸业务编码。
    // 未配置编码引擎（code_rule=None 或 GlobalCodeMinter 未注入）→ 跳过（现状零影响）。
    mint_codes_for_inserts(view, &mut rows, db_id).await;

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
// changeset 三段应用（save_apply 内部步骤，模块私有）
// ============================================================================

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
/// 返回 (受影响行数, idMap, 受影响节点 id 集合)。
///
/// 分级字典时，apply_inserts 跑完后所有 inserted 行的 pk 已是真值（经 mint_ids_for_inserts
/// 铸号）。touched_ids 同时收集「新增行自身 + 它的父 id」：
/// - 行自身：递归 CTE 重算其 level_no/full_path（backfill 兜底值会被覆盖为真值）
/// - 父 id：anchor 重算父时用 EXISTS 判定，把父的 is_leaf 置 0（有子了，不再是叶子）
///
/// 必须收集父 id——anchor 只重算 touched 集合里的节点，不会反向把父也加进来；
/// 若漏掉父，新增子节点后父仍保持 is_leaf=1（错误：明明有子却标成叶子）。
async fn apply_inserts(
    mm: &DatabaseManager,
    db_id: &str,
    txn_id: &str,
    view: &DictView,
    bucket: &Value,
) -> Result<(u64, serde_json::Map<String, Value>, Vec<String>)> {
    let mut affected = 0u64;
    let mut id_map = serde_json::Map::new();
    let mut touched_ids: Vec<String> = Vec::new();
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
        // 分级字典：收集「行自身 + 父 id」。
        // - 行自身：重算 level_no/full_path（is_leaf 默认 backfill=1，若无子则保持正确）
        // - 父 id：anchor EXISTS 判定把父 is_leaf 置 0（必有子：刚插入的本行）
        if let Some(pf) = pf.as_deref() {
            for o in &rows {
                if let Some(id) = o.get(&view.pk).and_then(non_empty_id_str) {
                    touched_ids.push(id);
                }
                if let Some(pid) = o.get(pf).and_then(non_empty_id_str) {
                    touched_ids.push(pid);
                }
            }
        }
    }
    Ok((affected, id_map, touched_ids))
}

/// 在事务内执行 updated 分支：带乐观锁基线的 UPDATE。
///
/// 返回 (受影响行数, updatedAt 列表, 是否乐观锁冲突, 受影响节点 id 集合)。
///
/// 分级字典时，若某行修改了 parent_field（节点移父），touched_ids 收集三者：
/// - 行自身：递归 CTE 重算其整棵子树的 level_no/full_path/is_leaf
/// - 旧父：anchor EXISTS 判定——移走后若无其他子，is_leaf 置 1（变回叶子）
/// - 新父：anchor EXISTS 判定——移入后必有子（本行），is_leaf 置 0
///
/// 旧父必须在 UPDATE **之前** SELECT（之后被新值覆盖查不到）；新父从 fields 里直接取。
/// 三者都不可省——anchor 只重算 touched 集合里的节点，漏掉任一父都会留下脏 is_leaf。
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
    // 分级字典：收集「改了 parent_field 的行自身 + 旧父 + 新父」三者 id。
    let mut touched_ids: Vec<String> = Vec::new();
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
                // 分级字典：记录新 parent 值（UPDATE 之前需 SELECT 旧 parent）
                if pf.as_deref() == Some(k.as_str()) {
                    new_parent = non_empty_id_str(v);
                }
            }
            if set_parts.is_empty() {
                continue;
            }
            // 分级字典 + 本行改了 parent：UPDATE 之前 SELECT 旧 parent（之后被覆盖查不到）。
            // 旧父先暂存，等 UPDATE 确认成功后再推进 touched_ids（冲突时父子关系未变，不重算）。
            let mut old_parent_to_touch: Option<String> = None;
            if new_parent.is_some()
                && let Some(pf) = &pf
            {
                old_parent_to_touch = select_parent_id(mm, db_id, txn_id, view, pf, &id).await?;
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
                    return Ok((affected, updated_at, true, touched_ids));
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
                // UPDATE 成功 + 改了 parent：推进「行自身 + 旧父 + 新父」三者。
                if rows_touched > 0 && new_parent.is_some() {
                    if let Some(id_str) = non_empty_id_str(&id) {
                        touched_ids.push(id_str);
                    }
                    if let Some(op) = old_parent_to_touch.take() {
                        touched_ids.push(op);
                    }
                    if let Some(np) = new_parent.take() {
                        touched_ids.push(np);
                    }
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
                    return Ok((affected, updated_at, true, touched_ids));
                }
                affected += n;
                // UPDATE 成功 + 改了 parent：推进「行自身 + 旧父 + 新父」三者。
                if n > 0 && new_parent.is_some() {
                    if let Some(id_str) = non_empty_id_str(&id) {
                        touched_ids.push(id_str);
                    }
                    if let Some(op) = old_parent_to_touch.take() {
                        touched_ids.push(op);
                    }
                    if let Some(np) = new_parent.take() {
                        touched_ids.push(np);
                    }
                }
            }
        }
    }
    Ok((affected, updated_at, false, touched_ids))
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
    // 旧 parent 作为"受影响节点"加入重算集合——递归 CTE 从它展开会修正其 is_leaf
    // （删除后若无其他子，自动变回叶子）。
    let (del_affected, del_parents) = apply_deletes(mm, db_id, txn_id, view, bucket).await?;
    let mut affected = del_affected;

    // inserted：铸号 + upsert（分级字典同时收集「新增行自身 + 父 id」）。
    let (ins_affected, id_map, ins_ids) =
        apply_inserts(mm, db_id, txn_id, view, bucket).await?;
    affected += ins_affected;

    // updated：乐观锁 UPDATE + 回查 update_time（命中冲突提前返回）。
    // 分级字典 + 改 parent 时收集「行自身 + 旧父 + 新父」三者。
    let (upd_affected, updated_at, conflict, upd_ids) =
        apply_updates(mm, db_id, txn_id, view, bucket).await?;
    affected += upd_affected;

    // 分级字典三字段（level_no / full_path / is_leaf）级联维护：
    // 把所有受影响节点 id 合并，用一次递归 CTE 重算它们及其整棵子树。
    // - del：被删行的旧父（修正 is_leaf——删除后若无其他子则变回叶子）
    // - ins：新增行自身 + 它的父（行自身算 level_no/full_path；父置 is_leaf=0）
    // - upd：改 parent 的行自身 + 旧父 + 新父（行整子树重算；旧父可能变回叶子；新父置 0）
    // 仅 conflict=false（无乐观锁冲突）才重算——冲突时上层 save 会 rollback，无需重算。
    if !conflict {
        let pf = hierarchy_parent_field(view);
        if let Some(pf) = pf {
            let mut touched = del_parents;
            touched.extend(ins_ids);
            touched.extend(upd_ids);
            if !touched.is_empty() {
                recompute_hierarchy_subtree(mm, db_id, txn_id, view, &pf, &touched).await?;
            }
        }
    }

    Ok((affected, updated_at, conflict, id_map))
}

/// 编码引擎铸号：若字典配置了 codeRule(mode=auto)，为 code_field 为空的行铸业务编码。
///
/// 批量铸号（方案 §4.5 + §4.1 buffer 推进）：待铸号行收集后一次调 `mint_batch`，
/// engine 内按 prefix 分组 + buffer 推进，同 prefix 多行一次反查 max 取连续号
/// （修复附录 C.2.10/C.2.11）。
/// 未配置编码引擎（code_rule=None 或 GlobalCodeMinter 未注入）→ 静默跳过（现状零影响）。
/// 铸号失败记 warn 日志（不阻断主流程——编码失败不应阻断业务保存）。
pub(crate) async fn mint_codes_for_inserts(
    view: &DictView,
    rows: &mut [serde_json::Map<String, Value>],
    db_id: &str,
) {
    // 无 codeRule → 跳过
    let Some(code_rule) = &view.code_rule else {
        return;
    };

    // 非 auto mode（manual）→ 跳过（用户手敲）
    let mode = code_rule.get("mode").and_then(|v| v.as_str()).unwrap_or("manual");
    if mode != "auto" {
        return;
    }

    // 编码引擎未注入 → 跳过（现状零影响）
    let Some(minter) = cmx_traits::code::GlobalCodeMinter::get() else {
        return;
    };

    let code_field = &view.code_field;
    let target = serde_json::json!({
        "kind": "dct",
        "code": view.table_name,
        "field": code_field,
    });

    // 收集待铸号行的索引 + attrs（跳过已有 code 的行）
    let mut pending: Vec<(usize, Value)> = Vec::new();
    for (idx, row) in rows.iter().enumerate() {
        // 已有 code 值 → 跳过（前端手填或预览传的）
        if let Some(existing) = row.get(code_field) {
            if !existing.is_null() && existing.as_str().map(|s| !s.is_empty()).unwrap_or(false) {
                continue;
            }
        }
        // 行属性（供 ref 段取字段值 + condition 求值）
        let attrs = Value::Object(row.clone());
        pending.push((idx, attrs));
    }

    if pending.is_empty() {
        return;
    }

    // 批量铸号：一次调 mint_batch，engine 内按 prefix 分组 + buffer 推进
    let attrs_list: Vec<Value> = pending.iter().map(|(_, a)| a.clone()).collect();
    match minter
        .mint_batch(code_rule, &target, &attrs_list, db_id, None)
        .await
    {
        Ok(codes) => {
            for ((row_idx, _), code) in pending.iter().zip(codes.iter()) {
                rows[*row_idx].insert(code_field.clone(), Value::String(code.clone()));
            }
        }
        Err(e) => {
            tracing::warn!(
                target: "cmx_dct::mint_code",
                dict = %view.dict_code, field = %code_field, error = %e,
                row_count = pending.len(),
                "编码引擎批量铸号失败，跳过这些行（不阻断保存）"
            );
        }
    }
}
