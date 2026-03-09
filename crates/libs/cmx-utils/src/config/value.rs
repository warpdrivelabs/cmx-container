//! 配置值类型和类型转换模块
//!
//! 提供配置值的抽象表示和类型转换功能

use std::collections::HashMap;
use std::fmt::Debug;

use super::error::{ConfigError, ConfigResult};

/// 配置值类型
///
/// 表示配置系统中可能出现的各种值类型
#[derive(Debug, Clone, PartialEq)]
pub enum ConfigValue {
    /// 字符串类型
    String(String),
    /// 整数类型
    Integer(i64),
    /// 浮点数类型
    Float(f64),
    /// 布尔类型
    Boolean(bool),
    /// 数组类型
    Array(Vec<ConfigValue>),
    /// 对象/映射类型
    Object(HashMap<String, ConfigValue>),
    /// 空值
    Null,
}

impl ConfigValue {
    /// 创建字符串类型的配置值
    ///
    /// # 参数
    /// - `value`: 字符串值
    ///
    /// # 返回值
    /// 返回包装后的配置值
    pub fn new_string(value: impl Into<String>) -> Self {
        ConfigValue::String(value.into())
    }

    /// 创建整数类型的配置值
    ///
    /// # 参数
    /// - `value`: 整数值
    ///
    /// # 返回值
    /// 返回包装后的配置值
    pub fn new_integer(value: i64) -> Self {
        ConfigValue::Integer(value)
    }

    /// 创建浮点数类型的配置值
    ///
    /// # 参数
    /// - `value`: 浮点数值
    ///
    /// # 返回值
    /// 返回包装后的配置值
    pub fn new_float(value: f64) -> Self {
        ConfigValue::Float(value)
    }

    /// 创建布尔类型的配置值
    ///
    /// # 参数
    /// - `value`: 布尔值
    ///
    /// # 返回值
    /// 返回包装后的配置值
    pub fn new_boolean(value: bool) -> Self {
        ConfigValue::Boolean(value)
    }

    /// 创建数组类型的配置值
    ///
    /// # 参数
    /// - `value`: 配置值数组
    ///
    /// # 返回值
    /// 返回包装后的配置值
    pub fn new_array(value: Vec<ConfigValue>) -> Self {
        ConfigValue::Array(value)
    }

    /// 创建对象类型的配置值
    ///
    /// # 参数
    /// - `value`: 配置值映射
    ///
    /// # 返回值
    /// 返回包装后的配置值
    pub fn new_object(value: HashMap<String, ConfigValue>) -> Self {
        ConfigValue::Object(value)
    }

    /// 创建空值类型的配置值
    ///
    /// # 返回值
    /// 返回空配置值
    pub fn new_null() -> Self {
        ConfigValue::Null
    }

    /// 尝试将配置值转换为指定类型
    ///
    /// # 类型参数
    /// - `T`: 目标类型，必须实现 `FromConfigValue` trait
    ///
    /// # 返回值
    /// 成功返回转换后的值，失败返回错误
    ///
    /// # 示例
    /// ```ignore
    /// let value = ConfigValue::new_integer(42);
    /// let num: i32 = value.try_into_type()?;
    /// ```
    pub fn try_into_type<T: FromConfigValue>(&self) -> ConfigResult<T> {
        T::from_config_value(self)
    }

    /// 检查配置值是否为指定类型
    ///
    /// # 返回值
    /// 如果配置值是指定类型返回 true，否则返回 false
    pub fn is_string(&self) -> bool {
        matches!(self, ConfigValue::String(_))
    }

    /// 检查配置值是否为整数类型
    pub fn is_integer(&self) -> bool {
        matches!(self, ConfigValue::Integer(_))
    }

    /// 检查配置值是否为浮点数类型
    pub fn is_float(&self) -> bool {
        matches!(self, ConfigValue::Float(_))
    }

    /// 检查配置值是否为布尔类型
    pub fn is_boolean(&self) -> bool {
        matches!(self, ConfigValue::Boolean(_))
    }

    /// 检查配置值是否为数组类型
    pub fn is_array(&self) -> bool {
        matches!(self, ConfigValue::Array(_))
    }

