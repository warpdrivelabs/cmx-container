//! 远程配置源
//!
//! 将远程 TOML 配置内容解析为 config::Value 树，
//! 可通过 ConfigBuilder::add_source() 注入。

use std::collections::HashMap;

use config::{Source, Value};

use crate::error::ConfigCenterError;

/// 远程配置源
///
/// 将 TOML 格式的远程配置内容解析为 config::Value 树，
/// 可通过 ConfigBuilder::add_source() 注入，自动覆盖本地同名配置项。
#[derive(Clone, Debug)]
pub struct RemoteConfigSource {
    values: HashMap<String, Value>,
}

impl RemoteConfigSource {
    /// 从 TOML 格式字符串创建配置源
    pub fn from_toml_str(content: &str) -> Result<Self, ConfigCenterError> {
        let toml_value: toml::Value = toml::from_str(content)
            .map_err(|e| ConfigCenterError::ParseFailed(format!("TOML 解析失败: {}", e)))?;
        let values = Self::toml_to_config_map(toml_value);
        Ok(Self { values })
    }

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
    fn clone_into_box(&self) -> Box<dyn Source + Send + Sync> {
        Box::new(self.clone())
    }

    fn collect(&self) -> Result<HashMap<String, Value>, config::ConfigError> {
        Ok(self.values.clone())
    }
}
