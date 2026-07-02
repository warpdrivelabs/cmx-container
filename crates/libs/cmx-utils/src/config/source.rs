//! 命令行参数配置来源模块
//!
//! 实现 `config::Source` trait，将命令行参数（`--key=value` / `--key value`）
//! 作为配置来源注入到 `config::Config` 中

use std::collections::HashMap;
use std::env;

use super::error::ConfigResult;

/// 命令行参数配置来源
///
/// 从命令行参数加载配置，支持两种格式：
/// - `--key=value`
/// - `--key value`
///
/// 实现 `config::Source` trait，可直接通过 `ConfigBuilder::add_source()` 使用
#[derive(Debug, Clone)]
pub struct CommandLineSource {
    /// 解析后的配置键值对
    args: HashMap<String, String>,
}

impl CommandLineSource {
    /// 从命令行参数创建配置来源
    ///
    /// # 参数
    /// - `args`: 命令行参数迭代器（通常为 `std::env::args().skip(1)`）
    ///
    /// # 返回值
    /// 返回命令行参数配置来源实例
    ///
    /// # 示例
    /// ```ignore
    /// let source = CommandLineSource::from_args(std::env::args().skip(1));
    /// ```
    pub fn from_args<I: Iterator<Item = String>>(args: I) -> Self {
        let mut config_args = HashMap::new();
        let mut iter = args.peekable();

        while let Some(arg) = iter.next() {
            if let Some(arg_content) = arg.strip_prefix("--") {
                if let Some(eq_pos) = arg_content.find('=') {
                    let key = arg_content[..eq_pos].to_string();
                    let value = arg_content[eq_pos + 1..].to_string();
                    config_args.insert(key, value);
                } else if let Some(next_arg) = iter.peek()
                    && !next_arg.starts_with("--")
                {
                    let key = arg_content.to_string();
                    let value = next_arg.clone();
                    config_args.insert(key, value);
                    iter.next();
                }
            }
        }

        CommandLineSource { args: config_args }
    }

    /// 从键值对映射创建配置来源
    ///
    /// # 参数
    /// - `args`: 键值对映射
    ///
    /// # 返回值
    /// 返回命令行参数配置来源实例
    pub fn from_map(args: HashMap<String, String>) -> Self {
        CommandLineSource { args }
    }

    /// 从环境变量获取 TOML 配置文件路径（可选）
    ///
    /// 如果环境变量存在则返回路径，不存在则返回 None
    ///
    /// # 参数
    /// - `env_var`: 环境变量名
    ///
    /// # 返回值
    /// 配置文件路径（可选）
    pub fn toml_path_from_env(env_var: &str) -> Option<String> {
        env::var(env_var).ok()
    }

    /// 从环境变量获取 TOML 配置文件路径（必需）
    ///
    /// 如果环境变量不存在则返回错误
    ///
    /// # 参数
    /// - `env_var`: 环境变量名
    ///
    /// # 返回值
    /// 成功返回配置文件路径，失败返回错误
    pub fn toml_path_from_env_required(env_var: &str) -> ConfigResult<String> {
        env::var(env_var).map_err(|_| super::error::ConfigError::EnvVarError {
            var_name: env_var.to_string(),
        })
    }

    /// 从环境变量获取 TOML 配置文件路径（带默认值）
    ///
    /// 如果环境变量不存在则使用默认路径
    ///
    /// # 参数
    /// - `env_var`: 环境变量名
    /// - `default_path`: 默认配置文件路径
    ///
    /// # 返回值
    /// 返回配置文件路径
    pub fn toml_path_from_env_or(env_var: &str, default_path: &str) -> String {
        env::var(env_var).unwrap_or_else(|_| default_path.to_string())
    }
}

/// 实现 `config::Source` trait
///
/// 将命令行参数作为字符串值注入到配置中，
/// 后续可通过 `config.get_string()` 或 `config.get()` 读取
impl config::Source for CommandLineSource {
    fn collect(&self) -> Result<HashMap<String, config::Value>, config::ConfigError> {
        let mut map = HashMap::new();
        for (key, value) in &self.args {
            map.insert(key.to_lowercase(), config::Value::from(value.as_str()));
        }
        Ok(map)
    }

    fn clone_into_box(&self) -> Box<dyn config::Source + Send + Sync> {
        Box::new(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use config::Source;

    #[test]
    fn test_command_line_source() {
        let args = vec![
            "--host".to_string(),
            "localhost".to_string(),
            "--port=8080".to_string(),
            "--debug".to_string(),
            "true".to_string(),
        ];

        let source = CommandLineSource::from_args(args.into_iter());
        let map = source.collect().unwrap();

        assert_eq!(
            map.get("host").unwrap().clone().into_string().unwrap(),
            "localhost"
        );
        assert_eq!(
            map.get("port").unwrap().clone().into_string().unwrap(),
            "8080"
        );
        assert_eq!(
            map.get("debug").unwrap().clone().into_string().unwrap(),
            "true"
        );
    }

    #[test]
    fn test_command_line_from_map() {
        let mut map = HashMap::new();
        map.insert("key1".to_string(), "value1".to_string());
        map.insert("key2".to_string(), "value2".to_string());

        let source = CommandLineSource::from_map(map);
        let result = source.collect().unwrap();

        assert_eq!(
            result.get("key1").unwrap().clone().into_string().unwrap(),
            "value1"
        );
        assert_eq!(
            result.get("key2").unwrap().clone().into_string().unwrap(),
            "value2"
        );
    }
}
