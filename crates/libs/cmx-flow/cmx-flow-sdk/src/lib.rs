//! # cmx-flow-sdk —— 流程引擎（cmx-flowengine）服务间调用契约
//!
//! 契约内容（全部自包含，不引用 flow 仓内部类型）：
//! - [`SERVICE_KEY`]：目录键（`[service_rpc.services].flow`）；
//! - [`paths`]：v1 路径常量 / 拼接函数（客户端拼 URL 与服务端挂路由同源）；
//! - wire DTO（[`StartInstanceReq`] 等，只依赖 serde）；
//! - [`FlowClient`] trait + HTTP 默认实现（基于 `cmx-service-rpc` 基座：
//!   定位 / 鉴权链 / 超时 / 重试 / 熔断全部由基座承担）。
//!
//! 方法集 = mdm 11 个方法 + report `start_close_instance`（complete_apply/complete_review
//! 同端点合并为 [`FlowClient::complete_task`]）。路径以 flow v1 openapi
//! （`/api/flow/v1/openapi.json`）核对定稿；响应信封为标准 `ApiResp`（code==0 成功）。
//!
//! ## 响应类型的务实取舍
//!
//! 请求 DTO 严格类型化（契约的受控面）；响应对稳定字段类型化（id / state / variables），
//! 任务明细等消费方各取所需的嵌套集合以 `#[serde(flatten)] extra` 保留原始 JSON——
//! 既给编译期保护，又不为 flow 内部演进的嵌套形状做虚假契约。
//!
//! ## 用法
//!
//! ```no_run
//! # async fn demo() -> Result<(), cmx_service_rpc::ServiceRpcError> {
//! use cmx_flow_sdk::{self, StartInstanceReq};
//!
//! let flow = cmx_flow_sdk::client()?;
//! let inst = flow
//!     .start_instance(
//!         StartInstanceReq {
//!             definition_key: "mdm_cr_approval".into(),
//!             business_key: Some("CR-2026-001".into()),
//!             variables: Some(serde_json::json!({"initiator": "u1"})),
//!             ..Default::default()
//!         },
//!         None, // 委托令牌：None = 取当前请求 task-local 用户 JWT
//!     )
//!     .await?;
//! # let _ = inst;
//! # Ok(())
//! # }
//! ```

use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use cmx_service_rpc::{ServiceRpcError, ServiceRpcHandle};

/// 流程服务在服务目录中的键（`[service_rpc.services].flow`）。
pub const SERVICE_KEY: &str = "flow";

/// v1 API 路径常量（R3 · 技术债 016 收敛：全 POST + 固定路径，资源标识进 JSON body）。
pub mod paths {
    /// 起实例：`POST /api/flow/v1/instances/start`。
    pub const INSTANCES_START: &str = "/api/flow/v1/instances/start";

    /// 实例详情：`POST /api/flow/v1/instances/detail`（body: {id}）。
    pub const INSTANCE_DETAIL: &str = "/api/flow/v1/instances/detail";

    /// 实例变量：`POST /api/flow/v1/instances/variables`（body: {id}）。
    pub const INSTANCE_VARIABLES: &str = "/api/flow/v1/instances/variables";

    /// 实例审批意见：`POST /api/flow/v1/instances/comments`（body: {id}）。
    pub const INSTANCE_COMMENTS: &str = "/api/flow/v1/instances/comments";

    /// 实例绑定的单据坐标：`POST /api/flow/v1/instances/biz`（body: {id}）。
    pub const INSTANCE_BIZ: &str = "/api/flow/v1/instances/biz";

    /// 取消实例：`POST /api/flow/v1/instances/cancel`（body: {id, reason?}）。
    pub const INSTANCE_CANCEL: &str = "/api/flow/v1/instances/cancel";

    /// 单据 → 实例列表（倒序，第一条 = 当前实例）：`POST /api/flow/v1/biz/instances`
    /// （body: {bizTable, bizId}）。
    pub const BIZ_INSTANCES: &str = "/api/flow/v1/biz/instances";

