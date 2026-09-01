//! # cmx-mdm-sdk —— MDM 主数据服务（cmx-mdm）服务间调用契约
//!
//! 首批契约：**flow → mdm 的生命周期 webhook 回调**。
//!
//! - 路径：[`paths::FLOW_CALLBACK`]（`POST /api/mdm/flow/callback`）；
//! - 鉴权：HMAC-SHA256 请求签名（[`SIGNATURE_HEADER`] = `x-cmx-flow-signature`，
//!   值形如 `sha256=<hex>`，对 **body 原始字节** 计算）——密钥（secret）由 flow 侧与
//!   mdm 侧部署配置共享，**不进 SDK**；mdm 侧未配密钥时拒收；
//! - 载荷：[`FlowEvent`]（camelCase wire DTO，与 flow 引擎的出站事件逐字段对齐）；
//! - 辅助头：[`EVENT_HEADER`]（事件名）/ [`DELIVERY_HEADER`]（投递幂等 id）。
//!
//! 双端用法：
//! - flow（发送方）：`WebhookSender` 重试循环里调 [`deliver_flow_event`]（或注入句柄的
//!   [`MdmFlowCallbackClient`]），body 先序列化后签名，签名与实际发送字节严格一致；
//! - mdm（接收方）：`flow_cb` handler 用 [`FlowEvent`] 反序列化载荷 +
//!   [`verify_signature`] 验签——两端同一份契约，字段漂移编译期 / 测试期暴露。
//!
//! mdm 仅处理 `instance.completed` / `instance.terminated`（载荷 `state=TERMINATED`），
//! 其余事件收下即忽略；处理失败仅记日志，靠发送方重投递 + mdm 懒同步兜底。

use std::sync::Arc;

use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use cmx_service_rpc::{ServiceRpcError, ServiceRpcHandle};

/// mdm 服务在服务目录中的键（`[service_rpc.services].mdm`）。
pub const SERVICE_KEY: &str = "mdm";

/// 路径常量。
pub mod paths {
    /// flow → mdm 生命周期 webhook 回调端点。
    pub const FLOW_CALLBACK: &str = "/api/mdm/flow/callback";
}

/// HMAC 签名头（值形如 `sha256=<hex(HMAC-SHA256(body, secret))>`）。
pub const SIGNATURE_HEADER: &str = "x-cmx-flow-signature";

/// 事件名头（载荷 `event` 字段的冗余副本，便于接收方路由 / 过滤）。
pub const EVENT_HEADER: &str = "x-cmx-flow-event";

/// 投递幂等头（`{instanceId}-{occurredAt}`，接收方可据此去重）。
pub const DELIVERY_HEADER: &str = "x-cmx-flow-delivery";

/// 生命周期事件（wire DTO，camelCase）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowEvent {
    /// 事件名（instance.started / instance.completed / instance.terminated /
    /// task.created / task.completed / task.reassigned）。
    pub event: String,
    /// 实例 id。
    pub instance_id: String,
    /// 实例状态（ACTIVE / COMPLETED / TERMINATED / …，可空）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    /// 流程定义 key（可空；接收方按此过滤本模块流程）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub definition_key: Option<String>,
    /// 业务键（可空）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub business_key: Option<String>,
    /// 任务 id（task.* 事件带）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    /// 节点 bpmn id（task.* 事件带）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_bpmn_id: Option<String>,
    /// 办理人（task.created / task.reassigned 带）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignee: Option<String>,
    /// 租户。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tenant: Option<String>,
    /// 事件时间（RFC3339）。
    pub occurred_at: String,
}

impl FlowEvent {
    /// 投递幂等 id（`{instanceId}-{occurredAt}`）。
    pub fn delivery_id(&self) -> String {
        format!("{}-{}", self.instance_id, self.occurred_at)
    }
}

/// HMAC-SHA256(body, secret) → hex（小写）。
pub fn sign_payload(secret: &str, body: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
        .expect("HMAC 接受任意长度密钥");
    mac.update(body);
    hex::encode(mac.finalize().into_bytes())
}

/// 签名头值（`sha256=<hex>`）。
pub fn signature_header_value(secret: &str, body: &[u8]) -> String {
    format!("sha256={}", sign_payload(secret, body))
}

