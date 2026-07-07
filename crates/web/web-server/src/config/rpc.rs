//! RPC 初始化模块
//!
//! 提供 gRPC 客户端和服务端的初始化功能。

use std::sync::Arc;

use cmx_registry_config::GlobalServiceInstanceCache;
use cmx_rpc::bundle::ServerDeps;
use cmx_rpc::config::RpcConfig;
use cmx_rpc::{init_rpc_clients, start_grpc_server, AuthVerifier};
use cmx_traits::auth::AuthService;
use cmx_traits::function_invoker::FunctionInvoker;
use cmx_traits::resource::ResourceDataImporter;
use cmx_traits::service::ServiceInvoker;
use cmx_utils::ConfigManager;
use serde::Deserialize;
use tracing::{info, warn};

pub use crate::Error;

/// `[service_auth]` 配置段：本服务对外身份。
#[derive(Debug, Clone, Deserialize, Default)]
pub struct ServiceAuthConfig {
    /// 本服务作为调用方时携带的服务级凭证（`cmx_sk_xxx`）。
    /// 留空表示不配置服务身份（仅 loopback/单体无跨服务调用场景）。
    #[serde(default)]
    pub outgoing_api_key: String,
}

/// 初始化 RPC 子系统。
///
/// 当 `rpc.enabled == true` 且 `rpc.protocol == "grpc"` 时：
/// 1. 使用 `GlobalServiceInstanceCache` 获取共享缓存（由 `init_infra` 创建）。
/// 2. 创建 RPC 客户端并注册到全局单例。
/// 3. 在后台 tokio task 中启动 gRPC Server，同步等待启动结果。
/// 4. 执行缓存预热（遍历 `warmup_services` 列表）。
/// 5. 启动服务列表定时同步。
///
/// # Arguments
///
/// * `service_invoker` - 服务调用器。
/// * `function_invoker` - 插件函数调用器（封装 RuntimeInvoker + PluginQuery 完整调用链，
///   由调用方在组装层构造 cmx-biz 的 `BizFunctionInvoker` 实现后注入，使 cmx-rpc
///   无需直接依赖 cmx-biz）。
/// * `data_importer` - 插件数据导入器（可选）。注入后 gRPC 服务端可处理
///   `CmxResourceDataService` 的 import/cleanup 请求。
/// * `auth_service` - 认证服务（用于构造 gRPC `AuthVerifier`，启用服务端鉴权）。
///   `None` 表示不启用 gRPC 鉴权（兼容单体 loopback 场景）。
///
/// # Returns
///
/// * `Ok(Option<u16>)` - RPC 启用且成功时返回 gRPC 端口，否则返回 `None`。
/// * `Err(Error)` - 初始化失败。
pub async fn init_rpc(
    service_invoker: Arc<dyn ServiceInvoker>,
    function_invoker: Arc<dyn FunctionInvoker>,
    data_importer: Option<Arc<dyn ResourceDataImporter>>,
    auth_service: Option<Arc<dyn AuthService>>,
) -> crate::Result<Option<u16>> {
    let rpc_config = load_rpc_config();

    let rpc = match rpc_config {
        Some(cfg) if cfg.enabled && cfg.protocol == "grpc" => cfg,
        Some(cfg) if cfg.enabled => {
            warn!(
                "RPC 已启用但协议 '{}' 暂不支持，跳过 RPC 初始化",
                cfg.protocol
            );
            return Ok(None);
        }
        _ => {
            info!("RPC 未启用，跳过 RPC 初始化");
            return Ok(None);
        }
    };

    info!("初始化 RPC 子系统（gRPC）...");

    // 1. 获取共享缓存（由 init_infra 创建）。
    let cache = GlobalServiceInstanceCache::get().clone();

    // 2. 获取注册中心引用（整函数复用，避免重复 clone）。
    let registry = cmx_registry_config::GlobalServiceRegistry::get().clone();

    // 3. 读取服务身份配置（outgoing_api_key），决定客户端出站是否注入鉴权 header。
    let service_auth = load_service_auth_config();
    let outbound_service_key = if service_auth.outgoing_api_key.is_empty() {
        info!("未配置 [service_auth].outgoing_api_key，RPC 出站不携带服务凭证");
        None
    } else {
        info!("已配置服务对外凭证，RPC 出站将携带服务级 header");
        Some(service_auth.outgoing_api_key)
    };

    // 4. 初始化全部领域客户端（迭代 bundles，注册到领域全局单例）。
    let bundles = init_rpc_clients(&rpc, cache, registry.clone(), outbound_service_key)
        .map_err(|e| Error::ServerSetup(format!("初始化 RPC 客户端失败: {}", e)))?;

    let grpc_port = rpc.grpc.port;

    // 5. 构造服务端鉴权器（若注入了 AuthService）。
    let auth_verifier = auth_service.map(AuthVerifier::new);
    if auth_verifier.is_some() {
        info!("已启用 gRPC 服务端鉴权");
    } else {
        warn!("未注入 AuthService，gRPC 服务端鉴权未启用（兼容单体 loopback 场景）");
    }

    // 6. 在后台 tokio task 中启动 gRPC Server，同步等待启动结果。
    let (server_ready_tx, server_ready_rx) = tokio::sync::oneshot::channel();
    let grpc_port_for_log = grpc_port;
    let deps = ServerDeps {
        service_invoker,
        function_invoker,
        data_importer,
        auth_verifier,
    };
    let server_handle = tokio::spawn(async move {
        info!("在后台启动 gRPC Server，端口: {}", grpc_port_for_log);
        match start_grpc_server(grpc_port_for_log, bundles, deps, server_ready_tx).await {
            Ok(()) => info!("gRPC Server 已正常退出"),
            Err(e) => warn!("gRPC Server 运行失败: {}", e),
        }
    });

    // 等待 Server 启动信号（最多 3 秒）
    match tokio::time::timeout(std::time::Duration::from_secs(3), server_ready_rx).await {
        Ok(Ok(())) => info!("gRPC Server 启动成功"),
        Ok(Err(_)) => {
            server_handle.abort();
            return Err(Error::ServerSetup("gRPC Server 启动失败".to_string()));
        }
        Err(_) => {
            server_handle.abort();
            return Err(Error::ServerSetup("gRPC Server 启动超时".to_string()));
        }
    }

    // 7. 缓存预热：遍历 warmup_services 列表，通过注册中心订阅并缓存。
    //    使用 subscribe_instances 替代手动 query+update，让注册中心层管理缓存。
    if !rpc.warmup_services.is_empty() {
        for service_name in &rpc.warmup_services {
            match registry
                .subscribe_instances(service_name, Arc::new(|_, _| {}))
                .await
            {
                Ok(()) => {
                    info!(service_name = %service_name, "服务预热完成");
                }
                Err(e) => {
                    warn!(service_name = %service_name, error = %e, "服务预热失败");
                }
            }
        }
    }

    info!("RPC 子系统初始化完成，gRPC 端口: {}", grpc_port);

    Ok(Some(grpc_port))
}

/// 从全局配置加载 RPC 配置。
///
/// 使用 `Option` 包裹，因为旧配置文件可能没有 `[rpc]` 段。
pub(crate) fn load_rpc_config() -> Option<RpcConfig> {
    ConfigManager::global().get_as::<RpcConfig>("rpc").ok()
}

/// 从全局配置加载 `[service_auth]` 段。
///
/// 缺省返回空配置（`outgoing_api_key` 为空字符串）。
pub(crate) fn load_service_auth_config() -> ServiceAuthConfig {
    ConfigManager::global()
        .get_as::<ServiceAuthConfig>("service_auth")
        .unwrap_or_default()
}

/// 读取服务对外凭证（供 web-server 其他装配点使用，如 HTTP 出站注入）。
///
/// 返回 `Credential`（服务级 API Key，统一走 `X-API-Key`）；
/// 未配置时返回 `None`。
pub(crate) fn load_outgoing_credential() -> Option<cmx_plugin::service::remote_importers::Credential> {
    let cfg = load_service_auth_config();
    if cfg.outgoing_api_key.is_empty() {
        None
    } else {
        Some(cmx_plugin::service::remote_importers::Credential {
            value: cfg.outgoing_api_key,
        })
    }
}
