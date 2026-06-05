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
    create_config_center, create_registry, ConfigCenter, ConfigCenterFullConfig,
    ConfigChangeEvent, ConfigReloader, GlobalChangeNotifier, GlobalConfigCenter, GlobalRegistry,
    RegistryConfig, RemoteConfigSource, ServiceRegistry,
};
use cmx_utils::{ConfigBuilder, ConfigManager};
use tracing::{info, warn};

pub use crate::Error;

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

    // 通过工厂函数创建注册中心和配置中心实例。
    let registry = create_registry(&registry_config).await.map_err(|e| {
        Error::ConfigError(format!("创建注册中心失败: {}", e))
    })?;
    let config_center = create_config_center(&cc_config).await.map_err(|e| {
        Error::ConfigError(format!("创建配置中心失败: {}", e))
    })?;

    // 构建配置（本地 TOML + 远程配置中心 + 环境变量）。
    let mut builder = ConfigBuilder::new().add_toml_file_from_env("CONFIG_FILE");

    if cc_config.enabled
        && let Some(listener) = cc_config.listeners.first()
    {
        // 配置中心启用时，尝试拉取首个 listener 的远程配置作为初始配置源。
        match config_center.get_config(&listener.data_id, &listener.group).await {
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

    // 存储到全局单例（其他 crate 可通过 `GlobalRegistry::get()` / `GlobalConfigCenter::get()` 访问）。
    GlobalRegistry::set(registry).map_err(|_| {
        Error::ConfigError("注册中心全局单例已设置".to_string())
    })?;
    GlobalConfigCenter::set(config_center).map_err(|_| {
        Error::ConfigError("配置中心全局单例已设置".to_string())
    })?;

    info!("配置初始化完成");

    // 注册服务到注册中心。
    let registry = GlobalRegistry::get();
    register_service(registry).await;

    // 设置配置变更监听，启动热更新。
    let config_center = GlobalConfigCenter::get();
    setup_config_listener(config_center, &cc_config).await;

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

    let registry_config = RegistryConfig::from_env();
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

/// 设置配置变更监听。
///
/// 注册到全局配置变更通知器（`GlobalChangeNotifier`）的处理器包括：
///
/// 1. **配置重载器**（key = `"config_reloader"`）：解析新配置并原子替换全局 `ConfigManager`，
///    之后通过 `notify_listeners()` 通知结构化监听器。
/// 2. **业务监听器**：其他模块可通过 `GlobalChangeNotifier::add_listener()` 注册。
///
/// 收到远程配置变更时，处理器按注册顺序被调用。
/// 环境变量优先级由 reload 时的 `add_env()` 保持，配置变更不会影响该优先级。
///
/// # Arguments
///
/// * `config_center` - 全局配置中心实例。
/// * `cc_config` - 配置中心配置（含 listeners 列表）。
async fn setup_config_listener(
    config_center: &Arc<dyn ConfigCenter>,
    cc_config: &ConfigCenterFullConfig,
) {
    if !config_center.is_enabled() {
        info!("配置中心未启用，跳过配置监听注册");
        return;
    }

    GlobalChangeNotifier::initialize();

    // 注册配置重载器：解析新配置 → 合并 → 原子替换全局 ConfigManager → 通知监听器。
    let config_file_path = std::env::var("CONFIG_FILE").ok();
    let reloader = Arc::new(ConfigReloader::new(config_file_path));
    GlobalChangeNotifier::register("config_reloader", {
        let reloader = reloader.clone();
        Arc::new(move |content: &str| {
            let reloader = reloader.clone();
            let content = content.to_string();
            // 使用 tokio::spawn 异步执行 reload，避免阻塞 Nacos 监听线程。
            tokio::spawn(async move {
                match reloader.reload(&content) {
                    Ok(changed_keys) => {
                        let event = ConfigChangeEvent {
                            changed_keys,
                            raw_content: content,
                        };
                        // 通知结构化监听器（仅 typed listener）。
                        GlobalChangeNotifier::notify_listeners(&event);
                    }
                    Err(e) => {
                        warn!("配置热更新失败: {}，保留当前配置", e);
                    }
                }
            });
        })
    });

    // 将 SDK 推送的变更转发到全局通知器，触发第一轮 handlers。
    let callback: cmx_registry_config::ConfigChangeCallback = Arc::new(|content: &str| {
        GlobalChangeNotifier::notify(content);
    });

    // 为配置中心中配置的每个 listener 注册 SDK 级别的监听。
    for listener in &cc_config.listeners {
        match config_center
            .listen(&listener.data_id, &listener.group, callback.clone())
            .await
        {
            Ok(_) => {
                info!(
                    "已注册配置变更监听: {}/{}",
                    listener.group, listener.data_id
                );
            }
            Err(e) => {
                warn!(
                    "注册配置变更监听失败 [{}/{}]: {}",
                    listener.group, listener.data_id, e
                );
            }
        }
    }
}

/// 优雅关闭：从注册中心注销服务实例。
///
/// 应在应用退出时调用，确保 Nacos 等注册中心能及时感知实例下线。
pub async fn shutdown_infra() {
    if !GlobalRegistry::is_initialized() {
        return;
    }
    let registry = GlobalRegistry::get();
    if !registry.is_enabled() {
        return;
    }

    let port = resolve_register_port();
    let ip = resolve_register_ip();

    let registry_config = RegistryConfig::from_env();
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
        if let Ok(port) = raw.trim().parse::<u16>() {
            if port > 0 {
                return port;
            }
        }
        warn!("SERVICE_REGISTRY_PORT={} 不是有效的端口号，将回退到其他来源", raw);
    }

    // 兼容旧 NACOS_REGISTER_SERVER_PORT。
    if let Ok(raw) = std::env::var("NACOS_REGISTER_SERVER_PORT") {
        if let Ok(port) = raw.trim().parse::<u16>() {
            if port > 0 {
                return port;
            }
        }
        warn!("NACOS_REGISTER_SERVER_PORT={} 不是有效的端口号，将回退到其他来源", raw);
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
