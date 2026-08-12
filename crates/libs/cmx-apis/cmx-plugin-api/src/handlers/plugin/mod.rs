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

pub mod data_handler;
pub mod handler;
pub mod request;
pub mod response;
// pub mod control;

use axum::Router;
use axum::routing::{get, post};

use cmx_api_core::CmxAppState;
use cmx_api_core::ModuleRoutes;

pub use handler::{
    plugin_deploy, plugin_downgrade, plugin_exists, plugin_functions, plugin_get, plugin_install,
    plugin_list, plugin_page, plugin_uninstall, plugin_upgrade,
};

/// 内部路由（不含前缀）
///
/// 所有路由挂在 `/api/plugin` 下，覆盖插件本地运行时的部署 / 生命周期 /
/// 查询 / 数据导入导出。市场（marketplace）相关接口见 cmx-plugin-api::marketplace。
fn inner_routes() -> Router<CmxAppState> {
    Router::new()
        // 部署插件（本地加载到运行时，等同于首次 install + 启用）
        .route("/deploy", post(plugin_deploy))
        // 安装插件（从市场或本地包，写入运行时）
        .route("/install", post(plugin_install))
        // 卸载插件（移除运行时实例，保留元数据）
        .route("/uninstall", post(plugin_uninstall))
        // 升级插件到更高版本
        .route("/upgrade", post(plugin_upgrade))
        // 降级插件到更低版本
        .route("/downgrade", post(plugin_downgrade))
        // 列表查询已安装插件（按条件）
        .route("/list", post(plugin_list))
        // 分页查询已安装插件
        .route("/page", post(plugin_page))
        // 判断指定 plugin_code 是否已安装
        .route("/exists", get(plugin_exists))
        // 列出插件对外暴露的可调用函数清单
        .route("/functions", post(plugin_functions))
        // 查询插件详情（元数据 + 运行时状态）
        .route("/{plugin_id}", get(plugin_get))
        // 通用插件数据导入（供远程模式 http_url/http_discovery 调用）
        .route("/data/import", post(data_handler::import_resource_data))
        // 通用插件资源数据列表查询
        .route("/data/list", get(data_handler::list_resource_data))
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
