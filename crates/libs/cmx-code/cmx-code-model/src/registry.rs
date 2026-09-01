//! 段 resolver 注册表。
//!
//! 内置 6 段（const/serial/date/dateSerial/ref/custom）在 [`SegmentRegistry::new`] 自动注册。
//! 随机段（random）在 C5 阶段注册。自定义段（`custom:xxx`）可通过 [`SegmentRegistry::register`] 扩展。

use crate::error::{CodeError, Result};
use crate::segments::{self, SegmentResolver};
use crate::spec::{SegmentSpec, SegmentValue};
use std::collections::HashMap;
use std::sync::Arc;

/// 段 resolver 注册表。
pub struct SegmentRegistry {
    resolvers: HashMap<String, Arc<dyn SegmentResolver>>,
}

impl SegmentRegistry {
    /// 创建注册表并注册内置段（const/serial/date/dateSerial/ref/custom）。
    pub fn new() -> Self {
        let mut reg = Self {
            resolvers: HashMap::new(),
        };
        segments::register_builtin(&mut reg);
        reg
    }

    /// 注册一个段 resolver。
    pub fn register(&mut self, resolver: Box<dyn SegmentResolver>) {
        let seg_type = resolver.seg_type().to_string();
        self.resolvers.insert(seg_type, resolver.into());
    }

    /// 取段 type 对应的 resolver（按 `type` 字段精确匹配或 `custom:*` 前缀匹配）。
    fn get(&self, seg_type: &str) -> Option<&Arc<dyn SegmentResolver>> {
        // 精确匹配（const/serial/date/dateSerial/ref/random）
        if let Some(r) = self.resolvers.get(seg_type) {
            return Some(r);
        }
        // custom:xxx 前缀匹配 → 用 custom resolver
        if seg_type.starts_with("custom:") {
            return self.resolvers.get("custom");
        }
        None
    }

    /// 求值单个段。
    pub fn resolve(
        &self,
        seg: &SegmentSpec,
        ctx: &crate::context::ResolveContext,
    ) -> Result<SegmentValue> {
        let type_str = seg.seg_type();
        let resolver = self
            .get(type_str)
            .ok_or_else(|| CodeError::UnknownSegmentType(type_str.into()))?;
        resolver.resolve(seg, ctx)
    }
}

impl Default for SegmentRegistry {
    fn default() -> Self {
        Self::new()
    }
}
