//! RPC 调用执行（invoke）——一次服务间调用的完整生命周期。
//!
//! [`RpcRequest`] 描述"调谁（key）+ 打哪（path）+ 带什么（body/头/幂等标记）"，由
//! [`crate::ServiceRpcHandle::execute`] 统一执行：目录定位（url 直连 | 实例缓存选例）
//! → 传输执行（鉴权注入 + 总超时）→ 状态码 / 信封错误映射 → 幂等重试（连接级错误换实例）
//! → 熔断与打点。消费方一般不直接拼 [`RpcRequest`]，而是使用契约 SDK
//! （`cmx-flow-sdk` 等，DTO + 路径常量 + 客户端 trait）。
//!
//! 两层 API：
//! - [`crate::ServiceRpcHandle::execute`]：传输层结果（HTTP 状态 + 已解析 JSON body），
//!   供信封方言特殊的消费方（远程导入器走 `{code:200,message}` 旧方言）自行解包；
//! - [`call_api`] / [`call_api_unit`]：标准 `ApiResp` 信封（`{code,msg,data}`，code==0 成功）
//!   解包为 `data` 的强类型，绝大多数服务间调用用这层。

use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::Value;

use crate::error::ServiceRpcError;

/// HTTP 方法（自有小枚举，避免传输 trait 绑死 reqwest 类型，便于 mock）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpMethod {
    /// 查询（幂等，可参与连接级重试）。
    Get,
    /// 提交（默认非幂等，不重试；`RpcRequest::idempotent` 显式标记可覆盖）。
    Post,
}

impl HttpMethod {
    /// 大写方法名。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
        }
    }
}

/// 请求体形态。
#[derive(Debug, Clone, Default)]
pub enum Body {
    /// 无 body（GET）。
    #[default]
    None,
    /// JSON body（`content-type: application/json`）。
    Json(Value),
    /// 原始字节 body（自定义 content-type；如 webhook 的"先序列化后签名"场景，
    /// 保证签名对象与实际发送字节一致）。
    Raw {
        /// body 字节。
        bytes: Vec<u8>,
        /// content-type 头值。
        content_type: String,
    },
    /// multipart 表单（远程导入器的 ZIP 上传等）。
    Multipart(Vec<FormPart>),
}

/// multipart 表单的单个 part。
#[derive(Debug, Clone)]
pub struct FormPart {
    /// 字段名。
    pub name: String,
    /// 文件名（`None` = 纯文本字段）。
    pub filename: Option<String>,
    /// MIME 类型（缺省由传输层推断/省略）。
    pub mime: Option<String>,
    /// 字节内容。
    pub data: Vec<u8>,
}

impl FormPart {
    /// 文本字段。
    pub fn text(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            filename: None,
            mime: None,
            data: value.into().into_bytes(),
        }
    }

    /// 文件字段。
    pub fn file(
        name: impl Into<String>,
        filename: impl Into<String>,
        mime: impl Into<String>,
        bytes: Vec<u8>,
    ) -> Self {
        Self {
            name: name.into(),
            filename: Some(filename.into()),
            mime: Some(mime.into()),
            data: bytes,
        }
    }
}

/// 出站鉴权头集合（由基座统一组装，传输层负责落到具体头名）。
///
/// - `X-API-Key`：服务身份（`[service_auth].outgoing_api_key`）；
/// - `X-Delegated-User-Token: Bearer {jwt}`：用户委托令牌（OBO），显式传入优先，
///   缺省取当前请求 task-local 的原始用户 JWT（后台任务无请求上下文时省略）；
/// - `X-Request-Id`：链路追踪（有请求上下文时透传）。
#[derive(Debug, Clone, Default)]
pub struct OutgoingHeaders {
    /// 服务间 API Key。
    pub api_key: Option<String>,
    /// 用户委托 JWT（裸值，传输层加 Bearer 前缀）。
    pub delegated_token: Option<String>,
    /// 请求 ID。
    pub request_id: Option<String>,
    /// 业务附加头（如 webhook 签名头）。
    pub extra: Vec<(String, String)>,
}

/// 一次服务间调用请求。
#[derive(Debug, Clone)]
pub struct RpcRequest {
    /// 目标服务键（`[service_rpc.services]` 中的键，如 "flow"）。
    pub key: String,
    /// HTTP 方法。
    pub method: HttpMethod,
    /// 完整路径（以 `/` 开头，如 `/api/flow/v1/instances`；来自契约 SDK 的路径常量）。
    pub path: String,
    /// 查询参数（按序拼接）。
    pub query: Vec<(String, String)>,
    /// 请求体。
    pub body: Body,
    /// 幂等标记：`true` 时连接级错误可换实例重试（GET 天然幂等；业务上确保幂等的 POST
    /// 可显式标记）。
    pub idempotent: bool,
    /// 显式委托令牌（覆盖 task-local 上下文）。
    pub delegated_token: Option<String>,
    /// 附加头（签名头等）。
    pub extra_headers: Vec<(String, String)>,
    /// 键级超时覆盖（缺省取目录 `timeout_of(key)`）。
    pub timeout: Option<Duration>,
}

