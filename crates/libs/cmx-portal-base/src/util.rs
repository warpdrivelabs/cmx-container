//! 共享小工具：ID/路径段校验 + 异步写锁。
//!
//! 复刻 Node 各 store 的 `SAFE_ID` / `SAFE_SEGMENT` 正则与 `withLock` 串行化语义。

use std::sync::OnceLock;

use tokio::sync::Mutex;

use crate::error::{PortalError, PortalResult};

/// 整体引用 ID（允许点分命名空间）：`[a-zA-Z0-9._-]{1,128}`。
pub fn is_safe_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-' || b == b'_')
}

/// 单路径段：`[a-zA-Z0-9_-]+`（不含点，用于 domain/app/module 段）。
pub fn is_safe_segment(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// 文件名：`[a-zA-Z0-9_.-]+\.json`。
pub fn is_safe_json_file(s: &str) -> bool {
    s.ends_with(".json")
        && !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-' || b == b'_')
}

/// 校验整体 ID，非法返回 [`PortalError::BadRequest`]。
pub fn validate_id(id: &str, field: &str) -> PortalResult<String> {
    let t = id.trim();
    if t.is_empty() {
        return Err(PortalError::bad_request(format!("缺少必填字段 {field}")));
    }
    if !is_safe_id(t) {
        return Err(PortalError::bad_request(format!(
            "{field} 仅允许字母、数字、._-，长度 1–128"
        )));
    }
    Ok(t.to_string())
}

/// 进程内全局写锁（粗粒度，串行化所有门户写操作）。
///
/// Node 端各 store 各有一把 `_lock`；这里用单把全局锁，对「文件 JSON 低频写」足够，
/// 且实现简单、不会死锁。若未来某资源写入成为热点，可按资源拆分细粒度锁。
pub fn write_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// 测试专用：串行化「修改进程级 `CMX_PORTAL_DATA_ROOT` 环境变量」的单元测试。
///
/// `cargo test` 默认并行跑用例，而 `data_root()` 读的是进程级环境变量；多个测试同时
/// `set_var` 会相互污染。需要切换数据根的测试统一锁此 `std::sync::Mutex`（同步锁，
/// 跨 `.await` 不持有即可），保证彼此独占。
///
/// 经 `testing` feature 暴露给**依赖本 crate 的下游 crate**（如 cmx-portal）的单元测试，
/// 因为它们的测试用例与本函数已不在同一 crate；正常构建不启用此 feature，不进入产物。
#[cfg(any(test, feature = "testing"))]
pub fn test_data_root_lock() -> &'static std::sync::Mutex<()> {
    static LOCK: OnceLock<std::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(()))
}
