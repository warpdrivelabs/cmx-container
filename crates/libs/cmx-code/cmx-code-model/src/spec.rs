//! 编码引擎核心类型定义。
//!
//! 分两类：
//! - **规则算法**（[`RuleSpec`]）：存于 `cmx_code_rule` 表，纯算法（段序列），不含 target。
//! - **挂载点声明**（[`CodeRule`]）：存于 DCT/DOC 定义，引用规则算法 + 声明 target 行为（field/mode/局部覆盖/cascade）。

use serde::{Deserialize, Serialize};

use crate::error::{CodeError, Result};

// ═══════════════════════════════════════════════════════════════════════════════
// 规则算法层（存 cmx_code_rule 表，纯算法，无 target）
// ═══════════════════════════════════════════════════════════════════════════════

/// 编码规则算法规范（规则表里存的纯算法，不含 target 信息）。
///
/// target 由 DCT/DOC 钩子调用时作为上下文传入（见 [`Target`]）。
/// 一条规则可被任意多个字典/单据复用。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleSpec {
    /// 规则码（人类可读，全局唯一，如 `supplier_hq`）
    #[serde(default)]
    pub rule_code: String,

    /// 规则名称（展示用）
    #[serde(default)]
    pub rule_name: String,

    /// 模式：`auto`（引擎生成）| `manual`（用户手敲，引擎只校验）
    #[serde(default = "default_mode")]
    pub mode: String,

    /// 受控组织（可选，组织命中才生效）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub org_scope: Option<String>,

    /// 适用条件表达式（可选，按属性分流，如 `attrs.bp_role=='supplier'`）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,

    /// 段序列（auto 必填）
    #[serde(default)]
    pub segments: Vec<SegmentSpec>,

    /// 段间连接符（默认空串）
    #[serde(default)]
    pub joiner: String,

    /// 校验正则（可选，manual 兜底 + auto 结果校验）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,

    /// 是否启用断号补偿（连号域才开）
    #[serde(default)]
    pub enable_gap: bool,

    /// 是否使用 PG SEQUENCE 兜底（极端高并发可选）
    #[serde(default)]
    pub use_sequence: bool,

    /// 规则优先级（多规则选优，取大）
    #[serde(default = "default_priority")]
    pub priority: i32,

    /// 是否启用
    #[serde(default = "default_active")]
    pub is_active: bool,

    /// 所属域编码（如 `fi`），空串=兼容存量/全局可见
    #[serde(default)]
    pub domain_code: String,

    /// 所属应用编码（如 `cmxfico`）
    #[serde(default)]
    pub application_code: String,

    /// 所属模块编码（如 `gl`）
    #[serde(default)]
    pub module_code: String,
}

fn default_mode() -> String {
    "auto".to_string()
}
fn default_priority() -> i32 {
    100
}
fn default_active() -> bool {
    true
}

impl RuleSpec {
    /// 流水段的宽度（取第一个 serial/dateSerial 段的 width，无则返回 0）。
    pub fn serial_width(&self) -> usize {
        self.segments
            .iter()
            .find(|s| s.seg_type() == "serial" || s.seg_type() == "dateSerial")
            .and_then(|s| s.width())
            .unwrap_or(0)
    }

    /// 流水段的起始值（默认 1）。
    pub fn serial_start(&self) -> i64 {
        self.segments
            .iter()
            .find(|s| s.seg_type() == "serial" || s.seg_type() == "dateSerial")
            .and_then(|s| s.start())
            .unwrap_or(1)
    }

    /// 流水段的步长（默认 1）。
    pub fn serial_step(&self) -> i64 {
        self.segments
            .iter()
            .find(|s| s.seg_type() == "serial" || s.seg_type() == "dateSerial")
            .and_then(|s| s.step())
            .unwrap_or(1)
    }

    /// 流水段的补位符（默认 `'0'`）。
    pub fn serial_pad_char(&self) -> char {
        self.segments
            .iter()
            .find(|s| s.seg_type() == "serial" || s.seg_type() == "dateSerial")
            .map(|s| s.pad_char())
            .unwrap_or('0')
    }

