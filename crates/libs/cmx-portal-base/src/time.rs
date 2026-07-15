//! 时间工具：统一 epoch 毫秒时间戳，消除各 store 重复实现。

/// 返回当前时间的 UNIX epoch 毫秒数。
///
/// 取值失败（系统时钟早于 `UNIX_EPOCH`）时返回 `0`，绝不 panic。
pub fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
