//! cmx-plugin-demo — 企业级 WASM 插件开发最佳实践示例
//!
//! 以"订单管理"为业务场景，演示所有宿主函数的使用方式：
//!
//! - **基础功能**：`greet`（简单入参出参）、`demo_log`（四级日志）
//! - **缓存操作**：`cache_order_status`、`get_cached_order`、`remove_order_cache`
//! - **数据库操作**：`query_orders`、`create_order`、`update_order`、`delete_order`
//! - **插件调用**：`check_inventory`、`check_remote_inventory`、`call_order_service`、`call_remote_order_service`
//! - **服务编排**：`route_check`、`branch_process`、`merge_result`、`tx_create_order`、`tx_update_stock`、`final_process`
//!
//! # 架构模式
//!
//! 采用三层分离模式：
//! - `handlers/` — 纯业务逻辑，通过泛型 `H: HostFunctions` 与宿主解耦
//! - `host.rs` — 抽象接口，定义宿主能力 trait
//! - `extism/` — Extism 适配层，将 HostCaller 委托为 HostFunctions 实现

pub mod handlers;
pub mod host;
pub mod models;

#[cfg(test)]
pub mod tests;

#[cfg(feature = "extism")]
pub mod extism;
