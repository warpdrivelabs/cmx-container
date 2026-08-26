//! 最小 5 字段 cron 匹配器（分 时 日 月 周）—— 支持 `*`、`a-b`、`a,b`、`*/n`、`a-b/n`。
//!
//! 供 W3 定时触发调度器判断"某绑定的 cron 表达式是否匹配当前时刻"。不引 cron crate（自持、可单测）。

use chrono::{DateTime, Datelike, Local, Timelike};

/// 判断 cron 表达式是否匹配给定本地时刻（精确到分）。非法表达式 → false（不触发）。
pub fn matches(expr: &str, now: DateTime<Local>) -> bool {
    let fields: Vec<&str> = expr.split_whitespace().collect();
    if fields.len() != 5 {
        return false;
    }
    let minute = now.minute();
    let hour = now.hour();
    let dom = now.day();
    let month = now.month();
    // chrono weekday: Mon=0..Sun=6；cron: Sun=0..Sat=6（也容忍 7=Sun）。
    let dow_cron = (now.weekday().num_days_from_sunday()) as u32;

    field_matches(fields[0], minute, 0, 59)
        && field_matches(fields[1], hour, 0, 23)
        && field_matches(fields[2], dom, 1, 31)
        && field_matches(fields[3], month, 1, 12)
        && dow_matches(fields[4], dow_cron)
}

fn dow_matches(field: &str, dow: u32) -> bool {
    // 容忍 7 表示周日。
    if field_matches(field, dow, 0, 6) {
        return true;
    }
    if dow == 0 {
        return field_matches(field, 7, 0, 7);
    }
    false
}

/// 单字段匹配：`*` / `n` / `a-b` / `a,b,c` / `*/n` / `a-b/n`（逗号分隔多段）。
fn field_matches(field: &str, val: u32, lo: u32, hi: u32) -> bool {
    field.split(',').any(|part| part_matches(part, val, lo, hi))
}

fn part_matches(part: &str, val: u32, lo: u32, hi: u32) -> bool {
    // 拆步长 base/step。
    let (base, step) = match part.split_once('/') {
        Some((b, s)) => (b, s.parse::<u32>().unwrap_or(1).max(1)),
        None => (part, 1),
    };

    let (start, end) = if base == "*" {
        (lo, hi)
    } else if let Some((a, b)) = base.split_once('-') {
        match (a.parse::<u32>(), b.parse::<u32>()) {
            (Ok(a), Ok(b)) => (a, b),
            _ => return false,
        }
    } else {
        match base.parse::<u32>() {
            Ok(n) => (n, n),
            Err(_) => return false,
        }
    };

    if val < start || val > end {
        return false;
    }
    // 步长：从 start 起每隔 step 命中。
    (val - start) % step == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(min: u32, hour: u32, day: u32, month: u32) -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, month, day, hour, min, 0).unwrap()
    }

    #[test]
    fn wildcard_every_minute() {
        assert!(matches("* * * * *", at(0, 0, 1, 1)));
        assert!(matches("* * * * *", at(37, 13, 15, 6)));
    }

    #[test]
    fn specific_minute_hour() {
        // 0 9 * * * → 每天 9:00
        assert!(matches("0 9 * * *", at(0, 9, 1, 1)));
        assert!(!matches("0 9 * * *", at(1, 9, 1, 1)));
        assert!(!matches("0 9 * * *", at(0, 10, 1, 1)));
    }

    #[test]
    fn step_every_5_min() {
        assert!(matches("*/5 * * * *", at(0, 0, 1, 1)));
        assert!(matches("*/5 * * * *", at(15, 0, 1, 1)));
        assert!(!matches("*/5 * * * *", at(7, 0, 1, 1)));
    }

    #[test]
    fn range_and_list() {
        // 分钟 10-12 或 30
        assert!(matches("10-12,30 * * * *", at(11, 0, 1, 1)));
        assert!(matches("10-12,30 * * * *", at(30, 0, 1, 1)));
        assert!(!matches("10-12,30 * * * *", at(13, 0, 1, 1)));
    }

    #[test]
    fn month_field() {
        assert!(matches("0 0 1 6 *", at(0, 0, 1, 6)));
        assert!(!matches("0 0 1 6 *", at(0, 0, 1, 7)));
    }

    #[test]
    fn invalid_expr_no_match() {
        assert!(!matches("bad", at(0, 0, 1, 1)));
        assert!(!matches("* * * *", at(0, 0, 1, 1))); // 4 字段
    }
}
