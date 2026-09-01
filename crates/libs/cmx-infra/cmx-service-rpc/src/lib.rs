//! # cmx-service-rpc —— 微服务间统一调用基座
//!
//! 把"服务间怎么找到对方、怎么通信"从各服务散装的 reqwest / center_client / cmx-rpc
//! 收敛为一套心智模型（对标 Spring Cloud OpenFeign + Discovery + LoadBalancer）：
//!
//! - **目录**（[`directory`]）：`[service_rpc.services]` per-key 自由表——`url` 静态直连
//!   （★无注册中心的回滚形态）或 `discovery` 服务发现选例（healthy 过滤 + weight 加权
//!   + `http_port` 元数据优先）；url 优先、删 url 即切发现（灰度切换语义）；
//! - **传输**（[`transport`]）：默认 feature `http`（reqwest，零 volo 依赖）；
//!   gRPC 按 feature 门控（`grpc-client` / `grpc-server`），纯 REST 服务不背 gRPC 依赖树；
//! - **横切**：统一鉴权链出站注入（`X-API-Key` + `X-Delegated-User-Token` OBO 委托 +
//!   `X-Request-Id`）、键级/全局总超时、幂等调用连接级换实例重试、per-key 熔断、
//!   tracing span 与 per-key 打点；
//! - **契约 SDK 配套**：服务方在 container 各域组发布 `cmx-{svc}-sdk`
//!   （路径常量 + wire DTO + `trait XxxClient`），消费方加一个依赖即获得类型化调用，
//!   契约变更双方编译期感知。
//!
//! ## 初始化
//!
//! [`init`]（或异步 [`init_and_warm`]，含服务发现订阅预热）在 `cmx-service-base::init_infra`
//! 末尾自动执行：读配置 → fail-fast 校验（判定矩阵见 [`config`] 模块文档）→ 构造全局句柄
//! → 打目录快照日志。段缺失 = 合法空目录（全内嵌 / 零出站形态）。
//!
//! ## 两层调用 API
//!
//! - 契约 SDK / 常规调用：[`invoke::call_api`]（标准 `ApiResp` 信封解包为强类型 `data`）；
//! - 信封方言特殊的消费方（远程导入器 `{code:200,message}` 旧方言）：
//!   [`ServiceRpcHandle::execute`] 拿 [`invoke::RpcResponse`] 自行解包。
//!
//! ## 与南北向反代的分工
//!
//! 反代（cmx-proxy-core + 各 `cmx-*-api` 壳）转发浏览器流量（流式、无总超时）；
//! 本基座做东西向服务间调用（一次性、有总超时）。反代的目标定位复用本基座目录
//! （[`locator`] / [`Locator::resolver_fn`]），一份目录两用。

pub mod config;
pub mod directory;
pub mod error;
pub mod guard;
pub mod invoke;
pub mod obs;
pub mod transport;

#[cfg(feature = "grpc-client")]
pub mod grpc;

use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use crate::guard::{BreakerGuard, BreakerVerdict};
use crate::invoke::map_http_status;
use crate::obs::Stats;

pub use config::{GrpcConfig, ServerConfig, ServiceEntry, ServiceRpcConfig, TransportKind};
pub use error::ServiceRpcError;
pub use directory::{
    pick_instance_base, registry_enabled, resolve_service_base, warm_discovery_targets, Locator,
    ServiceDirectory,
};
pub use invoke::{
    call_api, call_api_unit, Body, FormPart, HttpMethod, OutgoingHeaders, RpcRequest, RpcResponse,
};
pub use transport::Transport;

/// 服务间调用基座句柄（目录 + 传输 + 熔断 + 打点 + 出站凭据快照）。
///
/// 全局单例（[`GLOBAL`]，基础设施合规：只读配置快照 + 连接池 + 计数器，无业务态）；
/// 测试经 [`ServiceRpcHandle::with_transport`] 注入 mock 传输独立构造。
pub struct ServiceRpcHandle {
    directory: ServiceDirectory,
    transport: Arc<dyn Transport>,
    breaker: Arc<BreakerGuard>,
    stats: Arc<Stats>,
    api_key: Option<String>,
}

