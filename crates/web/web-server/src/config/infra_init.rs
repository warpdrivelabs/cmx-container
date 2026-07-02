//! 基础设施初始化模块。
//!
//! 通过 `cmx-registry-config` crate 的 trait 抽象初始化注册中心和配置中心，
//! 替代直接使用 `NacosClient` 的硬编码方式。
//!
//! 启动流程：
//! 1. 从环境变量加载注册中心/配置中心配置。
//! 2. 通过工厂函数创建实例并存储到全局单例。
//! 3. 合并多源配置（本地 TOML + 远程配置 + 环境变量）初始化 `ConfigManager`。
//! 4. 向注册中心注册当前服务实例。
//! 5. 注册配置变更监听器，开启配置热更新。

use std::sync::Arc;

use cmx_registry_config::{
    ConfigCenterFullConfig, ConfigChangeCallback, ConfigChangeEvent, ConfigReloader,
    GlobalChangeNotifier, GlobalConfigCenter, GlobalServiceInstanceCache, GlobalServiceRegistry,
    RegistryConfig, RemoteConfigSource, ServiceInstanceCache, ServiceListSyncer, ServiceRegistry,
    create_config_center, create_registry_with_cache,
};
use cmx_utils::{ConfigBuilder, ConfigManager};
use tokio::sync::watch;
use tracing::{info, warn};

pub use crate::Error;

/// 全局服务列表同步器的 shutdown 信号发送端。
///
/// 在 `start_service_list_syncer` 中创建，在 `shutdown_infra` 中发送 `true` 优雅停止同步器。
static SYNCER_SHUTDOWN: std::sync::OnceLock<watch::Sender<bool>> = std::sync::OnceLock::new();