    /// 办结任务：`POST /api/flow/v1/tasks/complete`（body 含 taskId）。
    pub const TASK_COMPLETE: &str = "/api/flow/v1/tasks/complete";

    /// 退回任务：`POST /api/flow/v1/tasks/reject`（body 含 taskId）。
    pub const TASK_REJECT: &str = "/api/flow/v1/tasks/reject";

    /// 我的任务（body: {assignee, kind=todo|claimable|all, ...}）：`POST /api/flow/v1/tasks/my`。
    pub const TASKS_MY: &str = "/api/flow/v1/tasks/my";
}

/// 单据绑定坐标（start 实例时绑业务表位置，供反查与待办投影）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BizLink {
    /// 业务表名（如 `cv_mdm_apply` / `cg_close_run`）。
    pub biz_table: String,
    /// 业务行 id（字符串形态）。
    pub biz_id: String,
    /// 业务键（可选，如单号）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub biz_key: Option<String>,
    /// 绑定角色（如 `approval` / `close_run`）。
    pub role: String,
}

/// 起实例请求（`POST /api/flow/v1/instances/start`）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartInstanceReq {
    /// 流程定义 key（必填，trim 后非空，缺失 HTTP 400）。
    pub definition_key: String,
    /// 业务键（单号等）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub business_key: Option<String>,
    /// 组织维度。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub org_id: Option<String>,
    /// 版本维度（dim_key → dim_value，子流程路由用）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dimensions: Option<std::collections::HashMap<String, String>>,
    /// 实例变量（任意对象；`initiator` 缺失时引擎按认证用户兜底注入）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variables: Option<Value>,
    /// 单据绑定。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub biz_link: Option<BizLink>,
}

/// 办结任务请求（`POST /api/flow/v1/tasks/complete`，taskId 由 SDK 并入 body）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompleteTaskReq {
    /// 实例 id（必填）。
    pub instance_id: String,
    /// 合并进实例的变量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variables: Option<Value>,
    /// 审批决定（并入 `variables.lastDecision`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision: Option<String>,
    /// 审批意见（落意见表 + 变量）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
    /// 办理人留痕（空则引擎按认证用户兜底）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operator: Option<String>,
}

/// 退回任务请求（`POST /api/flow/v1/tasks/{taskId}/reject`）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RejectTaskReq {
    /// 实例 id（必填）。
    pub instance_id: String,
    /// 退回发起人留痕。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_user: Option<String>,
    /// 目标节点（空 = 直接前驱用户任务）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_bpmn_id: Option<String>,
    /// 退回原因。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// 合并进实例的变量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variables: Option<Value>,
}

/// 取消实例请求（`POST /api/flow/v1/instances/{id}/cancel`）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelReq {
    /// 取消原因。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// 实例视图（稳定字段类型化，嵌套集合经 `extra` 保留原始 JSON）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceView {
    /// 实例 id（兼容 string / number 两种线格式）。
    #[serde(deserialize_with = "flex_string")]
    pub id: String,
    /// 定义 key。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub definition_key: Option<String>,
    /// 业务键。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub business_key: Option<String>,
    /// 实例状态（ACTIVE / COMPLETED / TERMINATED / …）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    /// 实例变量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variables: Option<Value>,
    /// 其余字段原样保留（tasks / tokens / activeNodes / incidents / …）。
    #[serde(flatten, default, skip_serializing_if = "Option::is_none")]
    pub extra: Option<Value>,
}

/// 单据 → 实例列表的条目（`{instances:[…]}`）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BizInstanceSummary {
    /// 实例 id（兼容 string / number）。
    #[serde(deserialize_with = "flex_string")]
    pub instance_id: String,
    /// 实例状态。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    /// 业务键。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub business_key: Option<String>,
    /// 其余字段原样保留。
    #[serde(flatten, default, skip_serializing_if = "Option::is_none")]
    pub extra: Option<Value>,
}

/// `{instances:[…]}` 包裹。
#[derive(Debug, Clone, Deserialize)]
pub struct InstancesPage {
    /// 实例列表（倒序，第一条 = 当前实例）。
    pub instances: Vec<BizInstanceSummary>,
}

