//! 反代转发核——目标解析、出站请求构建、流式转发与错误兜底。
//!
//! [`ProxyCore`] 是三反代壳共享的无路由转发器：壳负责路径重写（flow 升 `/v1`、rpt/rule 恒等），
//! 核负责"拼好目标 URL 之后的一切"——出站头卫生（P0：剥客户端可伪造的平台注入型头）、
//! 三层出站鉴权注入、流式转发（SSE 逐块透传）、502/503 兜底。

use std::sync::Arc;
use std::time::Duration;

use axum::extract::Request;
use axum::http::{StatusCode, Uri};
use axum::response::{IntoResponse, Response};

/// 反代目标 resolver：每次调用返回当前可用基址（`None` = 无可用实例 → 503）。
///
/// 由装配层（cmx-platform-app routes.rs）从 `cmx_plugin::center_client::ProxyUpstream::resolver_fn`
/// 构造——`Send + Sync` 无状态闭包，`Static` 固化返回基址，`Discovery` 查内存实例缓存。
pub type UpstreamResolver = Arc<dyn Fn() -> Option<String> + Send + Sync>;

/// 反代转发核：持目标 resolver + 出站服务凭证 + 复用的 HTTP 客户端。
///
/// 一个核对应一个下游域（flow/report/rules），API 反代与页面反代共享同一实例（同一连接池）。
pub struct ProxyCore {
    /// 目标基址 resolver（静态基址或 Nacos 服务发现）。
    resolver: UpstreamResolver,
    /// 平台对外服务凭证（`[service_auth].outgoing_api_key`，注入 X-API-Key）。可空（不注入）。
    api_key: Option<String>,
    client: reqwest::Client,
}

impl ProxyCore {
    /// 用目标 resolver + 出站 API Key 构建。
    ///
    /// 超时语义（P0 修复）：只设 **连接超时 5s + 读空闲超时 60s，不设总超时**——总超时是含
    /// 响应体读完的硬期限，会掐断 SSE/长轮询等长流（原 30s 总超时对 `/events` 会在 30s 处
    /// 硬切流）。读空闲超时只约束"两次数据之间的间隔"，流持续有数据就不受影响。
    pub fn new(resolver: UpstreamResolver, api_key: Option<String>) -> Self {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .read_timeout(Duration::from_secs(60))
            .build()
            .unwrap_or_else(|e| {
                tracing::error!(error = %e, "ProxyCore 构建 reqwest 客户端失败，退回默认");
                reqwest::Client::new()
            });
        Self {
            resolver,
            api_key,
            client,
        }
    }

    /// 目标 resolver 引用（页面反代中间件等需要自持 resolver 的场景）。
    pub fn resolver(&self) -> &UpstreamResolver {
        &self.resolver
    }

    /// 目标无可用实例时的 503 响应（区别于 502 不可达：服务发现未就绪或实例全部下线）。
    pub fn no_upstream(svc: &str) -> Response {
        tracing::error!(service = svc, "反代目标无可用实例（服务发现未就绪或实例全部下线）");
        (
            StatusCode::SERVICE_UNAVAILABLE,
            axum::Json(serde_json::json!({
                "code": 503,
                "msg": format!("{svc}无可用实例（服务发现未就绪或实例全部下线）")
            })),
        )
            .into_response()
    }

    /// 完整转发：解析目标 → `rewrite` 拼目标 URL → 头卫生 + 鉴权注入 → 流式转发。
    ///
    /// # Arguments
    ///
    /// * `svc`     - 下游服务中文名（502/503 错误信封与日志用，如"流程服务"）。
    /// * `req`     - 入站请求（method/headers/流式 body 原样取用）。
    /// * `rewrite` - 路径重写闭包：`(基址, 入站 Uri) → 目标绝对 URL`（壳的域差异全在这，
    ///   如 flow 的 `{base}/api/flow/v1/{rest}`、rpt/rule 的 `{base}/api{path}`）。
    ///
    /// # Returns
    ///
    /// 下游响应（透传给客户端）；无可用实例 → 503；下游不可达 → 502。
    pub async fn forward(
        &self,
        svc: &str,
        req: Request,
        rewrite: impl FnOnce(&str, &Uri) -> String,
    ) -> Response {
        // 目标基址按请求动态解析（服务发现模式实例列表可能变化）。
        let Some(base) = (self.resolver)() else {
            return Self::no_upstream(svc);
        };
        let method = req.method().clone();
        let uri = req.uri().clone();
        let headers = req.headers().clone();
        // 直连客户端地址（server 配了 into_make_service_with_connect_info 才有，用于 X-Forwarded-For）。
        let peer = req
            .extensions()
            .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
            .map(|ci| ci.0);
        let target = rewrite(&base, &uri);

        // 请求体 → reqwest stream（双向流式，避免整体缓冲）。
        let body = req.into_body();
        let reqwest_body = reqwest::Body::wrap_stream(body.into_data_stream());

        let mut rb = self.client.request(method, &target).body(reqwest_body);

        // 出站头卫生：剥逐跳/可伪造注入型头/Cookie + 补 X-Forwarded-*（见 headers.rs）。
        rb = rb.headers(crate::headers::sanitize_request_headers(
            &headers,
            peer.as_ref(),
            &uri,
        ));

        // 三层出站鉴权（对齐 remote_importers::apply_auth_headers）：剥除客户端伪造值后
        // 从可信源重新注入。
        if let Some(key) = &self.api_key {
            rb = rb.header("X-API-Key", key);
        }
        if let Some(user_jwt) = cmx_traits::auth::context_scope::current_original_token() {
            rb = rb.header("X-Delegated-User-Token", format!("Bearer {user_jwt}"));
        }
        if let Some(rid) = cmx_traits::auth::context_scope::current_request_id() {
            rb = rb.header("X-Request-Id", rid);
        }

        match rb.send().await {
            Ok(resp) => crate::headers::build_response(resp),
            Err(e) => {
                tracing::error!(error = %e, %target, service = svc, "反代转发失败");
                (
                    StatusCode::BAD_GATEWAY,
                    axum::Json(serde_json::json!({
                        "code": 502,
                        "msg": format!("{svc}不可达: {e}")
                    })),
                )
                    .into_response()
            }
        }
    }
}
