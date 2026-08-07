//! chassis 配置：host / port / 日志目录 / 日志级别 / banner 文案。
//!
//! 来源二选一叠加：可选 toml 文件（`CMX_SERVICE_CONFIG` 指定路径，或各服务默认）→ 环境变量覆盖。
//! chassis 只管这些**框架级**配置；服务专属配置（DB URL、adapter mode 等）由各服务自己读环境变量。

use serde::Deserialize;

/// 服务骨架的框架级配置。
#[derive(Debug, Clone)]
pub struct ChassisConfig {
    /// 监听主机（默认 0.0.0.0）。
    pub host: String,
    /// 监听端口（默认 8080）。
    pub port: u16,
    /// 日志目录（默认 "logs"）。
    pub log_dir: String,
    /// 日志文件名前缀（默认 "<service>.log"，由服务名给出）。
    pub log_file: String,
    /// 默认日志级别（RUST_LOG 未设时用；默认 "info"）。
    pub log_level: String,
    /// 优雅关闭最长等待秒数（默认 10）。
    pub graceful_timeout_secs: u64,
}

impl ChassisConfig {
    /// 用服务名构建默认值（log_file = "<service>.log"，port 8080）。
    pub fn defaults(service: &str) -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8080,
            log_dir: "logs".to_string(),
            log_file: format!("{service}.log"),
            log_level: "info".to_string(),
            graceful_timeout_secs: 10,
        }
    }

    /// 从「可选 toml + 环境变量覆盖」装配。
    ///
    /// - `service`：服务名（默认 log_file 前缀）。
    /// - toml 路径：环境变量 `env_prefix + "_CONFIG"`（如 `FLOW_CONFIG`）或参数 `default_toml`。
    /// - 环境变量覆盖：`{PREFIX}_HOST` / `{PREFIX}_PORT` / `{PREFIX}_LOG_DIR` /
    ///   `{PREFIX}_LOG_LEVEL` / `{PREFIX}_GRACEFUL_SECS`（PREFIX 如 "FLOW"）。
    pub fn load(service: &str, env_prefix: &str, default_toml: &str) -> Self {
        let mut cfg = Self::defaults(service);

        // 1) 可选 toml（先 env 指定路径，再默认路径；文件不存在则跳过）。
        let toml_path = std::env::var(format!("{env_prefix}_CONFIG"))
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| default_toml.to_string());
        if let Ok(text) = std::fs::read_to_string(&toml_path) {
            match toml::from_str::<TomlConfig>(&text) {
                Ok(t) => t.apply_onto(&mut cfg),
                Err(e) => tracing::warn!(path = %toml_path, error = %e, "chassis toml 解析失败，用默认+环境变量"),
            }
        }

        // 2) 环境变量覆盖（优先级最高）。
        if let Some(v) = env_opt(&format!("{env_prefix}_HOST")) {
            cfg.host = v;
        }
        if let Some(v) = env_opt(&format!("{env_prefix}_PORT")).and_then(|s| s.parse().ok()) {
            cfg.port = v;
        }
        if let Some(v) = env_opt(&format!("{env_prefix}_LOG_DIR")) {
            cfg.log_dir = v;
        }
        if let Some(v) = env_opt(&format!("{env_prefix}_LOG_LEVEL")) {
            cfg.log_level = v;
        }
        if let Some(v) = env_opt(&format!("{env_prefix}_GRACEFUL_SECS")).and_then(|s| s.parse().ok()) {
            cfg.graceful_timeout_secs = v;
        }

        cfg
    }
}

/// toml 反序列化壳（全可选，只覆盖出现的字段）。
#[derive(Debug, Default, Deserialize)]
struct TomlConfig {
    host: Option<String>,
    port: Option<u16>,
    log_dir: Option<String>,
    log_level: Option<String>,
    graceful_timeout_secs: Option<u64>,
}

impl TomlConfig {
    fn apply_onto(self, cfg: &mut ChassisConfig) {
        if let Some(v) = self.host {
            cfg.host = v;
        }
        if let Some(v) = self.port {
            cfg.port = v;
        }
        if let Some(v) = self.log_dir {
            cfg.log_dir = v;
        }
        if let Some(v) = self.log_level {
            cfg.log_level = v;
        }
        if let Some(v) = self.graceful_timeout_secs {
            cfg.graceful_timeout_secs = v;
        }
    }
}

/// 读非空环境变量。
fn env_opt(var: &str) -> Option<String> {
    std::env::var(var).ok().filter(|s| !s.trim().is_empty())
}
