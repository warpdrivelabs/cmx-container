//! DocSaver — 业务单据数据回存，双模式（方案 §6.4）
//!
//! 两种模式，由 `save_mode` 控制：
//!   - `merge`（默认）：按 changeset 精确 INSERT(UPSERT)/UPDATE/DELETE，保主键/审计、写入最小。
//!   - `replace`：对 rootId 子树先 DELETE 旧行、再全量 INSERT snapshot（前端免 diff）。
//!
//! 共性（两模式）：同一事务、按 relations 拓扑序（插/更父先、删子先）、参数化 DataValue 绑定。
//!
//! changeset 结构（merge，§6.3）：
//! ```json
//! { "cv_header": { "updated": [ { "id": "...", "fields": { "total_dr": 100 } } ],
//!                  "inserted": [ { "id": "...", "upper_id": "...", "fields": {...} } ],
//!                  "deleted": [ "id1", "id2" ] } }
//! ```

use chrono::{DateTime, Utc};
use serde_json::{Map, Value};
use std::collections::HashMap;

use cmx_core::model::cell::{DataValue, FieldType};
use cmx_core::model::data::dataset::Schema;
use cmx_database::DatabaseManager;

use super::loader::DocLoader;
use super::revision::{DocRevision, RevisionRecord};
use cmx_biz::{BizError, Result};
use cmx_doc_model::codec::{json_to_dv_loose, json_to_dv_typed};
use cmx_doc_model::meta::{DocMetaView, LayerView};
use cmx_doc_model::query::DocQuery;

/// 审计上下文（方案 C）：由服务端权威填充审计列，覆盖前端传值。
///
/// - `actor`：当前操作者用户 id（BIGINT）。缺失/非数字身份（如系统身份）兜底为 `0`（约定 0=系统）——
///   保存永不因身份缺失失败。
/// - `now`：本次保存的统一时间戳，`DocSaver::save` 入口一次性捕获，全事务共用。
#[derive(Debug, Clone, Copy)]
struct AuditCtx {
    actor: i64,
    now: DateTime<Utc>,
}

/// 保存上下文（操作者身份 + 单据定位 + 操作类型）。
///
/// 承载两件事：
///   - 审计填充（方案 C）：`actor_id` → [`AuditCtx`]。
///   - 版本快照（B1）：`actor_id`/`actor_name`/`doc_file`/`op_override` → [`DocRevision::record`]。
///
/// 由 handler 从 `CmxSvrContext` 构造（见 doc.rs `save_ctx`）。
#[derive(Debug, Clone, Default)]
pub struct SaveCtx {
    /// 操作者用户 id（BIGINT 审计列用；缺失/非数字兜底 0）。
    pub actor_id: i64,
    /// 操作者显示名（版本台账 actor_name 用；缺省 "系统"）。
    pub actor_name: String,
    /// 单据定义文件名（版本台账 doc_file 用，定位「哪种单据」）。
    pub doc_file: String,
    /// 操作类型覆盖（如 restore 传 Some("restore")）；None 时按 changeset 桶推断 create/update。
    pub op_override: Option<String>,
    /// 单据字段铸号规则覆盖 {field: ruleCode}（激活配置优先于单据元数据 codeRule）。
    /// 来自 save body 的 codeRuleOverrides（MDM cr-form 填 activation.doc_code_rules）。
    /// 默认空——非 MDM 单据零影响；mint_codes_for_changeset 据此覆盖对应字段的 ruleCode。
    pub code_rule_overrides: HashMap<String, String>,
}

/// 创建审计列（一旦写入不可变）：UPSERT 撞已存在 id 时，ON CONFLICT SET **不得**覆盖这两列，
/// 否则会把原创建人/创建时间冲掉。更新审计列（update_by/update_time）不在此列，仍随 EXCLUDED 刷新。
const CREATE_AUDIT_COLS: [&str; 2] = ["create_by", "create_time"];

/// 保存模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveMode {
    /// 增量：按 changeset 精确写。
    Merge,
    /// 先删后插：按 rootId 子树覆盖。
    Replace,
}

impl SaveMode {
    pub fn parse(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "replace" => SaveMode::Replace,
            _ => SaveMode::Merge,
        }
    }

    /// 模式名（"merge"/"replace"），供结果序列化。
    pub fn as_str(self) -> &'static str {
        match self {
            SaveMode::Merge => "merge",
            SaveMode::Replace => "replace",
        }
    }
}

/// 保存结果。
#[derive(Debug, Clone, serde::Serialize)]
pub struct SaveResult {
    pub ok: bool,
    pub mode: String,
    pub affected: u64,
    /// B2：本次保存后各根层已更新行的新 `update_time`（RFC3339），供前端刷新乐观锁基线，
    /// 支持「连续保存不刷新页」。空则不回传（`camelCase` 序列化为 `updatedAt`）。
    #[serde(rename = "updatedAt", skip_serializing_if = "Vec::is_empty")]
    pub updated_at: Vec<UpdatedBaseline>,
    /// 后端首次存储铸号后的「前端临时 id → 新真 id」映射（merge 新增行）。供前端把临时行的
    /// id 换成真号（避免「新建后立即再改」错位）。空则不回传（序列化为 `idMap`）。
    #[serde(rename = "idMap", skip_serializing_if = "Map::is_empty")]
    pub id_map: Map<String, Value>,
}

/// 一行更新后的新基线（id + 新 update_time）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct UpdatedBaseline {
    pub id: String,
    #[serde(rename = "updateTime")]
    pub update_time: String,
}

/// 批量保存的一个单据项（方案 F）。各单自带定义（meta）+ 模式 + changeset + 上下文，故一批可混多种单据。
pub struct BatchItem<'a> {
    pub meta: &'a DocMetaView,
    pub mode: SaveMode,
    pub changes: &'a Value,
    pub sctx: &'a SaveCtx,
}

/// 批量保存里单个单据的结果（方案 F）。`ok=false` 时 `error` 带原因（仅非 atomic 逐单模式会出现）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct BatchOutcome {
    /// 在 items 里的下标（定位是哪一单）。
    pub index: usize,
    pub ok: bool,
    pub mode: String,
    pub affected: u64,
    #[serde(rename = "updatedAt", skip_serializing_if = "Vec::is_empty")]
    pub updated_at: Vec<UpdatedBaseline>,
    #[serde(rename = "idMap", skip_serializing_if = "Map::is_empty")]
    pub id_map: Map<String, Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub struct DocSaver;

impl DocSaver {
    /// 回存单据。`changes` 为 merge 模式 changeset；`snapshot` 为 replace 模式整树列式包。
    /// `sctx` 为保存上下文（审计填充 + 版本快照，见 [`SaveCtx`]）。
    pub async fn save(
        mm: &DatabaseManager,
        db_id: &str,
        meta: &DocMetaView,
        mode: SaveMode,
        changes: &Value,
        sctx: &SaveCtx,
    ) -> Result<SaveResult> {
        let ctx = mm.get_transaction_context();
        let guard = ctx
            .begin_with_guard(db_id)
            .await
            .map_err(|e| BizError::internal(format!("开启事务失败: {e}")))?;
        let txn_id = guard.txn_id().to_string();

        // 写入 + 记版本 + 算基线（事务内一体，任一步失败即回滚）。
        let outcome = Self::apply_and_version(mm, db_id, &txn_id, meta, mode, changes, sctx).await;

        match outcome {
            Ok((affected, updated_at, id_map)) => {
                guard
                    .commit()
                    .await
                    .map_err(|e| BizError::internal(format!("提交事务失败: {e}")))?;
                Ok(SaveResult {
                    ok: true,
                    mode: mode.as_str().into(),
                    affected,
                    updated_at,
                    id_map,
                })
            }
            Err(e) => {
                // guard drop 自动回滚；显式 rollback 更清晰
                let _ = guard.rollback().await;
                Err(e)
            }
        }
    }

