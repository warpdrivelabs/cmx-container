//! Nacos 配置中心模块
//!
//! 提供 Nacos 连接、远程配置拉取、服务注册和配置变更监听功能。

use cmx_nacos::{GlobalConfigChangeNotifier, NacosClient, NacosConfig, RemoteConfigChangeListener};
use cmx_utils::{ConfigBuilder, ConfigManager};
use std::sync::{Arc, OnceLock};
use tracing::{info, warn};

pub use crate::Error;

static GLOBAL_NACOS_CLIENT: OnceLock<NacosClient> = OnceLock::new();

fn store_nacos_client(client: NacosClient) {
    let _ = GLOBAL_NACOS_CLIENT.set(client);
}

#[allow(dead_code)]
pub fn get_nacos_client() -> Option<&'static NacosClient> {
    GLOBAL_NACOS_CLIENT.get()
}

pub async fn init_global_config_with_nacos() -> crate::Result<()> {
    info!("加载环境变量和配置文件信息...");

    let nacos_config = NacosConfig::from_env();

    if !nacos_config.enabled {
        info!("Nacos 未启用（NACOS_ENABLED 未设置或为 false），使用本地配置");
        init_global_config_fallback()?;
        return Ok(());
    }

    if let Err(e) = ConfigBuilder::new()
        .add_toml_file_from_env("CONFIG_FILE")
        .build()
    {
        return Err(Error::ConfigError(format!("初始配置加载失败: {}", e)));
    }

    match NacosClient::new(nacos_config.clone()).await {
        Ok(client) => {
            let mut builder = ConfigBuilder::new()
                .add_toml_file_from_env("CONFIG_FILE");

            if nacos_config.config.enabled
                && let Some(listener) = nacos_config.config.listeners.first()
            {
                match client.get_config_source(&listener.data_id, &listener.group).await {
                    Ok(source) => {
                        info!(
                            "成功从 Nacos 拉取远程配置: {}/{}",
                            listener.group, listener.data_id
                        );
                        builder = builder.add_source(source);
                    }
                    Err(e) => {
                        warn!("从 Nacos 拉取远程配置失败: {}，使用本地配置", e);
                    }
                }
            }

            builder = builder.add_env();
            let final_config = builder.build().map_err(|e| {
                Error::ConfigError(format!("配置构建失败: {}", e))
            })?;
            ConfigManager::initialize(|| Ok::<_, cmx_utils::ConfigError>(final_config))
                .map_err(|e| Error::ConfigError(format!("配置管理器初始化失败: {}", e)))?;

            store_nacos_client(client);
            info!("配置初始化完成（含 Nacos 远程配置覆盖）");

            if let Some(client) = get_nacos_client() {
                register_nacos_service(client).await;
            }

            if let Some(client) = get_nacos_client() {
                setup_config_listener(client, &nacos_config).await;
            }
        }
        Err(e) => {
            warn!("Nacos 客户端初始化失败: {}，回退到本地配置", e);
            init_global_config_fallback()?;
        }
    }

    for key in ConfigManager::global().keys() {
        if "Path" == key {
            continue;
        }
        info!("{:?}: {:?}", key, ConfigManager::global().get(&key));
    }

    Ok(())
}

fn init_global_config_fallback() -> crate::Result<()> {
    // 收敛到 cmx-service-base 的共享装配（**所有能力中心同一段代码**：CONFIG_FILE toml + env
    // → ConfigManager::global()）。flow/report/mdm 亦调它，不再各写一套。Nacos 启用时上面的
    // 分支在此之上叠加远程源；未启用即走本函数。
    cmx_service_base::init_config_manager()
        .map_err(|e| Error::ConfigError(format!("本地配置加载失败: {}", e)))?;
    Ok(())
}

/// 解析注册到 Nacos 使用的服务端口。
///
/// 按以下顺序回退解析：
/// 1. 环境变量 `NACOS_REGISTER_SERVER_PORT`，需为 1-65535 的合法 `u16`，否则记录警告后回退。
/// 2. 配置文件中的 `server.port`。
/// 3. 内置默认值 `8080`。
///
/// # Returns
///
/// 返回最终解析到的端口号，永远不会返回 0。
fn resolve_register_port() -> u16 {
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

    ConfigManager::global()
        .get_string("server.port")
        .ok()
        .and_then(|s| s.trim().parse::<u16>().ok())
        .filter(|p| *p > 0)
        .unwrap_or(8080)
}

/// 解析注册到 Nacos 使用的服务 IP。
///
/// 按以下顺序回退解析：
/// 1. 环境变量 `NACOS_REGISTER_SERVER_IP`，去除首尾空格后非空即使用。
/// 2. 配置文件中的 `server.ip`，去除首尾空格后非空即使用。
/// 3. 自动获取本机 IP（兜底 `127.0.0.1`）。
///
/// # Returns
///
/// 返回最终解析到的 IP 地址字符串。
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

/// 注册服务到 Nacos 命名服务。
///
/// 从配置中读取服务端口，将当前服务实例注册到 Nacos。
/// 注册失败时仅记录警告，不阻止启动。
///
/// # Arguments
///
/// * `client` - NacosClient 实例
async fn register_nacos_service(client: &NacosClient) {
    if !client.is_naming_enabled() {
        info!("Nacos 命名服务未启用，跳过服务注册");
        return;
    }

    let port = resolve_register_port();
    let ip = resolve_register_ip();

    match client.register_service(&ip, port).await {
        Ok(_) => {
            info!("服务实例已注册到 Nacos: {}:{}", ip, port);
        }
        Err(e) => {
            warn!("服务注册到 Nacos 失败: {}，服务仍可正常运行", e);
        }
    }
}

async fn setup_config_listener(client: &NacosClient, nacos_config: &NacosConfig) {
    if !client.is_config_enabled() {
        info!("Nacos 配置中心未启用，跳过配置监听注册");
        return;
    }

    GlobalConfigChangeNotifier::initialize();

    let listener = Arc::new(RemoteConfigChangeListener);

    for config_listener in &nacos_config.config.listeners {
        match client
            .listen_config(&config_listener.data_id, &config_listener.group, listener.clone())
            .await
        {
            Ok(_) => {
                info!(
                    "已注册 Nacos 配置变更监听: {}/{}",
                    config_listener.group, config_listener.data_id
                );
            }
            Err(e) => {
                warn!(
                    "注册 Nacos 配置变更监听失败 [{}/{}]: {}",
                    config_listener.group, config_listener.data_id, e
                );
            }
        }
    }
}

/// 优雅关闭：从 Nacos 注销服务实例。
///
/// 在应用关闭时调用，确保服务实例从 Nacos 命名服务中注销，
/// 避免其他服务发现已下线的实例。
pub async fn shutdown_nacos() {
    if let Some(client) = get_nacos_client() {
        if !client.is_naming_enabled() {
            return;
        }

        let port = resolve_register_port();
        let ip = resolve_register_ip();

        match client.deregister_service(&ip, port).await {
            Ok(_) => {
                info!("服务实例已从 Nacos 注销: {}:{}", ip, port);
            }
            Err(e) => {
                warn!("服务从 Nacos 注销失败: {}", e);
            }
        }
    }
}