    /// 检查配置值是否为对象类型
    pub fn is_object(&self) -> bool {
        matches!(self, ConfigValue::Object(_))
    }

    /// 检查配置值是否为空值
    pub fn is_null(&self) -> bool {
        matches!(self, ConfigValue::Null)
    }

    /// 获取字符串值
    ///
    /// # 返回值
    /// 如果是字符串类型返回 Some，否则返回 None
    pub fn as_str(&self) -> Option<&str> {
        match self {
            ConfigValue::String(s) => Some(s),
            _ => None,
        }
    }

    /// 获取整数值
    ///
    /// # 返回值
    /// 如果是整数类型返回 Some，否则返回 None
    pub fn as_integer(&self) -> Option<i64> {
        match self {
            ConfigValue::Integer(i) => Some(*i),
            _ => None,
        }
    }

    /// 获取浮点数值
    ///
    /// # 返回值
    /// 如果是浮点数类型返回 Some，否则返回 None
    pub fn as_float(&self) -> Option<f64> {
        match self {
            ConfigValue::Float(f) => Some(*f),
            _ => None,
        }
    }

    /// 获取布尔值
    ///
    /// # 返回值
    /// 如果是布尔类型返回 Some，否则返回 None
    pub fn as_boolean(&self) -> Option<bool> {
        match self {
            ConfigValue::Boolean(b) => Some(*b),
            _ => None,
        }
    }

    /// 获取数组值
    ///
    /// # 返回值
    /// 如果是数组类型返回 Some，否则返回 None
    pub fn as_array(&self) -> Option<&Vec<ConfigValue>> {
        match self {
            ConfigValue::Array(arr) => Some(arr),
            _ => None,
        }
    }

    /// 获取对象值
    ///
    /// # 返回值
    /// 如果是对象类型返回 Some，否则返回 None
    pub fn as_object(&self) -> Option<&HashMap<String, ConfigValue>> {
        match self {
            ConfigValue::Object(obj) => Some(obj),
            _ => None,
        }
    }
}

impl From<String> for ConfigValue {
    fn from(value: String) -> Self {
        ConfigValue::String(value)
    }
}

impl From<&str> for ConfigValue {
    fn from(value: &str) -> Self {
        ConfigValue::String(value.to_string())
    }
}

impl From<i64> for ConfigValue {
    fn from(value: i64) -> Self {
        ConfigValue::Integer(value)
    }
}

impl From<i32> for ConfigValue {
    fn from(value: i32) -> Self {
        ConfigValue::Integer(value as i64)
    }
}

impl From<f64> for ConfigValue {
    fn from(value: f64) -> Self {
        ConfigValue::Float(value)
    }
}

impl From<bool> for ConfigValue {
    fn from(value: bool) -> Self {
        ConfigValue::Boolean(value)
    }
}

impl<T: Into<ConfigValue>> From<Vec<T>> for ConfigValue {
    fn from(value: Vec<T>) -> Self {
        ConfigValue::Array(value.into_iter().map(|v| v.into()).collect())
    }
}

/// 从配置值转换 trait
///
/// 实现此 trait 以支持从 ConfigValue 转换为目标类型
pub trait FromConfigValue: Sized {
    /// 从配置值转换
    ///
    /// # 参数
    /// - `value`: 配置值
    ///
    /// # 返回值
    /// 成功返回目标类型值，失败返回错误
    fn from_config_value(value: &ConfigValue) -> ConfigResult<Self>;
}

// 为基本类型实现 FromConfigValue
impl FromConfigValue for String {
    fn from_config_value(value: &ConfigValue) -> ConfigResult<Self> {
        match value {
            ConfigValue::String(s) => Ok(s.clone()),
            ConfigValue::Integer(i) => Ok(i.to_string()),
            ConfigValue::Float(f) => Ok(f.to_string()),
            ConfigValue::Boolean(b) => Ok(b.to_string()),
            _ => Err(ConfigError::TypeConversionError {
                key: "unknown".to_string(),
                target_type: "String".to_string(),
            }),
        }
    }
}

