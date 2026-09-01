//! ⑦ 自定义段（custom）：扩展点，对接已注册的 `custom:*` resolver。
//!
//! 内置实例：`custom:check_digit`（校验位段，mod11 算法）。
//! 用户可通过 `SegmentRegistry::register` 注册自己的 `custom:xxx` resolver。

use async_trait::async_trait;

use crate::context::ResolveContext;
use crate::error::{CodeError, Result};
use crate::segments::SegmentResolver;
use crate::spec::{SegmentSpec, SegmentValue};

/// 自定义段分发 resolver。
///
/// 根据 `type` 字段的 `custom:xxx` 前缀分发到对应实现。内置 `custom:check_digit`。
/// 未注册的自定义类型报错。
pub struct CustomResolver;

#[async_trait(?Send)]
impl SegmentResolver for CustomResolver {
    fn seg_type(&self) -> &str {
        "custom"
    }

    fn resolve(&self, seg: &SegmentSpec, ctx: &ResolveContext) -> Result<SegmentValue> {
        // type 格式：custom:<name>，取 <name>
        let type_str = seg.seg_type();
        let name = type_str
            .strip_prefix("custom:")
            .ok_or_else(|| CodeError::UnknownSegmentType(type_str.into()))?;

        match name {
            "check_digit" => resolve_check_digit(seg, ctx),
            other => Err(CodeError::UnknownSegmentType(format!("custom:{other}"))),
        }
    }
}

/// mod11 校验位算法。
///
/// 对前序段拼接结果算 mod11 校验位。
/// `algo` 参数：`mod11`（默认）/ `luhn`。
fn resolve_check_digit(seg: &SegmentSpec, ctx: &ResolveContext) -> Result<SegmentValue> {
    let prefix = ctx.resolved_so_far.join("");
    let algo = seg.get_str("algo").unwrap_or("mod11");
    let check = match algo {
        "mod11" => mod11_check(&prefix),
        "luhn" => luhn_check(&prefix),
        other => {
            return Err(CodeError::InvalidSegment(format!(
                "不支持的校验位算法：{other}（支持 mod11/luhn）"
            )))
        }
    };
    Ok(SegmentValue::Literal(check.to_string()))
}

/// mod11 校验位：权重从右到左 2..7 循环，乘积和 mod 11，取余数（10 → 'X'）。
fn mod11_check(s: &str) -> char {
    let digits: Vec<u32> = s
        .chars()
        .filter_map(|c| c.to_digit(10))
        .collect();
    if digits.is_empty() {
        return '0';
    }
    let sum: u32 = digits
        .iter()
        .rev()
        .zip((2..=7).cycle())
        .map(|(d, w)| d * w)
        .sum();
    let remainder = sum % 11;
    let check = (11 - remainder) % 11;
    if check == 10 {
        'X'
    } else {
        char::from_digit(check, 10).unwrap_or('0')
    }
}

/// Luhn 校验位。
fn luhn_check(s: &str) -> char {
    let digits: Vec<u32> = s
        .chars()
        .filter_map(|c| c.to_digit(10))
        .collect();
    if digits.is_empty() {
        return '0';
    }
    let sum: u32 = digits
        .iter()
        .rev()
        .enumerate()
        .map(|(i, d)| {
            if i % 2 == 0 {
                let doubled = d * 2;
                doubled / 10 + doubled % 10
            } else {
                *d
            }
        })
        .sum();
    let check = (10 - sum % 10) % 10;
    char::from_digit(check, 10).unwrap_or('0')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mod11() {
        // mod11 只对数字位算：26080008
        let check_digits = mod11_check("26080008");
        assert!(check_digits.is_ascii_digit());
    }

    #[test]
    fn test_luhn() {
        let check = luhn_check("7992739871");
        assert!(check.is_ascii_digit());
    }
}
