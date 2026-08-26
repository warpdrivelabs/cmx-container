//! 触发分发器（W3 核心）—— 把一次触发解析成"命中的绑定集"，逐条经 [`FunctionInvoker`] 调插件。
//!
//! **依赖倒置铁律**：本 crate 只经 `cmx-traits::FunctionInvoker` 调用插件，**不依赖 extism、不依赖
//! cmx-runtime、不依赖 cmx-job-core**。事件订阅器 / cron 消费者 / 业务钩子只需持一个
//! `TriggerDispatcher`，把触发键喂进来即可——运行时装配（谁提供 FunctionInvoker 实例）留给平台层。

use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use cmx_core::model::service::SVRContext;
use cmx_traits::function_invoker::{FunctionInvokeResult, FunctionInvoker};
use serde_json::{json, Value};

use crate::binding::{TriggerBinding, TriggerKind};
use crate::store::TriggerBindingStore;

/// 一次触发的分发结果（每个命中绑定一条）。
#[derive(Debug, Clone)]
pub struct DispatchOutcome {
    pub plugin_id: String,
    pub function_name: String,
    /// 调用是否成功（基础设施成功 + WASM 执行成功）。
    pub ok: bool,
    pub error: Option<String>,
}

/// 触发分发器。
pub struct TriggerDispatcher {
    invoker: Arc<dyn FunctionInvoker>,
    store: Arc<dyn TriggerBindingStore>,
}

impl TriggerDispatcher {
    pub fn new(invoker: Arc<dyn FunctionInvoker>, store: Arc<dyn TriggerBindingStore>) -> Self {
        Self { invoker, store }
    }

    /// 分发一次**事件**：topic + 载荷 → 命中绑定逐条调用。载荷进 `FunctionInput.input`。
    pub async fn dispatch_event(
        &self,
        topic: &str,
        payload: Value,
        tenant: Option<&str>,
    ) -> Vec<DispatchOutcome> {
        self.dispatch(TriggerKind::Event, topic, payload, tenant).await
    }

    /// 分发一次**定时**：cron 表达式命中 → 调用（载荷取绑定的 `payload_json`，无则空对象）。
    pub async fn dispatch_cron(&self, cron_expr: &str, tenant: Option<&str>) -> Vec<DispatchOutcome> {
        self.dispatch(TriggerKind::Cron, cron_expr, json!({}), tenant).await
    }

    /// 分发一次**业务钩子**：hook 键 + 上下文载荷 → 调用。
    pub async fn dispatch_biz_hook(
        &self,
        hook_key: &str,
        payload: Value,
        tenant: Option<&str>,
    ) -> Vec<DispatchOutcome> {
        self.dispatch(TriggerKind::BizHook, hook_key, payload, tenant).await
    }

    async fn dispatch(
        &self,
        kind: TriggerKind,
        trigger_key: &str,
        payload: Value,
        tenant: Option<&str>,
    ) -> Vec<DispatchOutcome> {
        let bindings = match self.store.bindings_for(kind, trigger_key, tenant).await {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!(kind = ?kind, key = trigger_key, error = %e, "触发绑定查询失败");
                return Vec::new();
            }
        };
        let mut out = Vec::with_capacity(bindings.len());
        for b in bindings.into_iter().filter(|b| b.enabled) {
            out.push(self.invoke_one(&b, &payload).await);
        }
        out
    }

    /// 调用单条绑定（合并绑定静态载荷 + 触发载荷）。
    async fn invoke_one(&self, b: &TriggerBinding, payload: &Value) -> DispatchOutcome {
        let input = merge_payload(b.payload_json.as_ref(), payload);
        let svr = SVRContext::new(input.clone(), HashMap::new(), Utc::now(), request_id());
        let res = self
            .invoker
            .invoke_plugin_function(&b.plugin_id, &b.function_name, input, None, svr, false)
            .await;
        match res {
            Ok(FunctionInvokeResult { success, error, .. }) => DispatchOutcome {
                plugin_id: b.plugin_id.clone(),
                function_name: b.function_name.clone(),
                ok: success,
                error,
            },
            Err(e) => DispatchOutcome {
                plugin_id: b.plugin_id.clone(),
                function_name: b.function_name.clone(),
                ok: false,
                error: Some(e.to_string()),
            },
        }
    }
}

/// 合并绑定静态载荷与触发载荷：触发载荷放 `event`/`trigger` 键，静态载荷平铺（静态优先级低）。
fn merge_payload(static_payload: Option<&Value>, trigger_payload: &Value) -> Value {
    let mut obj = serde_json::Map::new();
    if let Some(Value::Object(m)) = static_payload {
        for (k, v) in m {
            obj.insert(k.clone(), v.clone());
        }
    }
    obj.insert("trigger".to_string(), trigger_payload.clone());
    Value::Object(obj)
}

/// 生成一个请求 id（触发无 HTTP request-id，用时间戳派生；不引 uuid 依赖）。
fn request_id() -> String {
    format!("trg-{}", Utc::now().timestamp_micros())
}