/// 初始化基础设施（注册中心 + 配置中心 + 全局配置）。
///
/// 该函数是 `main.rs` 启动后第一个被调用的初始化入口，负责：
/// - 加载并合并多源配置
/// - 创建并存储注册中心、配置中心实例
/// - 注册服务实例到注册中心
/// - 启动配置变更监听
///
/// # Returns
///
/// * `Ok(())` - 全部初始化成功。
/// * `Err(Error::ConfigError)` - 配置加载、合并或单例设置失败。
pub async fn init_infra() -> crate::Result<()> {
    info!("加载环境变量和配置文件信息...");

    // 从环境变量构建注册中心与配置中心的配置。
    let registry_config = RegistryConfig::from_env();
    let cc_config = ConfigCenterFullConfig::from_env();

    // 通过工厂函数创建注册中心（带缓存）和配置中心实例。
    let (registry, cache) = create_registry_with_cache(&registry_config)
        .await
        .map_err(|e| Error::ConfigError(format!("创建注册中心失败: {}", e)))?;
    GlobalServiceInstanceCache::set(cache)
        .map_err(|e| Error::ConfigError(format!("设置全局服务实例缓存失败: {}", e)))?;

    // 初始化全局配置变更通知器（必须在创建配置中心前完成，以便 change_handler 注册到通知器）。
    GlobalChangeNotifier::initialize();

    // 构造配置变更处理器：解析新配置 → 合并 → 原子替换全局 ConfigManager → 通知结构化监听器。
    // 该处理器由 create_config_center 自动注册到每个 listener，配置变更时由 SDK 直接回调。
    let change_handler = build_config_change_handler();

    let config_center = create_config_center(&cc_config, change_handler)
        .await
        .map_err(|e| Error::ConfigError(format!("创建配置中心失败: {}", e)))?;

    // 构建配置（本地 TOML + 远程配置中心 + 环境变量）。
    let mut builder = ConfigBuilder::new().add_toml_file_from_env("CONFIG_FILE");

    if cc_config.enabled
        && let Some(listener) = cc_config.listeners.first()
    {
        // 配置中心启用时，尝试拉取首个 listener 的远程配置作为初始配置源。
        match config_center
            .get_config(&listener.data_id, &listener.group)
            .await
        {
            Ok(content) => {
                info!(
                    "成功从配置中心拉取远程配置: {}/{}",
                    listener.group, listener.data_id
                );
                match RemoteConfigSource::from_toml_str(&content) {
                    Ok(source) => {
                        builder = builder.add_source(source);
                    }
                    Err(e) => {
                        warn!("远程配置 TOML 解析失败: {}，跳过远程配置", e);
                    }
                }
            }
            Err(e) => {
                warn!("从配置中心拉取远程配置失败: {}，使用本地配置", e);
            }
        }
    }

    // 环境变量优先级最高，最后叠加。
    builder = builder.add_env();
    let final_config = builder
        .build()
        .map_err(|e| Error::ConfigError(format!("配置构建失败: {}", e)))?;
    ConfigManager::initialize(|| Ok::<_, cmx_utils::ConfigError>(final_config))
        .map_err(|e| Error::ConfigError(format!("配置管理器初始化失败: {}", e)))?;

    // 存储到全局单例（其他 crate 可通过 `GlobalServiceRegistry::get()` / `GlobalConfigCenter::get()` 访问）。
    GlobalServiceRegistry::set(registry)
        .map_err(|_| Error::ConfigError("注册中心全局单例已设置".to_string()))?;
    GlobalConfigCenter::set(config_center)
        .map_err(|_| Error::ConfigError("配置中心全局单例已设置".to_string()))?;

    info!("配置初始化完成");

    // 注册服务到注册中心。
    let registry = GlobalServiceRegistry::get();
    register_service(registry).await;

    // 启动服务列表定时同步（注册中心基础设施职责，不依赖 RPC 是否启用）。
    start_service_list_syncer().await;

    // 配置变更监听已在 create_config_center 时通过 change_handler 自动注册，
    // 业务模块可通过 GlobalChangeNotifier::add_listener() 注册结构化监听器。

    // 输出当前所有配置项（调试用，过滤掉 Path）。
    for key in ConfigManager::global().keys() {
        if key == "Path" {
            continue;
        }
        info!("{:?}: {:?}", key, ConfigManager::global().get(&key));
    }

    Ok(())
}

/// 注册服务到注册中心。
///
/// 仅在注册中心启用时执行；解析 IP/Port 时优先级：
/// `SERVICE_REGISTRY_*` 环境变量 > 兼容 `NACOS_REGISTER_SERVER_*` > 全局配置 > 自动检测。
///
/// # Arguments
///
/// * `registry` - 全局注册中心实例。
async fn register_service(registry: &Arc<dyn ServiceRegistry>) {
    if !registry.is_enabled() {
        info!("服务注册未启用，跳过服务注册");
        return;
    }

    let port = resolve_register_port();
    let ip = resolve_register_ip();

    let mut registry_config = RegistryConfig::from_env();
    inject_rpc_metadata(&mut registry_config);

    let instance = registry_config.build_instance(ip.clone(), port);

    match registry.register(&instance).await {
        Ok(_) => {
            info!("服务实例已注册: {}:{}", ip, port);
        }
        Err(e) => {
            warn!("服务注册失败: {}，服务仍可正常运行", e);
        }
    }
}

