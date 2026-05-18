//! 插件市场 API 模块。
//!
//! 提供插件市场相关的所有 REST API 端点。
//!
//! # API 路由
//!
//! 所有路由前缀：`/api/marketplace`
//!
//! | 路由 | 方法 | 端点说明 |
//! |------|------|---------|
//! | `/plugin/page` | POST | 分页查询插件列表 |
//! | `/plugin/get` | GET | 查询插件详情 |
//! | `/plugin/publish` | POST | 发布插件到市场 |
//! | `/plugin/update` | POST | 更新插件信息 |
//! | `/plugin/delete` | POST | 删除插件（逻辑删除） |
//! | `/plugin/version/list` | POST | 查询插件版本列表 |
//! | `/plugin/version/get` | GET | 查询版本详情 |
//! | `/plugin/install` | POST | 从市场安装插件 |
//! | `/plugin/rate` | POST | 对插件评分 |
//! | `/plugin/rating/list` | POST | 查询评分列表 |
//! | `/plugin/upgrade` | POST | 从市场升级插件 |
//! | `/plugin/check-updates` | POST | 检查插件更新 |
//! | `/plugin/download` | GET | 下载插件包 |
//! | `/category/list` | POST | 查询分类统计 |
//! | `/stats/trending/list` | POST | 查询热门插件 |
//!
//! # 路由设计原则
//!
//! 每个操作使用独立语义路径（如 `/plugin/get`、`/plugin/publish`），
//! 禁止同一路径用不同 HTTP 方法区分不同操作。

pub mod handler;
pub mod request;
pub mod response;

pub use handler::*;
pub use request::*;
pub use response::*;

use crate::app_state::CmxAppState;
use axum::routing::{get, post};
use axum::Router;

use crate::routes::traits::ModuleRoutes;

/// 定义插件市场内部子路由。
///
/// 聚合所有插件市场相关的路由路径。
fn inner_routes() -> Router<CmxAppState> {
    Router::new()
        .route("/plugin/page", post(marketplace_plugin_page))
        .route("/plugin/get", get(marketplace_plugin_get_by_id))
        .route("/plugin/publish", post(marketplace_plugin_publish))
        .route("/plugin/update", post(marketplace_plugin_update))
        .route("/plugin/delete", post(marketplace_plugin_delete))
        .route("/plugin/version/list", post(marketplace_plugin_version_list))
        .route("/plugin/version/get", get(marketplace_plugin_version_get_by_id))
        .route("/plugin/install", post(marketplace_plugin_install))
        .route("/plugin/upgrade", post(marketplace_plugin_upgrade))
        .route("/plugin/check-updates", post(marketplace_plugin_check_updates))
        .route("/plugin/download", get(marketplace_plugin_download))
        .route("/plugin/rate", post(marketplace_plugin_rate))
        .route("/plugin/rating/list", post(marketplace_plugin_rating_list))
        .route("/category/list", post(marketplace_category_list))
        .route("/stats/trending/list", post(marketplace_trending_list))
}

/// 插件市场路由模块。
///
/// 实现 `ModuleRoutes` trait，注册到全局路由树。
pub struct MarketplaceModule;

impl ModuleRoutes for MarketplaceModule {
    /// 返回聚合了所有插件市场路由的 Router。
    fn routes(self) -> Router<CmxAppState> {
        Router::new().nest("/marketplace", inner_routes())
    }

    /// 路由前缀（用于 OpenAPI 文档）。
    fn prefix() -> &'static str {
        "/marketplace"
    }

    /// 模块名称（用于日志和调试）。
    fn module_name(&self) -> &'static str {
        "marketplace"
    }
}