    /// 流水段的补位方向（默认 `Left`）。
    pub fn serial_pad_side(&self) -> PadSide {
        self.segments
            .iter()
            .find(|s| s.seg_type() == "serial" || s.seg_type() == "dateSerial")
            .map(|s| s.pad_side())
            .unwrap_or(PadSide::Left)
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 挂载点声明层（存 DCT/DOC 定义，引用规则 + 声明 target 行为）
// ═══════════════════════════════════════════════════════════════════════════════

/// DCT/DOC 定义里的 codeRule 挂载点声明。
///
/// 引用规则表的算法（`rule_code`）+ 声明 target 行为（`field`/`mode`/局部覆盖/`cascade`）。
/// cascade 放挂载点不放规则表（与 target 强绑定，放规则表会污染多目标复用）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeRule {
    /// 引用的规则码（auto 必填，JSON key = ruleCode）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule_code: Option<String>,

    /// target.field：铸号写回本表的哪列（引擎只认这个，不硬编码列名）
    #[serde(default = "default_field")]
    pub field: String,

    /// 模式（局部覆盖规则表 mode）
    #[serde(default = "default_mode")]
    pub mode: String,

    /// 是否启用断号补偿（局部覆盖规则表 enable_gap）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enable_gap: Option<bool>,

    /// 校验正则（manual 模式兜底）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,

    /// 是否强制 UNIQUE 校验
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unique_check: Option<bool>,

    /// 级联回填配置（可选，仅 DOC 父表）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cascade: Option<Cascade>,
}

fn default_field() -> String {
    "code".to_string()
}

/// 局部覆盖（挂载点对规则表 enable_gap/pattern 的覆盖）。
#[derive(Debug, Clone, Default)]
pub struct Overrides {
    pub enable_gap: Option<bool>,
    pub pattern: Option<String>,
}

impl CodeRule {
    /// 取挂载点的局部覆盖。
    pub fn local_overrides(&self) -> Overrides {
        Overrides {
            enable_gap: self.enable_gap,
            pattern: self.pattern.clone(),
        }
    }

    /// 将挂载点声明 + 规则算法合并为一条可执行的 RuleSpec。
    ///
    /// 挂载点的局部覆盖（enable_gap/pattern/mode）优先于规则表值。
    pub fn merge_with(&self, mut rule: RuleSpec) -> RuleSpec {
        // 局部覆盖
        if let Some(eg) = self.enable_gap {
            rule.enable_gap = eg;
        }
        if let Some(p) = &self.pattern {
            rule.pattern = Some(p.clone());
        }
        rule.mode = self.mode.clone();
        rule
    }
}

/// 级联回填配置（父表铸号后沿 relations 回填子表）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cascade {
    /// 回填到子表的哪个字段
    pub field: String,

    /// 回填范围
    #[serde(default = "default_cascade_scope")]
    pub scope: Scope,
}

fn default_cascade_scope() -> Scope {
    Scope::Children
}

/// 级联回填范围。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    /// 仅直接子表
    Children,
    /// 沿 relations 链下钻全部后代
    Descendants,
}

// ═══════════════════════════════════════════════════════════════════════════════
// 段定义 + 段求值结果
// ═══════════════════════════════════════════════════════════════════════════════

/// 单个段的声明（`segments[]` 数组元素）。
///
/// 用 `serde_json::Value` 宽松存储段参数，各段 resolver 自行取值——不同段类型的参数差异大，
/// 强类型 enum 会让 serde 反序列化变复杂，宽松存储 + resolver 内取值更灵活。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentSpec {
    /// 段类型：`const` / `serial` / `date` / `dateSerial` / `ref` / `random` / `custom[:name]`
    #[serde(rename = "type")]
    pub r#type: String,

    /// 段参数（各段类型专属字段都在这里）
    #[serde(flatten)]
    pub params: serde_json::Map<String, serde_json::Value>,
}

impl SegmentSpec {
    /// 段类型字符串（去掉 `custom:` 前缀的原始 type）。
    pub fn seg_type(&self) -> &str {
        &self.r#type
    }

    /// 取参数（字符串）。
    pub fn get_str(&self, key: &str) -> Option<&str> {
        self.params.get(key).and_then(|v| v.as_str())
    }

