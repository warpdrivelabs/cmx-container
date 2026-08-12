//! 激活器纯逻辑:字段搬运规则(配置驱动,无 DB)。
//!
//! 读 CR 头/行(serde_json::Value)→ 按 [`ActivationConfig`] 的映射配置 → 产出 cm_* 头行数据。
//! 新建(create)/变更(update)分支、明细关联列回填、line_action 处理。
//!
//! 纯计算层:不接 DB,可单测。DB 读写由 cmx-mdm-store-pg 的各 accessor 执行。

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// 激活映射配置(对应 cmx_mdm_activation 一行,由 activation_store 反序列化)。
///
/// 顶层字段对齐 DB 列名(snake_case);target_table 是目标物理表名(配置器选字典时一并落库)。
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ActivationConfig {
    pub activation_code: String,
    pub source_doc_type: String,
    /// 变更类型 create/update(M1);merge/block/flag_delete 后续
    pub cr_type: String,
    pub target_dict: String,
    /// 目标头表物理名(如 cm_supplier)。DDL 已有 target_table 列(governance.up.sql),
    /// 配置器 UI 选字典时从 dct/meta 的 tableName 一并写入;激活器直接用此字段拼 SQL。
    #[serde(default)]
    pub target_table: String,
    /// 头映射 {单据字段: 主数据列}
    #[serde(default)]
    pub header_mapping: Map<String, Value>,
    /// 明细映射数组(JSON 内容用 camelCase 键,见 [`LineMapping`] 的 serde rename)。
    #[serde(default)]
    pub line_mappings: Vec<LineMapping>,
    pub code_rule_code: Option<String>,
    /// 主体名字段来源（payload 内字段名，前端按此填 subject_name）。
    #[serde(default)]
    pub subject_name_field: Option<String>,
    /// 主体编码字段来源（为空则由 codeRule 铸号）。
    #[serde(default)]
    pub subject_code_field: Option<String>,
    /// 头映射分组（纯 UI 展示用，不影响激活器搬运）。
    /// fields 存 header_mapping 的 key（源字段名），用它把扁平映射行归组展示。
    /// 激活器（find_by_doc_type / plan_create）不读此字段——header_mapping 落库仍扁平。
    /// 外层 snake（对齐 line_mappings 范式 + DB 列名），内层 HeaderGroup 字段 camel。
    #[serde(default)]
    pub header_groups: Vec<HeaderGroup>,
}

/// 明细映射(一条 = 一类明细行,如 bank_account)。
///
/// JSON 内容键用 camelCase(对齐 DDL line_mappings 注释
/// `{lineType,targetDict,targetTable,parentIdField,fields}`),Rust 字段用 snake_case +
/// `#[serde(rename)]` 桥接。target_table 加 default 兼容历史数据。
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LineMapping {
    #[serde(rename = "lineType")]
    pub line_type: String,
    #[serde(rename = "targetDict")]
    pub target_dict: String,
    #[serde(default, rename = "targetTable")]
    pub target_table: String,
    #[serde(rename = "parentIdField")]
    pub parent_field: String,
    #[serde(default)]
    pub fields: Map<String, Value>,
}

/// 头映射分组（一条 = 一个展示分组，如「基础信息」「工商资质」）。
///
/// 纯 UI 组织用：fields 列出归入本组的 header_mapping key（源字段名），
/// 渲染时按此把扁平映射行分区展示。激活器不读此结构。
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HeaderGroup {
    #[serde(rename = "groupCode")]
    pub group_code: String,
    #[serde(rename = "groupName")]
    pub group_name: String,
    #[serde(default)]
    pub fields: Vec<String>,
}

/// 激活器产出:要写入 cm_* 的头行数据(供 dct_accessor 执行)。
#[derive(Debug, Clone)]
pub struct ActivationPlan {
    /// 头:目标列 → 值(已按 header_mapping 搬运 + 补 lifecycle_status='published')
    pub header_row: Map<String, Value>,
    /// 明细:每条 = (目标明细表, 关联列名, 行数据)
    pub line_rows: Vec<(String, String, Map<String, Value>)>,
}

