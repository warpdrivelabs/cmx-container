//! 审计辅助 trait — 消除三个 ServiceImpl 中的重复代码。
//!
//! 提供 `AuditHelper` trait，封装审计日志写入的通用逻辑，
//! 供 `UserServiceImpl` / `RoleServiceImpl` / `PermissionServiceImpl` 等共享复用。

use std::sync::Arc;

use cmx_audit::{AuditDomain, AuditLogger, AuditRecord, OperationResult};
use cmx_core::SVRContext;
use serde::Serialize;

/// 审计辅助 trait — 三个 ServiceImpl 共享 `audit_write` 默认实现。
///
/// 实现者只需提供 `audit_logger` 方法返回审计日志记录器引用，
/// 即可获得 `audit_write` 默认实现，避免在每个 Service 中重复编写审计日志构造代码。
#[allow(async_fn_in_trait)]
pub trait AuditHelper {
    /// 获取审计日志记录器。
    ///
    /// # Returns
    ///
    /// 返回审计日志记录器的 `Option` 引用；未注入审计记录器时返回 `None`。
    fn audit_logger(&self) -> Option<&Arc<dyn AuditLogger>>;

    /// 记录审计日志（尽力而为，失败不阻塞业务，通过 `warn!` 告警）。
    ///
    /// 当未注入审计记录器时直接返回；否则构造 `AuditRecord` 并异步写入。
    /// 写入失败仅记录 `warn` 日志，不影响业务流程。
    ///
    /// # Arguments
    ///
    /// * `svr_ctx` - 服务端上下文，用于填充操作者信息（`user_id` / `username`）。
    /// * `action` - 审计动作（如 `create_user` / `delete_role`）。
    /// * `target_type` - 目标对象类型（如 `user` / `role` / `permission`）。
    /// * `target_id` - 目标对象 ID（批量操作时传 `"batch"`）。
    /// * `detail` - 审计详情，需实现 `Serialize`。
    async fn audit_write<T: Serialize>(
        &self,
        svr_ctx: &SVRContext,
        action: &str,
        target_type: &str,
        target_id: &str,
        detail: &T,
    ) {
        if let Some(audit) = self.audit_logger() {
            let mut record = AuditRecord::new(AuditDomain::Iam, action, OperationResult::Success);
            // 填充操作者信息
            if let Some(ac) = &svr_ctx.auth_context {
                record = record.with_actor(&ac.user_id, &ac.username);
            } else {
                record = record.with_actor("unknown", "unknown");
            }
            record = record.with_target(target_type, target_id);
            record = record.with_details(serde_json::to_value(detail).unwrap_or_default());
            if let Err(e) = audit.log(record).await {
                tracing::warn!("审计日志写入失败: {e}");
            }
        }
    }
}