impl std::fmt::Debug for ServiceRpcHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServiceRpcHandle")
            .field("directory", &self.directory)
            .field("api_key_set", &self.api_key.is_some())
            .finish_non_exhaustive()
    }
}

impl ServiceRpcHandle {
    /// 用默认 HTTP 传输构造（`http` feature；全局初始化路径）。
    #[cfg(feature = "http")]
    pub fn new(config: ServiceRpcConfig) -> Self {
        Self::with_transport(config, Arc::new(crate::transport::HttpTransport::new()))
    }

    /// 注入自定义传输构造（契约 SDK 单测用 mock 传输走全链路）。
    pub fn with_transport(config: ServiceRpcConfig, transport: Arc<dyn Transport>) -> Self {
        let api_key = load_outgoing_api_key();
        Self {
            directory: ServiceDirectory::new(config),
            transport,
            breaker: Arc::new(BreakerGuard::new()),
            stats: Arc::new(Stats::default()),
            api_key,
        }
    }

    /// 服务目录（定位 / 传输 / 超时 / 重试的只读快照）。
    pub fn directory(&self) -> &ServiceDirectory {
        &self.directory
    }

    /// 出站服务凭据是否已配置（`[service_auth].outgoing_api_key`）。
    pub fn has_api_key(&self) -> bool {
        self.api_key.is_some()
    }

    /// 执行一次服务间调用（完整生命周期：熔断检查 → 定位 → 传输 → 状态映射 →
    /// 幂等重试 → 打点）。
    ///
    /// - 键配 `transport = "grpc"`：gRPC 不经通用请求通道（走契约 SDK 的 gRPC 绑定），
    ///   返回 [`ServiceRpcError::NoBinding`]；
    /// - 幂等重试：仅 `idempotent` 请求在**连接级**失败（Unavailable）时换实例重试，
    ///   上限 = 键级 `retry_max` ?? 全局；超时（Timeout）与业务级错误不重试；
    /// - 2xx 返回 [`RpcResponse`]（信封判定归上层）；401/403 → AuthRejected；
    ///   其余非 2xx → Remote（msg 取信封）。
    pub async fn execute(&self, req: RpcRequest) -> Result<RpcResponse, ServiceRpcError> {
        let key = req.key.clone();
        match self.directory.transport_of(&key) {
            TransportKind::Http => {}
            TransportKind::Grpc => {
                return Err(ServiceRpcError::NoBinding {
                    key,
                    transport: "grpc".to_string(),
                    cause: "gRPC 调用不经通用请求通道，请使用契约 SDK 的 gRPC 绑定".to_string(),
                });
            }
        }
        let timeout_ms = req
            .timeout
            .map(|d| d.as_millis() as u64)
            .unwrap_or_else(|| self.directory.timeout_ms_of(&key));
        let retries = if req.idempotent {
            self.directory.retry_max_of(&key) as usize
        } else {
            0
        };
        match self.breaker.check(&key) {
            BreakerVerdict::Allow => {}
            BreakerVerdict::Reject(wait_ms) => {
                return Err(ServiceRpcError::Unavailable {
                    key,
                    cause: format!("熔断开放中（连续传输级失败达阈值），约 {wait_ms}ms 后进入半开探活"),
                });
            }
        }
        let headers = self.outgoing_headers(&req);
        let span = obs::call_span(&key, req.method.as_str(), &req.path);
        let mut attempt = 0usize;
        loop {
            let base = self.directory.resolve_base(&key).ok_or_else(|| {
                ServiceRpcError::Unavailable {
                    key: key.clone(),
                    cause: format!(
                        "无可用目标（键未配置或服务发现无实例）：[service_rpc.services.{key}]"
                    ),
                }
            })?;
            let started = Instant::now();
            let result = self
                .transport
                .execute(&base, &req, Duration::from_millis(timeout_ms), &headers)
                .await;
            let dur = started.elapsed();
            match result {
                Ok(resp) => {
                    let outcome = map_http_status(&key, resp);
                    match &outcome {
                        Ok(_) => self.breaker.record_success(&key),
                        Err(e) if e.is_transport_failure() => self.breaker.record_failure(&key),
                        Err(_) => {}
                    }
                    self.stats.record(
                        &key,
                        dur,
                        matches!(&outcome, Err(e) if e.is_transport_failure()),
                    );
                    let _guard = span.enter();
                    tracing::debug!(
                        rpc.target = %base,
                        rpc.duration_ms = dur.as_millis() as u64,
                        outcome = ?outcome.as_ref().map(|_| "ok").map_err(|e| e.to_string()),
                        "service_rpc 调用完成"
                    );
                    return outcome;
                }
                Err(e) => {
                    self.breaker.record_failure(&key);
                    self.stats.record(&key, dur, true);
                    if e.is_transport_failure()
                        && matches!(e, ServiceRpcError::Unavailable { .. })
                        && attempt < retries
                    {
                        attempt += 1;
                        tracing::warn!(
                            service.key = %key,
                            attempt,
                            retries,
                            error = %e,
                            "幂等调用连接级失败，换实例重试"
                        );
                        continue;
                    }
                    // 传输层无配置视角，这里补齐生效超时数值。
                    let e = match e {
                        ServiceRpcError::Timeout { key, .. } => {
                            ServiceRpcError::Timeout { key, timeout_ms }
                        }
                        other => other,
                    };
                    return Err(e);
                }
            }
        }
    }

