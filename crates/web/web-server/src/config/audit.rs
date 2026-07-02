//! 审计日志器初始化
//!
//! 在 web-server 启动流程中创建 `DatabaseAuditStore` 并包装为 `Arc<dyn AuditLogger>`，
//! 供 `cmx-iam` / `cmx-auth` 等业务服务通过 `with_audit()` / `with_audit_logger()` 注入。
//!
//! # 初始化顺序
//!
//! 必须在 `init_datasources()` 之后调用（依赖 `cmx_database::get_default_db_manager()`）。
//! 推荐在 `init_datasources()` 之后、`init_iam_services()` 之前调用，以便把审计 logger
//! 注入到 IAM 各 Service。

use std::sync::Arc;

use cmx_audit::{AuditLogger, DefaultAuditLogger};
use cmx_database::get_default_db_manager;
use tracing::info;

use crate::error::Result;

/// 构建审计日志器
///
/// 基于全局 `DatabaseManager` + `application.id` 配置（缺省 `"default"`）构造
/// `DatabaseAuditStore`，并包装为 `Arc<dyn AuditLogger>`。
///
/// # 返回
///
/// `Arc<dyn AuditLogger>`，可直接传入 `service.with_audit(logger)` /
/// `service.with_audit_logger(logger)`。
///
/// # 错误
///
/// 当前 `DefaultAuditLogger::with_db` 自身不会失败，本函数保留 `Result` 签名以保持
/// 与同目录其他 `init_*` 函数的风格一致，便于未来扩展（例如校验 app_id 格式、
/// 探测数据库连通性等）。
pub async fn build_audit_logger() -> Result<Arc<dyn AuditLogger>> {
    // 1. 解析 app_id

    let app_id = cmx_utils::ConfigManager::global().get_app_id();

    // 2. 获取 DatabaseManager 与默认 db_id
    let mm = get_default_db_manager();
    let db_id = mm.get_default_db_id().await;

    // 3. 构造 DefaultAuditLogger（内部包装 DatabaseAuditStore）
    let logger: DefaultAuditLogger =
        DefaultAuditLogger::with_db(mm.clone(), db_id.clone(), app_id.clone());
    let audit_logger: Arc<dyn AuditLogger> = Arc::new(logger);
    info!(
        app_id = %app_id,
        db_id = %db_id,
        "审计日志器初始化完成（DatabaseAuditStore）"
    );
    Ok(audit_logger)
}
