//! ⑥ 随机段（random）：字符池随机 / 数值范围随机。
//!
//! 对应方案 §06。两种模式：
//! - charset：从字符池均匀抽取 N 位（如邀请码 INVK7M3PQ）
//! - range：区间随机整数 + 补位（如订单号 260804-7381）
//!
//! 随机段返回 `NeedsUniqueCheck`，不反查 max（无 max 概念），靠 UNIQUE 冲突重试。

use async_trait::async_trait;
use rand::Rng;

use crate::context::ResolveContext;
use crate::error::{CodeError, Result};
use crate::segments::SegmentResolver;
use crate::spec::{SegmentSpec, SegmentValue};

/// 随机段 resolver。
pub struct RandomResolver;

#[async_trait(?Send)]
impl SegmentResolver for RandomResolver {
    fn seg_type(&self) -> &str {
        "random"
    }

    fn resolve(&self, seg: &SegmentSpec, _ctx: &ResolveContext) -> Result<SegmentValue> {
        let mode = seg.get_str("mode").unwrap_or("charset");
        let candidate = match mode {
            "charset" => gen_charset(seg)?,
            "range" => gen_range(seg)?,
            other => {
                return Err(CodeError::InvalidSegment(format!(
                    "random 段 mode 不支持：{other}（支持 charset/range）"
                )))
            }
        };
        Ok(SegmentValue::NeedsUniqueCheck { candidate })
    }
}

/// charset 模式：从字符池均匀抽取 width 位。
fn gen_charset(seg: &SegmentSpec) -> Result<String> {
    let width = seg
        .width()
        .ok_or_else(|| CodeError::InvalidSegment("random charset 段缺 width".into()))?;
    let charset_name = seg.get_str("charset").unwrap_or("alnum");
    let exclude_ambiguous = seg
        .params
        .get("excludeAmbiguous")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let pool = charset_pool(charset_name, exclude_ambiguous);
    if pool.is_empty() {
        return Err(CodeError::InvalidSegment(format!(
            "random charset 段字符池为空：charset={charset_name}"
        )));
    }

    let mut rng = rand::thread_rng();
    let pool_vec: Vec<char> = pool.chars().collect();
    let s: String = (0..width)
        .map(|_| pool_vec[rng.gen_range(0..pool_vec.len())])
        .collect();
    Ok(s)
}

/// range 模式：[min, max] 区间随机整数 + 补位到 pad 位。
fn gen_range(seg: &SegmentSpec) -> Result<String> {
    let min = seg
        .get_i64("min")
        .ok_or_else(|| CodeError::InvalidSegment("random range 段缺 min".into()))?;
    let max = seg
        .get_i64("max")
        .ok_or_else(|| CodeError::InvalidSegment("random range 段缺 max".into()))?;
    let pad_width = seg
        .get_u64("pad")
        .ok_or_else(|| CodeError::InvalidSegment("random range 段缺 pad".into()))?
        as usize;

    if min >= max {
        return Err(CodeError::InvalidSegment(format!(
            "random range 段 min({min}) 必须 < max({max})"
        )));
    }

    let mut rng = rand::thread_rng();
    let n = rng.gen_range(min..=max);
    Ok(format!("{n:0pad_width$}", pad_width = pad_width))
}

/// 字符池（方案 §6.2）。
///
/// `exclude_ambiguous=true` 时：
/// - `alpha` / `alnum`：去除易混淆字符 0/O/1/I/l/Z/2/B/8（这些池默认已去部分，此处兜底全去）
/// - `digit` / `hex`：**不过滤**（纯数字/16 进制无字母歧义，过滤会删 0/2 导致池缺失）
/// - 用户自定义字符串（other）：按 alpha/alnum 同款过滤（保守，防用户池里混入歧义字符）
fn charset_pool(name: &str, exclude_ambiguous: bool) -> String {
    // digit / hex 是纯数字/16进制池，无字母歧义，exclude_ambiguous 对它们无效（方案 §6.2）
    let (pool, should_filter) = match name {
        "digit" => ("0123456789", false),
        "alpha" => ("ABCDEFGHJKLMNPQRSTUVWXYZ", true),
        "alnum" => ("23456789ABCDEFGHJKLMNPQRSTUVWXYZ", true),
        "hex" => ("0123456789abcdef", false),
        other => return other.to_string(), // 用户直接传字符池字符串（含 "custom"），不过滤
    };
    if should_filter && exclude_ambiguous {
        // 去除全部 9 个易混淆字符：0/O/1/I/l/Z/2/B/8（方案 §6.2）
        pool.chars()
            .filter(|c| !matches!(c, '0' | 'O' | '1' | 'I' | 'l' | 'Z' | '2' | 'B' | '8'))
            .collect()
    } else {
        pool.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_charset_seg(width: usize, charset: &str) -> SegmentSpec {
        let mut params = serde_json::Map::new();
        params.insert("type".into(), "random".into());
        params.insert("mode".into(), "charset".into());
        params.insert("width".into(), (width as u64).into());
        params.insert("charset".into(), charset.into());
        SegmentSpec {
            r#type: "random".into(),
            params,
        }
    }

    #[test]
    fn test_gen_charset_length() {
        let seg = make_charset_seg(6, "alnum");
        let s = gen_charset(&seg).unwrap();
        assert_eq!(s.len(), 6);
    }

    #[test]
    fn test_gen_range() {
        let mut params = serde_json::Map::new();
        params.insert("mode".into(), "range".into());
        params.insert("min".into(), 1000i64.into());
        params.insert("max".into(), 9999i64.into());
        params.insert("pad".into(), 4u64.into());
        let seg = SegmentSpec {
            r#type: "random".into(),
            params,
        };
        let s = gen_range(&seg).unwrap();
        assert_eq!(s.len(), 4);
        let n: i64 = s.parse().unwrap();
        assert!((1000..=9999).contains(&n));
    }

    /// excludeAmbiguous=true 时 alnum 池去掉全部 9 个易混淆字符（0/O/1/I/l/Z/2/B/8）。
    #[test]
    fn test_charset_pool_excludes_ambiguous() {
        let pool = charset_pool("alnum", true);
        for c in ['0', 'O', '1', 'I', 'l', 'Z', '2', 'B', '8'] {
            assert!(!pool.contains(c), "alnum 池 excludeAmbiguous 后仍含 '{}'", c);
        }
    }

    /// digit/hex 池 excludeAmbiguous 不生效（纯数字/16 进制无字母歧义）。
    #[test]
    fn test_charset_pool_digit_hex_not_filtered() {
        let digit_pool = charset_pool("digit", true);
        assert!(digit_pool.contains('0'));
        assert!(digit_pool.contains('2'));
        let hex_pool = charset_pool("hex", true);
        assert!(hex_pool.contains('0'));
        assert!(hex_pool.contains('2'));
    }
}
