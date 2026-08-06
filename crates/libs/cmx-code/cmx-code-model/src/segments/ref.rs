//! ⑤ 引用段（ref）：取字段值或映射。

use async_trait::async_trait;

use crate::context::ResolveContext;
use crate::error::{CodeError, Result};
use crate::segments::SegmentResolver;
use crate::spec::{SegmentSpec, SegmentValue};

/// 引用段 resolver。
///
/// 支持：
/// - `field`：取行数据的该字段值
/// - `refField`：树形引用——取父记录的字段值（钩子层以 refField 名将父记录字段值塞进 attrs）
/// - `map`：字段值映射（如 `{"raw": "RM"}`）
/// - `take`：取前 N 位
/// - `pad`：补位到指定宽度
/// - `fallback`：取不到值时的替代字符（默认空串）
/// - `truncate`：超长截断（`right`=右截断 / `none`=报错，默认 none）
pub struct RefResolver;

#[async_trait(?Send)]
impl SegmentResolver for RefResolver {
    fn seg_type(&self) -> &str {
        "ref"
    }

    fn resolve(&self, seg: &SegmentSpec, ctx: &ResolveContext) -> Result<SegmentValue> {
        let field = seg
            .get_str("field")
            .ok_or_else(|| CodeError::InvalidSegment("ref 段缺 field 参数".into()))?;

        // 取字段值：优先 refField（树形引用——钩子层把父记录的该字段值以 refField 名塞进 attrs）
        // 典型：{"field":"parent_id","refField":"code"} → 取 attrs["code"]（父记录的 code）
        let ref_field = seg.get_str("refField");
        let raw = ref_field
            .and_then(|rf| ctx.attr_str(rf))
            .or_else(|| ctx.attr_str(field));
        let fallback = seg.get_str("fallback").unwrap_or("");
        let mut value = raw.unwrap_or(fallback).to_string();

        // 映射（map）
        if let Some(map) = seg.params.get("map").and_then(|v| v.as_object()) {
            if let Some(mapped) = map.get(&value).and_then(|v| v.as_str()) {
                value = mapped.to_string();
            }
        }

        // 取前 N 位（take）
        if let Some(take) = seg.get_u64("take") {
            value = value.chars().take(take as usize).collect();
        }

        // 补位（pad 到指定宽度）
        if let Some(pad_width) = seg.get_u64("pad") {
            let pad_char = seg.pad_char();
            let side = seg.pad_side();
            value = crate::pad::pad(&value, pad_width as usize, pad_char, side);
        }

        // 截断（truncate：超长时 right=右截断 / none=报错）
        if let Some(tw) = seg.truncate_width() {
            value = crate::pad::truncate(&value, tw, seg.truncate_mode())?;
        }

        Ok(SegmentValue::Literal(value))
    }
}
