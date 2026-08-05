//! # CMX 编码引擎 · 纯逻辑层
//!
//! 通用业务编码生成引擎的核心模型层，提供编码规则的类型定义、段求值、补位/步长等纯算法。
//!
//! ## 设计要点
//!
//! - **无 DB / 无 HTTP 依赖**：本 crate 只含纯逻辑，可被 `cmx-dct-store-pg` / `cmx-doc-store-pg`
//!   等钩子轻量依赖，不会拉入数据库或 Web 框架。
//! - **Advance trait 抽象**：所有 DB 操作（反查 max、取断号、UNIQUE 重试插入）抽象为
//!   [`advance::Advance`] trait，由 `cmx-code-api` 实现，通过依赖注入传入——钩子层不直接
//!   依赖 `cmx-code-api`，避免环依赖。
//! - **七种段类型**：固定 / 流水 / 日期 / 日期流水 / 引用 / 随机 / 自定义（随机段在 C5 阶段补）。
//!
//! ## 快速开始
//!
//! ```rust,ignore
//! use cmx_code_model::{RuleSpec, Target, ResolveContext, advance::StubAdvance, rule_algo};
//!
//! // 构造规则（固定段 V + 流水段宽度 4）
//! let rule: RuleSpec = serde_json::from_str(r#"{
//!     "segments": [
//!         {"type": "const", "value": "V"},
//!         {"type": "serial", "width": 4}
//!     ]
//! }"#).unwrap();
//!
//! let target = Target::dct("bus_partner", "code");
//! let ctx = ResolveContext::for_test();
//! let advance = StubAdvance;
//! // evaluate_segments 是 async fn，需在 async runtime 内调用：
//! // let code = rule_algo::evaluate_segments(&rule, &target, &ctx, &advance).await.unwrap();
//! ```

pub mod advance;
pub mod context;
pub mod error;
pub mod pad;
pub mod registry;
pub mod rule_algo;
pub mod segments;
pub mod spec;

pub use advance::{Advance, StubAdvance};
pub use context::ResolveContext;
pub use error::{CodeError, Result};
pub use registry::SegmentRegistry;
pub use rule_algo::{evaluate_segments, resolve_fixed_segments};
pub use spec::{
    Cascade, CodeRule, Overrides, RuleSpec, Scope, SegmentSpec, SegmentValue, Target,
};
