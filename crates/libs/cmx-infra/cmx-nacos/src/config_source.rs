//! Nacos 远程配置源
//!
//! 实现 config::Source trait，将 Nacos 远程配置内容（TOML 格式）
//! 解析为 config::Value 树，可通过 ConfigBuilder::add_source() 注入

use std::collections::HashMap;

use config::{Source, Value};

use crate::error::NacosError;

/// Nacos 远程配置源
///
/// 将 Nacos 远程配置内容（TOML 格式）解析为 config::Value 树，
/// 可通过 ConfigBuilder::add_source() 注入，自动覆盖本地同名配置项。
#[derive(Clone, Debug)]
pub struct NacosConfigSource {
    /// 解析后的配置值映射
    values: HashMap<String, Value>,
}

impl NacosConfigSource {
    /// 从 TOML 格式字符串创建配置源
    ///
    /// 自动过滤掉 `nacos` 和 `migration` 相关的 key，防止远程配置覆盖
    /// 启动参数级别的配置（如 Nacos 连接地址、迁移锁配置等）。
    ///
    /// # 参数
    /// - `content`: TOML 格式的配置内容字符串
    ///
    /// # 返回值
    /// 成功返回 NacosConfigSource 实例，失败返回 NacosError
    pub fn from_toml_str(content: &str) -> Result<Self, NacosError> {
        let toml_value: toml::Value = toml::from_str(content)
            .map_err(|e| NacosError::ConfigParseFailed(format!("TOML 解析失败: {}", e)))?;
        let mut values = Self::toml_to_config_map(toml_value);

        // 过滤掉不应被远程配置覆盖的启动参数
        values.remove("nacos");
        values.remove("migration");

        Ok(Self { values })
    }

    /// 将 toml::Value（顶层 Table）递归转换为 config::Value 的 HashMap
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

    /// 将 toml::Value 递归转换为 config::Value
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

impl Source for NacosConfigSource {
    /// 克隆为 Box<dyn Source>
    fn clone_into_box(&self) -> Box<dyn Source + Send + Sync> {
        Box::new(self.clone())
    }

    /// 收集配置值，返回顶层 HashMap
    fn collect(&self) -> Result<HashMap<String, Value>, config::ConfigError> {
        Ok(self.values.clone())
    }
}
