//! W3 触发面装配（平台启动时）—— event_bus 订阅器 + cron 消费者。
//!
//! 构造 [`TriggerDispatcher`]（真 `FunctionInvoker` = cmx-biz `BizFunctionInvoker` + PG 绑定 store），
//! 然后：
//! - **事件**：从 DB 装载所有 event 绑定，`subscribe_events` 挂到 `GlobalEventBus`（真实生效）。
//! - **定时**：从 DB 装载 cron 绑定，起一个每分钟 tick 的后台任务，按 cron 表达式匹配触发。
//!
//! 依赖倒置守恒：dispatcher 经 `cmx-traits::FunctionInvoker` 调插件；本模块在平台层装配，**不把
//! extism 引入 cmx-job-core**（本模块属平台装配层，非 core）。

use std::sync::Arc;

use cmx_database::get_default_db_manager;
use cmx_plugin_trigger::{subscribe_events, TriggerBindingStore, TriggerDispatcher, TriggerKind};
use cmx_plugin_trigger_store_pg::PgTriggerBindingStore;

/// 装配触发面：event 订阅 + cron 调度。非致命——失败只 warn。
pub async fn init_triggers() -> crate::Result<()> {
    let db_id = get_default_db_manager().get_default_db_id().await;
    let store = Arc::new(PgTriggerBindingStore::new(db_id));
    if let Err(e) = store.ensure_schema().await {
        tracing::warn!(error = %e, "触发绑定表建失败，跳过触发面装配");
        return Ok(());
    }

    let invoker = crate::config::build_function_invoker();
    let dispatcher = Arc::new(TriggerDispatcher::new(invoker, store.clone()));

    // ① 事件订阅：装载所有 event 绑定的 topic，去重后订阅。
    match store.list_by_kind(TriggerKind::Event).await {
        Ok(bindings) => {
            let mut topics: Vec<String> = bindings.into_iter().map(|b| b.trigger_key).collect();
            topics.sort();
            topics.dedup();
            if !topics.is_empty() {
                subscribe_events(dispatcher.clone(), &topics, None).await;
                tracing::info!(count = topics.len(), "✅ 事件触发订阅已挂载");
            }
        }
        Err(e) => tracing::warn!(error = %e, "装载事件绑定失败"),
    }

    // ② 定时调度：起每分钟 tick 后台任务。
    spawn_cron_scheduler(dispatcher, store);
    tracing::info!("✅ 定时触发调度器已启动（每分钟 tick）");
    Ok(())
}

/// 起 cron 调度后台任务：每分钟从 DB 重载 cron 绑定，按表达式匹配当前时刻触发。
fn spawn_cron_scheduler(
    dispatcher: Arc<TriggerDispatcher>,
    store: Arc<PgTriggerBindingStore>,
) {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            ticker.tick().await;
            let bindings = match store.list_by_kind(TriggerKind::Cron).await {
                Ok(b) => b,
                Err(e) => {
                    tracing::warn!(error = %e, "cron 绑定装载失败");
                    continue;
                }
            };
            let now = chrono::Local::now();
            for b in bindings.into_iter().filter(|b| b.enabled) {
                if crate::config::cron::matches(&b.trigger_key, now) {
                    let d = dispatcher.clone();
                    let key = b.trigger_key.clone();
                    let tenant = b.tenant_id.clone();
                    tokio::spawn(async move {
                        let out = d.dispatch_cron(&key, tenant.as_deref()).await;
                        for o in &out {
                            if !o.ok {
                                tracing::warn!(cron = %key, plugin = %o.plugin_id,
                                    error = o.error.as_deref().unwrap_or(""), "定时触发插件失败");
                            }
                        }
                    });
                }
            }
        }
    });
}
