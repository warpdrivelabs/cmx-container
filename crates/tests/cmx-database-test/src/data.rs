//! 数据文件生成与加载。
//!
//! 生成一个固定的 50 列模板行到 JSON 文件（`bench_row.json`），两条驱动路径都从该文件
//! 加载同一模板，重复 N 次插入（内容相同，仅主键 id 递增）——满足“插入相同数据”的对比要求。

use crate::schema::RowTemplate;
use anyhow::{Context, Result};
use std::path::Path;
use std::str::FromStr;

/// 模板行 JSON 文件的默认路径（相对 crate 根）。
pub const DATA_FILE: &str = "bench_row.json";

/// 若数据文件不存在则生成一个固定模板行。
///
/// 内容确定（不含随机/时间），保证可复现。
pub fn ensure_data_file(path: &Path) -> Result<()> {
    if path.exists() {
        return Ok(());
    }
    let json = serde_json::json!({
        "ints":  [1_i64, 2, 3, 100, 200, 300, 1000, 2000, 3000, -1, -50, 99999, 8_000_000_000_i64, 42, 7],
        "texts": [
            "alpha", "bravo", "charlie", "delta", "echo",
            "状态正常", "PZ-0007", "描述文本内容适中长度用于模拟真实业务字段",
            "foxtrot", "golf", "hotel", "india", "juliet", "kilo", "lima"
        ],
        "nums":  ["1130000.0000", "0.5000", "99999.9900", "12345.6789", "0.0001", "888888.8888", "-42.4200", "1000000.0000"],
        "times": [
            "2026-07-05T12:00:00Z", "2026-01-01T00:00:00Z", "2025-12-31T23:59:59Z",
            "2026-06-15T08:30:00Z", "2024-02-29T06:00:00Z"
        ],
        "flags": [true, false, true],
        "uuids": ["11111111-1111-1111-1111-111111111111", "22222222-2222-2222-2222-222222222222"],
        "json":  {"k": "v", "n": 1, "arr": [1, 2, 3], "nested": {"a": "b"}}
    });
    let pretty = serde_json::to_string_pretty(&json)?;
    std::fs::write(path, pretty)
        .with_context(|| format!("写入数据文件失败: {}", path.display()))?;
    Ok(())
}

/// 从 JSON 文件加载模板行。
pub fn load_template(path: &Path) -> Result<RowTemplate> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("读取数据文件失败: {}", path.display()))?;
    let v: serde_json::Value = serde_json::from_str(&raw)?;

    let ints: Vec<i64> = v["ints"]
        .as_array()
        .context("ints 缺失")?
        .iter()
        .map(|x| x.as_i64().context("ints 元素非整数"))
        .collect::<Result<_>>()?;
    let texts: Vec<String> = v["texts"]
        .as_array()
        .context("texts 缺失")?
        .iter()
        .map(|x| x.as_str().unwrap_or_default().to_string())
        .collect();
    let nums: Vec<rust_decimal::Decimal> = v["nums"]
        .as_array()
        .context("nums 缺失")?
        .iter()
        .map(|x| {
            rust_decimal::Decimal::from_str(x.as_str().unwrap_or("0")).context("nums 解析失败")
        })
        .collect::<Result<_>>()?;
    let times: Vec<chrono::DateTime<chrono::Utc>> = v["times"]
        .as_array()
        .context("times 缺失")?
        .iter()
        .map(|x| {
            chrono::DateTime::parse_from_rfc3339(x.as_str().unwrap_or_default())
                .map(|d| d.with_timezone(&chrono::Utc))
                .context("times 解析失败")
        })
        .collect::<Result<_>>()?;
    let flags: Vec<bool> = v["flags"]
        .as_array()
        .context("flags 缺失")?
        .iter()
        .map(|x| x.as_bool().unwrap_or(false))
        .collect();
    let uuids: Vec<uuid::Uuid> = v["uuids"]
        .as_array()
        .context("uuids 缺失")?
        .iter()
        .map(|x| uuid::Uuid::from_str(x.as_str().unwrap_or_default()).context("uuids 解析失败"))
        .collect::<Result<_>>()?;
    let json = v["json"].clone();

    let tpl = RowTemplate {
        ints,
        texts,
        nums,
        times,
        flags,
        uuids,
        json,
    };

    let cc = tpl.col_count();
    anyhow::ensure!(
        cc == crate::schema::DATA_COLS,
        "模板列数 {} 与预期 {} 不符",
        cc,
        crate::schema::DATA_COLS
    );
    Ok(tpl)
}

/// 把一行模板序列化为 PostgreSQL 文本 COPY 的一行（TAB 分隔，`\N` 表示 NULL）。
///
/// 列顺序：id, 15×int, 15×text, 8×num, 5×ts, 3×flag, 2×uuid, 1×json。
/// 两条驱动的 COPY 路径共用此格式化，保证一致。
pub fn copy_line(tpl: &RowTemplate, id: i64) -> String {
    let mut fields: Vec<String> = Vec::with_capacity(50);
    fields.push(id.to_string());
    for v in &tpl.ints {
        fields.push(v.to_string());
    }
    for v in &tpl.texts {
        fields.push(escape_copy_text(v));
    }
    for v in &tpl.nums {
        fields.push(v.to_string());
    }
    for v in &tpl.times {
        // PG 可解析 RFC3339
        fields.push(v.to_rfc3339());
    }
    for v in &tpl.flags {
        fields.push(if *v { "t".into() } else { "f".into() });
    }
    for v in &tpl.uuids {
        fields.push(v.to_string());
    }
    fields.push(escape_copy_text(&tpl.json.to_string()));
    let mut line = fields.join("\t");
    line.push('\n');
    line
}

/// COPY 文本格式的字段转义：TAB / 换行 / 反斜杠 需转义。
fn escape_copy_text(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('\t', "\\t")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}