    /// 组装出站鉴权头（显式委托令牌优先，缺省 task-local 用户 JWT）。
    fn outgoing_headers(&self, req: &RpcRequest) -> OutgoingHeaders {
        OutgoingHeaders {
            api_key: self.api_key.clone(),
            delegated_token: req
                .delegated_token
                .clone()
                .or_else(cmx_traits::auth::context_scope::current_original_token),
            request_id: cmx_traits::auth::context_scope::current_request_id(),
            extra: req.extra_headers.clone(),
        }
    }

    /// 经本句柄执行请求并解标准信封，返回 `data` 强类型（契约 SDK 客户端内部用，
    /// 测试可注入自定义句柄）。
    pub async fn call_api<T: serde::de::DeserializeOwned>(
        &self,
        req: RpcRequest,
    ) -> Result<T, ServiceRpcError> {
        let key = req.key.clone();
        let resp = self.execute(req).await?;
        crate::invoke::unwrap_envelope::<T>(&key, &resp)
    }

    /// [`Self::call_api`] 的无数据版（只关心成败）。
    pub async fn call_api_unit(&self, req: RpcRequest) -> Result<(), ServiceRpcError> {
        self.call_api::<serde_json::Value>(req).await.map(|_| ())
    }

    /// per-key 打点快照（观测 / 未来 metrics 出口）。
    pub fn stats(&self) -> Vec<(String, obs::KeyStats)> {
        obs::stats_snapshot(&self.stats)
    }

    /// 熔断快照。
    pub fn breaker_snapshot(&self) -> Vec<(String, u32, bool)> {
        self.breaker.snapshot()
    }
}

/// 读取出站服务凭据（`[service_auth].outgoing_api_key`，与其他服务侧读取方同键）。
fn load_outgoing_api_key() -> Option<String> {
    let cm = cmx_utils::config::ConfigManager::try_global()?;
    let key = cm.get_string("service_auth.outgoing_api_key").ok()?;
    if key.trim().is_empty() { None } else { Some(key) }
}

static GLOBAL: OnceLock<Arc<ServiceRpcHandle>> = OnceLock::new();

/// 初始化全局基座：加载配置 → fail-fast 校验 → 构造句柄 → 打目录快照。
///
/// 由 `cmx-service-base::init_infra` 末尾自动调用；也可显式调用（幂等：已初始化直接返回）。
/// 服务发现订阅预热用异步版 [`init_and_warm`]。
pub fn init() -> Result<(), ServiceRpcError> {
    if GLOBAL.get().is_some() {
        return Ok(());
    }
    let config = ServiceRpcConfig::try_load()?;
    let directory = ServiceDirectory::new(config.clone());
    directory.validate(registry_enabled())?;
    #[cfg(feature = "http")]
    let handle = ServiceRpcHandle::new(config);
    #[cfg(not(feature = "http"))]
    let handle = ServiceRpcHandle::with_transport(
        config,
        Arc::new(crate::transport::NoopTransport),
    );
    let entries = handle.directory().snapshot_lines();
    tracing::info!(
        services = ?entries,
        "service_rpc 服务目录初始化完成（快照）"
    );
    install(handle)
}