/// `{comments:[…]}` 包裹（意见明细字段消费方各异，保留原始 JSON）。
#[derive(Debug, Clone, Deserialize)]
pub struct CommentsPage {
    /// 意见列表。
    pub comments: Vec<Value>,
}

/// `{links:[…]}` 包裹（单据绑定明细）。
#[derive(Debug, Clone, Deserialize)]
pub struct BizLinksPage {
    /// 绑定列表。
    pub links: Vec<Value>,
}

/// 我的任务条目（仅类型化消费方实际读取的字段）。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskSummary {
    /// 任务 id（兼容 string / number）。
    #[serde(deserialize_with = "flex_string")]
    pub task_id: String,
    /// 实例 id（兼容 string / number；列表投影可能缺省）。
    #[serde(default, deserialize_with = "flex_string_opt")]
    pub instance_id: Option<String>,
    /// 节点 bpmn id。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_bpmn_id: Option<String>,
    /// 节点名。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_name: Option<String>,
    /// 业务表（投影自实例变量）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub biz_table: Option<String>,
    /// 业务行 id（投影自实例变量）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub biz_id: Option<String>,
    /// 其余字段原样保留。
    #[serde(flatten, default, skip_serializing_if = "Option::is_none")]
    pub extra: Option<Value>,
}

/// `{tasks:[…], total, page, pageSize}` 包裹。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MyTasksPage {
    /// 任务列表。
    pub tasks: Vec<TaskSummary>,
    /// 总数。
    #[serde(default)]
    pub total: i64,
    /// 页码。
    #[serde(default)]
    pub page: i64,
    /// 页大小。
    #[serde(default)]
    pub page_size: i64,
}

/// [`flex_string`] 的可选项版（缺省 / null → `None`）。
fn flex_string_opt<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    let v = Option::<Value>::deserialize(d)?;
    match v {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s)),
        Some(Value::Number(n)) => Ok(Some(n.to_string())),
        Some(other) => Err(serde::de::Error::custom(format!(
            "期望 string/number/null，实际 {other}"
        ))),
    }
}

/// 兼容 string / number 两种线格式反序列化为 `String`。
fn flex_string<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let v = Value::deserialize(d)?;
    match v {
        Value::String(s) => Ok(s),
        Value::Number(n) => Ok(n.to_string()),
        other => Err(serde::de::Error::custom(format!(
            "期望 string/number，实际 {other}"
        ))),
    }
}

/// 流程服务客户端契约（服务方 = cmx-flowengine）。
///
/// `user_token`：显式委托令牌（OBO）——无请求上下文的后台链路传入；
/// `None` 时基座自动取当前请求 task-local 的原始用户 JWT。
#[async_trait]
pub trait FlowClient: Send + Sync {
    /// 起实例（返回实例视图，apply 任务从 `extra.tasks` 取）。
    async fn start_instance(
        &self,
        req: StartInstanceReq,
        user_token: Option<&str>,
    ) -> Result<InstanceView, ServiceRpcError>;

    /// 办结任务（complete_apply / complete_review 同端点；任务不可办返回
    /// `Remote{code:1}`，消费方按需做幂等归类）。
    async fn complete_task(
        &self,
        task_id: &str,
        req: CompleteTaskReq,
        user_token: Option<&str>,
    ) -> Result<InstanceView, ServiceRpcError>;

    /// 退回任务（打回上节点重办，实例仍 ACTIVE）。
    async fn reject_task(
        &self,
        task_id: &str,
        req: RejectTaskReq,
        user_token: Option<&str>,
    ) -> Result<InstanceView, ServiceRpcError>;

    /// 取消实例（撤回申请 = 终止本轮审批）。
    async fn cancel_instance(
        &self,
        instance_id: &str,
        req: CancelReq,
        user_token: Option<&str>,
    ) -> Result<InstanceView, ServiceRpcError>;

    /// 实例详情（state / tasks / openTasks）。
    async fn instance_detail(&self, instance_id: &str) -> Result<InstanceView, ServiceRpcError>;