impl FromConfigValue for i64 {
    fn from_config_value(value: &ConfigValue) -> ConfigResult<Self> {
        match value {
            ConfigValue::Integer(i) => Ok(*i),
            ConfigValue::String(s) => s.parse::<i64>().map_err(|_| ConfigError::TypeConversionError {
                key: "unknown".to_string(),
                target_type: "i64".to_string(),
            }),
            _ => Err(ConfigError::TypeConversionError {
                key: "unknown".to_string(),
                target_type: "i64".to_string(),
            }),
        }
    }
}

impl FromConfigValue for i32 {
    fn from_config_value(value: &ConfigValue) -> ConfigResult<Self> {
        match value {
            ConfigValue::Integer(i) => Ok(*i as i32),
            ConfigValue::String(s) => s.parse::<i32>().map_err(|_| ConfigError::TypeConversionError {
                key: "unknown".to_string(),
                target_type: "i32".to_string(),
            }),
            _ => Err(ConfigError::TypeConversionError {
                key: "unknown".to_string(),
                target_type: "i32".to_string(),
            }),
        }
    }
}

impl FromConfigValue for i16 {
    fn from_config_value(value: &ConfigValue) -> ConfigResult<Self> {
        match value {
            ConfigValue::Integer(i) => Ok(*i as i16),
            ConfigValue::String(s) => s.parse::<i16>().map_err(|_| ConfigError::TypeConversionError {
                key: "unknown".to_string(),
                target_type: "i16".to_string(),
            }),
            _ => Err(ConfigError::TypeConversionError {
                key: "unknown".to_string(),
                target_type: "i16".to_string(),
            }),
        }
    }
}

impl FromConfigValue for i8 {
    fn from_config_value(value: &ConfigValue) -> ConfigResult<Self> {
        match value {
            ConfigValue::Integer(i) => Ok(*i as i8),
            ConfigValue::String(s) => s.parse::<i8>().map_err(|_| ConfigError::TypeConversionError {
                key: "unknown".to_string(),
                target_type: "i8".to_string(),
            }),
            _ => Err(ConfigError::TypeConversionError {
                key: "unknown".to_string(),
                target_type: "i8".to_string(),
            }),
        }
    }
}

impl FromConfigValue for u64 {
    fn from_config_value(value: &ConfigValue) -> ConfigResult<Self> {
        match value {
            ConfigValue::Integer(i) => Ok(*i as u64),
            ConfigValue::String(s) => s.parse::<u64>().map_err(|_| ConfigError::TypeConversionError {
                key: "unknown".to_string(),
                target_type: "u64".to_string(),
            }),
            _ => Err(ConfigError::TypeConversionError {
                key: "unknown".to_string(),
                target_type: "u64".to_string(),
            }),
        }
    }
}

impl FromConfigValue for u32 {
    fn from_config_value(value: &ConfigValue) -> ConfigResult<Self> {
        match value {
            ConfigValue::Integer(i) => Ok(*i as u32),
            ConfigValue::String(s) => s.parse::<u32>().map_err(|_| ConfigError::TypeConversionError {
                key: "unknown".to_string(),
                target_type: "u32".to_string(),
            }),
            _ => Err(ConfigError::TypeConversionError {
                key: "unknown".to_string(),
                target_type: "u32".to_string(),
            }),
        }
    }
}

impl FromConfigValue for u16 {
    fn from_config_value(value: &ConfigValue) -> ConfigResult<Self> {
        match value {
            ConfigValue::Integer(i) => Ok(*i as u16),
            ConfigValue::String(s) => s.parse::<u16>().map_err(|_| ConfigError::TypeConversionError {
                key: "unknown".to_string(),
                target_type: "u16".to_string(),
            }),
            _ => Err(ConfigError::TypeConversionError {
                key: "unknown".to_string(),
                target_type: "u16".to_string(),
            }),
        }
    }
}

impl FromConfigValue for u8 {
    fn from_config_value(value: &ConfigValue) -> ConfigResult<Self> {
        match value {
            ConfigValue::Integer(i) => Ok(*i as u8),
            ConfigValue::String(s) => s.parse::<u8>().map_err(|_| ConfigError::TypeConversionError {
                key: "unknown".to_string(),
                target_type: "u8".to_string(),
            }),
            _ => Err(ConfigError::TypeConversionError {
                key: "unknown".to_string(),
                target_type: "u8".to_string(),
            }),
        }
    }
}