/// 启动服务列表定时同步器。
///
/// 以固定 30s 间隔在后台启动 `ServiceListSyncer`，
/// 定期从注册中心拉取服务列表并自动订阅新服务。
///
/// 这是注册中心基础设施职责，不依赖 RPC 是否启用。即使 RPC 未启用，
/// 其他需要服务发现的模块也能从缓存中获取实例信息。
///
/// 同步器通过 `watch::channel` 接收 shutdown 信号，在 `shutdown_infra` 时优雅停止。
async fn start_service_list_syncer() {
    let registry = GlobalServiceRegistry::get();

    // 注册中心未启用（使用 MockRegistry）时跳过定时同步，避免无意义的轮询。
    if !registry.is_enabled() {
        info!("注册中心未启用，跳过服务列表定时同步");
        return;
    }

    // 服务列表定时同步间隔固定为 30s，不通过配置控制。
    const SYNC_INTERVAL_SECS: u64 = 30;

    let registry = registry.clone();
    let cache: Arc<ServiceInstanceCache> = GlobalServiceInstanceCache::get().clone();
    let syncer = Arc::new(ServiceListSyncer::new(registry, cache, SYNC_INTERVAL_SECS));

    // 创建 shutdown 信号通道，发送端存入全局 OnceLock 供 shutdown_infra 使用。
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    if SYNCER_SHUTDOWN.set(shutdown_tx).is_err() {
        warn!("服务列表同步器已启动，跳过重复启动");
        return;
    }

    tokio::spawn(async move {
        info!("启动服务列表定时同步，间隔: {}s", SYNC_INTERVAL_SECS);
        syncer.run(shutdown_rx).await;
        info!("服务列表定时同步已停止");
    });
}

/// 将 RPC 相关信息注入到 RegistryConfig 的 metadata 中。
///
/// 从 RPC 配置中自动读取 `grpc_port` 等信息，注入到 `registry_config.metadata`，
/// 确保注册和注销时 metadata 保持一致。
///
/// 同时从全局配置加载 `[registry.metadata]` 段的用户自定义 metadata，
/// 合并到 `registry_config.metadata` 中（RPC 自动注入的 key 优先级更高）。
fn inject_rpc_metadata(registry_config: &mut RegistryConfig) {
    // 从配置文件加载用户自定义 metadata
    if let Ok(custom_meta) = ConfigManager::global()
        .get_as::<std::collections::HashMap<String, String>>("registry.metadata")
    {
        for (k, v) in custom_meta {
            registry_config.metadata.entry(k).or_insert(v);
        }
    }

    // RPC 自动注入的 metadata 优先级高于配置文件中的值
    if let Some(rpc) = super::rpc::load_rpc_config()
        && rpc.enabled
    {
        registry_config
            .metadata
            .insert("grpc_port".to_string(), rpc.grpc.port.to_string());
    }
}

/// 构造配置变更处理器。
///
/// 返回的回调负责：
/// 1. 解析新配置内容（原始 TOML 字符串）。
/// 2. 原子替换全局 `ConfigManager`。
/// 3. 通过 `GlobalChangeNotifier::notify_listeners` 通知结构化监听器。
///
/// 回调内部使用 `tokio::spawn` 异步执行 reload，避免阻塞 Nacos 监听线程。
/// 环境变量优先级由 reload 时的 `add_env()` 保持，配置变更不会影响该优先级。
fn build_config_change_handler() -> Option<ConfigChangeCallback> {
    let config_file_path = std::env::var("CONFIG_FILE").ok();
    let reloader = Arc::new(ConfigReloader::new(config_file_path));
    Some(Arc::new(move |content: &str| {
        let reloader = reloader.clone();
        let content = content.to_string();
        // 使用 tokio::spawn 异步执行 reload，避免阻塞 Nacos 监听线程。
        // ConfigReloader::reload 内部使用 tokio::sync::Mutex 串行化执行，
        // 避免配置变更频繁时多个 reload task 并发执行导致 changed_keys 漏报。
        tokio::spawn(async move {
            match reloader.reload(&content).await {
                Ok(changed_keys) => {
                    let event = ConfigChangeEvent {
                        changed_keys,
                        raw_content: content,
                    };
                    // 通知结构化监听器。
                    GlobalChangeNotifier::notify_listeners(&event);
                }
                Err(e) => {
                    warn!("配置热更新失败: {}，保留当前配置", e);
                }
            }
        });
    }))
}

