//! 段类型模块。
//!
//! 每种段类型实现 [`SegmentResolver`] trait，返回 [`SegmentValue`]（三态）。
//! 段注册到 [`crate::registry::SegmentRegistry`]，`rule_algo` 遍历段序列调对应 resolver。

pub mod const_;
pub mod custom;
pub mod date;
pub mod date_serial;
pub mod r#ref;
pub mod random;
pub mod serial;

use async_trait::async_trait;

use crate::context::ResolveContext;
use crate::error::Result;
use crate::spec::{SegmentSpec, SegmentValue};

/// 段求值 trait（每种段类型实现一个）。
#[async_trait(?Send)]
pub trait SegmentResolver: Send + Sync {
    /// 该段处理的 type 字符串（如 `"const"` / `"serial"`）。
    fn seg_type(&self) -> &str;

    /// 求值段，返回 [`SegmentValue`]（Literal / NeedsSerial / NeedsUniqueCheck）。
    fn resolve(&self, seg: &SegmentSpec, ctx: &ResolveContext) -> Result<SegmentValue>;
}

/// 注册内置段到 registry。
pub fn register_builtin(registry: &mut crate::registry::SegmentRegistry) {
    registry.register(Box::new(const_::ConstResolver));
    registry.register(Box::new(serial::SerialResolver));
    registry.register(Box::new(date::DateResolver));
    registry.register(Box::new(date_serial::DateSerialResolver));
    registry.register(Box::new(r#ref::RefResolver));
    registry.register(Box::new(random::RandomResolver));
    registry.register(Box::new(custom::CustomResolver));
}