    /// 取参数（u64）。
    pub fn get_u64(&self, key: &str) -> Option<u64> {
        self.params.get(key).and_then(|v| v.as_u64())
    }

    /// 取参数（i64）。
    pub fn get_i64(&self, key: &str) -> Option<i64> {
        self.params.get(key).and_then(|v| v.as_i64())
    }

    /// 流水宽度（serial/dateSerial 段用）。
    pub fn width(&self) -> Option<usize> {
        self.get_u64("width").map(|v| v as usize)
    }

    /// 流水起始值（默认 1）。
    pub fn start(&self) -> Option<i64> {
        self.get_i64("start")
    }

    /// 流水步长（默认 1）。
    pub fn step(&self) -> Option<i64> {
        self.get_i64("step")
    }

    /// 重置依据（`resetBy`）。
    pub fn reset_by(&self) -> Option<&str> {
        self.get_str("resetBy")
    }

    /// 补位符（默认 `"0"`）。
    pub fn pad_char(&self) -> char {
        self.get_str("padChar").and_then(|s| s.chars().next()).unwrap_or('0')
    }

    /// 补位方向（默认 `left`）。
    pub fn pad_side(&self) -> PadSide {
        match self.get_str("padSide") {
            Some("right") => PadSide::Right,
            _ => PadSide::Left,
        }
    }

    /// 截断模式（默认 `none` 超长报错；`right` 右截断）。
    pub fn truncate_mode(&self) -> &str {
        self.get_str("truncate").unwrap_or("none")
    }

    /// 截断目标宽度（配合 truncate 使用，取 `width` 或 `take` 字段）。
    pub fn truncate_width(&self) -> Option<usize> {
        self.get_u64("width")
            .or_else(|| self.get_u64("take"))
            .map(|v| v as usize)
    }
}

/// 补位方向。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PadSide {
    Left,
    Right,
}

/// 段求值结果（三态）。
///
/// 对应方案 §2.2：`Literal`（直接用）/ `NeedsSerial`（交流水推进器反查 max）/ `NeedsUniqueCheck`（随机段）。
#[derive(Debug, Clone)]
pub enum SegmentValue {
    /// 固定值（const/date/ref/custom 段返回）
    Literal(String),

    /// 需要流水推进（serial/dateSerial 段返回）
    NeedsSerial {
        /// 重置键（resetBy 求值结果，如日期字符串或分类值）
        reset_key: String,
        /// 流水宽度
        width: usize,
        /// 步长
        step: i64,
        /// 起始值
        start: i64,
        /// 补位符
        pad_char: char,
        /// 补位方向
        pad_side: PadSide,
    },

    /// 需要唯一性检查（random 段返回，C5 阶段用）
    NeedsUniqueCheck { candidate: String },
}

// ═══════════════════════════════════════════════════════════════════════════════
// Target（调用上下文，由钩子传入）
// ═══════════════════════════════════════════════════════════════════════════════

/// 铸号目标（由 DCT/DOC 钩子调用时传入）。
///
/// 引擎只认 `field`（如 `doc_no`），不认识具体列名——换采购订单只需改 `field="order_no"`，引擎零改。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Target {
    /// `dct` | `doc`
    pub kind: String,
    /// 物理表名（如 `cv_header`、`cf_bus_partner`）
    pub code: String,
    /// 铸号写回的列名（如 `doc_no`、`code`）
    pub field: String,
}

impl Target {
    /// 构造 DCT target。
    pub fn dct(code: &str, field: &str) -> Self {
        Self {
            kind: "dct".into(),
            code: code.into(),
            field: field.into(),
        }
    }

    /// 构造 DOC target。
    pub fn doc(code: &str, field: &str) -> Self {
        Self {
            kind: "doc".into(),
            code: code.into(),
            field: field.into(),
        }
    }
}

/// 校验 CodeRule 的 mode 是否合法。
pub fn validate_mode(mode: &str) -> Result<()> {
    match mode {
        "auto" | "manual" => Ok(()),
        _ => Err(CodeError::InvalidSegment(format!(
            "mode 必须是 auto 或 manual，实际：{mode}"
        ))),
    }
}