/// 优雅关闭：从注册中心注销服务实例。
///
/// 应在应用退出时调用，确保 Nacos 等注册中心能及时感知实例下线。
/// 同时发送 shutdown 信号停止服务列表定时同步器。
pub async fn shutdown_infra() {
    // 先停止服务列表定时同步器，避免注销后同步器仍尝试拉取已下线实例。
    if let Some(tx) = SYNCER_SHUTDOWN.get() {
        let _ = tx.send(true);
        info!("已发送服务列表同步器停止信号");
    }

    if !GlobalServiceRegistry::is_initialized() {
        return;
    }
    let registry = GlobalServiceRegistry::get();
    if !registry.is_enabled() {
        return;
    }

    let port = resolve_register_port();
    let ip = resolve_register_ip();

    let mut registry_config = RegistryConfig::from_env();
    inject_rpc_metadata(&mut registry_config);

    let instance = registry_config.build_instance(ip.clone(), port);

    match registry.deregister(&instance).await {
        Ok(_) => {
            info!("服务实例已注销: {}:{}", ip, port);
        }
        Err(e) => {
            warn!("服务注销失败: {}", e);
        }
    }
}

/// 解析注册使用的服务端口。
///
/// # Returns
///
/// * `SERVICE_REGISTRY_PORT` 环境变量（若为有效 u16 > 0）。
/// * `NACOS_REGISTER_SERVER_PORT` 兼容变量。
/// * 全局配置中的 `server.port`。
/// * 默认值 `8080`。
///
/// 优先级：`SERVICE_REGISTRY_PORT` > `NACOS_REGISTER_SERVER_PORT` > `server.port` > `8080`。
fn resolve_register_port() -> u16 {
    // 优先读取 SERVICE_REGISTRY_PORT。
    if let Ok(raw) = std::env::var("SERVICE_REGISTRY_PORT") {
        if let Ok(port) = raw.trim().parse::<u16>()
            && port > 0
        {
            return port;
        }
        warn!(
            "SERVICE_REGISTRY_PORT={} 不是有效的端口号，将回退到其他来源",
            raw
        );
    }

    // 兼容旧 NACOS_REGISTER_SERVER_PORT。
    if let Ok(raw) = std::env::var("NACOS_REGISTER_SERVER_PORT") {
        if let Ok(port) = raw.trim().parse::<u16>()
            && port > 0
        {
            return port;
        }
        warn!(
            "NACOS_REGISTER_SERVER_PORT={} 不是有效的端口号，将回退到其他来源",
            raw
        );
    }

    // 从全局配置中读取 server.port。
    ConfigManager::global()
        .get_string("server.port")
        .ok()
        .and_then(|s| s.trim().parse::<u16>().ok())
        .filter(|p| *p > 0)
        .unwrap_or(8080)
}

/// 解析注册使用的服务 IP。
///
/// # Returns
///
/// * `SERVICE_REGISTRY_IP` 环境变量。
/// * `NACOS_REGISTER_SERVER_IP` 兼容变量。
/// * 全局配置中的 `server.ip`。
/// * 通过 `local_ip_address` 自动检测的本机 IP。
/// * 默认值 `127.0.0.1`。
///
/// 优先级：`SERVICE_REGISTRY_IP` > `NACOS_REGISTER_SERVER_IP` > `server.ip` > 自动检测 > `127.0.0.1`。
fn resolve_register_ip() -> String {
    // 优先读取 SERVICE_REGISTRY_IP。
    if let Ok(raw) = std::env::var("SERVICE_REGISTRY_IP") {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }

    // 兼容旧 NACOS_REGISTER_SERVER_IP。
    if let Ok(raw) = std::env::var("NACOS_REGISTER_SERVER_IP") {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }

    // 从全局配置中读取 server.ip。
    ConfigManager::global()
        .get_string("server.ip")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| {
            local_ip_address::local_ip()
                .map(|addr| addr.to_string())
                .unwrap_or_else(|_| "127.0.0.1".to_string())
        })
}
