//! Handler 注册表：编译期 `inventory` 收集业务注册的 [`JobHandler`]（母版 = cmx-rpt-formula 函数注册）。
//!
//! 业务 crate 一条 `inventory::submit!` 即完成注册，无需碰框架（方案 §7.5）：
//! ```ignore
//! inventory::submit! { RegisteredJob { make: || Box::new(RptComputeJob) } }
//! ```
//! [`JobManager`](crate::JobManager) 启动时 [`build_registry`] 收集全部，建 `kind → 构造器` 映射。

use std::collections::HashMap;

use crate::context::JobHandler;

/// 一条注册项：`make` 是 handler 的构造器（每次 new 一个实例，handler 无状态）。
pub struct RegisteredJob {
    /// 构造 handler 实例（`kind()` 决定注册键）。
    pub make: fn() -> Box<dyn JobHandler>,
}

inventory::collect!(RegisteredJob);

/// 收集所有 `inventory` 注册的 handler，建 `kind → 构造器` 映射。
///
/// 同 kind 重复注册时后者覆盖并告警（正常情况唯一）。
pub fn build_registry() -> HashMap<&'static str, fn() -> Box<dyn JobHandler>> {
    let mut map: HashMap<&'static str, fn() -> Box<dyn JobHandler>> = HashMap::new();
    for reg in inventory::iter::<RegisteredJob> {
        let kind = (reg.make)().kind();
        if map.insert(kind, reg.make).is_some() {
            tracing::warn!(kind, "JobHandler 种类重复注册，后者覆盖前者");
        }
    }
    tracing::info!(count = map.len(), "作业 Handler 注册表构建完成");
    map
}

/// 列出所有已注册的 kind（诊断/日志用）。
pub fn registered_kinds() -> Vec<&'static str> {
    inventory::iter::<RegisteredJob>
        .into_iter()
        .map(|r| (r.make)().kind())
        .collect()
}
