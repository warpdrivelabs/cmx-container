//! 插件管理 API 模块
//!
//! 提供插件安装、卸载、升级、降级、列表查询等 HTTP API

pub mod handler;
pub mod request;
pub mod response;

use axum::routing::{get, post};
use axum::Router;

use crate::app_state::CmxAppState;

pub use handler::{
    plugin_downgrade, plugin_get, plugin_install, plugin_list, plugin_page, plugin_uninstall,
    plugin_upgrade,
};

/// 创建插件管理路由
pub fn plugin_routes() -> Router<CmxAppState> {
    Router::new()
        .route("/install", post(plugin_install))
        .route("/uninstall", post(plugin_uninstall))
        .route("/upgrade", post(plugin_upgrade))
        .route("/downgrade", post(plugin_downgrade))
        .route("/list", get(plugin_list))
        .route("/page", get(plugin_page))
        .route("/{plugin_id}", get(plugin_get))
}
