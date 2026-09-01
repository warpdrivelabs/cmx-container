//! ④ 日期流水段（dateSerial）：日期 + 按日重置流水（date + serial 语法糖）。
//!
//! 等价于先求日期段再求流水段，但 resetBy 固定为日期——按日重置流水。

use async_trait::async_trait;

use crate::context::ResolveContext;
use crate::error::{CodeError, Result};
use crate::segments::SegmentResolver;
use crate::spec::{SegmentSpec, SegmentValue};

/// 日期流水段 resolver。
pub struct DateSerialResolver;

#[async_trait(?Send)]
impl SegmentResolver for DateSerialResolver {
    fn seg_type(&self) -> &str {
        "dateSerial"
    }

    fn resolve(&self, seg: &SegmentSpec, ctx: &ResolveContext) -> Result<SegmentValue> {
        let format = seg
            .get_str("format")
            .ok_or_else(|| CodeError::InvalidSegment("dateSerial 段缺 format 参数".into()))?;
        let width = seg
            .width()
            .ok_or_else(|| CodeError::InvalidSegment("dateSerial 段缺 width 参数".into()))?;

        // 日期部分（Literal）
        let date_str = super::date::format_date(&ctx.now, format)?;

        // 流水部分（NeedsSerial，reset_key = 日期串，即按日重置）
        Ok(SegmentValue::NeedsSerial {
            reset_key: date_str,
            width,
            step: seg.step().unwrap_or(1),
            start: seg.start().unwrap_or(1),
            pad_char: seg.pad_char(),
            pad_side: seg.pad_side(),
        })
    }
}
