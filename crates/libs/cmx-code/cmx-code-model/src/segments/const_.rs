//! ① 固定段（const）：常量前缀/后缀。

use async_trait::async_trait;

use crate::context::ResolveContext;
use crate::error::{CodeError, Result};
use crate::segments::SegmentResolver;
use crate::spec::{SegmentSpec, SegmentValue};

/// 固定段 resolver。
pub struct ConstResolver;

#[async_trait(?Send)]
impl SegmentResolver for ConstResolver {
    fn seg_type(&self) -> &str {
        "const"
    }

    fn resolve(&self, seg: &SegmentSpec, _ctx: &ResolveContext) -> Result<SegmentValue> {
        let value = seg
            .get_str("value")
            .ok_or_else(|| CodeError::InvalidSegment("const 段缺 value 参数".into()))?;
        Ok(SegmentValue::Literal(value.to_string()))
    }
}
