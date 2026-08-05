//! 补位 / 填充 / 截断（借鉴金蝶）。
//!
//! 对应方案 §09：流水号补位符、填充方向、超长截断。

use crate::error::{CodeError, Result};
use crate::spec::PadSide;

/// 补位到指定宽度。
///
/// - `left`：左补位（数字流水号默认，如 `0008`）
/// - `right`：右补位（文本默认）
///
/// 如果 `s.len() >= width`，原样返回（不截断——截断由 [`truncate`] 单独控制）。
pub fn pad(s: &str, width: usize, pad_char: char, side: PadSide) -> String {
    let len = s.chars().count();
    if len >= width {
        return s.to_string();
    }
    let pad: String = std::iter::repeat(pad_char)
        .take(width - len)
        .collect();
    match side {
        PadSide::Left => format!("{pad}{s}"),
        PadSide::Right => format!("{s}{pad}"),
    }
}

/// 超长截断。
///
/// - `none`：超长报错（默认）
/// - `right`：右截断
pub fn truncate(s: &str, width: usize, mode: &str) -> Result<String> {
    let len = s.chars().count();
    if len <= width {
        return Ok(s.to_string());
    }
    match mode {
        "right" => Ok(s.chars().take(width).collect()),
        _ => Err(CodeError::SegmentEvalFailed {
            field: "truncate".into(),
            expected: format!("宽度 ≤ {width}"),
            actual: format!("实际 {len}"),
        }),
    }
}

/// 格式化流水号：补位 + 截断。
pub fn format_serial(n: i64, width: usize, pad_char: char, side: PadSide) -> String {
    let s = n.to_string();
    pad(&s, width, pad_char, side)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pad_left() {
        assert_eq!(pad("8", 4, '0', PadSide::Left), "0008");
        assert_eq!(pad("0008", 4, '0', PadSide::Left), "0008");
        assert_eq!(pad("12345", 4, '0', PadSide::Left), "12345");
    }

    #[test]
    fn test_pad_right() {
        assert_eq!(pad("AB", 5, ' ', PadSide::Right), "AB   ");
    }

    #[test]
    fn test_truncate_none_errors() {
        assert!(truncate("12345", 4, "none").is_err());
    }

    #[test]
    fn test_truncate_right() {
        assert_eq!(truncate("12345", 4, "right").unwrap(), "1234");
    }

    #[test]
    fn test_format_serial() {
        assert_eq!(format_serial(8, 4, '0', PadSide::Left), "0008");
        assert_eq!(format_serial(1001, 4, '0', PadSide::Left), "1001");
    }
}