    /// 实例变量（含 `lastDecision`）。
    async fn instance_variables(&self, instance_id: &str) -> Result<Value, ServiceRpcError>;

    /// 实例审批意见。
    async fn instance_comments(&self, instance_id: &str) -> Result<Vec<Value>, ServiceRpcError>;

    /// 实例绑定的单据坐标。
    async fn biz_of_instance(&self, instance_id: &str) -> Result<Vec<Value>, ServiceRpcError>;

    /// 单据 → 实例列表（倒序，第一条 = 当前实例）。
    async fn biz_instances(
        &self,
        biz_table: &str,
        biz_id: &str,
    ) -> Result<Vec<BizInstanceSummary>, ServiceRpcError>;

    /// 当前用户可认领的任务 id 列表（`POST /tasks/my`，body kind=claimable）。
    async fn my_claimable_tasks(&self, user: &str) -> Result<Vec<String>, ServiceRpcError>;
}

/// flow 的 HTTP 实现（基于服务间统一调用基座）。
pub struct HttpFlowClient {
    rpc: Arc<ServiceRpcHandle>,
    token: Option<String>,
}

impl HttpFlowClient {
    /// 从指定基座句柄构造（测试注入用）。
    pub fn from_handle(rpc: Arc<ServiceRpcHandle>) -> Self {
        Self { rpc, token: None }
    }

    /// 携带显式委托令牌（后台链路无请求上下文时用）。
    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.token = Some(token.into());
        self
    }

    fn base_req(
        &self,
        method: cmx_service_rpc::HttpMethod,
        path: String,
    ) -> cmx_service_rpc::RpcRequest {
        let mut req = cmx_service_rpc::RpcRequest {
            key: SERVICE_KEY.to_string(),
            method,
            path,
            query: Vec::new(),
            body: cmx_service_rpc::Body::None,
            idempotent: false,
            delegated_token: None,
            extra_headers: Vec::new(),
            timeout: None,
        };
        if let Some(t) = &self.token {
            req = req.with_token(t.clone());
        }
        req
    }
}

#[async_trait]
impl FlowClient for HttpFlowClient {
    async fn start_instance(
        &self,
        req: StartInstanceReq,
        user_token: Option<&str>,
    ) -> Result<InstanceView, ServiceRpcError> {
        let r = self
            .base_req(cmx_service_rpc::HttpMethod::Post, paths::INSTANCES_START.to_string())
            .json_body(serde_json::to_value(req).map_err(decode_err)?);
        let r = apply_token(r, user_token);
        self.rpc.call_api(r).await
    }

    async fn complete_task(
        &self,
        task_id: &str,
        req: CompleteTaskReq,
        user_token: Option<&str>,
    ) -> Result<InstanceView, ServiceRpcError> {
        // R3：taskId 进 body（路径段收编）。
        let mut body = serde_json::to_value(req).map_err(decode_err)?;
        body["taskId"] = serde_json::json!(task_id);
        let r = self
            .base_req(cmx_service_rpc::HttpMethod::Post, paths::TASK_COMPLETE.to_string())
            .json_body(body);
        let r = apply_token(r, user_token);
        self.rpc.call_api(r).await
    }

    async fn reject_task(
        &self,
        task_id: &str,
        req: RejectTaskReq,
        user_token: Option<&str>,
    ) -> Result<InstanceView, ServiceRpcError> {
        // R3：taskId 进 body（路径段收编）。
        let mut body = serde_json::to_value(req).map_err(decode_err)?;
        body["taskId"] = serde_json::json!(task_id);
        let r = self
            .base_req(cmx_service_rpc::HttpMethod::Post, paths::TASK_REJECT.to_string())
            .json_body(body);
        let r = apply_token(r, user_token);
        self.rpc.call_api(r).await
    }