/// 「未填」判断：`null` 或空字符串视作未提供（与 `cmx-biz` NOT NULL 校验「空串=missing」语义一致）。
///
/// 激活器搬运时跳过这些值——让目标表 DEFAULT / 服务端 backfill（如 status=1、sort_no=0）生效，
/// 避免空串写入 INT/DATE 等强类型列触发「类型不匹配」校验失败或 DB 绑定错误。
/// （前端表单数值框留空时回传空串，是业务最常见的「未填」形态。）
fn is_unfilled(v: &Value) -> bool {
    v.is_null() || v.as_str().is_some_and(str::is_empty)
}

/// 仅空字符串判断（update 场景：null 是显式清空意图，须保留给落库层 SET col=NULL）。
fn is_empty_str(v: &Value) -> bool {
    v.as_str().is_some_and(str::is_empty)
}

/// 按 mapping 把 CR 头字段搬运成 cm_* 头行(create 分支)。
///
/// - `cfg`:激活映射配置
/// - `cr_head`:cv_mdm_apply 头记录(字段名 → 值)
/// - `new_code`:新建时由 [`crate::codegen::CodeGenerator`] 产出的 code
pub fn plan_create(cfg: &ActivationConfig, cr_head: &Map<String, Value>, new_code: &str) -> ActivationPlan {
    let mut header_row = Map::new();
    // 通用回退:先查 payload 内(业务字段),再查 cr_head 顶层(公共搜索列)
    let payload_obj = cr_head.get("payload").and_then(|v| v.as_object());
    for (src_field, tgt_col) in &cfg.header_mapping {
        let val = payload_obj
            .and_then(|p| p.get(src_field))
            .or_else(|| cr_head.get(src_field));
        // null/空串跳过：未填字段不搬运，让目标表 DEFAULT / backfill（status=1、sort_no=0）兜底，
        // 避免空串落 INT/DATE 列触发「类型不匹配」（见 build_upsert_sql_dv 的 backfill 仅对未提供列生效）。
        if let Some(tgt) = tgt_col.as_str()
            && let Some(v) = val
            && !is_unfilled(v)
        {
            header_row.insert(tgt.to_string(), v.clone());
        }
    }
    header_row.insert("code".into(), Value::String(new_code.to_string()));
    // 闸口:强制 published(V3.1 dct_accessor 唯一写入入口约束)
    header_row.insert("lifecycle_status".into(), Value::String("published".to_string()));
    header_row.insert("published_version".into(), Value::Number(1.into()));
    ActivationPlan { header_row, line_rows: vec![] }
}

/// 按 mapping 把 CR 头字段搬运成 update delta(update 分支)。
///
/// 变更:只搬 field_deltas 里的新值(不覆盖整行);version+1。
///
/// - `field_deltas`:`{field: {old, new}}`,取 new 按 header_mapping 落到目标列
/// - `current_version`:目标记录当前 published_version(乐观锁快照)
pub fn plan_update(
    cfg: &ActivationConfig,
    _cr_head: &Map<String, Value>,
    field_deltas: &Value,
    current_version: i64,
) -> ActivationPlan {
    let mut header_row = Map::new();
    if let Some(deltas) = field_deltas.as_object() {
        for (src_field, tgt_col) in &cfg.header_mapping {
            if let Some(tgt) = tgt_col.as_str()
                && let Some(delta) = deltas.get(src_field)
                && let Some(new_val) = delta.get("new")
                && !is_empty_str(new_val)
            {
                // 空串跳过（前端表单未改的空串）；null 保留——update 时是显式清空意图，落库 SET col=NULL。
                header_row.insert(tgt.to_string(), new_val.clone());
            }
        }
    }
    header_row.insert("published_version".into(), Value::Number((current_version + 1).into()));
    // 变更不改 lifecycle_status(保持 published)
    ActivationPlan { header_row, line_rows: vec![] }
}