impl FromConfigValue for f64 {
    fn from_config_value(value: &ConfigValue) -> ConfigResult<Self> {
        match value {
            ConfigValue::Float(f) => Ok(*f),
            ConfigValue::Integer(i) => Ok(*i as f64),
            ConfigValue::String(s) => s.parse::<f64>().map_err(|_| ConfigError::TypeConversionError {
                key: "unknown".to_string(),
                target_type: "f64".to_string(),
            }),
            _ => Err(ConfigError::TypeConversionError {
                key: "unknown".to_string(),
                target_type: "f64".to_string(),
            }),
        }
    }
}

impl FromConfigValue for f32 {
    fn from_config_value(value: &ConfigValue) -> ConfigResult<Self> {
        match value {
            ConfigValue::Float(f) => Ok(*f as f32),
            ConfigValue::Integer(i) => Ok(*i as f32),
            ConfigValue::String(s) => s.parse::<f32>().map_err(|_| ConfigError::TypeConversionError {
                key: "unknown".to_string(),
                target_type: "f32".to_string(),
            }),
            _ => Err(ConfigError::TypeConversionError {
                key: "unknown".to_string(),
                target_type: "f32".to_string(),
            }),
        }
    }
}

impl FromConfigValue for bool {
    fn from_config_value(value: &ConfigValue) -> ConfigResult<Self> {
        match value {
            ConfigValue::Boolean(b) => Ok(*b),
            ConfigValue::String(s) => {
                let lower = s.to_lowercase();
                match lower.as_str() {
                    "true" | "1" | "yes" | "on" => Ok(true),
                    "false" | "0" | "no" | "off" => Ok(false),
                    _ => Err(ConfigError::TypeConversionError {
                        key: "unknown".to_string(),
                        target_type: "bool".to_string(),
                    }),
                }
            }
            ConfigValue::Integer(i) => Ok(*i != 0),
            _ => Err(ConfigError::TypeConversionError {
                key: "unknown".to_string(),
                target_type: "bool".to_string(),
            }),
        }
    }
}

impl FromConfigValue for Vec<ConfigValue> {
    fn from_config_value(value: &ConfigValue) -> ConfigResult<Self> {
        match value {
            ConfigValue::Array(arr) => Ok(arr.clone()),
            _ => Err(ConfigError::TypeConversionError {
                key: "unknown".to_string(),
                target_type: "Vec<ConfigValue>".to_string(),
            }),
        }
    }
}

impl FromConfigValue for HashMap<String, ConfigValue> {
    fn from_config_value(value: &ConfigValue) -> ConfigResult<Self> {
        match value {
            ConfigValue::Object(obj) => Ok(obj.clone()),
            _ => Err(ConfigError::TypeConversionError {
                key: "unknown".to_string(),
                target_type: "HashMap<String, ConfigValue>".to_string(),
            }),
        }
    }
}

/// 配置存储结构
///
/// 内部存储配置键值对的映射表
#[derive(Debug, Clone, Default)]
pub struct ConfigStore {
    /// 配置键值对映射
    data: HashMap<String, ConfigValue>,
}

impl ConfigStore {
    /// 创建新的配置存储
    ///
    /// # 返回值
    /// 返回空的配置存储实例
    pub fn new() -> Self {
        ConfigStore {
            data: HashMap::new(),
        }
    }

    /// 插入配置键值对
    ///
    /// # 参数
    /// - `key`: 配置键
    /// - `value`: 配置值
    pub fn insert(&mut self, key: impl Into<String>, value: ConfigValue) {
        self.data.insert(key.into(), value);
    }

    /// 获取配置值
    ///
    /// # 参数
    /// - `key`: 配置键
    ///
    /// # 返回值
    /// 如果存在返回 Some，否则返回 None
    pub fn get(&self, key: &str) -> Option<&ConfigValue> {
        self.data.get(key)
    }

