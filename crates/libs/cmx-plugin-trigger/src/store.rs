//! 绑定存储契约（W3）—— 驱动无关。平台侧用 PG store 实现（落 `cmx_plugin_*_binding` 表）。

use crate::binding::{TriggerBinding, TriggerKind};
use async_trait::async_trait;

/// 触发绑定存储。查询按 (kind, trigger_key, tenant) 命中启用中的绑定。
#[async_trait]
pub trait TriggerBindingStore: Send + Sync {
    /// 取命中某触发键的**启用中**绑定（按租户；`tenant=None` 表默认租户）。
    async fn bindings_for(
        &self,
        kind: TriggerKind,
        trigger_key: &str,
        tenant: Option<&str>,
    ) -> Result<Vec<TriggerBinding>, String>;

    /// 列出全部某类绑定（cron 调度器启动时装载所有 cron 绑定用）。
    async fn list_by_kind(&self, kind: TriggerKind) -> Result<Vec<TriggerBinding>, String>;

    /// upsert 一条绑定，返回 id。
    async fn save(&self, binding: &TriggerBinding) -> Result<i64, String>;

    /// 删除一条绑定。
    async fn delete(&self, id: i64) -> Result<u64, String>;
}
