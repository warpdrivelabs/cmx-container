//! 配置值类型模块
//!
//! 基于 `config::Value` 提供向后兼容的类型别名和工具函数

use super::error::{ConfigError, ConfigResult};
use std::collections::HashMap;

/// 配置值类型（向后兼容别名）
///
/// 实际底层使用 `config::Value`，保留此类型别名以兼容现有代码
pub type ConfigValue = config::Value;

/// 从配置值转换 trait（向后兼容）
///
/// 迁移到 config crate 后，建议直接使用 serde `Deserialize` trait
/// 此 trait 保留为向后兼容层，内部通过 `config::Value` 的方法实现
pub trait FromConfigValue: Sized {
    /// 从配置值转换为目标类型
    ///
    /// # 参数
    /// - `value`: 配置值
    ///
    /// # 返回值
    /// 成功返回目标类型值，失败返回错误
    fn from_config_value(value: &ConfigValue) -> ConfigResult<Self>;
}

/// 为 String 实现 FromConfigValue
impl FromConfigValue for String {
    fn from_config_value(value: &ConfigValue) -> ConfigResult<Self> {
        value
            .clone()
            .into_string()
            .map_err(|_| ConfigError::TypeConversionError {
                key: "unknown".to_string(),
                target_type: "String".to_string(),
            })
    }
}

/// 为 i64 实现 FromConfigValue
impl FromConfigValue for i64 {
    fn from_config_value(value: &ConfigValue) -> ConfigResult<Self> {
        value
            .clone()
            .into_int()
            .map_err(|_| ConfigError::TypeConversionError {
                key: "unknown".to_string(),
                target_type: "i64".to_string(),
            })
    }
}

/// 为 i32 实现 FromConfigValue
impl FromConfigValue for i32 {
    fn from_config_value(value: &ConfigValue) -> ConfigResult<Self> {
        let v = i64::from_config_value(value)?;
        Ok(v as i32)
    }
}

/// 为 i16 实现 FromConfigValue
impl FromConfigValue for i16 {
    fn from_config_value(value: &ConfigValue) -> ConfigResult<Self> {
        let v = i64::from_config_value(value)?;
        Ok(v as i16)
    }
}

/// 为 i8 实现 FromConfigValue
impl FromConfigValue for i8 {
    fn from_config_value(value: &ConfigValue) -> ConfigResult<Self> {
        let v = i64::from_config_value(value)?;
        Ok(v as i8)
    }
}

/// 为 u64 实现 FromConfigValue
impl FromConfigValue for u64 {
    fn from_config_value(value: &ConfigValue) -> ConfigResult<Self> {
        let v = i64::from_config_value(value)?;
        Ok(v as u64)
    }
}

/// 为 u32 实现 FromConfigValue
impl FromConfigValue for u32 {
    fn from_config_value(value: &ConfigValue) -> ConfigResult<Self> {
        let v = i64::from_config_value(value)?;
        Ok(v as u32)
    }
}

/// 为 u16 实现 FromConfigValue
impl FromConfigValue for u16 {
    fn from_config_value(value: &ConfigValue) -> ConfigResult<Self> {
        let v = i64::from_config_value(value)?;
        Ok(v as u16)
    }
}

/// 为 u8 实现 FromConfigValue
impl FromConfigValue for u8 {
    fn from_config_value(value: &ConfigValue) -> ConfigResult<Self> {
        let v = i64::from_config_value(value)?;
        Ok(v as u8)
    }
}

/// 为 f64 实现 FromConfigValue
impl FromConfigValue for f64 {
    fn from_config_value(value: &ConfigValue) -> ConfigResult<Self> {
        value
            .clone()
            .into_float()
            .map_err(|_| ConfigError::TypeConversionError {
                key: "unknown".to_string(),
                target_type: "f64".to_string(),
            })
    }
}

/// 为 f32 实现 FromConfigValue
impl FromConfigValue for f32 {
    fn from_config_value(value: &ConfigValue) -> ConfigResult<Self> {
        let v = f64::from_config_value(value)?;
        Ok(v as f32)
    }
}

/// 为 bool 实现 FromConfigValue
impl FromConfigValue for bool {
    fn from_config_value(value: &ConfigValue) -> ConfigResult<Self> {
        value
            .clone()
            .into_bool()
            .map_err(|_| ConfigError::TypeConversionError {
                key: "unknown".to_string(),
                target_type: "bool".to_string(),
            })
    }
}

/// 为 Vec<ConfigValue> 实现 FromConfigValue
impl FromConfigValue for Vec<ConfigValue> {
    fn from_config_value(value: &ConfigValue) -> ConfigResult<Self> {
        value
            .clone()
            .into_array()
            .map_err(|_| ConfigError::TypeConversionError {
                key: "unknown".to_string(),
                target_type: "Vec<ConfigValue>".to_string(),
            })
    }
}

/// 为 HashMap<String, ConfigValue> 实现 FromConfigValue
impl FromConfigValue for HashMap<String, ConfigValue> {
    fn from_config_value(value: &ConfigValue) -> ConfigResult<Self> {
        value
            .clone()
            .into_table()
            .map_err(|_| ConfigError::TypeConversionError {
                key: "unknown".to_string(),
                target_type: "HashMap<String, ConfigValue>".to_string(),
            })
    }
}

/// 配置存储结构（向后兼容别名）
///
/// 迁移后不再使用，保留以兼容可能的外部引用
#[derive(Debug, Clone, Default)]
pub struct ConfigStore {
    /// 配置键值对映射
    data: HashMap<String, ConfigValue>,
}

impl ConfigStore {
    /// 创建新的配置存储
    pub fn new() -> Self {
        ConfigStore {
            data: HashMap::new(),
        }
    }

    /// 插入配置键值对
    pub fn insert(&mut self, key: impl Into<String>, value: ConfigValue) {
        self.data.insert(key.into(), value);
    }

    /// 获取配置值
    pub fn get(&self, key: &str) -> Option<&ConfigValue> {
        self.data.get(key)
    }

    /// 检查配置键是否存在
    pub fn contains_key(&self, key: &str) -> bool {
        self.data.contains_key(key)
    }

    /// 获取所有配置键
    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.data.keys()
    }

    /// 获取配置项数量
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// 检查配置是否为空
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// 合并另一个配置存储
    pub fn merge(&mut self, other: ConfigStore) {
        for (key, value) in other.data {
            self.data.insert(key, value);
        }
    }

    /// 获取所有配置数据的引用
    pub fn data(&self) -> &HashMap<String, ConfigValue> {
        &self.data
    }
}
