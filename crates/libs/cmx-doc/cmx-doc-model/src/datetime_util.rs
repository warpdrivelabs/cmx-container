//! cmx-doc-model/datetime_util —— 日期/时间解析归一(集中收口)。
//!
//! 消除 query.rs:427 与 saver.rs:1494-1510 的重复实现,集中处理:
//! - RFC3339 带时区(`2026-07-07T09:00:00Z` / `+08:00`)
//! - 无时区常见格式(`2026-07-07T09:00:00` / `2026-07-07 09:00:00`,按 UTC 解释)
//! - 纯日期(`2026-07-07` → NaiveDate)

use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};

/// 无时区日期时间的兜底解析格式(按优先级尝试)。
///
/// `%.f` 支持小数秒;空格/T 两种分隔符兼容前端不同序列化路径。
const NAIVE_DATETIME_FORMATS: &[&str] = &[
    "%Y-%m-%dT%H:%M:%S%.f",
    "%Y-%m-%d %H:%M:%S%.f",
    "%Y-%m-%dT%H:%M:%S",
    "%Y-%m-%d %H:%M:%S",
];

/// 解析日期时间字符串,统一归一到 [`Utc`]。
///
/// 兼容 RFC3339(优先)与多种无时区格式(按 UTC 解释)。失败返回 `None`。
///
/// # Arguments
/// * `s` - 原始字符串,可能带或不带时区。
///
/// # Returns
/// * `Some(DateTime<Utc>)` - 解析成功并以 UTC 表示。
/// * `None` - 所有格式均无法解析。
///
/// # Examples
///
/// ```
/// use cmx_doc_model::datetime_util::parse_datetime_utc;
/// let a = parse_datetime_utc("2026-07-07T09:00:00Z");
/// let b = parse_datetime_utc("2026-07-07 09:00:00");
/// assert!(a.is_some() && b.is_some());
/// ```
pub fn parse_datetime_utc(s: &str) -> Option<DateTime<Utc>> {
    let s = s.trim();
    // 优先按 RFC3339 解析(带时区,最严格)
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&Utc));
    }
    // 退化为无时区格式,按 UTC 解释
    for fmt in NAIVE_DATETIME_FORMATS {
        if let Ok(ndt) = NaiveDateTime::parse_from_str(s, fmt) {
            return Some(DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc));
        }
    }
    None
}

/// 解析 `YYYY-MM-DD` 短日期。
///
/// # Arguments
/// * `s` - 日期字符串。
///
/// # Returns
/// * `Some(NaiveDate)` - 解析成功。`None` - 格式不符。
pub fn parse_naive_date(s: &str) -> Option<NaiveDate> {
    s.trim().parse::<NaiveDate>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_rfc3339() {
        let dt = parse_datetime_utc("2026-07-07T09:00:00Z").unwrap();
        assert_eq!(dt.to_rfc3339(), "2026-07-07T09:00:00+00:00");
    }

    #[test]
    fn parse_rfc3339_with_offset() {
        let dt = parse_datetime_utc("2026-07-07T17:00:00+08:00").unwrap();
        assert_eq!(dt.to_rfc3339(), "2026-07-07T09:00:00+00:00");
    }

    #[test]
    fn parse_naive_iso() {
        let dt = parse_datetime_utc("2026-07-07T09:00:00").unwrap();
        assert_eq!(dt.to_rfc3339(), "2026-07-07T09:00:00+00:00");
    }

    #[test]
    fn parse_naive_space() {
        let dt = parse_datetime_utc("2026-07-07 09:00:00").unwrap();
        assert_eq!(dt.to_rfc3339(), "2026-07-07T09:00:00+00:00");
    }

    #[test]
    fn parse_with_fractional() {
        let dt = parse_datetime_utc("2026-07-07T09:00:00.123Z").unwrap();
        assert!(dt.to_rfc3339().starts_with("2026-07-07T09:00:00.123"));
    }

    #[test]
    fn parse_invalid_returns_none() {
        assert!(parse_datetime_utc("not a date").is_none());
        assert!(parse_datetime_utc("").is_none());
    }

    #[test]
    fn parse_date_short() {
        let d = parse_naive_date("2026-07-07").unwrap();
        assert_eq!(d.to_string(), "2026-07-07");
    }

    #[test]
    fn parse_date_with_whitespace() {
        let d = parse_naive_date("  2026-07-07  ").unwrap();
        assert_eq!(d.to_string(), "2026-07-07");
    }
}
