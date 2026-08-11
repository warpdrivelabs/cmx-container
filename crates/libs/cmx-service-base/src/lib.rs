//! cmx-service-base —— 基础服务库。
//!
//! 把「一个微服务起服前要初始化的基础设施」收成可复用原语，供各微服务 main 按需调用：
//! - [`BaseConfig`]：配置加载（轻量 toml 直读 / 重量 ConfigManager façade）。
//! - [`init_cache`]（feature `redis`）：Redis 缓存 + 分布式锁。
//! - [`register_pg_datasources`]：tokio-postgres 数据源注册（flow + portal 共享）。
//! - [`register_sqlx_datasources`]（feature `db-sqlx`）：sqlx 数据源注册（portal 专属）。
//!
//! 与 cmx-web-chassis（纯框架层）分层：本 crate 碰 Redis/DB/ConfigManager，feature 门控让不需要的
//! 服务干净 opt-out（`default-features = false`）。各微服务 main 把自己的配置文件路径交给本库
//! （`from_toml_path` / `from_config_manager`），本库解析并驱动基础设施初始化。

mod config;
mod datasource;

pub use config::BaseConfig;
#[cfg(feature = "config-manager")]
pub use config::init_config_manager;
pub use datasource::register_pg_datasources;

#[cfg(feature = "redis")]
mod cache;
#[cfg(feature = "redis")]
pub use cache::init_cache;

// —— 纯全局单例（配置后可浮动）——
#[cfg(feature = "crypto")]
mod crypto;
#[cfg(feature = "crypto")]
pub use crypto::init_crypto;

#[cfg(feature = "debug")]
mod debug;
#[cfg(feature = "debug")]
pub use debug::init_debug;

#[cfg(feature = "event-bus")]
mod event_bus;
#[cfg(feature = "event-bus")]
pub use event_bus::init_event_bus;

// —— 中等基础设施（读 ConfigManager + 全局单例）——
#[cfg(feature = "storage")]
mod storage;
#[cfg(feature = "storage")]
pub use storage::init_storage;

#[cfg(feature = "plugins")]
mod plugins;
#[cfg(feature = "plugins")]
pub use plugins::init_plugins;

#[cfg(feature = "services")]
mod services;
#[cfg(feature = "services")]
pub use services::{init_service_invoker, init_services};

#[cfg(feature = "wasm")]
mod wasm;
#[cfg(feature = "wasm")]
pub use wasm::init_wasm;

// —— 通用微服务能力（Nacos 注册/配置中心 + RPC；非 portal 专属）——
#[cfg(feature = "registry-config")]
mod registry_config;
#[cfg(feature = "registry-config")]
pub use registry_config::{init_infra, shutdown_infra};

#[cfg(feature = "rpc")]
mod rpc;
#[cfg(feature = "rpc")]
pub use rpc::{ServiceAuthConfig, init_rpc, load_service_auth_config};

/// 基础服务错误。各微服务在自己的错误类型里 `From`/`map_err` 转换。
#[derive(Debug, thiserror::Error)]
pub enum BaseError {
    /// 配置加载/解析失败。
    #[error("配置错误: {0}")]
    Config(String),
    /// 基础设施初始化失败（Redis/DB 等）。
    #[error("基础设施初始化失败: {0}")]
    Setup(String),
}

/// 基础服务结果别名。
pub type Result<T> = std::result::Result<T, BaseError>;