impl RpcRequest {
    /// GET 请求（幂等）。
    pub fn get(key: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            method: HttpMethod::Get,
            path: path.into(),
            query: Vec::new(),
            body: Body::None,
            idempotent: true,
            delegated_token: None,
            extra_headers: Vec::new(),
            timeout: None,
        }
    }

    /// POST 请求（默认非幂等）。
    pub fn post(key: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            method: HttpMethod::Post,
            path: path.into(),
            query: Vec::new(),
            body: Body::None,
            idempotent: false,
            delegated_token: None,
            extra_headers: Vec::new(),
            timeout: None,
        }
    }

    /// 追加查询参数。
    pub fn query(mut self, k: impl Into<String>, v: impl Into<String>) -> Self {
        self.query.push((k.into(), v.into()));
        self
    }

    /// 设置 JSON body。
    pub fn json_body(mut self, v: Value) -> Self {
        self.body = Body::Json(v);
        self
    }

    /// 设置原始字节 body（自定义 content-type）。
    pub fn raw_body(mut self, bytes: Vec<u8>, content_type: impl Into<String>) -> Self {
        self.body = Body::Raw {
            bytes,
            content_type: content_type.into(),
        };
        self
    }

    /// 设置 multipart body。
    pub fn multipart(mut self, parts: Vec<FormPart>) -> Self {
        self.body = Body::Multipart(parts);
        self
    }

    /// 显式标记幂等（连接级错误换实例重试）。
    pub fn idempotent(mut self) -> Self {
        self.idempotent = true;
        self
    }

    /// 显式设置委托令牌（无请求上下文的后台链路用）。
    pub fn with_token(mut self, token: impl Into<String>) -> Self {
        self.delegated_token = Some(token.into());
        self
    }

    /// 追加附加头。
    pub fn header(mut self, k: impl Into<String>, v: impl Into<String>) -> Self {
        self.extra_headers.push((k.into(), v.into()));
        self
    }

    /// 覆盖超时。
    pub fn timeout(mut self, d: Duration) -> Self {
        self.timeout = Some(d);
        self
    }
}

/// 传输层执行结果：HTTP 状态 + 已解析的 JSON body（非 JSON 响应为 `Value::Null`）。
///
/// 信封语义由上层（[`call_api`] 或消费方）解释——本层不做业务判定。
#[derive(Debug, Clone)]
pub struct RpcResponse {
    /// HTTP 状态码。
    pub status: u16,
    /// 响应 JSON body（无法解析为 JSON 时为 `Value::Null`）。
    pub body: Value,
}

/// 标准 `ApiResp` 信封（`{code, msg, data}`，code==0 成功；宽容缺省字段）。
#[derive(Debug, Deserialize)]
struct Envelope {
    #[serde(default = "default_envelope_code")]
    code: i64,
    #[serde(default)]
    msg: Option<String>,
    #[serde(default, alias = "message")]
    message: Option<String>,
    #[serde(default)]
    data: Option<Value>,
}

fn default_envelope_code() -> i64 {
    -1
}

/// HTTP 状态 / 信封 → 错误映射（2xx 且不判信封；供 execute 层统一使用）。
///
/// - 401/403 → [`ServiceRpcError::AuthRejected`]；
/// - 其余非 2xx → [`ServiceRpcError::Remote`]（msg 尽量取响应信封的 msg/message）；
/// - 2xx → 原样返回 [`RpcResponse`]。
pub(crate) fn map_http_status(
    key: &str,
    resp: RpcResponse,
) -> Result<RpcResponse, ServiceRpcError> {
    if (200..300).contains(&resp.status) {
        return Ok(resp);
    }
    let envelope_msg = resp
        .body
        .get("msg")
        .or_else(|| resp.body.get("message"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let code = resp.body.get("code").and_then(Value::as_i64).unwrap_or(-1);
    if resp.status == 401 || resp.status == 403 {
        return Err(ServiceRpcError::AuthRejected {
            key: key.to_string(),
            cause: envelope_msg.unwrap_or_else(|| format!("HTTP {}", resp.status)),
        });
    }
    Err(ServiceRpcError::Remote {
        key: key.to_string(),
        http_status: resp.status,
        code,
        msg: envelope_msg.unwrap_or_else(|| "未知错误".to_string()),
    })
}

/// 解标准信封并反序列化 `data` 为 `T`（HTTP 2xx 且信封 code==0）。
pub(crate) fn unwrap_envelope<T: DeserializeOwned>(
    key: &str,
    resp: &RpcResponse,
) -> Result<T, ServiceRpcError> {
    let envelope: Envelope = serde_json::from_value(resp.body.clone()).map_err(|e| {
        ServiceRpcError::Decode {
            key: key.to_string(),
            cause: format!("响应不是 ApiResp 信封: {e}"),
        }
    })?;
    if envelope.code != 0 {
        return Err(ServiceRpcError::Remote {
            key: key.to_string(),
            http_status: resp.status,
            code: envelope.code,
            msg: envelope
                .msg
                .or(envelope.message)
                .unwrap_or_else(|| "未知错误".to_string()),
        });
    }
    let data = envelope.data.unwrap_or(Value::Null);
    serde_json::from_value(data).map_err(|e| ServiceRpcError::Decode {
        key: key.to_string(),
        cause: format!("信封 data 反序列化失败: {e}"),
    })
}

/// 经全局基座执行请求并解标准信封，返回 `data` 强类型。
///
/// 基座未初始化（未跑 [`crate::init`]）时返回 `Unavailable` 错误，不 panic。
pub async fn call_api<T: DeserializeOwned>(req: RpcRequest) -> Result<T, ServiceRpcError> {
    let key = req.key.clone();
    let handle = crate::global_or_err(&key)?;
    let resp = handle.execute(req).await?;
    unwrap_envelope(&key, &resp)
}

/// [`call_api`] 的无数据版（只关心成败）。
pub async fn call_api_unit(req: RpcRequest) -> Result<(), ServiceRpcError> {
    call_api::<serde_json::Value>(req).await.map(|_| ())
}
