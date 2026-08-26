//! cmx-plugin-trigger —— 触发面泛化（W3）。
//!
//! 把"事件订阅 / 定时任务 / 业务钩子"三类新入口，经 `cmx-traits::FunctionInvoker` 汇入既有插件调用
//! 核。**依赖倒置**：本 crate 零 extism、零 cmx-runtime、零 cmx-job-core 依赖，故可被 event_bus 订阅
//! 器与 cmx-job 消费者安全引用而不破坏"core 不碰 extism"的边界。
//!
//! 运行时装配（谁提供 `FunctionInvoker` 实例、谁把 `TriggerDispatcher` 挂到 event_bus/cron）留给平台层
//! （cmx-platform-app），本 crate 只提供可单测的模型 + 分发逻辑。

pub mod binding;
pub mod dispatcher;
pub mod eventbus;
pub mod store;

pub use binding::{TriggerBinding, TriggerKind};
pub use dispatcher::{DispatchOutcome, TriggerDispatcher};
pub use eventbus::subscribe_events;
pub use store::TriggerBindingStore;

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use cmx_core::model::service::SVRContext;
    use cmx_traits::function_invoker::{FunctionInvokeResult, FunctionInvoker};
    use serde_json::{json, Value};
    use std::sync::{Arc, Mutex};

    /// mock invoker：记录每次调用，按 plugin_id 决定成功/失败。
    #[derive(Default)]
    struct MockInvoker {
        calls: Mutex<Vec<(String, String, Value)>>,
        fail_plugin: Option<String>,
    }

    #[async_trait]
    impl FunctionInvoker for MockInvoker {
        async fn invoke_plugin_function(
            &self,
            plugin_id: &str,
            function_name: &str,
            input: Value,
            _initial_input: Option<Value>,
            _svr_ctx: SVRContext,
            _debug: bool,
        ) -> Result<FunctionInvokeResult, cmx_traits::error::TraitError> {
            self.calls
                .lock()
                .unwrap()
                .push((plugin_id.to_string(), function_name.to_string(), input));
            let fail = self.fail_plugin.as_deref() == Some(plugin_id);
            Ok(FunctionInvokeResult {
                success: !fail,
                result: json!({"ok": !fail}),
                elapsed_us: 1,
                error: if fail { Some("boom".into()) } else { None },
                debug: None,
            })
        }
    }

    /// mock store：内存绑定列表。
    struct MockStore(Vec<TriggerBinding>);

    #[async_trait]
    impl TriggerBindingStore for MockStore {
        async fn bindings_for(
            &self,
            kind: TriggerKind,
            trigger_key: &str,
            _tenant: Option<&str>,
        ) -> Result<Vec<TriggerBinding>, String> {
            Ok(self
                .0
                .iter()
                .filter(|b| b.kind == kind && b.trigger_key == trigger_key)
                .cloned()
                .collect())
        }
        async fn list_by_kind(&self, kind: TriggerKind) -> Result<Vec<TriggerBinding>, String> {
            Ok(self.0.iter().filter(|b| b.kind == kind).cloned().collect())
        }
        async fn save(&self, _b: &TriggerBinding) -> Result<i64, String> {
            Ok(1)
        }
        async fn delete(&self, _id: i64) -> Result<u64, String> {
            Ok(1)
        }
    }

    fn dispatcher(bindings: Vec<TriggerBinding>, fail: Option<&str>) -> (TriggerDispatcher, Arc<MockInvoker>) {
        let inv = Arc::new(MockInvoker {
            calls: Mutex::new(Vec::new()),
            fail_plugin: fail.map(String::from),
        });
        let store = Arc::new(MockStore(bindings));
        (TriggerDispatcher::new(inv.clone(), store), inv)
    }

    #[tokio::test]
    async fn event_dispatches_to_bound_plugin() {
        let (d, inv) = dispatcher(vec![TriggerBinding::event("order.created", "p1", "on_order")], None);
        let out = d.dispatch_event("order.created", json!({"id": 7}), None).await;
        assert_eq!(out.len(), 1);
        assert!(out[0].ok);
        let calls = inv.calls.lock().unwrap();
        assert_eq!(calls[0].0, "p1");
        assert_eq!(calls[0].1, "on_order");
        // 触发载荷进 trigger 键。
        assert_eq!(calls[0].2["trigger"], json!({"id": 7}));
    }

    #[tokio::test]
    async fn unmatched_topic_no_dispatch() {
        let (d, inv) = dispatcher(vec![TriggerBinding::event("a", "p1", "f")], None);
        let out = d.dispatch_event("b", json!({}), None).await;
        assert!(out.is_empty());
        assert!(inv.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn disabled_binding_skipped() {
        let mut b = TriggerBinding::event("t", "p1", "f");
        b.enabled = false;
        let (d, inv) = dispatcher(vec![b], None);
        let out = d.dispatch_event("t", json!({}), None).await;
        assert!(out.is_empty());
        assert!(inv.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn multiple_bindings_all_fire() {
        let (d, _) = dispatcher(
            vec![
                TriggerBinding::event("t", "p1", "f1"),
                TriggerBinding::event("t", "p2", "f2"),
            ],
            None,
        );
        let out = d.dispatch_event("t", json!({}), None).await;
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|o| o.ok));
    }

    #[tokio::test]
    async fn failure_is_reported_not_panicked() {
        let (d, _) = dispatcher(vec![TriggerBinding::event("t", "bad", "f")], Some("bad"));
        let out = d.dispatch_event("t", json!({}), None).await;
        assert_eq!(out.len(), 1);
        assert!(!out[0].ok);
        assert_eq!(out[0].error.as_deref(), Some("boom"));
    }

    #[tokio::test]
    async fn cron_dispatch_with_static_payload() {
        let mut b = TriggerBinding::cron("0 0 * * *", "p1", "nightly");
        b.payload_json = Some(json!({"job": "cleanup"}));
        let (d, inv) = dispatcher(vec![b], None);
        let out = d.dispatch_cron("0 0 * * *", None).await;
        assert_eq!(out.len(), 1);
        let calls = inv.calls.lock().unwrap();
        // 静态载荷平铺 + 空触发载荷。
        assert_eq!(calls[0].2["job"], json!("cleanup"));
        assert_eq!(calls[0].2["trigger"], json!({}));
    }

    #[tokio::test]
    async fn biz_hook_dispatch() {
        let (d, inv) = dispatcher(
            vec![TriggerBinding {
                kind: TriggerKind::BizHook,
                trigger_key: "flow:serviceTask:approve".into(),
                ..TriggerBinding::event("x", "p1", "hook")
            }],
            None,
        );
        let out = d.dispatch_biz_hook("flow:serviceTask:approve", json!({"amount": 100}), None).await;
        assert_eq!(out.len(), 1);
        assert!(out[0].ok);
        assert_eq!(inv.calls.lock().unwrap()[0].2["trigger"]["amount"], json!(100));
    }
}