/// 按 line_mappings 把 CR 行搬运成明细行。
///
/// 遍历 cr_lines,按 line_type 匹配 mapping,产出 (目标明细表, 关联列, 行数据)。
///
/// **M1 范围**:只处理 insert/update 行;**delete 行在此过滤掉**(activate_inner 不处理明细删除,
/// 留 M3 merge 分支)。返回元组第 1 项是目标明细**物理表名**(target_table)。
pub fn plan_lines(
    cfg: &ActivationConfig,
    cr_lines: &[Value],
    header_id: i64,
) -> Vec<(String, String, Map<String, Value>)> {
    let mut out = vec![];
    for line in cr_lines {
        let Some(line_obj) = line.as_object() else { continue };
        let Some(line_type) = line_obj.get("line_type").and_then(|v| v.as_str()) else {
            continue;
        };
        let line_action = line_obj.get("line_action").and_then(|v| v.as_str()).unwrap_or("insert");
        // M1 跳过 delete 明细(留 M3)
        if line_action == "delete" {
            continue;
        }
        let Some(lm) = cfg.line_mappings.iter().find(|m| m.line_type == line_type) else {
            continue;
        };
        let mut row = Map::new();
        if let Some(payload) = line_obj.get("line_payload").and_then(|v| v.as_object()) {
            for (src, tgt) in &lm.fields {
                // null/空串跳过（同 plan_create）：让明细表 DEFAULT / backfill 兜底，避免空串落强类型列。
                if let Some(t) = tgt.as_str()
                    && let Some(v) = payload.get(src)
                    && !is_unfilled(v)
                {
                    row.insert(t.to_string(), v.clone());
                }
            }
        }
        row.insert(lm.parent_field.clone(), Value::Number(header_id.into()));
        // 闸口:明细行也强制 published
        row.insert("lifecycle_status".into(), Value::String("published".to_string()));
        // 元组:(目标明细物理表, 关联列名, 行数据)
        out.push((lm.target_table.clone(), lm.parent_field.clone(), row));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_cfg() -> ActivationConfig {
        // 模拟 cmx_mdm_activation 一行(经 find_by_doc_type 反序列化)
        serde_json::from_value(json!({
            "activation_code": "supplier_apply",
            "source_doc_type": "mdm_supplier_apply",
            "cr_type": "create",
            "target_dict": "supplier",
            "target_table": "cm_supplier",
            "header_mapping": { "name": "name", "tax_no": "tax_no" },
            "line_mappings": [{
                "lineType": "bank_account",
                "targetDict": "supplier_bank",
                "targetTable": "cm_bank_account",
                "parentIdField": "supplier_id",
                "fields": { "account_no": "account_no" }
            }],
            "code_rule_code": null
        }))
        .unwrap()
    }

    #[test]
    fn plan_create_carries_mapped_fields_and_forces_published() {
        let cfg = sample_cfg();
        // 业务字段（name/tax_no）走 payload；公共搜索列 subject_name 留顶层
        let cr_head = serde_json::from_value(json!({
            "subject_name": "B公司",
            "payload": { "name": "B公司", "tax_no": "911", "extra": "忽略" }
        })).unwrap();
        let plan = plan_create(&cfg, &cr_head, "SUPPLI-abc");
        assert_eq!(plan.header_row.get("name").and_then(|v| v.as_str()), Some("B公司"));
        assert_eq!(plan.header_row.get("tax_no").and_then(|v| v.as_str()), Some("911"));
        assert_eq!(plan.header_row.get("code").and_then(|v| v.as_str()), Some("SUPPLI-abc"));
        // extra 未在 header_mapping,不搬运
        assert!(plan.header_row.get("extra").is_none());
        // 闸口:强制 published
        assert_eq!(plan.header_row.get("lifecycle_status").and_then(|v| v.as_str()), Some("published"));
        assert_eq!(plan.header_row.get("published_version").and_then(|v| v.as_i64()), Some(1));
    }

    #[test]
    fn plan_create_skips_empty_and_null_fields() {
        // 前端表单数值框留空时回传空串（业务最常见的「未填」形态）。
        // 激活器搬运时须跳过空串/null，让目标表 DEFAULT / 服务端 backfill（status=1、sort_no=0）生效，
        // 否则空串落 INT 列会触发「类型不匹配」校验失败。
        let mut cfg = sample_cfg();
        cfg.header_mapping = serde_json::from_value(json!({
            "name": "name", "tax_no": "tax_no", "status": "status", "sort_no": "sort_no"
        })).unwrap();
        let cr_head = serde_json::from_value(json!({
            "payload": { "name": "A公司", "tax_no": "911", "status": "", "sort_no": null }
        })).unwrap();
        let plan = plan_create(&cfg, &cr_head, "SUPPLI-x");
        // 有值字段正常搬运
        assert_eq!(plan.header_row.get("name").and_then(|v| v.as_str()), Some("A公司"));
        assert_eq!(plan.header_row.get("tax_no").and_then(|v| v.as_str()), Some("911"));
        // 空串 / null 字段不搬运（交给 backfill）
        assert!(plan.header_row.get("status").is_none(), "空串 status 应跳过");
        assert!(plan.header_row.get("sort_no").is_none(), "null sort_no 应跳过");
    }

    #[test]
    fn plan_update_skips_empty_str_but_keeps_null() {
        // update 场景：空串=前端未改（跳过）；null=显式清空（保留落库 SET col=NULL）。
        let mut cfg = sample_cfg();
        cfg.header_mapping = serde_json::from_value(json!({
            "tax_no": "tax_no", "status": "status"
        })).unwrap();
        let cr_head = Map::new();
        let deltas = json!({
            "tax_no": { "old": "911", "new": "" },
            "status": { "old": 1, "new": null }
        });
        let plan = plan_update(&cfg, &cr_head, &deltas, 1);
        // 空串跳过（不更新）
        assert!(plan.header_row.get("tax_no").is_none(), "空串 new 应跳过不更新");
        // null 保留（显式清空）
        assert!(plan.header_row.get("status").and_then(|v| v.as_null()).is_some(), "null new 应保留");
    }

    #[test]
    fn plan_update_takes_new_value_from_deltas_and_bumps_version() {
        let mut cfg = sample_cfg();
        cfg.cr_type = "update".into();
        let cr_head = Map::new();
        let deltas = json!({ "tax_no": { "old": "911", "new": "922" }, "name": { "old": "B", "new": "B公司" } });
        let plan = plan_update(&cfg, &cr_head, &deltas, 3);
        assert_eq!(plan.header_row.get("tax_no").and_then(|v| v.as_str()), Some("922"));
        assert_eq!(plan.header_row.get("name").and_then(|v| v.as_str()), Some("B公司"));
        assert_eq!(plan.header_row.get("published_version").and_then(|v| v.as_i64()), Some(4));
        // 变更不改 lifecycle_status
        assert!(plan.header_row.get("lifecycle_status").is_none());
    }

    #[test]
    fn plan_lines_fills_parent_and_skips_delete() {
        let cfg = sample_cfg();
        let cr_lines = vec![
            json!({ "line_type": "bank_account", "line_action": "insert",
                    "line_payload": { "account_no": "工行6222" } }),
            json!({ "line_type": "bank_account", "line_action": "delete",
                    "line_payload": { "account_no": "旧账号" } }),
        ];
        let rows = plan_lines(&cfg, &cr_lines, 8001);
        // delete 行被过滤
        assert_eq!(rows.len(), 1);
        let (table, parent_col, row) = &rows[0];
        assert_eq!(table, "cm_bank_account");
        assert_eq!(parent_col, "supplier_id");
        assert_eq!(row.get("account_no").and_then(|v| v.as_str()), Some("工行6222"));
        assert_eq!(row.get("supplier_id").and_then(|v| v.as_i64()), Some(8001));
        assert_eq!(row.get("lifecycle_status").and_then(|v| v.as_str()), Some("published"));
    }

    #[test]
    fn activation_config_deserializes_camel_case_line_mappings() {
        // 验证 LineMapping 的 serde rename 生效(从 DB JSON 反序列化)
        let cfg = sample_cfg();
        assert_eq!(cfg.target_table, "cm_supplier");
        assert_eq!(cfg.line_mappings.len(), 1);
        let lm = &cfg.line_mappings[0];
        assert_eq!(lm.line_type, "bank_account");
        assert_eq!(lm.target_table, "cm_bank_account");
        assert_eq!(lm.parent_field, "supplier_id");
    }
}
