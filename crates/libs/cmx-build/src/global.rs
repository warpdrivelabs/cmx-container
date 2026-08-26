//! 全局构建执行器单例（平台层装配一次，端点共享）。
//!
//! 平台启动时 [`init`] 一次（注入真 Builder + scan/sign/deploy 实现）；端点经 [`get`] 取用。
//! 未 init 时 [`try_get`] 返回 None，端点可据此回退"仅落作业记录"。

use std::sync::Arc;
use std::sync::OnceLock;

use crate::executor::BuildExecutor;

static EXECUTOR: OnceLock<Arc<BuildExecutor>> = OnceLock::new();

/// 装配全局执行器（幂等：已装配则忽略后续调用）。
pub fn init(executor: Arc<BuildExecutor>) {
    let _ = EXECUTOR.set(executor);
}

/// 取全局执行器（未装配返回 None）。
pub fn try_get() -> Option<Arc<BuildExecutor>> {
    EXECUTOR.get().cloned()
}
