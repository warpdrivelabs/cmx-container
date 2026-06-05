//! 基础设施初始化模块
//!
//! 通过 cmx-registry-config crate 的 trait 抽象初始化注册中心和配置中心，
//! 替代直接使用 NacosClient 的硬编码方式。

use std::sync::Arc;

use cmx_registry_config::{
    create_config_center, create_registry, ConfigCenter, ConfigCenterFullConfig,
    GlobalChangeNotifier, GlobalConfigCenter, GlobalRegistry, RegistryConfig, RemoteConfigSource,
    ServiceInstance, ServiceRegistry,
};
use cmx_utils::{ConfigBuilder, ConfigManager};
use tracing::{info, warn};

pub use crate::Error;

/// 初始化基础设施（注册中心 + 配置中心 + 全局配置）
pub async fn init_infra() -> crate::Result<()> {
    info!("加载环境变量和配置文件信息...");

    let registry_config = RegistryConfig::from_env();
    let cc_config = ConfigCenterFullConfig::from_env();

    // 创建注册中心和配置中心实例
    let registry = create_registry(&registry_config).map_err(|e| {
        Error::ConfigError(format!("创建注册中心失败: {}", e))
    })?;
    let config_center = create_config_center(&cc_config).map_err(|e| {
        Error::ConfigError(format!("创建配置中心失败: {}", e))
    })?;

    // 构建配置（本地 TOML + 远程配置中心 + 环境变量）
    let mut builder = ConfigBuilder::new().add_toml_file_from_env("CONFIG_FILE");

    if cc_config.enabled
        && let Some(listener) = cc_config.listeners.first()
    {
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

    builder = builder.add_env();
    let final_config = builder
        .build()
        .map_err(|e| Error::ConfigError(format!("配置构建失败: {}", e)))?;
    ConfigManager::initialize(|| Ok::<_, cmx_utils::ConfigError>(final_config))
        .map_err(|e| Error::ConfigError(format!("配置管理器初始化失败: {}", e)))?;

    // 存储到全局单例（其他 crate 可通过 GlobalRegistry::get() / GlobalConfigCenter::get() 访问）
    GlobalRegistry::set(registry).map_err(|_| {
        Error::ConfigError("注册中心全局单例已设置".to_string())
    })?;
    GlobalConfigCenter::set(config_center).map_err(|_| {
        Error::ConfigError("配置中心全局单例已设置".to_string())
    })?;

    info!("配置初始化完成");

    // 注册服务
    let registry = GlobalRegistry::get();
    register_service(registry).await;

    // 设置配置监听
    let config_center = GlobalConfigCenter::get();
    setup_config_listener(config_center, &cc_config).await;

    for key in ConfigManager::global().keys() {
        if key == "Path" {
            continue;
        }
        info!("{:?}: {:?}", key, ConfigManager::global().get(&key));
    }

    Ok(())
}

/// 注册服务到注册中心
async fn register_service(registry: &Arc<dyn ServiceRegistry>) {
    if !registry.is_enabled() {
        info!("服务注册未启用，跳过服务注册");
        return;
    }

    let port = resolve_register_port();
    let ip = resolve_register_ip();

    let registry_config = RegistryConfig::from_env();
    let instance = ServiceInstance {
        ip: ip.clone(),
        port,
        service_name: registry_config.service_name(),
        group_name: Some(registry_config.nacos.group_name.clone()),
        cluster_name: Some(registry_config.nacos.cluster_name.clone()),
        weight: registry_config.nacos.weight,
        metadata: registry_config.nacos.metadata.clone(),
        healthy: true,
        ephemeral: true,
    };

    match registry.register(&instance).await {
        Ok(_) => {
            info!("服务实例已注册: {}:{}", ip, port);
        }
        Err(e) => {
            warn!("服务注册失败: {}，服务仍可正常运行", e);
        }
    }
}

/// 设置配置变更监听
async fn setup_config_listener(
    config_center: &Arc<dyn ConfigCenter>,
    cc_config: &ConfigCenterFullConfig,
) {
    if !config_center.is_enabled() {
        info!("配置中心未启用，跳过配置监听注册");
        return;
    }

    GlobalChangeNotifier::initialize();

    let callback = Arc::new(|content: &str| {
        GlobalChangeNotifier::notify(content);
    });

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

/// 优雅关闭：从注册中心注销服务实例
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
    let instance = ServiceInstance {
        ip: ip.clone(),
        port,
        service_name: registry_config.service_name(),
        group_name: Some(registry_config.nacos.group_name.clone()),
        cluster_name: None,
        weight: 1.0,
        metadata: Default::default(),
        healthy: true,
        ephemeral: true,
    };

    match registry.deregister(&instance).await {
        Ok(_) => {
            info!("服务实例已注销: {}:{}", ip, port);
        }
        Err(e) => {
            warn!("服务注销失败: {}", e);
        }
    }
}

/// 解析注册使用的服务端口
fn resolve_register_port() -> u16 {
    if let Ok(raw) = std::env::var("NACOS_REGISTER_SERVER_PORT") {
        if let Ok(port) = raw.trim().parse::<u16>() {
            if port > 0 {
                return port;
            }
        }
        warn!(
            "NACOS_REGISTER_SERVER_PORT={} 不是有效的端口号，将回退到其他来源",
            raw
        );
    }

    ConfigManager::global()
        .get_string("server.port")
        .ok()
        .and_then(|s| s.trim().parse::<u16>().ok())
        .filter(|p| *p > 0)
        .unwrap_or(8080)
}

/// 解析注册使用的服务 IP
fn resolve_register_ip() -> String {
    if let Ok(raw) = std::env::var("NACOS_REGISTER_SERVER_IP") {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }

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
