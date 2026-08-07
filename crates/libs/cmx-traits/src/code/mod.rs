//! 编码引擎全局注入点。
//!
//! `CodeMinter` trait 在本 crate 定义（用 `serde_json::Value` 传参，不依赖 cmx-code-model），
//! 实现在 cmx-code-api（`impl CodeMinter for CodeEngine`）。
//! DCT/DOC 钩子（cmx-dct-store-pg / cmx-doc-store-pg）通过 [`GlobalCodeMinter::get`] 获取实例，
//! 无需直接依赖 cmx-code-api——避免环依赖。
//!
//! 设计参照 `runtime/global.rs`（OnceLock + set/get 全局注入）。

use std::sync::{Arc, OnceLock};

use async_trait::async_trait;

/// 编码引擎铸号 trait（DCT/DOC 钩子调用入口）。
///
/// 参数全用 `serde_json::Value` 传递（不依赖 cmx-code-model 的强类型）：
/// - `code_rule`：挂载点声明（`{mode, field, ruleCode, enableGap, ...}`）
/// - `target`：`{kind, code, field}`
/// - `attrs`：行属性
/// - `db_id` / `txn_id`：事务句柄
///
/// 返回铸出的编码字符串。未配置编码引擎（mode=manual 或 code_rule=None）时不应调用此 trait。
#[async_trait]
pub trait CodeMinter: Send + Sync {
    /// 单条铸号。
    async fn mint(
        &self,
        code_rule: &serde_json::Value,
        target: &serde_json::Value,
        attrs: &serde_json::Value,
        db_id: &str,
        txn_id: Option<&str>,
    ) -> Result<String, String>;

    /// 批量铸号（N 行同表，返回 N 个编码）。
    async fn mint_batch(
        &self,
        code_rule: &serde_json::Value,
        target: &serde_json::Value,
        rows: &[serde_json::Value],
        db_id: &str,
        txn_id: Option<&str>,
    ) -> Result<Vec<String>, String>;

    /// 删行记断号：把被删行的编码解析成断号记录到 cmx_code_gap 表。
    ///
    /// 仅 enable_gap=true（连号域）的规则才记录；其他规则静默跳过（删了不产生可填补断号）。
    /// 内部查规则表拿 merge 后的 RuleSpec（含 segments），用 `parse_code_serial` 反解 prefix + serial_val。
    ///
    /// - `code_rule`：挂载点声明（含 ruleCode + enableGap）
    /// - `code`：被删行的编码值
    /// - `attrs`：被删行的字段属性（供 ref/resetBy 段求 prefix）
    /// - `db_id`：数据库 ID
    ///
    /// 返回是否成功记录（false=无需记录/解析失败/规则未开断号）。
    async fn record_gap_for_code(
        &self,
        code_rule: &serde_json::Value,
        code: &str,
        attrs: &serde_json::Value,
        db_id: &str,
    ) -> bool;
}

/// 全局编码引擎存储器。
///
/// 在 web-server 启动时通过 [`GlobalCodeMinter::set`] 注入 `CodeEngine` 实例。
/// DCT/DOC 钩子通过 [`GlobalCodeMinter::get`] 获取（None 时跳过=现状零影响）。
pub struct GlobalCodeMinter;

static CODE_MINTER: OnceLock<Arc<dyn CodeMinter>> = OnceLock::new();

impl GlobalCodeMinter {
    /// 设置全局编码引擎实例。
    pub fn set(minter: Arc<dyn CodeMinter>) -> Result<(), &'static str> {
        CODE_MINTER
            .set(minter)
            .map_err(|_| "GlobalCodeMinter 已初始化，无法重复设置")
    }

    /// 获取全局编码引擎实例（None=未注入，钩子应跳过=现状零影响）。
    pub fn get() -> Option<&'static Arc<dyn CodeMinter>> {
        CODE_MINTER.get()
    }
}