    /// 检查配置键是否存在
    ///
    /// # 参数
    /// - `key`: 配置键
    ///
    /// # 返回值
    /// 如果存在返回 true，否则返回 false
    pub fn contains_key(&self, key: &str) -> bool {
        self.data.contains_key(key)
    }

    /// 移除配置键值对
    ///
    /// # 参数
    /// - `key`: 配置键
    ///
    /// # 返回值
    /// 如果存在并移除成功返回 Some，否则返回 None
    pub fn remove(&mut self, key: &str) -> Option<ConfigValue> {
        self.data.remove(key)
    }

    /// 获取所有配置键
    ///
    /// # 返回值
    /// 返回所有配置键的迭代器
    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.data.keys()
    }

    /// 获取配置项数量
    ///
    /// # 返回值
    /// 返回配置项的数量
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// 检查配置是否为空
    ///
    /// # 返回值
    /// 如果没有配置项返回 true，否则返回 false
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// 合并另一个配置存储
    ///
    /// 当前配置存储中的值会被传入的配置存储中的值覆盖
    ///
    /// # 参数
    /// - `other`: 要合并的配置存储
    pub fn merge(&mut self, other: ConfigStore) {
        for (key, value) in other.data {
            self.data.insert(key, value);
        }
    }

    /// 获取所有配置数据的引用
    ///
    /// # 返回值
    /// 返回配置数据的不可变引用
    pub fn data(&self) -> &HashMap<String, ConfigValue> {
        &self.data
    }

    /// 将配置转换为可序列化的值
    ///
    /// # 返回值
    /// 返回可以序列化的值
    pub fn to_serializable(&self) -> HashMap<String, serde_json::Value> {
        self.data
            .iter()
            .filter_map(|(k, v)| {
                Some((k.clone(), config_value_to_json(v)?))
            })
            .collect()
    }
}

/// 将 ConfigValue 转换为 serde_json::Value
///
/// # 参数
/// - `value`: 配置值
///
/// # 返回值
/// 返回 JSON 值
fn config_value_to_json(value: &ConfigValue) -> Option<serde_json::Value> {
    match value {
        ConfigValue::String(s) => Some(serde_json::Value::String(s.clone())),
        ConfigValue::Integer(i) => Some(serde_json::Value::Number((*i).into())),
        ConfigValue::Float(f) => {
            serde_json::Number::from_f64(*f).map(serde_json::Value::Number)
        }
        ConfigValue::Boolean(b) => Some(serde_json::Value::Bool(*b)),
        ConfigValue::Array(arr) => {
            let json_arr: Vec<serde_json::Value> = arr
                .iter()
                .filter_map(|v| config_value_to_json(v))
                .collect();
            Some(serde_json::Value::Array(json_arr))
        }
        ConfigValue::Object(obj) => {
            let json_obj: serde_json::Map<String, serde_json::Value> = obj
                .iter()
                .filter_map(|(k, v)| Some((k.clone(), config_value_to_json(v)?)))
                .collect();
            Some(serde_json::Value::Object(json_obj))
        }
        ConfigValue::Null => Some(serde_json::Value::Null),
    }
}

/// 从 serde_json::Value 转换为 ConfigValue
///
/// # 参数
/// - `value`: JSON 值
///
/// # 返回值
/// 返回配置值
pub fn json_to_config_value(value: &serde_json::Value) -> ConfigValue {
    match value {
        serde_json::Value::Null => ConfigValue::Null,
        serde_json::Value::Bool(b) => ConfigValue::Boolean(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                ConfigValue::Integer(i)
            } else if let Some(f) = n.as_f64() {
                ConfigValue::Float(f)
            } else {
                ConfigValue::Null
            }
        }
        serde_json::Value::String(s) => ConfigValue::String(s.clone()),
        serde_json::Value::Array(arr) => {
            ConfigValue::Array(arr.iter().map(json_to_config_value).collect())
        }
        serde_json::Value::Object(obj) => {
            ConfigValue::Object(
                obj.iter()
                    .map(|(k, v)| (k.clone(), json_to_config_value(v)))
                    .collect(),
            )
        }
    }
}
