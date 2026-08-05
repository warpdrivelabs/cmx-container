//! 段求值上下文（`ResolveContext`）。
//!
//! 携带段求值所需的全部上下文：行属性、组织、时间、已铸号 buffer、局部覆盖、DB 事务句柄。
//! 通过 builder 方法链式构造（`.with()/.with_minted()/.with_overrides()/.txn()`）。

use std::sync::Arc;

use chrono::{DateTime, Utc};

use crate::spec::Overrides;

/// 组织上下文（简化版，只含编码引擎需要的字段）。
#[derive(Debug, Clone, Default)]
pub struct OrgContext {
    /// 组织码（与规则的 orgScope 匹配）
    pub org_code: String,
}

/// 段求值上下文。
///
/// 设计要点：
/// - **不可变借用友好**：`minted_buffer` / `overrides` 用 `Arc` 包装，builder 方法 clone Arc 而非内部数据。
/// - **DB 事务透传**：`db_id` + `txn_id` 透传给 `Advance` 实现层（cmx-code-api），model 层不直接用。
/// - **测试友好**：[`ResolveContext::for_test`] 提供最小可用上下文。
#[derive(Debug, Clone)]
pub struct ResolveContext {
    /// 行属性（ref 段取字段值用，如 `{"category": "raw"}`）
    pub attrs: serde_json::Value,

    /// 组织上下文
    pub org: OrgContext,

    /// 当前时间（date/dateSerial 段用）
    pub now: DateTime<Utc>,

    /// 求值过程中已解析的段值（custom 段如校验位段需要前序段结果）
    pub resolved_so_far: Vec<String>,

    /// 同事务已铸号（反查 max 时 union 进候选集，保证多行号连续不重）
    pub minted_buffer: Arc<Vec<String>>,

    /// 挂载点局部覆盖（enableGap/pattern）
    pub overrides: Arc<Overrides>,

    /// 数据库 ID（透传给 Advance 实现层）
    pub db_id: String,

    /// 事务 ID（透传给 Advance 实现层，None = 非事务）
    pub txn_id: Option<String>,
}

impl ResolveContext {
    /// 测试用：最小可用上下文（空 attrs、UTC 当前时间、空 buffer）。
    pub fn for_test() -> Self {
        Self {
            attrs: serde_json::json!({}),
            org: OrgContext::default(),
            now: Utc::now(),
            resolved_so_far: Vec::new(),
            minted_buffer: Arc::new(Vec::new()),
            overrides: Arc::new(Overrides::default()),
            db_id: String::new(),
            txn_id: None,
        }
    }

    /// 带 db_id + txn_id 构造（生产用，钩子层调用时构造）。
    pub fn new(db_id: &str, txn_id: Option<&str>) -> Self {
        Self {
            attrs: serde_json::json!({}),
            org: OrgContext::default(),
            now: Utc::now(),
            resolved_so_far: Vec::new(),
            minted_buffer: Arc::new(Vec::new()),
            overrides: Arc::new(Overrides::default()),
            db_id: db_id.to_string(),
            txn_id: txn_id.map(|s| s.to_string()),
        }
    }

    // ── builder 方法 ──────────────────────────────────────────────────────────

    /// 设置行属性。
    pub fn with(mut self, attrs: serde_json::Value) -> Self {
        self.attrs = attrs;
        self
    }

    /// 设置组织上下文。
    pub fn org(mut self, org: OrgContext) -> Self {
        self.org = org;
        self
    }

    /// 设置已铸号 buffer（同事务多行铸号推进 max 用）。
    pub fn with_minted(mut self, buffer: &[String]) -> Self {
        self.minted_buffer = Arc::new(buffer.to_vec());
        self
    }

    /// 设置局部覆盖。
    pub fn with_overrides(mut self, overrides: Overrides) -> Self {
        self.overrides = Arc::new(overrides);
        self
    }

    /// 取事务 ID（Advance 实现层用）。
    pub fn txn(&self) -> Option<&str> {
        self.txn_id.as_deref()
    }

    /// 取 minted_buffer 引用。
    pub fn minted_buffer(&self) -> &[String] {
        &self.minted_buffer
    }

    /// 取行属性的某个字段值（ref 段用）。
    pub fn attr_str(&self, key: &str) -> Option<&str> {
        self.attrs.get(key).and_then(|v| v.as_str())
    }

    /// 取局部覆盖里的 enable_gap（优先于规则表值）。
    pub fn effective_enable_gap(&self, rule_default: bool) -> bool {
        self.overrides.enable_gap.unwrap_or(rule_default)
    }

    /// 取局部覆盖里的 pattern（优先于规则表值）。
    pub fn effective_pattern(&self, rule_default: &Option<String>) -> Option<String> {
        self.overrides
            .pattern
            .clone()
            .or_else(|| rule_default.clone())
    }
}

impl Default for ResolveContext {
    fn default() -> Self {
        Self::for_test()
    }
}
