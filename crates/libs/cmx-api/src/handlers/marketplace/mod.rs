//! 插件市场 API 模块
//!
//! 提供插件市场的 HTTP API，包括：
//! - 插件的发布、查询、更新、删除
//! - 版本管理
//! - 从市场安装插件
//! - 评分和评论
//! - 分类和热门统计

pub mod handler;
pub mod request;
pub mod response;

use axum::routing::{get, post};
use axum::Router;

use crate::app_state::CmxAppState;
use crate::routes::traits::ModuleRoutes;

pub use handler::{
    marketplace_category_list, marketplace_plugin_delete, marketplace_plugin_get_by_id,
    marketplace_plugin_install, marketplace_plugin_page, marketplace_plugin_publish,
    marketplace_plugin_rate, marketplace_plugin_rating_list, marketplace_plugin_update,
    marketplace_plugin_version_get_by_id, marketplace_plugin_version_list,
    marketplace_trending_list,
};

/// 内部路由（不含前缀）
fn inner_routes() -> Router<CmxAppState> {
    Router::new()
        // 分页查询插件
        .route("/plugin/page", post(marketplace_plugin_page))
        // 查询单条插件
        .route("/plugin", get(marketplace_plugin_get_by_id))
        // 发布插件
        .route("/plugin", post(marketplace_plugin_publish))
        // 更新插件
        .route("/plugin/update", post(marketplace_plugin_update))
        // 删除插件
        .route("/plugin/delete", post(marketplace_plugin_delete))
        // 版本列表
        .route("/plugin/version/list", post(marketplace_plugin_version_list))
        // 版本详情
        .route("/plugin/version", get(marketplace_plugin_version_get_by_id))
        // 从市场安装
        .route("/plugin/install", post(marketplace_plugin_install))
        // 评分
        .route("/plugin/rate", post(marketplace_plugin_rate))
        // 评分列表
        .route("/plugin/rating/list", post(marketplace_plugin_rating_list))
        // 分类列表
        .route("/category/list", post(marketplace_category_list))
        // 热门插件
        .route("/stats/trending/list", post(marketplace_trending_list))
}

/// Marketplace 模块路由
pub struct MarketplaceModule;

impl ModuleRoutes for MarketplaceModule {
    fn routes(self) -> Router<CmxAppState> {
        Router::new().nest("/marketplace", inner_routes())
    }

    fn prefix() -> &'static str {
        "marketplace"
    }

    fn module_name(&self) -> &'static str {
        "marketplace"
    }
}