/// 验证签名头：常量时间比较，大小写不敏感；密钥为空 / 头缺失 / 前缀不符均拒绝。
pub fn verify_signature(secret: &str, body: &[u8], sig_header: Option<&str>) -> bool {
    if secret.is_empty() {
        // 未配置密钥：拒绝接收（签名即凭证，无密钥等于裸奔）。
        return false;
    }
    let Some(raw) = sig_header else { return false };
    let Some(hex_sig) = raw.trim().strip_prefix("sha256=") else {
        return false;
    };
    // verify_slice 比较的是二进制 MAC（常量时间），先做 hex 解码。
    let Ok(expected) = hex::decode(hex_sig.trim()) else {
        return false;
    };
    let mut mac = match Hmac::<Sha256>::new_from_slice(secret.as_bytes()) {
        Ok(m) => m,
        Err(_) => return false,
    };
    mac.update(body);
    mac.verify_slice(&expected).is_ok()
}

/// webhook 回调客户端（flow 侧发送用；句柄注入便于测试）。
pub struct MdmFlowCallbackClient {
    rpc: Arc<ServiceRpcHandle>,
}

impl MdmFlowCallbackClient {
    /// 从基座句柄构造。
    pub fn new(rpc: Arc<ServiceRpcHandle>) -> Self {
        Self { rpc }
    }

    /// 投递一条事件：序列化 → 签名（对实际发送字节）→ POST 回调端点 → 解标准信封。
    ///
    /// 单次投递（不内置重试）——重试 / 退避策略归发送方（flow 的 WebhookSender）。
    pub async fn deliver(&self, event: &FlowEvent, secret: &str) -> Result<(), ServiceRpcError> {
        // body 用紧凑 JSON；签名对 body 字节，必须与发出的字节一致（Raw body 保证）。
        let body = serde_json::to_vec(event).map_err(|e| ServiceRpcError::Decode {
            key: SERVICE_KEY.to_string(),
            cause: format!("事件序列化失败: {e}"),
        })?;
        let signature = signature_header_value(secret, &body);
        let req = cmx_service_rpc::RpcRequest::post(SERVICE_KEY, paths::FLOW_CALLBACK)
            .raw_body(body, "application/json")
            .header(EVENT_HEADER, &event.event)
            .header(DELIVERY_HEADER, event.delivery_id())
            .header(SIGNATURE_HEADER, signature);
        self.rpc.call_api_unit(req).await
    }
}

