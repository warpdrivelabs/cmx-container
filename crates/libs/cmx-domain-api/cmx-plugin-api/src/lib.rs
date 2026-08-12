//! cmx-plugin-api —— 插件管理 / 市场 / 表元数据的 HTTP 层。
//!
//! 薄 axum handler 调 cmx-plugin 服务（PluginManager / MarketplaceService / TableMetadataService）。
//! 各 Module 实现 cmx-api-core 的 ModuleRoutes，由 cmx-platform-app 合并进主路由。
//! PluginApiDoc 提供本域 OpenApi 切片，由 platform-app 用 OpenApi::merge() 聚合。

pub mod handlers;
pub mod openapi;

pub use openapi::PluginApiDoc;
pub use handlers::{
    marketplace::MarketplaceModule, plugin::PluginModule,
    table_metadata::TableMetadataModule,
};