/// 直装全局句柄（嵌入装配 / 集成测试用；正常服务启动走 [`init`]——含校验与预热）。
///
/// 已安装时返回 `Err`（不覆盖——防热替换竞态）。
pub fn install(handle: ServiceRpcHandle) -> Result<(), ServiceRpcError> {
    GLOBAL
        .set(Arc::new(handle))
        .map_err(|_| ServiceRpcError::Unavailable {
            key: "service_rpc".to_string(),
            cause: "全局基座已初始化（不可重复安装）".to_string(),
        })
}

/// 初始化 + 服务发现目标订阅预热（`init_infra` 末尾调用；幂等）。
pub async fn init_and_warm() -> Result<(), ServiceRpcError> {
    init()?;
    if let Some(handle) = GLOBAL.get() {
        let names = handle.directory().discovery_service_names();
        warm_discovery_targets(&names).await;
    }
    Ok(())
}

/// 全局句柄（未初始化返回 `None`）。
pub fn global() -> Option<&'static ServiceRpcHandle> {
    GLOBAL.get().map(Arc::as_ref)
}

/// 全局句柄的 Arc 克隆（契约 SDK 持有句柄构造客户端用）。
pub fn global_arc() -> Option<Arc<ServiceRpcHandle>> {
    GLOBAL.get().cloned()
}

/// 全局句柄（未初始化返回 `Unavailable` 错误并给出初始化提示，不 panic）。
pub fn global_or_err(key: &str) -> Result<&'static ServiceRpcHandle, ServiceRpcError> {
    GLOBAL.get().map(Arc::as_ref).ok_or_else(|| ServiceRpcError::Unavailable {
        key: key.to_string(),
        cause: "service_rpc 基座未初始化（需先执行 cmx_service_base::init_infra / "
            .to_string()
            + "cmx_service_rpc::init）",
    })
}

/// 便捷定位：从全局目录解析服务键的目标定位（反代 / 页面消费方用；
/// 未初始化或键未配置返回 `None`）。
pub fn locator(key: &str) -> Option<Locator> {
    global().and_then(|h| h.directory().locator(key))
}

