//! HTTP 传输实现（reqwest，默认 feature）。
//!
//! 与南北向反代核（cmx-proxy-core）职责分离：本层做**东西向服务间调用**——
//! 一次性请求/响应（有总超时，无流式），反代核做浏览器流量的透明双向转发。

use std::time::Duration;

use async_trait::async_trait;
use serde_json::Value;

use crate::error::ServiceRpcError;
use crate::invoke::{Body, HttpMethod, OutgoingHeaders, RpcRequest, RpcResponse};
use crate::transport::Transport;

/// reqwest HTTP 传输。
///
/// 连接超时 5s（快速失败，参与幂等重试的连接级判定）；总超时由调用层按请求传入
/// （键级 `timeout_ms` ?? 全局缺省）。
#[derive(Debug, Clone)]
pub struct HttpTransport {
    client: reqwest::Client,
}

impl Default for HttpTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpTransport {
    /// 构造传输（连接池复用；纯连接池无业务状态，集群无状态合规）。
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .build()
            .unwrap_or_default();
        Self { client }
    }
}

#[async_trait]
impl Transport for HttpTransport {
    async fn execute(
        &self,
        base: &str,
        req: &RpcRequest,
        timeout: Duration,
        headers: &OutgoingHeaders,
    ) -> Result<RpcResponse, ServiceRpcError> {
        let url = format!("{base}{}", req.path);
        let builder = match req.method {
            HttpMethod::Get => self.client.get(&url),
            HttpMethod::Post => self.client.post(&url),
        }
        .timeout(timeout)
        .query(&req.query)
        .headers(build_header_map(headers));

        let builder = match &req.body {
            Body::None => builder,
            Body::Json(v) => builder.json(v),
            Body::Raw { bytes, content_type } => builder
                .header(reqwest::header::CONTENT_TYPE, content_type.as_str())
                .body(bytes.clone()),
            Body::Multipart(parts) => {
                let mut form = reqwest::multipart::Form::new();
                for part in parts {
                    let mut p = reqwest::multipart::Part::bytes(part.data.clone());
                    if let Some(filename) = &part.filename {
                        p = p.file_name(filename.clone());
                    }
                    let p = if let Some(mime) = &part.mime {
                        // MIME 非法值极罕见（内部常量）：重建不带 MIME 的 part 兜底。
                        p.mime_str(mime).unwrap_or_else(|_| {
                            let mut f = reqwest::multipart::Part::bytes(part.data.clone());
                            if let Some(filename) = &part.filename {
                                f = f.file_name(filename.clone());
                            }
                            f
                        })
                    } else {
                        p
                    };
                    form = form.part(part.name.clone(), p);
                }
                builder.multipart(form)
            }
        };

        let resp = builder
            .send()
            .await
            .map_err(|e| map_reqwest_error(&req.key, &e))?;
        let status = resp.status().as_u16();
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| ServiceRpcError::Unavailable {
                key: req.key.clone(),
                cause: format!("读取响应失败: {e}"),
            })?;
        let body: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        Ok(RpcResponse { status, body })
    }
}

/// 组装出站头：鉴权三件套（有值才发，空值不发）+ 业务附加头。
fn build_header_map(headers: &OutgoingHeaders) -> reqwest::header::HeaderMap {
    let mut map = reqwest::header::HeaderMap::new();
    let mut insert = |name: &str, value: Option<&str>| {
        if let Some(v) = value.filter(|s| !s.is_empty())
            && let (Ok(n), Ok(val)) = (
                name.parse::<reqwest::header::HeaderName>(),
                v.parse::<reqwest::header::HeaderValue>(),
            )
        {
            map.insert(n, val);
        }
    };
    insert("X-API-Key", headers.api_key.as_deref());
    insert("X-Request-Id", headers.request_id.as_deref());
    if let Some(token) = headers.delegated_token.as_deref().filter(|s| !s.is_empty()) {
        let bearer = format!("Bearer {token}");
        if let (Ok(n), Ok(val)) = (
            "X-Delegated-User-Token".parse::<reqwest::header::HeaderName>(),
            bearer.parse::<reqwest::header::HeaderValue>(),
        ) {
            map.insert(n, val);
        }
    }
    for (k, v) in &headers.extra {
        if let (Ok(name), Ok(value)) = (
            k.parse::<reqwest::header::HeaderName>(),
            v.parse::<reqwest::header::HeaderValue>(),
        ) {
            map.insert(name, value);
        }
    }
    map
}

/// reqwest 错误分类：超时 → Timeout；其余（连接 / DNS / 读取等网络错）→ Unavailable
/// （Unavailable 参与幂等调用的连接级换实例重试）。
fn map_reqwest_error(key: &str, e: &reqwest::Error) -> ServiceRpcError {
    if e.is_timeout() {
        ServiceRpcError::Timeout {
            key: key.to_string(),
            timeout_ms: 0,
        }
    } else {
        ServiceRpcError::Unavailable {
            key: key.to_string(),
            cause: format!("网络错误: {e}"),
        }
    }
}
