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
pub mod control;

use axum::routing::{get, post};
use axum::Router;

use crate::app_state::CmxAppState;
use crate::routes::traits::ModuleRoutes;

pub use handler::{
    plugin_deploy, plugin_downgrade, plugin_exists, plugin_functions, plugin_get, plugin_install,
    plugin_list, plugin_page, plugin_uninstall, plugin_upgrade,
};

/// 内部路由（不含前缀）
fn inner_routes() -> Router<CmxAppState> {
    Router::new()
        .route("/deploy", post(plugin_deploy))
        .route("/install", post(plugin_install))
        .route("/uninstall", post(plugin_uninstall))
        .route("/upgrade", post(plugin_upgrade))
        .route("/downgrade", post(plugin_downgrade))
        .route("/list", post(plugin_list))
        .route("/page", post(plugin_page))
        .route("/exists", get(plugin_exists))
        .route("/functions", post(plugin_functions))
        .route("/{plugin_id}", get(plugin_get))
}

/// 创建插件管理路由
pub fn plugin_routes() -> Router<CmxAppState> {
    inner_routes()
}

/// Plugin 模块路由
pub struct PluginModule;

impl ModuleRoutes for PluginModule {
    fn routes(self) -> Router<CmxAppState> {
        Router::new().nest("/plugin", inner_routes())
    }

    fn prefix() -> &'static str {
        "plugin"
    }

    fn module_name(&self) -> &'static str {
        "plugin"
    }
}