    async fn cancel_instance(
        &self,
        instance_id: &str,
        req: CancelReq,
        user_token: Option<&str>,
    ) -> Result<InstanceView, ServiceRpcError> {
        // R3：id 进 body（路径段收编）。
        let mut body = serde_json::to_value(req).map_err(decode_err)?;
        body["id"] = serde_json::json!(instance_id);
        let r = self
            .base_req(cmx_service_rpc::HttpMethod::Post, paths::INSTANCE_CANCEL.to_string())
            .json_body(body);
        let r = apply_token(r, user_token);
        self.rpc.call_api(r).await
    }

    async fn instance_detail(&self, instance_id: &str) -> Result<InstanceView, ServiceRpcError> {
        self.rpc.call_api(
            self.base_req(cmx_service_rpc::HttpMethod::Post, paths::INSTANCE_DETAIL.to_string())
                .json_body(serde_json::json!({ "id": instance_id }))
                .idempotent(),
        )
        .await
    }

    async fn instance_variables(&self, instance_id: &str) -> Result<Value, ServiceRpcError> {
        self.rpc.call_api(
            self.base_req(cmx_service_rpc::HttpMethod::Post, paths::INSTANCE_VARIABLES.to_string())
                .json_body(serde_json::json!({ "id": instance_id }))
                .idempotent(),
        )
        .await
    }

    async fn instance_comments(&self, instance_id: &str) -> Result<Vec<Value>, ServiceRpcError> {
        let page: CommentsPage = self.rpc.call_api(
            self.base_req(cmx_service_rpc::HttpMethod::Post, paths::INSTANCE_COMMENTS.to_string())
                .json_body(serde_json::json!({ "id": instance_id }))
                .idempotent(),
        )
        .await?;
        Ok(page.comments)
    }

    async fn biz_of_instance(&self, instance_id: &str) -> Result<Vec<Value>, ServiceRpcError> {
        let page: BizLinksPage = self.rpc.call_api(
            self.base_req(cmx_service_rpc::HttpMethod::Post, paths::INSTANCE_BIZ.to_string())
                .json_body(serde_json::json!({ "id": instance_id }))
                .idempotent(),
        )
        .await?;
        Ok(page.links)
    }

    async fn biz_instances(
        &self,
        biz_table: &str,
        biz_id: &str,
    ) -> Result<Vec<BizInstanceSummary>, ServiceRpcError> {
        let page: InstancesPage = self.rpc.call_api(
            self.base_req(cmx_service_rpc::HttpMethod::Post, paths::BIZ_INSTANCES.to_string())
                .json_body(serde_json::json!({ "bizTable": biz_table, "bizId": biz_id }))
                .idempotent(),
        )
        .await?;
        Ok(page.instances)
    }

    async fn my_claimable_tasks(&self, user: &str) -> Result<Vec<String>, ServiceRpcError> {
        let page: MyTasksPage = self.rpc.call_api(
            self.base_req(cmx_service_rpc::HttpMethod::Post, paths::TASKS_MY.to_string())
                .json_body(serde_json::json!({ "assignee": user, "kind": "claimable" }))
                .idempotent(),
        )
        .await?;
        Ok(page.tasks.into_iter().map(|t| t.task_id).collect())
    }
}

/// 显式令牌优先（构造期 token 次之）。
fn apply_token(
    mut req: cmx_service_rpc::RpcRequest,
    user_token: Option<&str>,
) -> cmx_service_rpc::RpcRequest {
    if let Some(t) = user_token.filter(|s| !s.is_empty()) {
        req.delegated_token = Some(t.to_string());
    }
    req
}

fn decode_err(e: serde_json::Error) -> ServiceRpcError {
    ServiceRpcError::Decode {
        key: SERVICE_KEY.to_string(),
        cause: format!("请求序列化失败: {e}"),
    }
}

/// 从全局服务目录构造 flow 客户端。
///
/// 目录未初始化 / 未配置 `flow` 键 → `Err`（错误信息给出配置提示，不 panic）。
pub fn client() -> Result<Arc<dyn FlowClient>, ServiceRpcError> {
    let handle = cmx_service_rpc::global_arc().ok_or_else(|| {
        ServiceRpcError::Unavailable {
            key: SERVICE_KEY.to_string(),
            cause: "service_rpc 基座未初始化（需先执行 init_infra）".to_string(),
        }
    })?;
    ensure_flow_key(&handle)?;
    Ok(Arc::new(HttpFlowClient::from_handle(handle)))
}