    /// 批量保存多单（方案 F）。一批可混多种单据（各 item 自带 meta/mode/changes/sctx）。
    ///
    /// - `atomic = true`：N 单共用**一个大事务**，任一单失败 → 整批回滚（真批量原子，过账/导入语义）。
    ///   返回的 outcomes 全 `ok=true`；失败时返回 `Err`（带出错单 index），无部分提交。
    /// - `atomic = false`：**每单独立事务**（复用单单 [`save`]），逐单成败互不影响，全部收进 outcomes
    ///   （失败单 `ok=false` + `error`），永不整体 `Err`（除非批本身非法）。
    ///
    /// C（审计）/B1（版本快照）/B2（乐观锁）对每单自动生效——批量只是外层事务编排，不改单单语义。
    pub async fn save_batch(
        mm: &DatabaseManager,
        db_id: &str,
        items: &[BatchItem<'_>],
        atomic: bool,
    ) -> Result<Vec<BatchOutcome>> {
        if atomic {
            Self::save_batch_atomic(mm, db_id, items).await
        } else {
            let mut outcomes = Vec::with_capacity(items.len());
            for (index, item) in items.iter().enumerate() {
                let outcome = match Self::save(
                    mm,
                    db_id,
                    item.meta,
                    item.mode,
                    item.changes,
                    item.sctx,
                )
                .await
                {
                    Ok(r) => BatchOutcome {
                        index,
                        ok: true,
                        mode: r.mode,
                        affected: r.affected,
                        updated_at: r.updated_at,
                        id_map: r.id_map,
                        error: None,
                    },
                    Err(e) => BatchOutcome {
                        index,
                        ok: false,
                        mode: item.mode.as_str().into(),
                        affected: 0,
                        updated_at: Vec::new(),
                        id_map: Map::new(),
                        error: Some(e.to_string()),
                    },
                };
                outcomes.push(outcome);
            }
            Ok(outcomes)
        }
    }

    /// atomic 批量：一个 guard 包住 N 单，任一失败即回滚整批。
    async fn save_batch_atomic(
        mm: &DatabaseManager,
        db_id: &str,
        items: &[BatchItem<'_>],
    ) -> Result<Vec<BatchOutcome>> {
        let ctx = mm.get_transaction_context();
        let guard = ctx
            .begin_with_guard(db_id)
            .await
            .map_err(|e| BizError::internal(format!("开启批量事务失败: {e}")))?;
        let txn_id = guard.txn_id().to_string();

        let mut outcomes = Vec::with_capacity(items.len());
        let mut failed: Option<BizError> = None;
        for (index, item) in items.iter().enumerate() {
            match Self::apply_and_version(
                mm,
                db_id,
                &txn_id,
                item.meta,
                item.mode,
                item.changes,
                item.sctx,
            )
            .await
            {
                Ok((affected, updated_at, id_map)) => outcomes.push(BatchOutcome {
                    index,
                    ok: true,
                    mode: item.mode.as_str().into(),
                    affected,
                    updated_at,
                    id_map,
                    error: None,
                }),
                Err(e) => {
                    // 定位到出错单（第 index 单），整批回滚。
                    failed = Some(BizError::from_batch_item(index, e));
                    break;
                }
            }
        }

        match failed {
            None => {
                guard
                    .commit()
                    .await
                    .map_err(|e| BizError::internal(format!("提交批量事务失败: {e}")))?;
                Ok(outcomes)
            }
            Some(e) => {
                let _ = guard.rollback().await;
                Err(e)
            }
        }
    }

    /// 事务内一体：写入（apply_merge/replace）+ 版本快照（record_versions）+ 算新基线（B2）。
    ///
    /// 抽出供单单 [`save`]（自开 guard）与批量 [`save_batch`]（多单共用一个 guard）复用——
    /// **不含 begin/commit/rollback**，事务生命周期由调用方掌握。任一步 `?` 失败即向上抛（调用方回滚）。
    /// 返回 (影响行数, 已更新根行新基线, 铸号 idMap)。审计时间戳每次调用独立捕获（`Utc::now()`）。
    async fn apply_and_version(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: &str,
        meta: &DocMetaView,
        mode: SaveMode,
        changes: &Value,
        sctx: &SaveCtx,
    ) -> Result<(u64, Vec<UpdatedBaseline>, Map<String, Value>)> {
        // 审计上下文：本单据全程共用一个时间戳，由服务端权威填充（覆盖前端传值）。
        let audit = AuditCtx {
            actor: sctx.actor_id,
            now: Utc::now(),
        };
        // 后端首次存储铸号（merge）：为 inserted 的「临时 id」行铸真号（52 位 JS 安全），
        // 并把子层外键（upper_id 或命名 childKey 如 header_id）指向的父临时 id 重指向为真号。
        // 全局两遍改写，与层序无关。在 apply/version/baseline **之前**做，确保写库、版本快照、
        // 基线都看到真号。
        let (changes_owned, id_map) = match mode {
            SaveMode::Merge => {
                // 本单据全部 childKey（去重）：upper_id + 命名外键。子行的父引用可能落在其中任一列。
                let mut child_keys: Vec<String> =
                    meta.relations.iter().map(|r| r.child_key.clone()).collect();
                child_keys.push("upper_id".to_string()); // 前端 collector 规范化的默认外键
                child_keys.sort();
                child_keys.dedup();
                mint_ids_for_changeset(changes, &child_keys)
            }
            SaveMode::Replace => (changes.clone(), Map::new()),
        };
        let mut changes_owned = changes_owned;
        // 编码引擎铸号：遍历每张挂了 codeRule(mode=auto) 的层，为 code_field 为空的行铸业务编码。
        // 未配置编码引擎（code_rule=None 或 GlobalCodeMinter 未注入）→ 静默跳过（现状零影响）。
        mint_codes_for_changeset(&mut changes_owned, meta, db_id, txn_id, &sctx.code_rule_overrides).await;
        let changes = &changes_owned;
        let affected = match mode {
            SaveMode::Merge => Self::apply_merge(mm, db_id, txn_id, meta, changes, &audit).await?,
            SaveMode::Replace => {
                Self::apply_replace(mm, db_id, txn_id, meta, changes, &audit).await?
            }
        };
        // B1：版本快照接线 —— 仅当单据定义开启 versioning 时记（record 内部再判一次，双保险）。
        Self::record_versions(mm, db_id, txn_id, meta, mode, changes, sctx).await?;
        // B2：本次已更新根行的新基线（新 update_time = audit.now，服务端已知无需再查）。
        let updated_at = new_root_baselines(changes, meta, mode, &audit);
        Ok((affected, updated_at, id_map))
    }

    // ─────────────────── merge 模式 ───────────────────

    async fn apply_merge(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: &str,
        meta: &DocMetaView,
        changes: &Value,
        audit: &AuditCtx,
    ) -> Result<u64> {
        let mut affected: u64 = 0;
        // 对账分两类：
        //   write_expected/write_affected —— INSERT(UPSERT)+UPDATE，须严格相等（每行必落地 1 行）。
        //   delete 允许幂等空删（前端可能删已不存在的行），不纳入严格对账，仅累加实际数。
        let mut write_expected: u64 = 0;
        let mut write_affected: u64 = 0;
        let changes = changes
            .as_object()
            .ok_or_else(|| BizError::business("changes 必须是对象"))?;

        // 静默零写防护（H1）：changes 里每个 key 都必须能对上某一层，否则报错而非静默丢弃。
        Self::assert_all_keys_matched(changes, meta)?;

        // 落库前列级校验（类型/长度/精度/非空）：逐层 inserted 整行校验 + updated 字段校验。
        // 有违规 → BizError::Validation（handler 转 422 + 结构化 violations），开写前拦截。
        let violations = Self::validate_changeset(changes, meta);
        if !violations.is_empty() {
            return Err(BizError::validation(violations));
        }

        // 父先：按 layer_order 正序 批量 UPSERT / UPDATE
        for (idx, layer_id) in meta.layer_order.iter().enumerate() {
            let Some(layer) = meta.layer(layer_id) else {
                continue;
            };
            let Some(layer_changes) = layer_changes_for(changes, meta, idx, layer) else {
                continue;
            };

            if let Some(rows) = layer_changes.get("inserted").and_then(|v| v.as_array())
                && !rows.is_empty()
            {
                write_expected += rows.len() as u64;
                write_affected += Self::upsert_rows(mm, db_id, txn_id, layer, rows, audit).await?;
            }
            if let Some(rows) = layer_changes.get("updated").and_then(|v| v.as_array())
                && !rows.is_empty()
            {
                write_expected += rows.len() as u64;
                // B2 乐观锁仅根层（idx==0）：带前端回传的 update_time 基线做并发冲突检测。
                let oplock = idx == 0;
                write_affected +=
                    Self::update_rows(mm, db_id, txn_id, layer, rows, audit, oplock).await?;
            }
        }
        affected += write_affected;

        // 子先：按 layer_order 逆序 批量 DELETE（幂等，不纳入严格对账）
        for (idx, layer_id) in meta.layer_order.iter().enumerate().rev() {
            let Some(layer) = meta.layer(layer_id) else {
                continue;
            };
            let Some(layer_changes) = layer_changes_for(changes, meta, idx, layer) else {
                continue;
            };
            if let Some(ids) = layer_changes.get("deleted").and_then(|v| v.as_array())
                && !ids.is_empty()
            {
                affected += Self::delete_ids(mm, db_id, txn_id, layer, ids).await?;
            }
        }

        // 对账（H1/H2）：INSERT+UPDATE 每行必须精确落地。实际 < 期望 = 有行未写
        // （UPDATE 命中 0 行：id 不存在/被并发删；UPSERT 本应恒等），报错回滚，杜绝「假成功」。
        if write_affected < write_expected {
            return Err(BizError::business(format!(
                "回存对账失败：写入期望 {write_expected} 行，实际 {write_affected} 行（有行 id 不存在或被并发修改）"
            )));
        }

        Ok(affected)
    }

    // ─────────────────── replace 模式 ───────────────────

    async fn apply_replace(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: &str,
        meta: &DocMetaView,
        snapshot: &Value,
        audit: &AuditCtx,
    ) -> Result<u64> {
        // snapshot 结构同 merge 的按层 { table: { rows: [ {id,upper_id,fields} ] } }
        // 简化：replace 也按层给全量行；先删（子先）、再插（父先）。
        let obj = snapshot
            .as_object()
            .ok_or_else(|| BizError::business("snapshot 必须是对象"))?;
        let mut affected: u64 = 0;

        // 收集 rootId（根层 rows 的 id），界定删除范围
        let root = meta
            .root_layer()
            .ok_or_else(|| BizError::business("单据无根层"))?;
        let root_ids: Vec<Value> = obj
            .get(&root.table_name)
            .or_else(|| obj.get(&root.id))
            .and_then(|l| l.get("rows"))
            .and_then(|v| v.as_array())
            .map(|rows| rows.iter().filter_map(|r| r.get("id").cloned()).collect())
            .unwrap_or_default();

        if root_ids.is_empty() {
            return Err(BizError::business(
                "replace 模式必须提供根层 rows 以界定覆盖范围",
            ));
        }

        // 先删：子先父后，沿 upper_id 链圈定 rootId 子树（方案 E）。
        // 旧实现逐层 SELECT id 收集再删（O(层×关系) 次往返）；
        // 新实现用「子查询链」直接 DELETE —— 每层一条 DELETE，WHERE 用嵌套子查询上溯到根层，
        // 零预 SELECT、无 id 物化（避免并发漂移），往返数 = 层数。
        for layer_id in meta.layer_order.iter().rev() {
            let Some(layer) = meta.layer(layer_id) else {
                continue;
            };
            affected +=
                Self::delete_subtree_layer(mm, db_id, txn_id, meta, layer, &root_ids).await?;
        }

        // 再插：父先，按 snapshot 各层 rows 批量 INSERT
        for layer_id in &meta.layer_order {
            let Some(layer) = meta.layer(layer_id) else {
                continue;
            };
            let Some(rows) = obj
                .get(&layer.table_name)
                .or_else(|| obj.get(layer_id))
                .and_then(|l| l.get("rows"))
                .and_then(|v| v.as_array())
            else {
                continue;
            };
            if !rows.is_empty() {
                affected += Self::insert_rows(mm, db_id, txn_id, layer, rows, audit).await?;
            }
        }

        Ok(affected)
    }

    // ─────────────────── 版本快照（B1：事务内接线 DocRevision::record） ───────────────────

    /// 保存事务内、写入后 commit 前，为**受影响的每个 rootId** 重装载整单 → 列式快照 → 追加一版。
    ///
    /// 「仅 versioning 开启时记」：单据定义 `moduleMeta.versioning.enabled != true` 直接返回，
    /// 通用存储层不强制版本化，是否开启由定义控制。
    ///
    /// 关键（B1 核心）：用 [`DocLoader::load_txn`] 传 `Some(txn_id)` 在**同一事务连接**上重装载，
    /// 才能看到本事务刚写入、尚未 commit 的行 —— 快照因此反映「保存后」的整单终态。
    async fn record_versions(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: &str,
        meta: &DocMetaView,
        mode: SaveMode,
        changes: &Value,
        sctx: &SaveCtx,
    ) -> Result<()> {
        if !meta.versioning_enabled() {
            return Ok(());
        }
        let root = meta
            .root_layer()
            .ok_or_else(|| BizError::business("单据无根层，无法版本化"))?;

        // 受影响 rootId + 每个的 op（create/update）。本期只快照仍存在的根（inserted/updated）；
        // deleted 根的版本化（记一版 op=delete 的空/末态快照）留待后续。
        let roots = collect_versioned_roots(changes, meta, mode, root, sctx)?;

        for (root_id, op) in roots {
            // 单根 DocQuery：根层 filter id = <root_id>，全深度、含兄弟表。
            let dq = DocQuery::from_json(&serde_json::json!({
                "layers": { root.id.clone(): { "filter": { "id": root_id.clone() } } }
            }))?;
            // 事务内重装载（看得到未提交写）→ 整单终态 DataSet。
            let root_ds = DocLoader::load_txn(mm, db_id, meta, &dq, Some(txn_id)).await?;
            let actor_id = sctx.actor_id.to_string();
            // record 内部用 ColumnarCodec 编列式快照存 JSONB(与装载同序列化器)。
            DocRevision::record(
                mm,
                db_id,
                txn_id,
                &RevisionRecord {
                    doc_file: &sctx.doc_file,
                    root_table: &root.table_name,
                    root_id: &root_id,
                    op: &op,
                    root_ds: &root_ds,
                    actor_id: Some(&actor_id),
                    actor_name: Some(&sctx.actor_name),
                    reason: None,
                    biz_status: None,
                },
            )
            .await?;
        }
        Ok(())
    }

    // ─────────────────── 批量 SQL（方案 A：消除逐行 N+1） ───────────────────

    /// sqlx/PG 单条语句参数上限 65535，留余量取 60000。按 列数 折算每批最大行数。
    const MAX_PARAMS: usize = 60000;

    /// 批量 UPSERT：同层多行合并为多值 INSERT ... ON CONFLICT(id) DO UPDATE。
    /// 各行列集可能不同（fields 稀疏），按「列集合」分组，每组一条多值语句。
    /// 走单值 DataValue 绑定（覆盖 Decimal/Date/Float 全类型；数组绑定不支持这些，故不用 UNNEST）。
    async fn upsert_rows(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: &str,
        layer: &LayerView,
        rows: &[Value],
        audit: &AuditCtx,
    ) -> Result<u64> {
        Self::batch_insert_grouped(mm, db_id, txn_id, layer, rows, true, audit).await
    }

    /// 批量纯 INSERT（replace 模式；子树已先删）。
    async fn insert_rows(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: &str,
        layer: &LayerView,
        rows: &[Value],
        audit: &AuditCtx,
    ) -> Result<u64> {
        Self::batch_insert_grouped(mm, db_id, txn_id, layer, rows, false, audit).await
    }

    /// 批量 INSERT 内核：按列集合分组 → 每组多值 INSERT（可选 ON CONFLICT UPSERT），按参数上限分批。
    async fn batch_insert_grouped(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: &str,
        layer: &LayerView,
        rows: &[Value],
        upsert: bool,
        audit: &AuditCtx,
    ) -> Result<u64> {
        use std::collections::BTreeMap;
        // 按「列名序列」分组：同列集的行走同一条多值 INSERT
        let mut groups: BTreeMap<Vec<String>, Vec<Vec<DataValue>>> = BTreeMap::new();
        for row in rows {
            let (mut cols, mut vals) = Self::row_cols_vals(layer, row)?;
            // 审计填充（方案 C）：服务端权威写 create_*/update_*/delete_flag，覆盖前端传值。
            // 审计列对全表统一，反使按列集分组更聚合（同层新增行通常收敛到一组）。
            apply_audit_insert(&mut cols, &mut vals, &layer.schema, audit);
            if cols.is_empty() {
                continue;
            }
            groups.entry(cols).or_default().push(vals);
        }

        let mut affected: u64 = 0;
        for (cols, value_rows) in groups {
            let ncol = cols.len();
            if ncol == 0 {
                continue;
            }
            // 按列类型构建占位符强转数组（$p::bigint / $p::jsonb / …）：规避 ON CONFLICT
            // 上下文参数类型推断退化为 text（jsonb 早已单修，现推广到全部类型列——
            // 修复明细行 line_target_id 落 NULL 报 bigint=text）。
            let col_casts: Vec<&str> = cols
                .iter()
                .map(|c| {
                    layer
                        .schema
                        .get_index(c)
                        .and_then(|i| layer.schema.fields.get(i))
                        .map(|f| pg_cast_for(&f.field_type))
                        .unwrap_or("")
                })
                .collect();
            let rows_per_batch = (Self::MAX_PARAMS / ncol).max(1);
            for chunk in value_rows.chunks(rows_per_batch) {
                let sql = build_multi_insert_sql(
                    &layer.table_name,
                    &cols,
                    chunk.len(),
                    upsert,
                    &col_casts,
                );
                let flat: Vec<DataValue> = chunk.iter().flatten().cloned().collect();
                affected += Self::exec(mm, db_id, txn_id, &sql, flat).await?;
            }
        }
        Ok(affected)
    }

    /// 批量 UPDATE：按「变更列集合」分组，每组一条 `UPDATE ... SET c=v.c FROM (VALUES ...) AS v(id,cols) WHERE t.id=v.id`。
    /// 各行变更列不同（updated.fields 稀疏），故先分组。
    ///
    /// `oplock`（B2 乐观锁，仅根层传 true）：带 `baseline`（前端回传的装载时 update_time）的行，UPDATE
    /// 加 `AND update_time = baseline` 谓词——基线陈旧则该行不更新，由对账判为冲突（409）。
    /// 分组键含 `has_baseline`，保证同组 SQL 结构一致（带锁/不带锁不混批）。
    async fn update_rows(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: &str,
        layer: &LayerView,
        rows: &[Value],
        audit: &AuditCtx,
        oplock: bool,
    ) -> Result<u64> {
        use std::collections::BTreeMap;
        // 分组键 = (排序后的变更列名, 是否带基线锁)；值 = 每行 (id_dv, [col_dv...], baseline?)。
        let has_update_time = layer.schema.get_index("update_time").is_some();
        type Grp = Vec<(DataValue, Vec<DataValue>, Option<DataValue>)>;
        let mut groups: BTreeMap<(Vec<String>, bool), Grp> = BTreeMap::new();
        for row in rows {
            let Some(id) = row.get("id") else {
                return Err(BizError::business("updated 行缺少 id"));
            };
            let fields = row
                .get("fields")
                .and_then(|v| v.as_object())
                .cloned()
                .unwrap_or_default();
            // 只取定义里的列（防注入），排序保证同列集分到一组
            let mut cols: Vec<String> = fields
                .keys()
                .filter(|c| layer.schema.get_index(c).is_some())
                .cloned()
                .collect();
            cols.sort();
            if cols.is_empty() {
                continue; // 无业务变更列 → 跳过整行（不因审计而写幽灵更新）
            }
            let id_dv = dv_for_col(id, layer, "id");
            let mut col_vals: Vec<DataValue> = cols
                .iter()
                .map(|c| dv_for_col(&fields[c], layer, c))
                .collect();
            // 审计填充（方案 C）：强制注入/覆盖 update_by、update_time（服务端权威，覆盖前端传值）。
            apply_audit_update(&mut cols, &mut col_vals, &layer.schema, audit);
            // B2：仅根层 + 该行带 baseline + 表有 update_time 列 → 走带锁分支。
            // baseline 是「装载时」的 update_time（WHERE 比旧值）；SET 里写的是新值（apply_audit_update），二者分离。
            let baseline = if oplock && has_update_time {
                row.get("baseline")
                    .filter(|b| !b.is_null())
                    .map(|b| dv_for_col(b, layer, "update_time"))
            } else {
                None
            };
            groups
                .entry((cols, baseline.is_some()))
                .or_default()
                .push((id_dv, col_vals, baseline));
        }

        let mut affected: u64 = 0;
        for ((cols, locked), id_rows) in groups {
            let per_row = cols.len() + 1 + if locked { 1 } else { 0 }; // id + cols [+ baseline]
            let rows_per_batch = (Self::MAX_PARAMS / per_row).max(1);
            for chunk in id_rows.chunks(rows_per_batch) {
                let oplock_col = if locked { Some("update_time") } else { None };
                // 按列类型构建占位符强转数组（与 batch_insert_grouped 对称）：规避
                // FROM (VALUES ($p)) 无列上下文时参数被推断为 text。
                let col_casts: Vec<&str> = cols
                    .iter()
                    .map(|c| {
                        layer
                            .schema
                            .get_index(c)
                            .and_then(|i| layer.schema.fields.get(i))
                            .map(|f| pg_cast_for(&f.field_type))
                            .unwrap_or("")
                    })
                    .collect();
                let sql = build_multi_update_sql(
                    &layer.table_name,
                    &cols,
                    chunk.len(),
                    oplock_col,
                    &col_casts,
                );
                // 展平参数：每行 (id, col1..colN [, baseline])
                let mut flat: Vec<DataValue> = Vec::with_capacity(chunk.len() * per_row);
                for (id_dv, col_vals, baseline) in chunk {
                    flat.push(id_dv.clone());
                    flat.extend(col_vals.iter().cloned());
                    if let Some(b) = baseline {
                        flat.push(b.clone());
                    }
                }
                let got = Self::exec(mm, db_id, txn_id, &sql, flat).await?;
                // B2 冲突判定：带锁组若命中 < 期望，补 SELECT 区分「冲突」vs「行不存在」。
                if locked && got < chunk.len() as u64 {
                    let ids: Vec<DataValue> = chunk.iter().map(|(id, _, _)| id.clone()).collect();
                    Self::classify_update_shortfall(mm, db_id, txn_id, layer, &ids).await?;
                }
                affected += got;
            }
        }
        Ok(affected)
    }

    /// 带锁 UPDATE 命中不足时判因（B2）：对本组 id 补一次 `SELECT id WHERE id = ANY($1)`。
    /// 存在的 id（却没更新成功）= 基线陈旧 → 冲突 409；不存在 = 行已被删/id 错 → 沿用 H2 业务错。
    async fn classify_update_shortfall(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: &str,
        layer: &LayerView,
        ids: &[DataValue],
    ) -> Result<()> {
        let sql = format!(
            "SELECT id FROM {} WHERE id = ANY($1)",
            quote_ident(&layer.table_name)
        );
        let ds = mm
            .query_sql_with_datavalues(
                db_id,
                Some(txn_id),
                &sql,
                vec![DataValue::Array(ids.to_vec())],
                "oplock_probe",
            )
            .await
            .map_err(|e| BizError::internal(format!("冲突判定查询失败: {e}")))?;
        // 有任一 id 仍存在（却没被 UPDATE 命中）→ 基线不符 = 并发冲突。
        if !ds.rows.is_empty() {
            return Err(BizError::conflict(
                "单据已被他人修改，请刷新后重试（乐观锁冲突）",
            ));
        }
        // 全部 id 都不存在 → 非冲突，交回原 H2 对账报「行不存在」业务错。
        Err(BizError::business(
            "回存的行不存在（可能已被删除），请刷新后重试",
        ))
    }

    // ─────────────────── 批量 DELETE / 子层圈定 ───────────────────

    /// 删除某层「属于本次 rootId 子树」的行（方案 E：子查询链，零预 SELECT）。
    ///
    /// 对 layer_order 中第 i 层，构造 WHERE 子查询链上溯到根层（第 0 层）：
    ///   根层：  `DELETE FROM L0 WHERE id = ANY($1)`
    ///   第 i 层：`DELETE FROM Li WHERE {ck_i} IN (SELECT id FROM L(i-1) WHERE {ck_(i-1)} IN (... WHERE id = ANY($1)))`
    /// 其中 ck_k = 第 k 层相对其父的 childKey（按 layer_order 相邻推导，独立于 relation 命名）。
    async fn delete_subtree_layer(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: &str,
        meta: &DocMetaView,
        layer: &LayerView,
        root_ids: &[Value],
    ) -> Result<u64> {
        let Some(depth) = meta.layer_order.iter().position(|id| id == &layer.id) else {
            return Ok(0);
        };
        let root = meta
            .root_layer()
            .ok_or_else(|| BizError::business("单据无根层"))?;

        // 自内向外构造子查询链：最内层是根层 `id = ANY($1)`
        // sql_inner 初始 = 根层选择条件的表 + WHERE
        let mut inner = format!(
            "SELECT id FROM {} WHERE id = ANY($1)",
            quote_ident(&root.table_name)
        );
        // 从第 1 层到第 depth 层，逐层包裹（第 depth 层是目标层，其 childKey 用于最外 WHERE）
        // 目标层自身的 WHERE 由外层 DELETE 提供，故这里只需包到 depth-1 层的 SELECT。
        for k in 1..depth {
            let Some(mid) = meta.layer(&meta.layer_order[k]) else {
                return Ok(0);
            };
            let ck = meta
                .child_key_for_child(&mid.id)
                .unwrap_or_else(|| "upper_id".to_string());
            inner = format!(
                "SELECT id FROM {} WHERE {} IN ({})",
                quote_ident(&mid.table_name),
                quote_ident(&ck),
                inner
            );
        }

        let sql = if depth == 0 {
            // 根层：直接按 id 删
            format!(
                "DELETE FROM {} WHERE id = ANY($1)",
                quote_ident(&root.table_name)
            )
        } else {
            let ck = meta
                .child_key_for_child(&layer.id)
                .unwrap_or_else(|| "upper_id".to_string());
            format!(
                "DELETE FROM {} WHERE {} IN ({})",
                quote_ident(&layer.table_name),
                quote_ident(&ck),
                inner
            )
        };

        let dv_ids: Vec<DataValue> = root_ids.iter().map(|v| dv_for_col(v, root, "id")).collect();
        Self::exec(mm, db_id, txn_id, &sql, vec![DataValue::Array(dv_ids)]).await
    }

    /// DELETE WHERE id = ANY($1)。删前对挂了 code_rule(auto+enableGap) 的层记断号。
    async fn delete_ids(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: &str,
        layer: &LayerView,
        ids: &[Value],
    ) -> Result<u64> {
        // 删行记断号：有 code_rule + 引擎注入时，先批量 SELECT 旧 code + 整行，解析记断号
        if let Some(minter) = cmx_traits::code::GlobalCodeMinter::get() {
            if let Some(code_rule) = &layer.code_rule {
                let field = code_rule
                    .get("field")
                    .and_then(|v| v.as_str())
                    .unwrap_or("doc_no");
                // 批量 SELECT 旧 code + 整行（事务内，删前）
                if let Ok(old_rows) =
                    Self::select_rows_for_gap(mm, db_id, txn_id, layer, field, ids).await
                {
                    for row in &old_rows {
                        if let Some(code) = row.get(field).and_then(|v| v.as_str()) {
                            if !code.is_empty() {
                                minter.record_gap_for_code(code_rule, code, row, db_id).await;
                            }
                        }
                    }
                }
            }
        }
        let dv_ids: Vec<DataValue> = ids.iter().map(|v| dv_for_col(v, layer, "id")).collect();
        let sql = format!(
            "DELETE FROM {} WHERE id = ANY($1)",
            quote_ident(&layer.table_name)
        );
        Self::exec(mm, db_id, txn_id, &sql, vec![DataValue::Array(dv_ids)]).await
    }

    /// 删行前批量 SELECT 被删行整行（事务内），供记断号用。
    async fn select_rows_for_gap(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: &str,
        layer: &LayerView,
        field: &str,
        ids: &[Value],
    ) -> Result<Vec<Value>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let sql = format!(
            "SELECT * FROM {} WHERE id = ANY($1)",
            quote_ident(&layer.table_name)
        );
        let dv_ids: Vec<DataValue> = ids.iter().map(|v| dv_for_col(v, layer, "id")).collect();
        let ds = mm
            .query_sql_with_datavalues(
                db_id,
                Some(txn_id),
                &sql,
                vec![DataValue::Array(dv_ids)],
                "del_rows",
            )
            .await
            .map_err(|e| {
                tracing::warn!(
                    target: "cmx_doc::saver",
                    table = %layer.table_name, field = field, error = %e,
                    "select_rows_for_gap 失败（不阻断删行）"
                );
                BizError::from_db_error(&e.to_string())
            })?;
        let ds_val = serde_json::to_value(&ds).unwrap_or_default();
        let rows = ds_val
            .get("rows")
            .and_then(|r| r.as_array())
            .cloned()
            .unwrap_or_default();
        Ok(rows)
    }

    /// 从 row {id, upper_id, fields} 拼列名+值（只取定义里的列）。
    fn row_cols_vals(layer: &LayerView, row: &Value) -> Result<(Vec<String>, Vec<DataValue>)> {
        let mut cols = Vec::new();
        let mut vals = Vec::new();
        // 顶层 id / upper_id / line_no（若在 schema）
        for top in ["id", "upper_id", "line_no"] {
            if layer.schema.get_index(top).is_some()
                && let Some(v) = row.get(top)
            {
                cols.push(top.to_string());
                vals.push(dv_for_col(v, layer, top));
            }
        }
        // fields 里的业务列
        if let Some(fields) = row.get("fields").and_then(|v| v.as_object()) {
            for (col, v) in fields {
                if layer.schema.get_index(col).is_some() && !cols.contains(col) {
                    cols.push(col.clone());
                    vals.push(dv_for_col(v, layer, col));
                }
            }
        }
        Ok((cols, vals))
    }

    async fn exec(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: &str,
        sql: &str,
        params: Vec<DataValue>,
    ) -> Result<u64> {
        mm.execute_sql_with_datavalues(db_id, Some(txn_id), sql, params)
            .await
            // 落库失败：把 PG 原始错误翻译成优雅提示 + 稳定错误码（唯一键/外键/非空等），
            // 不再暴露英文原文 + SQL。前置列校验已拦大部分，此为兜底。
            .map_err(|e| {
                let raw = e.to_string();
                let biz = BizError::from_db_error(&raw);
                // UNIQUE 冲突时补充提示（DOC 批量 INSERT 不逐行重试：
                // use_sequence=true 时发号序列表保证唯一；use_sequence=false 时建议开启）
                if matches!(biz, BizError::DbConstraint { code: cmx_biz::errcode::CmxErrCode::UniqueViolation, .. }) {
                    tracing::warn!(
                        target: "cmx_doc::saver::exec",
                        error = %e,
                        "落库 UNIQUE 冲突（建议规则开启 use_sequence=true 规避并发冲突）"
                    );
                } else {
                    tracing::error!(
                        target: "cmx_doc::saver::exec",
                        error = %e,
                        "落库失败（PG 原文见此日志，handler 返回优雅文案）"
                    );
                }
                biz
            })
    }

    /// 静默零写防护（H1）：changes 里每个 key 必须能对上某一层，否则报错。
    /// 防前端 path 约定漂移（表名 vs 嵌套路径）导致「保存成功却一行没写」。
    fn assert_all_keys_matched(changes: &Map<String, Value>, meta: &DocMetaView) -> Result<()> {
        // 构造所有合法 key：每层的 表名 / 层 id / 嵌套全路径
        let mut valid: std::collections::HashSet<String> = std::collections::HashSet::new();
        for (idx, layer_id) in meta.layer_order.iter().enumerate() {
            if let Some(layer) = meta.layer(layer_id) {
                valid.insert(layer.table_name.clone());
                valid.insert(layer.id.clone());
            }
            valid.insert(meta.layer_order[..=idx].join("."));
        }
        for key in changes.keys() {
            if !valid.contains(key) {
                return Err(BizError::business(format!(
                    "changeset 含未知层 key「{key}」，无法对应任何层（防静默零写）"
                )));
            }
        }
        Ok(())
    }

    /// 落库前列级校验：逐层 inserted 整行校验（含 NOT NULL，跳过服务端 backfill 列）+
    /// updated 字段校验（不做整表 NOT NULL）。返回全部 [`Violation`]（不遇错即停）。
    fn validate_changeset(
        changes: &Map<String, Value>,
        meta: &DocMetaView,
    ) -> Vec<cmx_biz::errcode::Violation> {
        use cmx_biz::validation::{ValidateOptions, validate_insert_row, validate_update_fields};
        let vopts_insert = ValidateOptions {
            server_filled: DOC_SERVER_FILLED_COLS,
            server_replaced: DOC_SERVER_REPLACED_COLS,
            check_unknown: false,
            check_not_null: true,
        };
        let vopts_update = ValidateOptions {
            server_filled: DOC_SERVER_FILLED_COLS,
            server_replaced: DOC_SERVER_REPLACED_COLS,
            check_unknown: false,
            check_not_null: false,
        };
        let mut out = Vec::new();
        for (idx, layer_id) in meta.layer_order.iter().enumerate() {
            let Some(layer) = meta.layer(layer_id) else {
                continue;
            };
            let Some(lc) = layer_changes_for(changes, meta, idx, layer) else {
                continue;
            };
            // inserted：{ id, upper_id?, fields:{...} } → 铺平成整行对象再校验（含顶层 id/upper_id）。
            if let Some(rows) = lc.get("inserted").and_then(|v| v.as_array()) {
                for (i, row) in rows.iter().enumerate() {
                    let flat = flatten_insert_row(row);
                    out.extend(validate_insert_row(
                        &layer.spec,
                        &flat,
                        Some(i),
                        &vopts_insert,
                    ));
                }
            }
            // updated：只校验 fields。
            if let Some(rows) = lc.get("updated").and_then(|v| v.as_array()) {
                for (i, row) in rows.iter().enumerate() {
                    if let Some(fields) = row.get("fields").and_then(|v| v.as_object()) {
                        out.extend(validate_update_fields(
                            &layer.spec,
                            fields,
                            Some(i),
                            &vopts_update,
                        ));
                    }
                }
            }
        }
        out
    }
}

/// DOC 服务端 backfill 列（仅影响 NOT NULL；用户提供值仍校验）——审计人/删除标识。
const DOC_SERVER_FILLED_COLS: &[&str] = &["create_by", "update_by", "delete_flag"];

/// DOC 服务端**始终替换值**的列（完全跳过值校验）——id 铸号、结构键、时间戳。
const DOC_SERVER_REPLACED_COLS: &[&str] =
    &["id", "upper_id", "line_no", "create_time", "update_time"];

/// 把 inserted 行 `{ id, upper_id?, line_no?, fields:{...} }` 铺平成整行对象（顶层键 + fields）。
fn flatten_insert_row(row: &Value) -> Map<String, Value> {
    let mut flat = Map::new();
    for top in ["id", "upper_id", "line_no"] {
        if let Some(v) = row.get(top) {
            flat.insert(top.to_string(), v.clone());
        }
    }
    if let Some(fields) = row.get("fields").and_then(|v| v.as_object()) {
        for (k, v) in fields {
            flat.insert(k.clone(), v.clone());
        }
    }
    flat
}

/// 从 changes 里取某层的变更桶，兼容三种 key 形态：
///   - 表名（如 `cv_acc_line`）
///   - 层 id（同表名）
///   - **嵌套全路径**（前端 collector 用，如 `cv_batch.cv_header.cv_acc_line`）
///
/// 前端 ChangeSetCollector 的 path 是 schema 嵌套路径（rootId + 各层 child id 累积），
/// 故子层 key 是嵌套路径而非表名 —— 这里补上嵌套路径匹配，避免子层保存匹配不到（affected 0）。
fn layer_changes_for<'a>(
    changes: &'a serde_json::Map<String, Value>,
    meta: &DocMetaView,
    layer_idx: usize,
    layer: &LayerView,
) -> Option<&'a Value> {
    // ① 表名 / 层 id
    if let Some(v) = changes
        .get(&layer.table_name)
        .or_else(|| changes.get(&layer.id))
    {
        return Some(v);
    }
    // ② 嵌套全路径：layer_order[0..=layer_idx].join(".")
    let nested = meta.layer_order[..=layer_idx].join(".");
    changes.get(&nested)
}

