//! 同步原语工具函数。
//!
//! 提供 `std::sync::RwLock` 的辅助函数，统一处理锁 poisoned 的策略，
//! 避免 panic 传播导致整个服务不可用。
//!
//! # Poisoned 锁策略
//!
//! - 锁 poisoned 时打印警告日志并返回内部数据（可能不一致）。
//! - 调用方应检查返回的数据完整性，poisoned 后的状态属于异常降级。
//! - 适用于锁持有时间短、不跨 `await` 的场景。
//! - **禁止持锁跨 `await`**，否则会阻塞整个 tokio runtime。

use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

use tracing::warn;

/// 读取 `std::sync::RwLock` 的辅助函数。
///
/// 锁 poisoned 时打印警告并返回内部数据，避免 panic 传播。
/// 适用于锁持有时间短、不跨 `await` 的场景。
///
/// # Poisoned 锁说明
///
/// 返回的数据可能不一致（panic 发生时数据可能处于半更新状态），
/// 调用方应自行评估数据完整性。该策略属于"优雅降级"，避免因单点 panic
/// 导致整个服务不可用。建议运维层面监控 poisoned 锁告警频率。
pub fn read_lock<T>(lock: &RwLock<T>) -> RwLockReadGuard<'_, T> {
    lock.read().unwrap_or_else(|e| {
        warn!("RwLock 读锁 poisoned: {}", e);
        e.into_inner()
    })
}

/// 写入 `std::sync::RwLock` 的辅助函数。
///
/// 锁 poisoned 时打印警告并返回内部数据，避免 panic 传播。
/// 适用于锁持有时间短、不跨 `await` 的场景。
///
/// # Poisoned 锁说明
///
/// 返回的数据可能不一致（panic 发生时数据可能处于半更新状态），
/// 调用方应自行评估数据完整性。该策略属于"优雅降级"，避免因单点 panic
/// 导致整个服务不可用。建议运维层面监控 poisoned 锁告警频率。
pub fn write_lock<T>(lock: &RwLock<T>) -> RwLockWriteGuard<'_, T> {
    lock.write().unwrap_or_else(|e| {
        warn!("RwLock 写锁 poisoned: {}", e);
        e.into_inner()
    })
}