/// 携带显式委托令牌构造（后台链路用）。
pub fn client_with_token(token: impl Into<String>) -> Result<Arc<dyn FlowClient>, ServiceRpcError> {
    let handle = cmx_service_rpc::global_arc().ok_or_else(|| {
        ServiceRpcError::Unavailable {
            key: SERVICE_KEY.to_string(),
            cause: "service_rpc 基座未初始化（需先执行 init_infra）".to_string(),
        }
    })?;
    ensure_flow_key(&handle)?;
    Ok(Arc::new(
        HttpFlowClient::from_handle(handle).with_token(token),
    ))
}

fn ensure_flow_key(handle: &ServiceRpcHandle) -> Result<(), ServiceRpcError> {
    if handle.directory().contains(SERVICE_KEY) {
        Ok(())
    } else {
        Err(ServiceRpcError::Unavailable {
            key: SERVICE_KEY.to_string(),
            cause: "目录未配置 flow 键：请在 [service_rpc.services.flow] 配 url（直连）或 "
                .to_string()
                + "discovery（服务发现）",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cmx_service_rpc::{
        Body, HttpMethod, OutgoingHeaders, RpcResponse, ServiceEntry, ServiceRpcConfig, Transport,
    };
    use async_trait::async_trait;
    use serde_json::json;
    use std::time::Duration;

    type Seen = Arc<std::sync::Mutex<Vec<(String, cmx_service_rpc::RpcRequest)>>>;

    /// mock 传输（与基座单测同构）：记录请求（外部共享 Arc 供断言）、回放响应。
    struct MockTransport {
        script: std::sync::Mutex<Vec<Result<RpcResponse, ServiceRpcError>>>,
        seen: Seen,
    }

    #[async_trait]
    impl Transport for MockTransport {
        async fn execute(
            &self,
            base: &str,
            req: &cmx_service_rpc::RpcRequest,
            _timeout: Duration,
            _headers: &OutgoingHeaders,
        ) -> Result<RpcResponse, ServiceRpcError> {
            self.seen.lock().unwrap().push((base.to_string(), req.clone()));
            let mut script = self.script.lock().unwrap();
            if script.is_empty() {
                panic!("mock 脚本耗尽");
            }
            script.remove(0)
        }
    }

    fn flow_config() -> ServiceRpcConfig {
        let mut cfg = ServiceRpcConfig::default();
        cfg.services.insert(
            SERVICE_KEY.to_string(),
            ServiceEntry {
                url: Some("http://10.0.0.1:8091".to_string()),
                ..Default::default()
            },
        );
        cfg
    }

    fn handle(script: Vec<Result<RpcResponse, ServiceRpcError>>) -> (Arc<ServiceRpcHandle>, Seen) {
        let seen: Seen = Arc::default();
        let handle = Arc::new(ServiceRpcHandle::with_transport(
            flow_config(),
            Arc::new(MockTransport {
                script: std::sync::Mutex::new(script),
                seen: seen.clone(),
            }),
        ));
        (handle, seen)
    }

    fn seen_requests(seen: &Seen) -> Vec<(String, cmx_service_rpc::RpcRequest)> {
        seen.lock().unwrap().clone()
    }

    fn envelope(data: Value) -> RpcResponse {
        RpcResponse {
            status: 200,
            body: json!({ "code": 0, "msg": "ok", "data": data }),
        }
    }

    /// start_instance：路径 / 方法 / body 字段（camelCase）/ 委托令牌透传 / 类型化响应。
    #[tokio::test]
    async fn start_instance_contract() {
        let (mock, seen) = handle(vec![Ok(envelope(json!({
            "id": 12345,
            "definitionKey": "mdm_cr_approval",
            "state": "ACTIVE",
            "variables": {"initiator": "u1"},
            "tasks": [{"id": "t-1", "assignee": "u1"}]
        })))]);

        // 直接构造客户端（绕全局单例），显式令牌走 builder。
        let client = HttpFlowClient::from_handle(mock.clone()).with_token("jwt-abc");
        let resp = client
            .start_instance(
                StartInstanceReq {
                    definition_key: "mdm_cr_approval".to_string(),
                    business_key: Some("CR-1".to_string()),
                    variables: Some(json!({"initiator": "u1"})),
                    biz_link: Some(BizLink {
                        biz_table: "cv_mdm_apply".to_string(),
                        biz_id: "42".to_string(),
                        biz_key: None,
                        role: "approval".to_string(),
                    }),
                    ..Default::default()
                },
                None,
            )
            .await
            .expect("应成功");

        // id 兼容 number → String。
        assert_eq!(resp.id, "12345");
        assert_eq!(resp.state.as_deref(), Some("ACTIVE"));
        // flatten extra 保留 tasks。
        assert!(resp.extra.as_ref().unwrap()["tasks"].is_array());

        let seen = seen_requests(&seen);
        assert_eq!(seen[0].0, "http://10.0.0.1:8091");
        assert_eq!(seen[0].1.path, paths::INSTANCES_START);
        assert_eq!(seen[0].1.method, HttpMethod::Post);
        assert!(matches!(&seen[0].1.body, Body::Json(_)));
        let body = json_of(&seen[0].1);
        assert_eq!(body["definitionKey"], "mdm_cr_approval");
        assert_eq!(body["bizLink"]["bizTable"], "cv_mdm_apply");
        assert_eq!(body["bizLink"]["bizId"], "42");
        assert_eq!(body["variables"]["initiator"], "u1");
    }

    /// complete / reject / cancel 的路径拼接与请求体字段。
    #[tokio::test]
    async fn task_and_cancel_paths() {
        let (mock, seen) = handle(vec![
            Ok(envelope(json!({"id": "i-1", "state": "COMPLETED"}))),
            Ok(envelope(json!({"id": "i-1"}))),
            Ok(envelope(json!({"id": "i-1", "state": "TERMINATED"}))),
        ]);
        let client = HttpFlowClient::from_handle(mock.clone());

        client
            .complete_task(
                "t-9",
                CompleteTaskReq {
                    instance_id: "i-1".to_string(),
                    decision: Some("approve".to_string()),
                    operator: Some("u1".to_string()),
                    ..Default::default()
                },
                Some("jwt-xyz"),
            )
            .await
            .expect("应成功");
        client
            .reject_task(
                "t-9",
                RejectTaskReq {
                    instance_id: "i-1".to_string(),
                    from_user: Some("u2".to_string()),
                    ..Default::default()
                },
                None,
            )
            .await
            .expect("应成功");
        client
            .cancel_instance("i-1", CancelReq { reason: Some("撤回".to_string()) }, None)
            .await
            .expect("应成功");

        let seen = seen_requests(&seen);
        assert_eq!(seen[0].1.path, "/api/flow/v1/tasks/t-9/complete");
        assert_eq!(seen[0].1.delegated_token.as_deref(), Some("jwt-xyz"));
        let b0 = json_of(&seen[0].1);
        assert_eq!(b0["instanceId"], "i-1");
        assert_eq!(b0["decision"], "approve");
        assert_eq!(b0["operator"], "u1");
        assert_eq!(seen[1].1.path, "/api/flow/v1/tasks/t-9/reject");
        assert_eq!(json_of(&seen[1].1)["fromUser"], "u2");
        assert_eq!(seen[2].1.path, "/api/flow/v1/instances/i-1/cancel");
        assert_eq!(json_of(&seen[2].1)["reason"], "撤回");
    }

    /// 查询族：GET + 幂等标记 + query 拼接 + 包裹解包。
    #[tokio::test]
    async fn query_family_contract() {
        let (mock, seen) = handle(vec![
            Ok(envelope(json!({"id": "i-1", "state": "ACTIVE", "tasks": []}))),
            Ok(envelope(json!({"lastDecision": "approve"}))),
            Ok(envelope(json!({"comments": [{"comment": "ok"}]}))),
            Ok(envelope(json!({"links": [{"bizTable": "cv_mdm_apply"}]}))),
            Ok(envelope(json!({"instances": [{"instanceId": "i-1", "state": "ACTIVE"}]}))),
            Ok(envelope(json!({"tasks": [{"taskId": "t-1"}, {"taskId": 7}],
                                 "total": 2, "page": 1, "pageSize": 20}))),
        ]);
        let client = HttpFlowClient::from_handle(mock.clone());

        client.instance_detail("i-1").await.expect("应成功");
        client.instance_variables("i-1").await.expect("应成功");
        let comments = client.instance_comments("i-1").await.expect("应成功");
        assert_eq!(comments.len(), 1);
        let links = client.biz_of_instance("i-1").await.expect("应成功");
        assert_eq!(links.len(), 1);
        let instances = client.biz_instances("cv_mdm_apply", "42").await.expect("应成功");
        assert_eq!(instances[0].instance_id, "i-1");
        let tasks = client.my_claimable_tasks("张三").await.expect("应成功");
        // taskId 兼容 number → String。
        assert_eq!(tasks, vec!["t-1".to_string(), "7".to_string()]);

        let seen = seen_requests(&seen);
        assert_eq!(seen[0].1.method, HttpMethod::Get);
        assert!(seen[0].1.idempotent);
        assert_eq!(seen[0].1.path, "/api/flow/v1/instances/i-1");
        assert_eq!(seen[1].1.path, "/api/flow/v1/instances/i-1/variables");
        assert_eq!(seen[2].1.path, "/api/flow/v1/instances/i-1/comments");
        assert_eq!(seen[3].1.path, "/api/flow/v1/instances/i-1/biz");
        assert_eq!(seen[4].1.path, "/api/flow/v1/biz/cv_mdm_apply/42/instances");
        assert_eq!(seen[5].1.path, "/api/flow/v1/tasks/my");
        assert!(seen[5].1.query.contains(&("assignee".to_string(), "张三".to_string())));
    }

    /// 信封业务失败（code=1）→ Remote 错误，msg 透传。
    #[tokio::test]
    async fn envelope_business_error() {
        let (mock, _seen) = handle(vec![Ok(RpcResponse {
            status: 200,
            body: json!({ "code": 1, "msg": "任务不可办 TaskNotActionable" }),
        })]);
        let client = HttpFlowClient::from_handle(mock);
        let err = client
            .complete_task(
                "t-1",
                CompleteTaskReq { instance_id: "i-1".to_string(), ..Default::default() },
                None,
            )
            .await
            .expect_err("code=1 应失败");
        match err {
            ServiceRpcError::Remote { msg, .. } => assert!(msg.contains("TaskNotActionable")),
            other => panic!("应映射 Remote，实际 {other:?}"),
        }
    }

    /// DTO 序列化形状快照（防无意的 wire 破坏：camelCase + skip_none）。
    #[test]
    fn dto_wire_shapes() {
        let req = StartInstanceReq {
            definition_key: "k".to_string(),
            ..Default::default()
        };
        assert_eq!(
            serde_json::to_value(&req).unwrap(),
            json!({ "definitionKey": "k" })
        );
        let req = CompleteTaskReq {
            instance_id: "i".to_string(),
            comment: Some("意见".to_string()),
            ..Default::default()
        };
        assert_eq!(
            serde_json::to_value(&req).unwrap(),
            json!({ "instanceId": "i", "comment": "意见" })
        );
        let link = BizLink {
            biz_table: "t".to_string(),
            biz_id: "1".to_string(),
            biz_key: None,
            role: "approval".to_string(),
        };
        assert_eq!(
            serde_json::to_value(&link).unwrap(),
            json!({ "bizTable": "t", "bizId": "1", "role": "approval" })
        );
    }

    fn json_of(req: &cmx_service_rpc::RpcRequest) -> Value {
        match &req.body {
            Body::Json(v) => v.clone(),
            _ => Value::Null,
        }
    }
}