/// 收集本次保存**受影响的根单据**及其操作类型（B1 版本快照用）。
///
/// - merge：读根层桶的 `inserted`（op=create）与 `updated`（op=update），各取行 id。
/// - replace：取根层 rows 的 id，op 统一 `update`（先删后插，视为整单覆盖）。
/// - `op_override`（如 restore）优先于上面推断。
///
/// 只收「仍存在」的根（inserted/updated / replace rows）——本期不为 deleted 根记版本。
/// id 统一字符串化（`cmx_doc_revision.root_id` 为 VARCHAR）。返回 (root_id, op) 列表（去重、稳定序）。
fn collect_versioned_roots(
    changes: &Value,
    meta: &DocMetaView,
    mode: SaveMode,
    root: &LayerView,
    sctx: &SaveCtx,
) -> Result<Vec<(String, String)>> {
    let obj = changes
        .as_object()
        .ok_or_else(|| BizError::business("changes 必须是对象"))?;
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut out: Vec<(String, String)> = Vec::new();
    let mut push = |id: &Value, op: &str, out: &mut Vec<(String, String)>| {
        if let Some(s) = value_to_id_string(id)
            && seen.insert(s.clone())
        {
            let op = sctx.op_override.clone().unwrap_or_else(|| op.to_string());
            out.push((s, op));
        }
    };

    match mode {
        SaveMode::Merge => {
            let Some(bucket) = layer_changes_for(obj, meta, 0, root) else {
                return Ok(out); // 根层无变更（只动了子层）：本期不为其单独记版本
            };
            for (b, op) in [("inserted", "create"), ("updated", "update")] {
                if let Some(rows) = bucket.get(b).and_then(|v| v.as_array()) {
                    for r in rows {
                        if let Some(id) = r.get("id") {
                            push(id, op, &mut out);
                        }
                    }
                }
            }
        }
        SaveMode::Replace => {
            let rows = obj
                .get(&root.table_name)
                .or_else(|| obj.get(&root.id))
                .and_then(|l| l.get("rows"))
                .and_then(|v| v.as_array());
            if let Some(rows) = rows {
                for r in rows {
                    if let Some(id) = r.get("id") {
                        push(id, "update", &mut out);
                    }
                }
            }
        }
    }
    Ok(out)
}

