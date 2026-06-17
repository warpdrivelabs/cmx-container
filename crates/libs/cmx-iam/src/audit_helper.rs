//! 审计辅助 trait — 消除三个 ServiceImpl 中的重复代码

use std::sync::Arc;

use cmx_audit::{AuditDomain, AuditLogger, AuditRecord, OperationResult};
use cmx_core::SVRContext;
use serde::Serialize;

/// 审计辅助 trait — 三个 ServiceImpl 共享 audit_write 默认实现
#[allow(async_fn_in_trait)]
pub trait AuditHelper {
    /// 获取审计日志记录器
    fn audit_logger(&self) -> Option<&Arc<dyn AuditLogger>>;

    /// 记录审计日志（尽力而为，失败不阻塞业务，通过 warn! 告警）
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
