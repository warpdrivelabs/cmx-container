//! Web 服务器配置模块

use cmx_utils::{CommandLineSource, ConfigBuilder, ConfigManager, ConfigResult, Priority};
use std::sync::OnceLock;
use serde::Deserialize;

/// 获取 Web 配置单例
///
/// 返回一个静态的 WebConfig 引用，确保配置只加载一次
///
/// # 返回值
/// - `&'static WebConfig` - Web 服务器配置引用
pub fn web_config() -> &'static WebConfig {
    static INSTANCE: OnceLock<WebConfig> = OnceLock::new();

    INSTANCE.get_or_init(|| {
        WebConfig::load_from_env()
            .unwrap_or_else(|ex| panic!("FATAL - WHILE LOADING CONF - Cause: {ex:?}"))
    })
}

/// Web 服务器配置结构
#[allow(non_snake_case)]
#[derive(Debug, Deserialize)]
pub struct WebConfig {
    /// Web 静态文件目录
    pub WEB_FOLDER: String,
}

impl WebConfig {
    /// 从环境变量加载配置
    ///
    /// # 返回值
    fn load_from_env() -> ConfigResult<WebConfig> {
        tracing::info!("加载环境变量和配置文件信息...");
        // 加载.env文件
        dotenvy::dotenv().ok();

        //初始化配置管理器
        ConfigManager::initialize(|| {
            ConfigBuilder::new()
                // 添加.env支持
                .add_env()
                // 添加命令行参数
                .add_source(CommandLineSource::from_args(std::env::args().skip(1)))
                .add_toml_file_from_env("CONFIG_FILE", Priority(10))
                .build()
        })
        .unwrap();
        let webConfig: ConfigResult<WebConfig> = ConfigManager::global().deserialize();

        webConfig
    }
}
