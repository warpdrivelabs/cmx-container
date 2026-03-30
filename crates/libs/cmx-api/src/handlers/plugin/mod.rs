/*
 * @Author: yqs
 * @Date: 2026-03-26 16:37:40
 * @Describe: 
 * @LastEditors: yqs
 * @LastEditTime: 2026-03-30 08:50:35
 */
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
    plugin_deploy, plugin_downgrade, plugin_get, plugin_install, plugin_list, plugin_page,
    plugin_uninstall, plugin_upgrade,
};

/// 创建插件管理路由
pub fn plugin_routes() -> Router<CmxAppState> {
    Router::new()
        .route("/deploy", post(plugin_deploy))
        .route("/install", post(plugin_install))
        .route("/uninstall", post(plugin_uninstall))
        .route("/upgrade", post(plugin_upgrade))
        .route("/downgrade", post(plugin_downgrade))
        .route("/list", post(plugin_list))
        .route("/page", post(plugin_page))
        .route("/{plugin_id}", get(plugin_get))
}
