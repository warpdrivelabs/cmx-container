//! cmx-doc-model/error —— 本 crate 内部错误精度。
//!
//! 不破坏全局 `BizError` 桥接:对外提供 `From<ModelError> for cmx_biz::BizError`,
//! 让上层 `?` 继续工作;同时让公式/解析等错误在本 crate 内有结构化精度,
//! 便于 rule.rs 等调用方按错误类别决策(如"不可求值则跳过")。
//!
//! > 注:本 crate 只在内部用 thiserror 提升精度,不算 P3 全局错误类型重构。

use cmx_biz::BizError;

/// cmx-doc-model 内部错误。
#[derive(thiserror::Error, Debug)]
pub enum ModelError {
    /// 公式求值错误(词法/解析/求值/类型/约束)。
    #[error("公式求值: {0}")]
    Formula(#[from] FormulaError),

    /// 单据定义或查询 JSON 解析错误。
    #[error("解析: {0}")]
    Parse(String),
}

/// 公式引擎错误类别。
#[derive(thiserror::Error, Debug)]
pub enum FormulaError {
    /// 除零(`a / 0`)。改造后不再静默返回 0,而是上抛让 rule 决策。
    #[error("除零")]
    DivByZero,

    /// 函数参数数量不符(如 `ABS()` 空参、`IF` 缺 else 分支)。
    #[error("函数 {name} 参数数不符(期望 {expected}, 实际 {actual})")]
    Arity {
        /// 函数名。
        name: String,
        /// 期望参数数(下界)。
        expected: usize,
        /// 实际参数数。
        actual: usize,
    },

    /// 调用了未知函数。
    #[error("未知函数 {0}")]
    UnknownFunction(String),

    /// 调用了未知运算符。
    #[error("未知运算符 {0}")]
    UnknownOperator(String),

    /// 其他求值期错误(类型不符、字段缺失等)。
    #[error("{0}")]
    Eval(String),
}

impl From<ModelError> for BizError {
    /// 把本 crate 内部错误翻译为全局 BizError,保持 `?` 桥接不变。
    fn from(e: ModelError) -> Self {
        BizError::business(e.to_string())
    }
}

impl From<FormulaError> for BizError {
    /// 公式错误可直接 `?` 传播到 BizError(经 ModelError 中转,保持单一转译源)。
    fn from(e: FormulaError) -> Self {
        BizError::business(ModelError::from(e).to_string())
    }
}