/// changeset 里的 id 值（可能是 JSON 数字或字符串）→ 稳定字符串。null/其它 → None。
fn value_to_id_string(v: &Value) -> Option<String> {
    match v {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// B2：本次保存后各根层已更新行的新基线（id + 新 update_time = `audit.now`）。
///
/// 只对 merge 的根层 `updated` 行回传（replace 是整树覆盖，前端会整体重载，无需增量刷基线）。
/// 根表无 `update_time` 列时返回空（该单据不参与乐观锁）。新 update_time 对所有根更新行相同
/// （= `apply_audit_update` 写入的 `audit.now`），故服务端直接取，无需回查库。
fn new_root_baselines(
    changes: &Value,
    meta: &DocMetaView,
    mode: SaveMode,
    audit: &AuditCtx,
) -> Vec<UpdatedBaseline> {
    if mode != SaveMode::Merge {
        return Vec::new();
    }
    let Some(root) = meta.root_layer() else {
        return Vec::new();
    };
    if root.schema.get_index("update_time").is_none() {
        return Vec::new();
    }
    let Some(obj) = changes.as_object() else {
        return Vec::new();
    };
    let Some(bucket) = layer_changes_for(obj, meta, 0, root) else {
        return Vec::new();
    };
    let new_ts = DataValue::DateTime(audit.now);
    // 与 apply_audit_update 写库口径一致：DataValue::DateTime 的序列化即 to_rfc3339。
    let ts_str = match serde_json::to_value(&new_ts) {
        Ok(Value::String(s)) => s,
        _ => audit.now.to_rfc3339(),
    };
    let mut out = Vec::new();
    if let Some(rows) = bucket.get("updated").and_then(|v| v.as_array()) {
        for r in rows {
            // 只回传「有实际变更列」的行（与 update_rows 的 cols.is_empty() 跳过口径一致）——
            // 无变更列的行不会被 UPDATE，其 update_time 未变，不应误导前端刷新。
            let has_field = r
                .get("fields")
                .and_then(|v| v.as_object())
                .map(|f| f.keys().any(|c| root.schema.get_index(c).is_some()))
                .unwrap_or(false);
            if !has_field {
                continue;
            }
            if let Some(id) = r.get("id").and_then(value_to_id_string) {
                out.push(UpdatedBaseline {
                    id,
                    update_time: ts_str.clone(),
                });
            }
        }
    }
    out
}

/// PG 标识符双引号包裹（防列名/表名撞关键字，与 sql_builder.rs 的 SELECT 侧风格对齐）。
/// 内部双引号转义为两个双引号。列名已过 schema 白名单，这里只防关键字冲突。
fn quote_ident(ident: &str) -> String {
    format!("\"{}\"", ident.replace('"', "\"\""))
}

// ─────────────────── 后端首次存储铸号（merge changeset 预处理） ───────────────────

/// 后端为 merge changeset 的**新增行**铸主键 id（52 位 JS 安全），并重指向跨层父外键。
///
/// 背景：id 原由前端生成（`maxId+1`，跨系统必撞）。改为后端首次存储铸号后，前端新增行只带
/// **临时 id**（缺失/null/非纯数字串，如 `t3`）。本函数：
///   1. **第一遍**：遍历所有层的 `inserted`，对临时 id 行铸真号，登记 `临时id→真id`（全局 map，
///      跨层唯一，故一张表足矣）。已带真数字 id 的行保留（导入既有真号 / 重存）。
///   2. **第二遍**：遍历所有 `inserted` 行的每个 childKey 外键（`upper_id` 或命名如 `header_id`；
///      前端 collector 把默认外键提到顶层，命名外键留在 `fields` 里，故两处都查），若指向某临时
///      父 id → 换成父的真号。跨层父子（父在 L1、子在 L2）因用同一张 map，天然连对，与层序无关。
///
/// 编码引擎铸号：遍历每张挂了 codeRule(mode=auto) 的层，为 code_field 为空的 inserted 行铸业务编码。
///
/// 对应方案 §10.3 `before_save_doc`。批量铸号（方案 §4.5 + §4.1 buffer 推进）：
/// 同层待铸号行收集后一次调 `mint_batch`，engine 内按 prefix 分组 + buffer 推进，
/// 同 prefix 多行一次反查 max 取连续号（修复附录 C.2.10/C.2.11）。
/// 未配置编码引擎（code_rule=None 或 GlobalCodeMinter 未注入）→ 静默跳过（现状零影响）。
/// 铸号失败记 warn 日志（不阻断主流程）。
async fn mint_codes_for_changeset(
    changes: &mut Value,
    meta: &DocMetaView,
    db_id: &str,
    _txn_id: &str,
    overrides: &HashMap<String, String>,
) {
    // 遍历每层，找挂了 codeRule(auto) 的层
    for layer in &meta.layers {
        let Some(code_rule) = &layer.code_rule else {
            continue;
        };
        let mode = code_rule.get("mode").and_then(|v| v.as_str()).unwrap_or("manual");
        if mode != "auto" {
            continue;
        }

        let field = code_rule
            .get("field")
            .and_then(|v| v.as_str())
            .unwrap_or("doc_no")
            .to_string();

        // 激活配置覆盖：若 overrides 含此 field，用其 ruleCode 替换（激活配置优先于单据元数据）。
        // 场景：cmx_mdm_activation.doc_code_rules={doc_no:MDM_GYS} → cr-form 经 codeRuleOverrides
        // 覆盖单据元数据 cv_mdm_apply.codeRule.ruleCode（MDM_BILL）→ 铸号用 MDM_GYS。
        let effective_rule: Value = match overrides.get(&field) {
            Some(rc) => {
                let mut r = code_rule.clone();
                if let Some(obj) = r.as_object_mut() {
                    obj.insert("ruleCode".into(), Value::String(rc.clone()));
                }
                r
            }
            None => code_rule.clone(),
        };

        let target = serde_json::json!({
            "kind": "doc",
            "code": layer.table_name,
            "field": field,
        });

        let Some(obj) = changes.as_object_mut() else {
            continue;
        };

        // 先收集匹配当前层的桶 key（不可变扫描，避免与后续 get_mut 冲突）
        let matching_keys: Vec<String> = obj.keys()
            .filter(|k| *k == &layer.table_name || *k == &layer.id)
            .cloned()
            .collect();
        if matching_keys.is_empty() { continue; }

        for layer_key in &matching_keys {
            let Some(layer_changes) = obj.get_mut(layer_key) else { continue };
            let Some(layer_obj) = layer_changes.as_object_mut() else { continue };
            let Some(inserted) = layer_obj.get_mut("inserted").and_then(|v| v.as_array_mut())
            else { continue };

            // 收集本桶待铸号的行索引 + attrs（跳过已有 code 的行）
            let mut pending: Vec<(usize, Value)> = Vec::new();
            for (idx, row_val) in inserted.iter().enumerate() {
                let Some(row) = row_val.as_object() else { continue };
                // 已有 code 值（顶层或 fields）→ 跳过
                let already_has = row.get(&field)
                    .and_then(|v| v.as_str())
                    .map(|s| !s.is_empty())
                    .unwrap_or(false)
                    || row.get("fields")
                        .and_then(|f| f.get(&field))
                        .and_then(|v| v.as_str())
                        .map(|s| !s.is_empty())
                        .unwrap_or(false);
                if already_has { continue; }
                // 构造 attrs（fields 平铺到顶层 + id）
                let mut attrs = serde_json::Map::new();
                if let Some(fields) = row.get("fields").and_then(|v| v.as_object()) {
                    attrs.extend(fields.iter().map(|(k, v)| (k.clone(), v.clone())));
                }
                if let Some(id) = row.get("id") {
                    attrs.insert("id".into(), id.clone());
                }
                pending.push((idx, Value::Object(attrs)));
            }

            if pending.is_empty() { continue; }

            // 公共铸号流水线（cmx-traits）：mode 校验 + 引擎取号 + warn，返回 (attrs索引, code)。
            // txn_id=None：CodeEngine async 调用链跨线程，主事务 holder 不可用，反查 max 用独立连接。
            let attrs_list: Vec<Value> = pending.iter().map(|(_, a)| a.clone()).collect();
            let minted = cmx_traits::code::mint_codes_batch(&effective_rule, &target, &attrs_list, db_id, None).await;

            if minted.is_empty() { continue; }

            // 写回每行：code 同时写顶层（供校验读）和 fields（供落库 row_cols_vals 读）
            let mut code_by_id: Vec<(String, String)> = Vec::new();
            for (attrs_idx, code) in &minted {
                let row_idx = pending[*attrs_idx].0;
                if let Some(row) = inserted.get_mut(row_idx).and_then(|v| v.as_object_mut()) {
                    row.insert(field.clone(), Value::String(code.clone()));
                    if let Some(fields) = row.get_mut("fields").and_then(|v| v.as_object_mut()) {
                        fields.insert(field.clone(), Value::String(code.clone()));
                    }
                    // id 可能是字符串（前端临时 id）或数字（mint_ids 后的雪花 id），统一转字符串
                    if let Some(id) = row.get("id").and_then(value_to_id_string) {
                        code_by_id.push((id, code.clone()));
                    }
                }
            }
            // inserted 借用到此结束 → 可安全借 obj 做 cascade
            let _ = inserted;
            // cascade 回填：父层铸号后，把同值 code 回填到子层同名字段为空的行
            if !code_by_id.is_empty() {
                cascade_code_to_children(obj, &field, &code_by_id);
            }
        }
    }
}

/// cascade 回填：父层铸号后，把同值 code 回填到子层同名字段为空的行。
///
/// 场景：cv_batch 挂了 codeRule 铸 doc_no（批号 BATCH...），cv_header 的 doc_no 通过
/// documentIdentityFields 引入（nullable=false，NOT NULL 校验）。前端建子行时父行 doc_no
/// 还没铸（铸号在 saver 内部），继承不到 → 子行 doc_no 空 → 校验失败。
///
/// 解法：铸完父层后，遍历所有层的 inserted 行，对 `field` 字段为空且 `upper_id` 匹配父行 id
/// 的子行，回填父行的 code。子行已有非空值则不覆盖（尊重独立铸号，如 cv_header 挂了自己的 codeRule）。
fn cascade_code_to_children(obj: &mut serde_json::Map<String, Value>, field: &str, code_by_id: &[(String, String)]) {
    if code_by_id.is_empty() { return; }
    for (_layer_key, layer_changes) in obj.iter_mut() {
        let Some(layer_obj) = layer_changes.as_object_mut() else { continue };
        let Some(inserted) = layer_obj.get_mut("inserted").and_then(|v| v.as_array_mut())
        else { continue };
        for row_val in inserted.iter_mut() {
            let Some(row) = row_val.as_object_mut() else { continue };
            let already_has = row.get(field)
                .and_then(|v| v.as_str())
                .map(|s| !s.is_empty())
                .unwrap_or(false)
                || row.get("fields")
                    .and_then(|f| f.get(field))
                    .and_then(|v| v.as_str())
                    .map(|s| !s.is_empty())
                    .unwrap_or(false);
            if already_has { continue; }
            let Some(uid) = row.get("upper_id").and_then(value_to_id_string) else { continue };
            let Some((_, code)) = code_by_id.iter().find(|(pid, _)| *pid == uid) else { continue };
            row.insert(field.to_string(), Value::String(code.clone()));
            if let Some(fields) = row.get_mut("fields").and_then(|v| v.as_object_mut()) {
                fields.insert(field.to_string(), Value::String(code.clone()));
            }
        }
    }
}

fn mint_ids_for_changeset(changes: &Value, child_keys: &[String]) -> (Value, Map<String, Value>) {
    let Some(obj) = changes.as_object() else {
        return (changes.clone(), Map::new());
    };
    let mut out = obj.clone();
    let mut id_map: Map<String, Value> = Map::new();

    // 第一遍：铸号。
    for layer in out.values_mut() {
        let Some(ins) = layer.get_mut("inserted").and_then(|v| v.as_array_mut()) else {
            continue;
        };
        for row in ins.iter_mut() {
            let Some(r) = row.as_object_mut() else {
                continue;
            };
            let cur = r.get("id");
            if !is_temp_id(cur) {
                continue; // 已是真号 → 不重铸。
            }
            let old_key = id_to_key(cur);
            let new_id = cmx_utils::next_pk_id();
            r.insert("id".into(), Value::Number(new_id.into()));
            if let Some(k) = old_key {
                id_map.insert(k, Value::Number(new_id.into()));
            }
        }
    }

    if id_map.is_empty() {
        return (Value::Object(out), id_map);
    }

    // 第二遍：父外键重指向（子行外键 == 某临时父 id → 父的真号）。顶层 + fields 两处都查。
    for layer in out.values_mut() {
        let Some(ins) = layer.get_mut("inserted").and_then(|v| v.as_array_mut()) else {
            continue;
        };
        for row in ins.iter_mut() {
            let Some(r) = row.as_object_mut() else {
                continue;
            };
            for ck in child_keys {
                // ① 顶层外键（collector 规范化的 upper_id）。
                if let Some(uv) = r.get(ck).cloned()
                    && let Some(k) = id_to_key(Some(&uv))
                    && let Some(real) = id_map.get(&k)
                {
                    r.insert(ck.clone(), real.clone());
                }
                // ② fields 内的命名外键（如 header_id/entry_id）。
                if let Some(fields) = r.get_mut("fields").and_then(|v| v.as_object_mut())
                    && let Some(uv) = fields.get(ck).cloned()
                    && let Some(k) = id_to_key(Some(&uv))
                    && let Some(real) = id_map.get(&k)
                {
                    fields.insert(ck.clone(), real.clone());
                }
            }
        }
    }

    (Value::Object(out), id_map)
}

// is_temp_id / id_to_key 用公共 cmx_utils::id（与 dct 共用，消除两份复刻）。
use cmx_utils::id::{id_to_key, is_temp_id};

/// 审计填充（方案 C）—— INSERT 路径：服务端权威写审计列，覆盖前端可能传来的同名值。
///
/// 对每行：先剔除 cols/vals 里任何既存审计列（防重复列名把多值 INSERT 拼炸），
/// 再按该列**是否在 schema**（通用单据不一定每张表都有全套审计列）追加：
///   create_by=actor, create_time=now, update_by=actor, update_time=now, delete_flag=0。
/// insert 时 create/update 同值——无 NULL、语义一致；delete_flag=0 满足 NOT NULL 无默认的物理约束。
fn apply_audit_insert(
    cols: &mut Vec<String>,
    vals: &mut Vec<DataValue>,
    schema: &Schema,
    audit: &AuditCtx,
) {
    const AUDIT_COLS: [&str; 5] = [
        "create_by",
        "create_time",
        "update_by",
        "update_time",
        "delete_flag",
    ];
    remove_cols(cols, vals, &AUDIT_COLS);
    let actor = DataValue::Int(audit.actor);
    let now = DataValue::DateTime(audit.now);
    for (col, dv) in [
        ("create_by", actor.clone()),
        ("create_time", now.clone()),
        ("update_by", actor),
        ("update_time", now),
        ("delete_flag", DataValue::Int(0)),
    ] {
        if schema.get_index(col).is_some() {
            cols.push(col.to_string());
            vals.push(dv);
        }
    }
}

/// 审计填充（方案 C）—— UPDATE 路径：强制注入/覆盖 update_by、update_time（服务端权威）。
///
/// 先剔除既存的 update_by/update_time（前端若传了则覆盖），再按 schema 存在性追加。
/// 保持 cols 与 col_vals 一一对应；调用方随后按 cols 分组。
fn apply_audit_update(
    cols: &mut Vec<String>,
    col_vals: &mut Vec<DataValue>,
    schema: &Schema,
    audit: &AuditCtx,
) {
    const UPDATE_AUDIT_COLS: [&str; 2] = ["update_by", "update_time"];
    remove_cols(cols, col_vals, &UPDATE_AUDIT_COLS);
    for (col, dv) in [
        ("update_by", DataValue::Int(audit.actor)),
        ("update_time", DataValue::DateTime(audit.now)),
    ] {
        if schema.get_index(col).is_some() {
            cols.push(col.to_string());
            col_vals.push(dv);
        }
    }
}

/// 从并列的 cols/vals 里删除指定列名（保持两者一一对应）。审计填充前的去重用。
fn remove_cols(cols: &mut Vec<String>, vals: &mut Vec<DataValue>, drop: &[&str]) {
    let mut i = 0;
    while i < cols.len() {
        if drop.contains(&cols[i].as_str()) {
            cols.remove(i);
            vals.remove(i);
        } else {
            i += 1;
        }
    }
}

/// 按列类型给占位符加显式 cast 后缀（空串 = 不加）。
///
/// 修复场景：`INSERT ... ON CONFLICT DO UPDATE` 与 `UPDATE ... FROM (VALUES ($p))` 上下文里，
/// 占位符缺乏目标列上下文，PG 把参数推断为 text——非文本列落 NULL/数值时报
/// 「column "line_target_id" is of type bigint but expression is of type text」
/// （cr-form 新增带明细保存即触发：新明细行 line_target_id 恒为 NULL）。
/// jsonb 列早已用 `$p::jsonb` 同法修复（见 [`build_multi_insert_sql`]），此处推广到全部类型列。
/// 配套：值侧须用 [`codec::json_to_dv_typed`]（类型列 NULL → `DataValue::NullTyped`），
/// 否则 text 型 NULL 绑到强转占位符上客户端 to_sql 校验不过。
fn pg_cast_for(ft: &FieldType) -> &'static str {
    match ft {
        FieldType::Int => "::bigint",
        FieldType::Float => "::float8",
        FieldType::Decimal => "::numeric",
        FieldType::Date => "::date",
        FieldType::DateTime => "::timestamptz",
        FieldType::Bool => "::boolean",
        FieldType::Json => "::jsonb",
        _ => "",
    }
}

/// 构造多值 INSERT（方案 A）：`INSERT INTO t (c...) VALUES ($1..$k),($k+1..) [ON CONFLICT (id) DO ...]`。
/// nrows 行 × cols.len() 列，占位符自 $1 连续编号。upsert=true 时加 ON CONFLICT 子句。
/// 纯函数（无 IO），便于单测占位符/列数/冲突子句正确性。
fn build_multi_insert_sql(
    table: &str,
    cols: &[String],
    nrows: usize,
    upsert: bool,
    col_casts: &[&str],
) -> String {
    let ncol = cols.len();
    let cols_sql = cols
        .iter()
        .map(|c| quote_ident(c))
        .collect::<Vec<_>>()
        .join(", ");
    let mut p = 0usize;
    let value_groups: Vec<String> = (0..nrows)
        .map(|_| {
            let group: Vec<String> = (0..ncol)
                .map(|ci| {
                    p += 1;
                    // 按列类型显式强转：规避 ON CONFLICT 上下文参数类型推断退化为 text
                    // （jsonb 当年已修，2026-08-14 推广到 bigint/numeric/date 等全部类型列）
                    let cast = col_casts.get(ci).copied().unwrap_or("");
                    format!("${p}{cast}")
                })
                .collect();
            format!("({})", group.join(", "))
        })
        .collect();
    let mut sql = format!(
        "INSERT INTO {} ({}) VALUES {}",
        quote_ident(table),
        cols_sql,
        value_groups.join(", ")
    );
    if upsert {
        // ON CONFLICT DO UPDATE SET：排除 id（冲突键）与创建审计列（create_by/create_time 不可变，
        // 否则「inserted 撞已存在 id」会把原创建人/创建时间冲掉）。update_by/update_time 仍随 EXCLUDED 刷新。
        let updates: Vec<String> = cols
            .iter()
            .filter(|c| c.as_str() != "id" && !CREATE_AUDIT_COLS.contains(&c.as_str()))
            .map(|c| format!("{q} = EXCLUDED.{q}", q = quote_ident(c)))
            .collect();
        if updates.is_empty() {
            sql.push_str(" ON CONFLICT (id) DO NOTHING");
        } else {
            sql.push_str(&format!(
                " ON CONFLICT (id) DO UPDATE SET {}",
                updates.join(", ")
            ));
        }
    }
    sql
}

/// 构造多值 UPDATE（方案 A）：`UPDATE t SET c=v.c FROM (VALUES (id,c..),...) AS v(id,c..) WHERE t.id=v.id`。
/// 每行参数序 = (id, col1, col2, ...)，占位符自 $1 连续。纯函数，便于单测。
///
/// `oplock`（B2 乐观锁）：Some(col) 时，每行 VALUES 末尾追加一个基线占位（该行装载时的 `col` 值），
/// alias 加 `__oplock` 列，WHERE 加 `AND t.col = v.__oplock` —— 基线陈旧的行不匹配 → 不更新 →
/// affected 减少（由调用方对账判为冲突）。None 时退化为原始无锁 UPDATE。
/// 注意参数序：Some 时每行为 (id, col1..colN, baseline)，即末位是基线。
fn build_multi_update_sql(
    table: &str,
    cols: &[String],
    nrows: usize,
    oplock: Option<&str>,
    col_casts: &[&str],
) -> String {
    let extra = if oplock.is_some() { 2 } else { 1 }; // id (+ baseline)
    let ncol = cols.len() + extra;
    let t = quote_ident(table);
    let set_sql = cols
        .iter()
        .map(|c| format!("{q} = v.{q}", q = quote_ident(c)))
        .collect::<Vec<_>>()
        .join(", ");
    // alias: id, col1..colN [, __oplock]
    let alias_cols = std::iter::once("id".to_string())
        .chain(cols.iter().map(|c| quote_ident(c)))
        .chain(oplock.map(|_| "__oplock".to_string()))
        .collect::<Vec<_>>()
        .join(", ");
    let mut p = 0usize;
    let value_groups: Vec<String> = (0..nrows)
        .map(|_| {
            let group: Vec<String> = (0..ncol)
                .map(|ci| {
                    p += 1;
                    // 每行 VALUES 布局：(id, col1..colN [, baseline])。
                    // 仅 cols 区间（ci ∈ 1..=cols.len()）按列类型加显式强转，
                    // 与 build_multi_insert_sql 对称：规避 FROM (VALUES) 无列上下文时参数被
                    // 推断为 text（jsonb 列当年已修；2026-08-14 推广到 bigint 等全部类型列，
                    // 修复明细行 update 落 NULL line_target_id 报 bigint=text）。
                    // id 与 baseline 不加强转：WHERE 比较上下文已给足类型。
                    let cast = if (1..=cols.len()).contains(&ci) {
                        col_casts.get(ci - 1).copied().unwrap_or("")
                    } else {
                        ""
                    };
                    format!("${p}{cast}")
                })
                .collect();
            format!("({})", group.join(", "))
        })
        .collect();
    let where_sql = match oplock {
        Some(col) => format!(
            "{t}.id = v.id AND {t}.{q} = v.\"__oplock\"",
            t = t,
            q = quote_ident(col)
        ),
        None => format!("{t}.id = v.id", t = t),
    };
    format!(
        "UPDATE {t} SET {set} FROM (VALUES {vals}) AS v({alias}) WHERE {where_sql}",
        t = t,
        set = set_sql,
        vals = value_groups.join(", "),
        alias = alias_cols,
        where_sql = where_sql,
    )
}

/// 按列的定义类型把 JSON 值转成匹配的 DataValue(薄包装:查 schema + 委托 codec)。
///
/// 前端 changeset 里 id/数值常是 JSON 字符串(如 "1000000001"),而列可能是 BIGINT。
/// 这里读 layer.schema 该列的 FieldType,委托 [`cmx_doc_model::codec::json_to_dv_typed`]
/// 做强转(避免 PG `bigint = text` 类型不匹配);类型缺失时走 [`json_to_dv_loose`] 兜底。
fn dv_for_col(v: &Value, layer: &LayerView, col: &str) -> DataValue {
    let ft = layer
        .schema
        .get_index(col)
        .and_then(|i| layer.schema.fields.get(i))
        .map(|f| f.field_type.clone());
    match ft {
        Some(ft) => json_to_dv_typed(&ft, v),
        None => json_to_dv_loose(v),
    }
}

/// 从请求 body 提取 saveMode / changes(handler 用)。
pub fn parse_save_body(body: &Value) -> (SaveMode, Value) {
    let mode = body
        .get("saveMode")
        .and_then(|v| v.as_str())
        .map(SaveMode::parse)
        .unwrap_or(SaveMode::Merge);
    let changes = match mode {
        SaveMode::Merge => body.get("changes").cloned().unwrap_or(Value::Null),
        SaveMode::Replace => body
            .get("snapshot")
            .cloned()
            .or_else(|| body.get("changes").cloned())
            .unwrap_or(Value::Null),
    };
    (mode, changes)
}

/// 从 save body 提取单据字段铸号规则覆盖 {field: ruleCode}（codeRuleOverrides）。
///
/// 与 [`parse_save_body`] 分离——overrides 是可选的铸号增强，非核心 changeset，
/// 独立提取避免改动 parse_save_body 签名（它有多处调用 + 测试）。MDM cr-form 把
/// activation.doc_code_rules 填进 body.codeRuleOverrides，handler 提取后注入
/// SaveCtx.code_rule_overrides，最终由 mint_codes_for_changeset 覆盖单据元数据 codeRule。
pub fn parse_code_rule_overrides(body: &Value) -> HashMap<String, String> {
    body.get("codeRuleOverrides")
        .and_then(|v| v.as_object())
        .map(|obj| {
            obj.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_save_body_defaults_merge() {
        let (mode, changes) = parse_save_body(&json!({ "changes": { "t": {} } }));
        assert_eq!(mode, SaveMode::Merge);
        assert!(changes.is_object());
    }

    #[test]
    fn parse_save_body_replace_takes_snapshot() {
        let (mode, changes) =
            parse_save_body(&json!({ "saveMode": "replace", "snapshot": { "t": { "rows": [] } } }));
        assert_eq!(mode, SaveMode::Replace);
        assert!(changes.get("t").is_some());
    }

    #[test]
    fn save_mode_parse() {
        assert_eq!(SaveMode::parse("replace"), SaveMode::Replace);
        assert_eq!(SaveMode::parse("merge"), SaveMode::Merge);
        assert_eq!(SaveMode::parse("xyz"), SaveMode::Merge);
    }

    #[test]
    fn parse_code_rule_overrides_extracts_field_rule_map() {
        // MDM cr-form 填 activation.doc_code_rules → body.codeRuleOverrides = {doc_no: MDM_GYS}
        let body = json!({ "codeRuleOverrides": { "doc_no": "MDM_GYS", "other": "R2" } });
        let m = parse_code_rule_overrides(&body);
        assert_eq!(m.get("doc_no"), Some(&"MDM_GYS".to_string()));
        assert_eq!(m.get("other"), Some(&"R2".to_string()));
        assert_eq!(m.len(), 2);
    }

    #[test]
    fn parse_code_rule_overrides_empty_when_missing_or_non_object() {
        // 无 codeRuleOverrides → 空 map（非 MDM 单据零影响）
        assert!(parse_code_rule_overrides(&json!({ "changes": {} })).is_empty());
        // codeRuleOverrides 非 object（如数组）→ 空 map（容错）
        assert!(parse_code_rule_overrides(&json!({ "codeRuleOverrides": [1, 2] })).is_empty());
        // 值非字符串的项被跳过（filter_map as_str）
        let m = parse_code_rule_overrides(&json!({ "codeRuleOverrides": { "doc_no": "MDM_GYS", "bad": 123 } }));
        assert_eq!(m.len(), 1);
        assert_eq!(m.get("doc_no"), Some(&"MDM_GYS".to_string()));
    }

    #[test]
    fn json_to_dv_types() {
        assert!(matches!(json_to_dv_loose(&json!(5)), DataValue::Int(5)));
        assert!(matches!(json_to_dv_loose(&json!("a")), DataValue::String(_)));
        assert!(matches!(json_to_dv_loose(&json!(null)), DataValue::Null));
        assert!(matches!(json_to_dv_loose(&json!(true)), DataValue::Bool(true)));
    }

    #[test]
    fn quote_ident_wraps_and_escapes() {
        assert_eq!(quote_ident("id"), "\"id\"");
        assert_eq!(quote_ident("user"), "\"user\""); // 关键字也安全
        assert_eq!(quote_ident("a\"b"), "\"a\"\"b\""); // 内部引号转义
    }

    #[test]
    fn multi_insert_single_row_upsert() {
        let cols = vec!["id".to_string(), "amount".to_string()];
        let sql = build_multi_insert_sql("cv_acc_line", &cols, 1, true, &[]);
        assert_eq!(
            sql,
            "INSERT INTO \"cv_acc_line\" (\"id\", \"amount\") VALUES ($1, $2) \
             ON CONFLICT (id) DO UPDATE SET \"amount\" = EXCLUDED.\"amount\""
        );
    }

    #[test]
    fn multi_insert_three_rows_placeholders_continuous() {
        let cols = vec!["id".to_string(), "a".to_string()];
        let sql = build_multi_insert_sql("t", &cols, 3, false, &[]);
        // 3 行 × 2 列 = $1..$6，连续编号
        assert_eq!(
            sql,
            "INSERT INTO \"t\" (\"id\", \"a\") VALUES ($1, $2), ($3, $4), ($5, $6)"
        );
    }

    #[test]
    fn multi_insert_id_only_do_nothing() {
        let cols = vec!["id".to_string()];
        let sql = build_multi_insert_sql("t", &cols, 1, true, &[]);
        // 只有 id 列时冲突不更新
        assert!(sql.ends_with("ON CONFLICT (id) DO NOTHING"));
    }

    #[test]
    fn multi_update_from_values() {
        let cols = vec!["a".to_string(), "b".to_string()];
        let sql = build_multi_update_sql("cv_header", &cols, 2, None, &["", ""]);
        // 每行 (id,a,b) = 3 参，2 行 = $1..$6
        assert_eq!(
            sql,
            "UPDATE \"cv_header\" SET \"a\" = v.\"a\", \"b\" = v.\"b\" \
             FROM (VALUES ($1, $2, $3), ($4, $5, $6)) AS v(id, \"a\", \"b\") \
             WHERE \"cv_header\".id = v.id"
        );
    }

    #[test]
    fn multi_update_with_oplock_baseline() {
        // B2 乐观锁：带 update_time 基线 → 每行 (id, a, baseline) = 3 参，WHERE 加基线谓词。
        let cols = vec!["a".to_string()];
        let sql = build_multi_update_sql("cv_batch", &cols, 2, Some("update_time"), &[""]);
        assert_eq!(
            sql,
            "UPDATE \"cv_batch\" SET \"a\" = v.\"a\" \
             FROM (VALUES ($1, $2, $3), ($4, $5, $6)) AS v(id, \"a\", __oplock) \
             WHERE \"cv_batch\".id = v.id AND \"cv_batch\".\"update_time\" = v.\"__oplock\""
        );
    }

    #[test]
    fn multi_update_jsonb_cast() {
        // jsonb 列（payload）占位符加 ::jsonb，与 build_multi_insert_sql 对称：
        // 规避「column "payload" is of type jsonb but expression is of type text」
        // （cr-form 第二次保存草稿走 UPDATE 路径时 payload 列必现）。
        let cols = vec!["name".to_string(), "payload".to_string()];
        let sql = build_multi_update_sql("cv_mdm_apply", &cols, 1, None, &["", "::jsonb"]);
        assert_eq!(
            sql,
            "UPDATE \"cv_mdm_apply\" SET \"name\" = v.\"name\", \"payload\" = v.\"payload\" \
             FROM (VALUES ($1, $2, $3::jsonb)) AS v(id, \"name\", \"payload\") \
             WHERE \"cv_mdm_apply\".id = v.id"
        );
    }

    #[test]
    fn multi_insert_and_update_bigint_cast() {
        // bigint 列（line_target_id）占位符加 ::bigint——修复明细行保存落 NULL 时
        // 「column "line_target_id" is of type bigint but expression is of type text」
        // （INSERT ON CONFLICT 与 UPDATE FROM VALUES 两个上下文均会推断退化为 text）。
        let cols = vec!["line_payload".to_string(), "line_target_id".to_string()];
        let sql = build_multi_insert_sql(
            "cv_mdm_apply_line",
            &cols,
            1,
            true,
            &["::jsonb", "::bigint"],
        );
        assert_eq!(
            sql,
            "INSERT INTO \"cv_mdm_apply_line\" (\"line_payload\", \"line_target_id\") \
             VALUES ($1::jsonb, $2::bigint) \
             ON CONFLICT (id) DO UPDATE SET \"line_payload\" = EXCLUDED.\"line_payload\", \
             \"line_target_id\" = EXCLUDED.\"line_target_id\""
        );
        let sql = build_multi_update_sql("cv_mdm_apply_line", &cols, 1, None, &["::jsonb", "::bigint"]);
        assert_eq!(
            sql,
            "UPDATE \"cv_mdm_apply_line\" SET \"line_payload\" = v.\"line_payload\", \
             \"line_target_id\" = v.\"line_target_id\" \
             FROM (VALUES ($1, $2::jsonb, $3::bigint)) AS v(id, \"line_payload\", \"line_target_id\") \
             WHERE \"cv_mdm_apply_line\".id = v.id"
        );
    }

    #[test]
    fn param_count_matches_placeholders() {
        // 批量插入的参数展平数应 == 占位符数（nrows × ncol）
        let cols = vec!["id".to_string(), "x".to_string(), "y".to_string()];
        let sql = build_multi_insert_sql("t", &cols, 4, false, &[]);
        let max_ph = (1..=100)
            .rev()
            .find(|i| sql.contains(&format!("${i}")))
            .unwrap();
        assert_eq!(max_ph, 4 * 3); // 12 个占位符
    }

    // ─────────────────── 审计填充（方案 C）单测 ───────────────────

    use cmx_core::model::cell::{Field, FieldType};
    use cmx_core::model::data::dataset::Schema;

    /// 建含全套审计列的 schema（模拟 documentTechnicalFields 已并入层）。
    fn schema_with_audit() -> Schema {
        let f = |n: &str, t: FieldType| Field {
            name: n.to_string(),
            field_type: t,
            label: String::new(),
        };
        Schema::new_unchecked(
            "t",
            vec![
                f("id", FieldType::Int),
                f("amount", FieldType::Decimal),
                f("create_by", FieldType::Int),
                f("create_time", FieldType::DateTime),
                f("update_by", FieldType::Int),
                f("update_time", FieldType::DateTime),
                f("delete_flag", FieldType::Int),
            ],
        )
    }

    fn audit_at(actor: i64) -> AuditCtx {
        // 固定时间戳，便于断言（不依赖 Utc::now()）
        use chrono::TimeZone;
        AuditCtx {
            actor,
            now: Utc.with_ymd_and_hms(2026, 7, 7, 1, 2, 3).unwrap(),
        }
    }

    #[test]
    fn audit_insert_injects_five_cols_with_delete_flag_zero() {
        let schema = schema_with_audit();
        let mut cols = vec!["id".to_string(), "amount".to_string()];
        let mut vals = vec![DataValue::Int(1000), DataValue::Decimal(5.into())];
        apply_audit_insert(&mut cols, &mut vals, &schema, &audit_at(42));

        // 业务列在前，审计 5 列追加在后
        assert_eq!(
            cols,
            vec![
                "id",
                "amount",
                "create_by",
                "create_time",
                "update_by",
                "update_time",
                "delete_flag"
            ]
        );
        assert_eq!(cols.len(), vals.len(), "cols/vals 一一对应");
        // create_by / update_by = actor
        assert_eq!(vals[2], DataValue::Int(42));
        assert_eq!(vals[4], DataValue::Int(42));
        // create_time / update_time = 同一 now（insert 时 create/update 同值）
        assert_eq!(vals[3], vals[5]);
        assert!(matches!(vals[3], DataValue::DateTime(_)));
        // delete_flag = 0
        assert_eq!(vals[6], DataValue::Int(0));
    }

    #[test]
    fn audit_insert_overwrites_client_supplied_audit_cols() {
        // 前端若传了 create_by/delete_flag，服务端权威值必须覆盖，且不出现重复列（防 SQL 拼炸）。
        let schema = schema_with_audit();
        let mut cols = vec![
            "id".to_string(),
            "create_by".to_string(),
            "delete_flag".to_string(),
        ];
        let mut vals = vec![
            DataValue::Int(1000),
            DataValue::Int(999), // 伪造的创建人
            DataValue::Int(1),   // 伪造的删除标识
        ];
        apply_audit_insert(&mut cols, &mut vals, &schema, &audit_at(42));

        // 每个审计列只出现一次
        assert_eq!(cols.iter().filter(|c| *c == "create_by").count(), 1);
        assert_eq!(cols.iter().filter(|c| *c == "delete_flag").count(), 1);
        // 服务端值胜出：create_by=42，delete_flag=0
        let idx = |name: &str| cols.iter().position(|c| c == name).unwrap();
        assert_eq!(vals[idx("create_by")], DataValue::Int(42));
        assert_eq!(vals[idx("delete_flag")], DataValue::Int(0));
    }

    #[test]
    fn audit_insert_actor_zero_fallback() {
        // 未认证/非数字身份兜底 0（永不阻断）。
        let schema = schema_with_audit();
        let mut cols = vec!["id".to_string()];
        let mut vals = vec![DataValue::Int(1)];
        apply_audit_insert(&mut cols, &mut vals, &schema, &audit_at(0));
        let idx = |name: &str| cols.iter().position(|c| c == name).unwrap();
        assert_eq!(vals[idx("create_by")], DataValue::Int(0));
        assert_eq!(vals[idx("update_by")], DataValue::Int(0));
    }

    #[test]
    fn audit_insert_only_injects_columns_present_in_schema() {
        // 通用单据：某表无审计列时，一列都不注入（不硬造不存在的列）。
        let schema = Schema::new_unchecked(
            "bare",
            vec![Field {
                name: "id".to_string(),
                field_type: FieldType::Int,
                label: String::new(),
            }],
        );
        let mut cols = vec!["id".to_string()];
        let mut vals = vec![DataValue::Int(1)];
        apply_audit_insert(&mut cols, &mut vals, &schema, &audit_at(42));
        assert_eq!(cols, vec!["id"], "无审计列的表不注入任何审计列");
        assert_eq!(vals.len(), 1);
    }

    #[test]
    fn audit_update_injects_update_cols_only() {
        let schema = schema_with_audit();
        let mut cols = vec!["amount".to_string()];
        let mut vals = vec![DataValue::Decimal(9.into())];
        apply_audit_update(&mut cols, &mut vals, &schema, &audit_at(42));
        assert_eq!(cols, vec!["amount", "update_by", "update_time"]);
        assert_eq!(cols.len(), vals.len());
        assert_eq!(vals[1], DataValue::Int(42));
        assert!(matches!(vals[2], DataValue::DateTime(_)));
        // update 路径不碰 create_*
        assert!(!cols.iter().any(|c| c == "create_by" || c == "create_time"));
    }

    #[test]
    fn audit_update_overwrites_client_supplied_update_cols() {
        // 前端若传了 update_by/update_time，覆盖为服务端值且不重复。
        let schema = schema_with_audit();
        let mut cols = vec!["update_by".to_string(), "update_time".to_string()];
        let mut vals = vec![DataValue::Int(999), DataValue::DateTime(audit_at(0).now)];
        apply_audit_update(&mut cols, &mut vals, &schema, &audit_at(42));
        assert_eq!(cols.iter().filter(|c| *c == "update_by").count(), 1);
        assert_eq!(cols.iter().filter(|c| *c == "update_time").count(), 1);
        let idx = |name: &str| cols.iter().position(|c| c == name).unwrap();
        assert_eq!(vals[idx("update_by")], DataValue::Int(42));
    }

    #[test]
    fn on_conflict_set_excludes_create_audit_keeps_update_audit() {
        // UPSERT 撞已存在 id：ON CONFLICT SET 不得覆盖 create_by/create_time（创建审计不可变），
        // 但仍刷新 update_by/update_time。
        let cols = vec![
            "id".to_string(),
            "amount".to_string(),
            "create_by".to_string(),
            "create_time".to_string(),
            "update_by".to_string(),
            "update_time".to_string(),
            "delete_flag".to_string(),
        ];
        let sql = build_multi_insert_sql("cv_header", &cols, 1, true, &[]);
        // 创建审计列不在 SET 子句
        assert!(!sql.contains("\"create_by\" = EXCLUDED"));
        assert!(!sql.contains("\"create_time\" = EXCLUDED"));
        // 更新审计列在 SET 子句
        assert!(sql.contains("\"update_by\" = EXCLUDED.\"update_by\""));
        assert!(sql.contains("\"update_time\" = EXCLUDED.\"update_time\""));
        // 普通业务列照常刷新
        assert!(sql.contains("\"amount\" = EXCLUDED.\"amount\""));
    }

    // ─────────────────── 版本快照根提取（B1：collect_versioned_roots）单测 ───────────────────

    #[test]
    fn value_to_id_string_variants() {
        assert_eq!(value_to_id_string(&json!("1001")), Some("1001".into()));
        assert_eq!(value_to_id_string(&json!(1001)), Some("1001".into()));
        assert_eq!(value_to_id_string(&json!("")), None); // 空串跳过
        assert_eq!(value_to_id_string(&json!(null)), None);
        assert_eq!(value_to_id_string(&json!(true)), None);
    }

    /// 建一个最小两层 DocMetaView（根 cv_batch），供 collect_versioned_roots 测试。
    fn mini_meta() -> DocMetaView {
        let doc = json!({
            "moduleMeta": { "moduleCode": "cmxfico", "metaKind": "DOC", "version": 1 },
            "voucherSchema": {
                "schema": [
                    [ { "id": "cv_batch",  "level": "L1" } ],
                    [ { "id": "cv_header", "level": "L2" } ]
                ],
                "relations": [
                    { "parent": "cv_batch", "child": "cv_header", "parentKey": "id", "childKey": "upper_id" }
                ]
            },
            "voucherTables": [
                { "level": "L1", "tableName": "cv_batch",  "fields": [ { "name": "id", "dataType": "BIGINT" } ] },
                { "level": "L2", "tableName": "cv_header", "fields": [ { "name": "id", "dataType": "BIGINT" }, { "name": "upper_id", "dataType": "BIGINT" } ] }
            ]
        });
        DocMetaView::parse(&doc, &Value::Null).unwrap()
    }

    fn ctx_no_override() -> SaveCtx {
        SaveCtx {
            actor_id: 7,
            actor_name: "张三".into(),
            doc_file: "f.json".into(),
            op_override: None,
            code_rule_overrides: HashMap::new(),
        }
    }

    #[test]
    fn collect_roots_merge_inserted_is_create_updated_is_update() {
        let meta = mini_meta();
        let root = meta.root_layer().unwrap();
        let changes = json!({
            "cv_batch": {
                "inserted": [ { "id": "1001", "fields": {} } ],
                "updated":  [ { "id": 1002, "fields": { "x": 1 } } ]
            }
        });
        let mut roots =
            collect_versioned_roots(&changes, &meta, SaveMode::Merge, root, &ctx_no_override())
                .unwrap();
        roots.sort();
        assert_eq!(
            roots,
            vec![
                ("1001".to_string(), "create".to_string()),
                ("1002".to_string(), "update".to_string())
            ]
        );
    }

    #[test]
    fn collect_roots_merge_child_only_change_yields_none() {
        // 只动子层（根层无桶）→ 本期不为根记版本。
        let meta = mini_meta();
        let root = meta.root_layer().unwrap();
        let changes = json!({
            "cv_header": { "updated": [ { "id": "2001", "fields": { "x": 1 } } ] }
        });
        let roots =
            collect_versioned_roots(&changes, &meta, SaveMode::Merge, root, &ctx_no_override())
                .unwrap();
        assert!(roots.is_empty());
    }

    #[test]
    fn collect_roots_replace_uses_root_rows_update_op() {
        let meta = mini_meta();
        let root = meta.root_layer().unwrap();
        let snapshot = json!({
            "cv_batch": { "rows": [ { "id": "3001" }, { "id": "3002" } ] }
        });
        let mut roots = collect_versioned_roots(
            &snapshot,
            &meta,
            SaveMode::Replace,
            root,
            &ctx_no_override(),
        )
        .unwrap();
        roots.sort();
        assert_eq!(
            roots,
            vec![
                ("3001".to_string(), "update".to_string()),
                ("3002".to_string(), "update".to_string())
            ]
        );
    }

    #[test]
    fn collect_roots_op_override_wins() {
        // restore 传 op_override=Some("restore") → 覆盖桶推断。
        let meta = mini_meta();
        let root = meta.root_layer().unwrap();
        let ctx = SaveCtx {
            actor_id: 7,
            actor_name: "张三".into(),
            doc_file: "f.json".into(),
            op_override: Some("restore".into()),
            code_rule_overrides: HashMap::new(),
        };
        let snapshot = json!({ "cv_batch": { "rows": [ { "id": "4001" } ] } });
        let roots =
            collect_versioned_roots(&snapshot, &meta, SaveMode::Replace, root, &ctx).unwrap();
        assert_eq!(roots, vec![("4001".to_string(), "restore".to_string())]);
    }

    #[test]
    fn collect_roots_dedups_same_id() {
        // 同 id 同时在 inserted 与 updated（异常但要稳）→ 只记一次（首次 create 胜）。
        let meta = mini_meta();
        let root = meta.root_layer().unwrap();
        let changes = json!({
            "cv_batch": {
                "inserted": [ { "id": "5001", "fields": {} } ],
                "updated":  [ { "id": "5001", "fields": { "x": 1 } } ]
            }
        });
        let roots =
            collect_versioned_roots(&changes, &meta, SaveMode::Merge, root, &ctx_no_override())
                .unwrap();
        assert_eq!(roots, vec![("5001".to_string(), "create".to_string())]);
    }

    // ─────────────────── 后端铸号（mint_ids_for_changeset）单测 ───────────────────

    #[test]
    fn is_temp_id_classification() {
        // 临时：缺失 / null / 空串 / 非纯数字串。
        assert!(is_temp_id(None));
        assert!(is_temp_id(Some(&json!(null))));
        assert!(is_temp_id(Some(&json!(""))));
        assert!(is_temp_id(Some(&json!("t3"))));
        assert!(is_temp_id(Some(&json!("r7x9k"))));
        // 真号：纯数字串 / 数字。
        assert!(!is_temp_id(Some(&json!("1002"))));
        assert!(!is_temp_id(Some(&json!(1002))));
    }

    #[test]
    fn mint_assigns_js_safe_ids_to_temp_rows() {
        // 临时 id 行被铸真号；真号 < 2^53（JS 安全）；idMap 记录 临时→真。
        let changes = json!({
            "cv_batch": { "inserted": [ { "id": "t1", "fields": { "code": "A" } } ] }
        });
        let (out, id_map) = mint_ids_for_changeset(&changes, &["upper_id".to_string()]);
        let new_id = out["cv_batch"]["inserted"][0]["id"].as_i64().unwrap();
        assert!(
            new_id > 0 && new_id <= 9_007_199_254_740_991,
            "id 必须 JS 安全"
        );
        assert_eq!(id_map.get("t1").and_then(|v| v.as_i64()), Some(new_id));
    }

    #[test]
    fn mint_preserves_real_ids() {
        // 已带真号的行不铸、不进 idMap（重存/导入既有真号幂等）。
        let changes = json!({
            "cv_batch": { "inserted": [ { "id": "1002", "fields": {} } ] }
        });
        let (out, id_map) = mint_ids_for_changeset(&changes, &["upper_id".to_string()]);
        assert_eq!(out["cv_batch"]["inserted"][0]["id"], json!("1002"));
        assert!(id_map.is_empty());
    }

    #[test]
    fn mint_remaps_cross_layer_upper_id() {
        // 父在 L1（临时 t1）、子在 L2（upper_id=t1）→ 子的 upper_id 重指向父的真号。
        let changes = json!({
            "cv_batch":  { "inserted": [ { "id": "t1", "fields": {} } ] },
            "cv_header": { "inserted": [ { "id": "t2", "upper_id": "t1", "fields": {} } ] }
        });
        let (out, id_map) = mint_ids_for_changeset(&changes, &["upper_id".to_string()]);
        let parent_real = id_map.get("t1").unwrap().as_i64().unwrap();
        let child_upper = out["cv_header"]["inserted"][0]["upper_id"]
            .as_i64()
            .unwrap();
        assert_eq!(child_upper, parent_real, "子 upper_id 应指向父真号");
        // 子自身也铸了真号。
        assert!(id_map.contains_key("t2"));
    }

    #[test]
    fn mint_remaps_named_fk_inside_fields() {
        // 命名外键（header_id 在 fields 里）也要重指向父真号。
        let changes = json!({
            "cv_header":   { "inserted": [ { "id": "h1", "fields": {} } ] },
            "cv_acc_line": { "inserted": [ { "id": "e1", "fields": { "header_id": "h1", "amount": 5 } } ] }
        });
        let (out, id_map) = mint_ids_for_changeset(&changes, &["header_id".to_string()]);
        let parent_real = id_map.get("h1").unwrap().as_i64().unwrap();
        let child_fk = out["cv_acc_line"]["inserted"][0]["fields"]["header_id"]
            .as_i64()
            .unwrap();
        assert_eq!(child_fk, parent_real, "fields 里的 header_id 应指向父真号");
    }

    #[test]
    fn mint_preserves_real_ids_child_only() {
        // child_keys 空也安全（无 relations 的单表单据）。
        let changes = json!({
            "cv_batch": { "inserted": [ { "id": "1002", "fields": {} } ] }
        });
        let (out, id_map) = mint_ids_for_changeset(&changes, &[]);
        assert_eq!(out["cv_batch"]["inserted"][0]["id"], json!("1002"));
        assert!(id_map.is_empty());
    }

    #[test]
    fn mint_child_upper_id_to_existing_parent_untouched() {
        // 子挂到「已存在的真号父」（upper_id=已有真号，父不在本批 inserted）→ upper_id 原样保留。
        let changes = json!({
            "cv_header": { "inserted": [ { "id": "t9", "upper_id": "5000", "fields": {} } ] }
        });
        let (out, _id_map) = mint_ids_for_changeset(&changes, &["upper_id".to_string()]);
        assert_eq!(out["cv_header"]["inserted"][0]["upper_id"], json!("5000"));
    }

    #[test]
    fn mint_leaves_updated_deleted_untouched() {
        // 只动 inserted；updated/deleted 的 id 不变。
        let changes = json!({
            "cv_batch": {
                "inserted": [ { "id": "t1", "fields": {} } ],
                "updated":  [ { "id": "1002", "fields": { "x": 1 } } ],
                "deleted":  [ "1003" ]
            }
        });
        let (out, _id_map) = mint_ids_for_changeset(&changes, &["upper_id".to_string()]);
        assert_eq!(out["cv_batch"]["updated"][0]["id"], json!("1002"));
        assert_eq!(out["cv_batch"]["deleted"][0], json!("1003"));
    }

    #[test]
    fn mint_ids_are_globally_unique() {
        // 多层多行临时 id 各得唯一真号。
        let changes = json!({
            "cv_batch":  { "inserted": [ { "id": "t1", "fields": {} } ] },
            "cv_header": { "inserted": [ { "id": "t2", "upper_id": "t1", "fields": {} },
                                        { "id": "t3", "upper_id": "t1", "fields": {} } ] }
        });
        let (_out, id_map) = mint_ids_for_changeset(&changes, &["upper_id".to_string()]);
        let ids: std::collections::HashSet<i64> =
            id_map.values().filter_map(|v| v.as_i64()).collect();
        assert_eq!(ids.len(), 3, "3 个临时 id 应铸出 3 个互异真号");
    }
}
