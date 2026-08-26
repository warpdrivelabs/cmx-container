//! event_bus 订阅接线（W3）—— 把 [`TriggerDispatcher`] 挂到全局事件总线。
//!
//! `EventHandler` 是同步闭包（`Arc<dyn Fn(topic,payload)>`），而分发是异步的：沿用平台既有范式
//! （见 `cmx-service::ServiceLifecycleListener`）——闭包内 clone Arc + `tokio::spawn` 异步分发。
//!
//! 平台层在 WASM 运行时就绪后调用 [`subscribe_events`]，把关心的 topic 订阅上即可；无需改
//! event_bus 本身。

use std::sync::Arc;

use cmx_traits::event_bus::{EventHandler, GlobalEventBus};

use crate::dispatcher::TriggerDispatcher;

/// 把 dispatcher 订阅到给定 topic 列表。每个 topic 命中时，异步分发到其绑定的插件函数。
///
/// - `dispatcher`：持 FunctionInvoker + 绑定 store 的分发器（平台层构造）。
/// - `topics`：要订阅的事件主题（如 `["order.created", "user.registered"]`）。
/// - `tenant`：租户（多租户下按租户过滤绑定；`None`=默认）。
pub async fn subscribe_events(
    dispatcher: Arc<TriggerDispatcher>,
    topics: &[String],
    tenant: Option<String>,
) {
    let bus = GlobalEventBus::get();
    for topic in topics {
        let d = dispatcher.clone();
        let t = tenant.clone();
        let topic_owned = topic.clone();
        let handler: EventHandler = Arc::new(move |_topic, payload| {
            let d = d.clone();
            let t = t.clone();
            let topic_owned = topic_owned.clone();
            // 同步 handler → spawn 异步分发（不阻塞发布者）。
            tokio::spawn(async move {
                let outcomes = d.dispatch_event(&topic_owned, payload, t.as_deref()).await;
                for o in &outcomes {
                    if !o.ok {
                        tracing::warn!(
                            topic = %topic_owned, plugin = %o.plugin_id, function = %o.function_name,
                            error = o.error.as_deref().unwrap_or(""), "触发插件执行失败"
                        );
                    }
                }
            });
        });
        bus.subscribe(topic, handler).await;
        tracing::info!(topic = %topic, "已订阅事件 → 触发插件分发");
    }
}
