-- 删除插件市场相关表（按依赖顺序逆序删除）
DROP TABLE IF EXISTS cmx_marketplace_rating;
DROP TABLE IF EXISTS cmx_marketplace_download_stats;
DROP TABLE IF EXISTS cmx_marketplace_plugin_version;
DROP TABLE IF EXISTS cmx_marketplace_plugin;