/// 经全局基座投递一条事件（便捷入口；基座未初始化返回 `Unavailable`）。
pub async fn deliver_flow_event(
    event: &FlowEvent,
    secret: &str,
) -> Result<(), ServiceRpcError> {
    let handle = cmx_service_rpc::global_arc().ok_or_else(|| ServiceRpcError::Unavailable {
        key: SERVICE_KEY.to_string(),
        cause: "service_rpc 基座未初始化（需先执行 init_infra）".to_string(),
    })?;
    MdmFlowCallbackClient::new(handle).deliver(event, secret).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use cmx_service_rpc::{
        Body, OutgoingHeaders, RpcResponse, ServiceEntry, ServiceRpcConfig, Transport,
    };
    use async_trait::async_trait;
    use serde_json::json;
    use std::time::Duration;

    /// 签名 / 验签往返：正确通过、篡改 body 拒绝、错误密钥拒绝、空密钥拒绝、
    /// 头缺失 / 前缀错误拒绝、大小写不敏感。
    #[test]
    fn signature_roundtrip_rules() {
        let body = br#"{"event":"instance.completed","instanceId":"i-1"}"#;
        let sig = signature_header_value("secret", body);
        assert!(sig.starts_with("sha256="));
        assert_eq!(sig.len(), "sha256=".len() + 64);
        assert!(verify_signature("secret", body, Some(&sig)));
        // hex 部分大小写不敏感（前缀仍为小写 sha256=）。
        let upper_hex = format!("sha256={}", sig.trim_start_matches("sha256=").to_uppercase());
        assert!(verify_signature("secret", body, Some(&upper_hex)));
        // 篡改 body / 错误密钥 / 头缺失 / 前缀错误 / 空密钥。
        assert!(!verify_signature("secret", b"other", Some(&sig)));
        assert!(!verify_signature("other", body, Some(&sig)));
        assert!(!verify_signature("secret", body, None));
        assert!(!verify_signature("secret", body, Some("md5=abc")));
        assert!(!verify_signature("", body, Some(&sig)));
        // 同一 body 签名稳定。
        assert_eq!(sig, signature_header_value("secret", body));
    }

    /// wire DTO 形状：camelCase + skip_none + delivery_id。
    #[test]
    fn event_wire_shape() {
        let ev = FlowEvent {
            event: "instance.terminated".to_string(),
            instance_id: "i-1".to_string(),
            state: Some("TERMINATED".to_string()),
            definition_key: Some("mdm_cr_approval".to_string()),
            business_key: None,
            task_id: None,
            node_bpmn_id: None,
            assignee: None,
            tenant: None,
            occurred_at: "2026-08-31T10:00:00Z".to_string(),
        };
        let v = serde_json::to_value(&ev).unwrap();
        assert_eq!(v["instanceId"], "i-1");
        assert_eq!(v["definitionKey"], "mdm_cr_approval");
        assert_eq!(v["occurredAt"], "2026-08-31T10:00:00Z");
        assert!(v.get("businessKey").is_none(), "None 字段不上线");
        // 反序列化对称（接收端契约）。
        let back: FlowEvent = serde_json::from_value(v).unwrap();
        assert_eq!(back.event, ev.event);
        assert_eq!(back.delivery_id(), "i-1-2026-08-31T10:00:00Z");
    }

    /// 投递契约：路径 / 三头齐全 / 签名对实际发送字节可被接收端验回。
    #[tokio::test]
    async fn deliver_contract() {
        let seen: Arc<std::sync::Mutex<Vec<(String, cmx_service_rpc::RpcRequest, OutgoingHeaders)>>> =
            Arc::default();
        let mut cfg = ServiceRpcConfig::default();
        cfg.services.insert(
            SERVICE_KEY.to_string(),
            ServiceEntry {
                url: Some("http://10.0.0.5:8095".to_string()),
                ..Default::default()
            },
        );
        let handle = Arc::new(ServiceRpcHandle::with_transport(
            cfg,
            Arc::new(SharedMock { seen: seen.clone() }),
        ));

        let ev = FlowEvent {
            event: "instance.completed".to_string(),
            instance_id: "i-9".to_string(),
            state: Some("COMPLETED".to_string()),
            definition_key: None,
            business_key: None,
            task_id: None,
            node_bpmn_id: None,
            assignee: None,
            tenant: None,
            occurred_at: "2026-08-31T12:00:00Z".to_string(),
        };
        MdmFlowCallbackClient::new(handle)
            .deliver(&ev, "mdm_flow_hook_dev_secret")
            .await
            .expect("应成功");

        let calls = seen.lock().unwrap();
        let (base, req, headers) = &calls[0];
        assert_eq!(base, "http://10.0.0.5:8095");
        assert_eq!(req.path, paths::FLOW_CALLBACK);
        let Body::Raw { bytes, content_type } = &req.body else {
            panic!("应为 Raw body");
        };
        assert_eq!(content_type, "application/json");
        // 发送字节可被接收端反序列化回事件（wire 对齐）。
        let parsed: FlowEvent = serde_json::from_slice(bytes).unwrap();
        assert_eq!(parsed.instance_id, "i-9");
        // 签名头对实际字节有效（接收端视角验回）。
        let sig = headers
            .extra
            .iter()
            .find(|(k, _)| k == SIGNATURE_HEADER)
            .map(|(_, v)| v.clone())
            .expect("应带签名头");
        assert!(verify_signature("mdm_flow_hook_dev_secret", bytes, Some(&sig)));
        // 事件名 / 幂等头齐全。
        assert!(headers.extra.iter().any(|(k, v)| k == EVENT_HEADER && v == "instance.completed"));
        assert!(headers.extra.iter().any(|(k, v)| k == DELIVERY_HEADER && v.contains("i-9")));
    }

    struct SharedMock {
        seen: Arc<std::sync::Mutex<Vec<(String, cmx_service_rpc::RpcRequest, OutgoingHeaders)>>>,
    }

    #[async_trait]
    impl Transport for SharedMock {
        async fn execute(
            &self,
            base: &str,
            req: &cmx_service_rpc::RpcRequest,
            _timeout: Duration,
            headers: &OutgoingHeaders,
        ) -> Result<RpcResponse, ServiceRpcError> {
            self.seen
                .lock()
                .unwrap()
                .push((base.to_string(), req.clone(), headers.clone()));
            Ok(RpcResponse {
                status: 200,
                body: json!({ "code": 0, "msg": "ok", "data": { "received": true } }),
            })
        }
    }
}