/// 便捷解析：服务键当前可用的 HTTP 基址。
pub fn resolve_base(key: &str) -> Option<String> {
    global().and_then(|h| h.directory().resolve_base(key))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ServiceEntry;
    use crate::error::ServiceRpcError;
    use crate::invoke::unwrap_envelope;
    use crate::invoke::{Body, HttpMethod};
    use async_trait::async_trait;
    use serde_json::{json, Value};
    

    /// mock 传输：按脚本回放响应 / 错误，记录每次收到的 base 与请求。
    struct MockTransport {
        script: std::sync::Mutex<Vec<Result<RpcResponse, ServiceRpcError>>>,
        seen: std::sync::Mutex<Vec<(String, String, Option<OutgoingHeaders>)>>,
    }

    impl MockTransport {
        fn new(script: Vec<Result<RpcResponse, ServiceRpcError>>) -> Self {
            Self {
                script: std::sync::Mutex::new(script),
                seen: std::sync::Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl Transport for MockTransport {
        async fn execute(
            &self,
            base: &str,
            req: &RpcRequest,
            _timeout: Duration,
            headers: &OutgoingHeaders,
        ) -> Result<RpcResponse, ServiceRpcError> {
            self.seen.lock().unwrap().push((
                base.to_string(),
                req.path.clone(),
                Some(headers.clone()),
            ));
            let mut script = self.script.lock().unwrap();
            if script.is_empty() {
                Err(ServiceRpcError::Unavailable {
                    key: req.key.clone(),
                    cause: "脚本耗尽".to_string(),
                })
            } else {
                script.remove(0)
            }
        }
    }

    fn test_config() -> ServiceRpcConfig {
        let mut cfg = ServiceRpcConfig::default();
        cfg.services.insert(
            "flow".to_string(),
            ServiceEntry {
                url: Some("http://10.0.0.1:8091".to_string()),
                ..Default::default()
            },
        );
        cfg
    }

    fn ok_envelope(data: Value) -> RpcResponse {
        RpcResponse {
            status: 200,
            body: json!({ "code": 0, "msg": "ok", "data": data }),
        }
    }

    /// 成功链路：2xx 信封 code=0 → execute 返回 RpcResponse，unwrap 出 data。
    #[tokio::test]
    async fn execute_success_envelope() {
        let mock = Arc::new(MockTransport::new(vec![Ok(ok_envelope(json!({"id": "i-1"})))]));
        let handle = ServiceRpcHandle::with_transport(test_config(), mock.clone());
        let req = RpcRequest::post("flow", "/api/flow/v1/instances").json_body(json!({"definitionKey": "k"}));
        let resp = handle.execute(req).await.expect("应成功");
        assert_eq!(resp.status, 200);
        let seen = mock.seen.lock().unwrap();
        assert_eq!(seen[0].0, "http://10.0.0.1:8091");
        assert_eq!(seen[0].1, "/api/flow/v1/instances");
    }

    /// 非幂等调用连接级失败不重试；幂等调用在 retry_max 内换实例重试成功。
    #[tokio::test]
    async fn retry_only_idempotent_connect_errors() {
        // 非幂等 POST：单次失败即返回。
        let mock = Arc::new(MockTransport::new(vec![
            Err(ServiceRpcError::Unavailable {
                key: "flow".to_string(),
                cause: "拒连".to_string(),
            }),
        ]));
        let handle = ServiceRpcHandle::with_transport(test_config(), mock.clone());
        let req = RpcRequest::post("flow", "/p");
        let err = handle.execute(req).await.expect_err("应失败");
        assert!(matches!(err, ServiceRpcError::Unavailable { .. }));
        assert_eq!(mock.seen.lock().unwrap().len(), 1);

        // 幂等 GET：第一次连接失败，第二次成功（重试 1 次 = 全局 retry_max）。
        let mock = Arc::new(MockTransport::new(vec![
            Err(ServiceRpcError::Unavailable {
                key: "flow".to_string(),
                cause: "拒连".to_string(),
            }),
            Ok(ok_envelope(json!({}))),
        ]));
        let handle = ServiceRpcHandle::with_transport(test_config(), mock.clone());
        let req = RpcRequest::get("flow", "/p");
        handle.execute(req).await.expect("重试后应成功");
        assert_eq!(mock.seen.lock().unwrap().len(), 2);

        // 超时不重试（连接级才重试）。
        let mock = Arc::new(MockTransport::new(vec![
            Err(ServiceRpcError::Timeout {
                key: "flow".to_string(),
                timeout_ms: 0,
            }),
        ]));
        let handle = ServiceRpcHandle::with_transport(test_config(), mock.clone());
        let req = RpcRequest::get("flow", "/p");
        let err = handle.execute(req).await.expect_err("超时应直接失败");
        assert!(matches!(err, ServiceRpcError::Timeout { .. }));
        assert_eq!(mock.seen.lock().unwrap().len(), 1);
    }

    /// 状态映射：401 → AuthRejected；404 → Remote；2xx 信封 code!=0 由 unwrap 判 Remote。
    #[tokio::test]
    async fn status_error_mapping() {
        let mock = Arc::new(MockTransport::new(vec![Ok(RpcResponse {
            status: 401,
            body: json!({"code": 401, "msg": "未授权"}),
        })]));
        let handle = ServiceRpcHandle::with_transport(test_config(), mock);
        let err = handle
            .execute(RpcRequest::get("flow", "/p"))
            .await
            .expect_err("401 应映射鉴权错误");
        assert!(matches!(err, ServiceRpcError::AuthRejected { .. }));

        let mock = Arc::new(MockTransport::new(vec![Ok(RpcResponse {
            status: 404,
            body: json!({"code": 404, "msg": "不存在"}),
        })]));
        let handle = ServiceRpcHandle::with_transport(test_config(), mock);
        let err = handle
            .execute(RpcRequest::get("flow", "/p"))
            .await
            .expect_err("404 应映射远端错误");
        assert!(matches!(err, ServiceRpcError::Remote { http_status: 404, .. }));

        // 2xx 但信封 code!=0：execute 层不判，unwrap_envelope 层判。
        let mock = Arc::new(MockTransport::new(vec![Ok(RpcResponse {
            status: 200,
            body: json!({ "code": 1, "msg": "业务失败" }),
        })]));
        let handle = ServiceRpcHandle::with_transport(test_config(), mock);
        let resp = handle
            .execute(RpcRequest::get("flow", "/p"))
            .await
            .expect("2xx 传输层成功");
        let err = unwrap_envelope::<Value>("flow", &resp).expect_err("信封 code=1 应失败");
        assert!(matches!(err, ServiceRpcError::Remote { code: 1, .. }));
    }

    /// 熔断联动：连续连接级失败达阈值后，后续调用快速失败（不再触达传输）。
    #[tokio::test]
    async fn breaker_integration() {
        let fails: Vec<Result<RpcResponse, ServiceRpcError>> = (0..10)
            .map(|_| {
                Err(ServiceRpcError::Unavailable {
                    key: "flow".to_string(),
                    cause: "拒连".to_string(),
                })
            })
            .collect();
        let mock = Arc::new(MockTransport::new(fails));
        let handle = ServiceRpcHandle::with_transport(test_config(), mock.clone());
        for _ in 0..5 {
            // 非幂等 POST 不重试，5 次失败正好触达阈值。
            let _ = handle.execute(RpcRequest::post("flow", "/p")).await;
        }
        assert_eq!(mock.seen.lock().unwrap().len(), 5);
        let err = handle
            .execute(RpcRequest::post("flow", "/p"))
            .await
            .expect_err("熔断开放应快速失败");
        assert!(matches!(err, ServiceRpcError::Unavailable { .. }));
        assert_eq!(mock.seen.lock().unwrap().len(), 5, "熔断后不应再触达传输");
    }

    /// 键未配置 / grpc 键走通用通道 → 明确错误。
    #[tokio::test]
    async fn missing_key_and_grpc_binding() {
        let mock = Arc::new(MockTransport::new(vec![]));
        let handle = ServiceRpcHandle::with_transport(test_config(), mock.clone());
        let err = handle
            .execute(RpcRequest::get("nope", "/p"))
            .await
            .expect_err("未配置键应失败");
        assert!(matches!(err, ServiceRpcError::Unavailable { .. }));
        assert_eq!(mock.seen.lock().unwrap().len(), 0);

        let mut cfg = test_config();
        cfg.services.insert(
            "perm".to_string(),
            ServiceEntry {
                discovery: Some("cmx-portal-local".to_string()),
                transport: Some("grpc".to_string()),
                ..Default::default()
            },
        );
        let handle = ServiceRpcHandle::with_transport(cfg, mock);
        let err = handle
            .execute(RpcRequest::get("perm", "/p"))
            .await
            .expect_err("grpc 键走通用通道应 NoBinding");
        assert!(matches!(err, ServiceRpcError::NoBinding { .. }));
    }

    /// 出站头组装：无 ConfigManager / 无请求上下文时三件套缺省省略，extra 透传。
    #[tokio::test]
    async fn outgoing_headers_defaults() {
        let mock = Arc::new(MockTransport::new(vec![Ok(ok_envelope(json!({})))]));
        let handle = ServiceRpcHandle::with_transport(test_config(), mock.clone());
        handle
            .execute(
                RpcRequest::get("flow", "/p").header("x-cmx-flow-signature", "sha256=abc"),
            )
            .await
            .expect("应成功");
        let seen = mock.seen.lock().unwrap();
        let headers = seen[0].2.as_ref().expect("应记录头");
        assert!(headers.api_key.is_none(), "无 ConfigManager 时无 API Key");
        assert!(headers.delegated_token.is_none());
        assert_eq!(
            headers.extra,
            vec![("x-cmx-flow-signature".to_string(), "sha256=abc".to_string())]
        );
    }

    /// Body 形态与请求方法默认幂等标记。
    #[test]
    fn request_builder_defaults() {
        let get = RpcRequest::get("flow", "/p");
        assert_eq!(get.method, HttpMethod::Get);
        assert!(get.idempotent);
        assert!(matches!(get.body, Body::None));

        let post = RpcRequest::post("flow", "/p").json_body(json!({"a": 1}));
        assert_eq!(post.method, HttpMethod::Post);
        assert!(!post.idempotent);
        assert!(matches!(post.body, Body::Json(_)));
        assert!(post.idempotent().idempotent);
    }
}
