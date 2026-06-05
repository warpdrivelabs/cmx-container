//! 远程配置源适配器。
//!
//! 该模块将远程 TOML 配置内容解析为 `config::Value` 树，
//! 实现 `config::Source` trait，使其能够通过
//! [`ConfigBuilder::add_source()`](cmx_utils::ConfigBuilder::add_source) 注入到 config-rs 的合并链中。
//!
//! # 设计目的
//!
//! `config-rs` 原生支持 TOML 文件作为配置源，但不支持从字符串直接加载。
//! 本适配器填补这一空缺，使远程配置中心的内容（字符串形式）能像本地文件一样
//! 无缝接入配置合并管线，并享受与本地 TOML 相同的优先级语义。

use std::collections::HashMap;

use config::{Source, Value};

use crate::error::ConfigCenterError;

/// 远程配置源。
///
/// 将 TOML 格式的远程配置内容解析为 `config::Value` 树，
/// 可通过 `ConfigBuilder::add_source()` 注入，自动覆盖本地同名配置项。
///
/// # Examples
///
/// ```ignore
/// use cmx_registry_config::RemoteConfigSource;
/// use cmx_utils::ConfigBuilder;
///
/// let toml = r#"
///     [database]
///     host = "127.0.0.1"
///     port = 5432
/// "#;
/// let source = RemoteConfigSource::from_toml_str(toml)?;
/// let config = ConfigBuilder::new().add_source(source).build()?;
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone, Debug)]
pub struct RemoteConfigSource {
    /// 解析后的 `config::Value` 树，键为扁平化的配置 key。
    values: HashMap<String, Value>,
}

impl RemoteConfigSource {
    /// 从 TOML 格式字符串创建配置源。
    ///
    /// # Arguments
    ///
    /// * `content` - TOML 格式的远程配置内容。
    ///
    /// # Returns
    ///
    /// * `Ok(RemoteConfigSource)` - 解析成功。
    /// * `Err(ConfigCenterError::ParseFailed)` - TOML 内容格式错误。
    pub fn from_toml_str(content: &str) -> Result<Self, ConfigCenterError> {
        let toml_value: toml::Value = toml::from_str(content)
            .map_err(|e| ConfigCenterError::ParseFailed(format!("TOML 解析失败: {}", e)))?;
        let values = Self::toml_to_config_map(toml_value);
        Ok(Self { values })
    }

    /// 将 TOML 顶层 Table 转换为 `config::Value` 映射。
    ///
    /// 非 Table 类型视为无效顶层，返回空 map。
    fn toml_to_config_map(toml_val: toml::Value) -> HashMap<String, Value> {
        match toml_val {
            toml::Value::Table(table) => {
                let mut map = HashMap::new();
                for (k, v) in table {
                    map.insert(k, Self::toml_to_config_value(v));
                }
                map
            }
            _ => HashMap::new(),
        }
    }

    /// 递归将 `toml::Value` 转换为 `config::Value`。
    ///
    /// 支持所有 TOML 原始类型、数组、嵌套 Table 和 DateTime（转为字符串）。
    fn toml_to_config_value(toml_val: toml::Value) -> Value {
        match toml_val {
            toml::Value::String(s) => Value::new(None, s),
            toml::Value::Integer(i) => Value::new(None, i),
            toml::Value::Float(f) => Value::new(None, f),
            toml::Value::Boolean(b) => Value::new(None, b),
            toml::Value::Table(table) => {
                let mut map = HashMap::new();
                for (k, v) in table {
                    map.insert(k, Self::toml_to_config_value(v));
                }
                Value::new(None, map)
            }
            toml::Value::Array(arr) => {
                let vec: Vec<Value> = arr.into_iter().map(Self::toml_to_config_value).collect();
                Value::new(None, vec)
            }
            toml::Value::Datetime(dt) => Value::new(None, dt.to_string()),
        }
    }
}

impl Source for RemoteConfigSource {
    /// 克隆为 `Box<dyn Source>`，满足 `config-rs` 内部对 `Send + Sync` 约束。
    fn clone_into_box(&self) -> Box<dyn Source + Send + Sync> {
        Box::new(self.clone())
    }

    /// 收集配置键值对，供 `config-rs` 合并。
    ///
    /// # Returns
    ///
    /// 返回 `HashMap<String, Value>`，由 `config-rs` 进一步扁平化。
    fn collect(&self) -> Result<HashMap<String, Value>, config::ConfigError> {
        Ok(self.values.clone())
    }
}
