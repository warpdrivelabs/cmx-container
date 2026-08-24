//! chassis 配置：host / port / 日志目录 / 日志级别 / 优雅关闭。
//!
//! 来源二选一叠加：可选 toml 文件的 `[server]` 段（路径由 `CONFIG_FILE` 指定，缺省用各服务
//! 默认文件名）→ 环境变量覆盖。chassis 只管这些**框架级**配置；服务专属配置（数据库、认证等）
//! 由各服务自己经 ConfigManager 读同一份 toml。

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
    /// - toml 路径：统一从 `CONFIG_FILE` 取（全服务同名，与门户一致）→ 参数 `default_toml`
    ///   （内置默认）。文件不存在则跳过。
    /// - 环境变量覆盖：`SERVER__HOST` / `SERVER__PORT` / `SERVER__LOG_DIR` /
    ///   `SERVER__LOG_LEVEL` / `SERVER__GRACEFUL_TIMEOUT_SECS`——**与 ConfigManager 的
    ///   `__` 约定同名**（`SERVER__PORT` → `server.port`）。chassis 在 ConfigManager 初始化
    ///   之前直读同名 env；此后 ConfigManager 的 env 层把同一变量合并到同一键，故注册中心等
    ///   `get_string("server.port")` 消费方与实际监听端口永远一致（一元命名，两条读取链同值）。
    ///   多服务共存同一环境时如需单独覆盖端口，改各自 toml 的 `[server]` 段。
    pub fn load(service: &str, default_toml: &str) -> Self {
        let mut cfg = Self::defaults(service);

        // 1) 可选 toml。只认 [server] 段；顶层出现旧格式散字段时打迁移提示（不生效）。
        let toml_path = std::env::var("CONFIG_FILE")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| default_toml.to_string());
        if let Ok(text) = std::fs::read_to_string(&toml_path) {
            match toml::from_str::<toml::Value>(&text) {
                Ok(value) => {
                    warn_legacy_top_level(&value);
                    match TomlConfig::deserialize(value) {
                        Ok(t) => t.apply_onto(&mut cfg),
                        Err(e) => tracing::warn!(path = %toml_path, error = %e, "chassis toml 解析失败，用默认+环境变量"),
                    }
                }
                Err(e) => tracing::warn!(path = %toml_path, error = %e, "chassis toml 解析失败，用默认+环境变量"),
            }
        }

        // 2) 环境变量覆盖（优先级最高）。命名 = ConfigManager `__` 约定（SERVER__PORT →
        //    server.port），见 load 文档注释；chassis 不依赖 cmx-utils，仅认这几个同名变量。
        if let Some(v) = env_opt("SERVER__HOST") {
            cfg.host = v;
        }
        if let Some(v) = env_opt("SERVER__PORT").and_then(|s| s.parse().ok()) {
            cfg.port = v;
        }
        if let Some(v) = env_opt("SERVER__LOG_DIR") {
            cfg.log_dir = v;
        }
        if let Some(v) = env_opt("SERVER__LOG_LEVEL") {
            cfg.log_level = v;
        }
        if let Some(v) = env_opt("SERVER__GRACEFUL_TIMEOUT_SECS").and_then(|s| s.parse().ok()) {
            cfg.graceful_timeout_secs = v;
        }

        cfg
    }
}

/// toml 反序列化壳：**只认 `[server]` 段**（全平台统一形态，与门户 dev.toml 同段名同字段）；
/// 其余段（[[databases]]、[auth] 等）归各服务自己的 ConfigManager，这里一概不解析。
#[derive(Debug, Default, Deserialize)]
struct TomlConfig {
    server: Option<ServerToml>,
}

/// `[server]` 段：host / port / log_dir / log_level / graceful_timeout_secs（全可选，
/// 只覆盖出现的字段）。
#[derive(Debug, Default, Deserialize)]
struct ServerToml {
    host: Option<String>,
    port: Option<u16>,
    log_dir: Option<String>,
    log_level: Option<String>,
    graceful_timeout_secs: Option<u64>,
}

impl TomlConfig {
    fn apply_onto(self, cfg: &mut ChassisConfig) {
        if let Some(s) = self.server {
            if let Some(v) = s.host {
                cfg.host = v;
            }
            if let Some(v) = s.port {
                cfg.port = v;
            }
            if let Some(v) = s.log_dir {
                cfg.log_dir = v;
            }
            if let Some(v) = s.log_level {
                cfg.log_level = v;
            }
            if let Some(v) = s.graceful_timeout_secs {
                cfg.graceful_timeout_secs = v;
            }
        }
    }
}

/// 顶层旧格式散字段（历史形态，已废弃）出现时打迁移提示——**不生效**。
///
/// 用 `eprintln!` 而非 tracing：`ChassisConfig::load` 在 init_tracing（全局 subscriber）
/// 之前执行，tracing 事件此时会被丢弃。
fn warn_legacy_top_level(value: &toml::Value) {
    const LEGACY_KEYS: [&str; 6] = [
        "host",
        "port",
        "log_dir",
        "log_level",
        "graceful_timeout_secs",
        "graceful_shutdown_timeout_secs",
    ];
    let hit: Vec<&str> = LEGACY_KEYS
        .into_iter()
        .filter(|k| value.get(*k).is_some())
        .collect();
    if !hit.is_empty() {
        eprintln!(
            "[chassis] 顶层散字段 {hit:?} 已废弃（不生效）：框架级配置请迁到 [server] 段 \
             （host/port/log_dir/log_level/graceful_timeout_secs），文件内其余段不受影响"
        );
    }
}

/// 读非空环境变量。
fn env_opt(var: &str) -> Option<String> {
    std::env::var(var).ok().filter(|s| !s.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `[server]` 段五键全部装配生效。
    #[test]
    fn server_section_applies_all_fields() {
        let text = r#"
[server]
host = "127.0.0.1"
port = 8093
log_dir = "/var/log/cmx"
log_level = "info,cmx_access=off"
graceful_timeout_secs = 20
"#;
        let t: TomlConfig = toml::from_str(text).unwrap();
        let mut cfg = ChassisConfig::defaults("test");
        t.apply_onto(&mut cfg);
        assert_eq!(cfg.host, "127.0.0.1");
        assert_eq!(cfg.port, 8093);
        assert_eq!(cfg.log_dir, "/var/log/cmx");
        assert_eq!(cfg.log_level, "info,cmx_access=off");
        assert_eq!(cfg.graceful_timeout_secs, 20);
    }

    /// 顶层散字段（旧格式）不生效——只认 `[server]` 段，其余保持默认。
    #[test]
    fn top_level_legacy_keys_are_ignored() {
        let text = "port = 8091\nlog_level = \"debug\"\n";
        let value: toml::Value = toml::from_str(text).unwrap();
        warn_legacy_top_level(&value);
        let t = TomlConfig::deserialize(value).unwrap();
        let mut cfg = ChassisConfig::defaults("test");
        t.apply_onto(&mut cfg);
        assert_eq!(cfg.port, 8080);
        assert_eq!(cfg.log_level, "info");
    }

    /// 缺 `[server]` 段（如只配了业务段的文件）照常解析，无任何覆盖。
    #[test]
    fn missing_server_section_is_noop() {
        let text = "[auth]\njwt_secret = \"x\"\n";
        let t: TomlConfig = toml::from_str(text).unwrap();
        let mut cfg = ChassisConfig::defaults("test");
        t.apply_onto(&mut cfg);
        assert_eq!(cfg.port, 8080);
    }
}
