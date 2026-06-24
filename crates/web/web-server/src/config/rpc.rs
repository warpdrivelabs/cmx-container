//! RPC 初始化模块
//!
//! 提供 gRPC 客户端和服务端的初始化功能。

use std::sync::Arc;

use cmx_registry_config::GlobalServiceInstanceCache;
use cmx_rpc::{create_rpc_client, start_grpc_server, GlobalRpcClient};
use cmx_rpc::config::RpcConfig;
use cmx_traits::plugin::{PluginDataImporter, PluginQuery};
use cmx_traits::runtime::RuntimeInvoker;
use cmx_traits::service::ServiceInvoker;
use cmx_utils::ConfigManager;
use tracing::{info, warn};

pub use crate::Error;

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
/// * `runtime_invoker` - 运行时调用器。
/// * `plugin_query` - 插件查询器。
/// * `data_importer` - 插件数据导入器（可选）。注入后 gRPC 服务端可处理
///   `CmxPluginDataService` 的 import/cleanup 请求。
///
/// # Returns
///
/// * `Ok(Option<u16>)` - RPC 启用且成功时返回 gRPC 端口，否则返回 `None`。
/// * `Err(Error)` - 初始化失败。
pub async fn init_rpc(
    service_invoker: Arc<dyn ServiceInvoker>,
    runtime_invoker: Arc<dyn RuntimeInvoker>,
    plugin_query: Arc<dyn PluginQuery>,
    data_importer: Option<Arc<dyn PluginDataImporter>>,
) -> crate::Result<Option<u16>> {
    let rpc_config = load_rpc_config();

    let rpc = match rpc_config {
        Some(cfg) if cfg.enabled && cfg.protocol == "grpc" => cfg,
        Some(cfg) if cfg.enabled => {
            warn!("RPC 已启用但协议 '{}' 暂不支持，跳过 RPC 初始化", cfg.protocol);
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

    // 3. 创建 RPC 客户端并注册到全局单例。
    let rpc_client = create_rpc_client(&rpc, cache, registry.clone())
        .map_err(|e| Error::ServerSetup(format!("创建 RPC 客户端失败: {}", e)))?;
    GlobalRpcClient::set(rpc_client)
        .map_err(|e| Error::ServerSetup(format!("设置全局 RPC 客户端失败: {}", e)))?;

    let grpc_port = rpc.grpc.port;

    // 4. 在后台 tokio task 中启动 gRPC Server，同步等待启动结果。
    let (server_ready_tx, server_ready_rx) = tokio::sync::oneshot::channel();
    let grpc_port_for_log = grpc_port;
    let server_handle = tokio::spawn(async move {
        info!("在后台启动 gRPC Server，端口: {}", grpc_port_for_log);
        match start_grpc_server(grpc_port_for_log, service_invoker, runtime_invoker, plugin_query, data_importer, server_ready_tx).await {
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

    // 5. 缓存预热：遍历 warmup_services 列表，通过注册中心订阅并缓存。
    //    使用 subscribe_instances 替代手动 query+update，让注册中心层管理缓存。
    if !rpc.warmup_services.is_empty() {
        for service_name in &rpc.warmup_services {
            match registry.subscribe_instances(service_name, Arc::new(|_, _| {})).await {
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
    ConfigManager::global()
        .get_as::<RpcConfig>("rpc")
        .ok()
}
