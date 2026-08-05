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

/// 激活器产出:要写入 cm_* 的头行数据(供 dct_accessor 执行)。
#[derive(Debug, Clone)]
pub struct ActivationPlan {
    /// 头:目标列 → 值(已按 header_mapping 搬运 + 补 lifecycle_status='published')
    pub header_row: Map<String, Value>,
    /// 明细:每条 = (目标明细表, 关联列名, 行数据)
    pub line_rows: Vec<(String, String, Map<String, Value>)>,
}

/// 按 mapping 把 CR 头字段搬运成 cm_* 头行(create 分支)。
///
/// - `cfg`:激活映射配置
/// - `cr_head`:cv_mdm_apply 头记录(字段名 → 值)
/// - `new_code`:新建时由 [`crate::codegen::CodeGenerator`] 产出的 code
pub fn plan_create(cfg: &ActivationConfig, cr_head: &Map<String, Value>, new_code: &str) -> ActivationPlan {
    let mut header_row = Map::new();
    for (src_field, tgt_col) in &cfg.header_mapping {
        if let Some(tgt) = tgt_col.as_str()
            && let Some(val) = cr_head.get(src_field)
        {
            header_row.insert(tgt.to_string(), val.clone());
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
            {
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
                if let Some(t) = tgt.as_str()
                    && let Some(v) = payload.get(src)
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
        let cr_head = serde_json::from_value(json!({ "name": "B公司", "tax_no": "911", "extra": "忽略" })).unwrap();
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
