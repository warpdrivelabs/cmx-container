//! 插件市场模块
//!
//! 提供插件市场的数据模型、数据仓库、业务服务和统计功能。
//!
//! # 模块结构
//!
//! - `model`: 数据模型（MarketplacePlugin, MarketplacePluginVersion, MarketplaceDownloadStats, MarketplaceRating）
//! - `repository`: 数据仓库，提供 CRUD 操作
//! - `service`: 业务服务，包含搜索、发布、评分、安装等核心逻辑
//! - `stats`: 统计服务，提供下载统计和评分汇总

pub mod model;
pub mod repository;
pub mod service;
pub mod stats;

// 导出核心类型
pub use model::{
    MarketplacePlugin, MarketplacePluginVersion, MarketplaceDownloadStats, MarketplaceRating,
};
pub use repository::MarketplaceRepository;
pub use service::MarketplaceService;
pub use stats::StatsService;
