//! 配置文件解析器模块
//!
//! 提供对 .env、JSON 和 TOML 格式配置文件的解析支持

use std::fs;
use std::path::Path;

use super::error::{ConfigError, ConfigResult};
use super::value::{json_to_config_value, ConfigValue, ConfigStore};

/// 配置文件解析器 trait
///
/// 定义配置文件解析的通用接口
pub trait ConfigParser {
    /// 解析配置文件
    ///
    /// # 参数
    /// - `content`: 文件内容
    ///
    /// # 返回值
    /// 成功返回配置存储，失败返回错误
    fn parse(&self, content: &str) -> ConfigResult<ConfigStore>;

    /// 从文件解析配置
    ///
    /// # 参数
    /// - `path`: 文件路径
    ///
    /// # 返回值
    /// 成功返回配置存储，失败返回错误
    fn parse_file(&self, path: &Path) -> ConfigResult<ConfigStore> {
        let content = fs::read_to_string(path).map_err(|e| ConfigError::FileReadError {
            path: path.to_path_buf(),
            source: e,
        })?;
        self.parse(&content)
    }

    /// 获取支持的文件扩展名
    ///
    /// # 返回值
    /// 返回支持的文件扩展名列表
    fn supported_extensions(&self) -> Vec<&'static str>;
}

/// TOML 配置文件解析器
pub struct TomlParser;

impl TomlParser {
    /// 创建新的 TOML 解析器
    pub fn new() -> Self {
        TomlParser
    }
}

impl Default for TomlParser {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigParser for TomlParser {
    fn parse(&self, content: &str) -> ConfigResult<ConfigStore> {
        let value: toml::Value = toml::from_str(content).map_err(|e| ConfigError::TomlParseError { source: e })?;
        
        let mut store = ConfigStore::new();
        if let toml::Value::Table(table) = value {
            for (key, val) in table {
                flatten_toml_value(&key, &val, &mut store);
            }
        }
        
        Ok(store)
    }

    fn supported_extensions(&self) -> Vec<&'static str> {
        vec!["toml"]
    }
}

/// 递归扁平化 TOML 值为点分隔的键
///
/// # 参数
/// - `prefix`: 当前键前缀
/// - `value`: TOML 值
/// - `store`: 配置存储
fn flatten_toml_value(prefix: &str, value: &toml::Value, store: &mut ConfigStore) {
    match value {
        toml::Value::Table(table) => {
            for (key, val) in table {
                let new_prefix = format!("{}.{}", prefix, key);
                flatten_toml_value(&new_prefix, val, store);
            }
        }
        _ => {
            let config_value = toml_to_config_value(value);
            store.insert(prefix.to_string(), config_value);
        }
    }
}

/// 将 TOML 值转换为 ConfigValue
///
/// # 参数
/// - `value`: TOML 值
///
/// # 返回值
/// 返回配置值
fn toml_to_config_value(value: &toml::Value) -> ConfigValue {
    match value {
        toml::Value::String(s) => ConfigValue::String(s.clone()),
        toml::Value::Integer(i) => ConfigValue::Integer(*i),
        toml::Value::Float(f) => ConfigValue::Float(*f),
        toml::Value::Boolean(b) => ConfigValue::Boolean(*b),
        toml::Value::Datetime(dt) => ConfigValue::String(dt.to_string()),
        toml::Value::Array(arr) => {
            ConfigValue::Array(arr.iter().map(toml_to_config_value).collect())
        }
        toml::Value::Table(table) => {
            ConfigValue::Object(
                table
                    .iter()
                    .map(|(k, v)| (k.clone(), toml_to_config_value(v)))
                    .collect(),
            )
        }
    }
}

/// JSON 配置文件解析器
pub struct JsonParser;

impl JsonParser {
    /// 创建新的 JSON 解析器
    pub fn new() -> Self {
        JsonParser
    }
}

impl Default for JsonParser {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigParser for JsonParser {
    fn parse(&self, content: &str) -> ConfigResult<ConfigStore> {
        let value: serde_json::Value = serde_json::from_str(content).map_err(|e| ConfigError::JsonParseError { source: e })?;
        
        let mut store = ConfigStore::new();
        if let serde_json::Value::Object(map) = value {
            for (key, val) in map {
                flatten_json_value(&key, &val, &mut store);
            }
        }
        
        Ok(store)
    }

    fn supported_extensions(&self) -> Vec<&'static str> {
        vec!["json"]
    }
}

/// 递归扁平化 JSON 值为点分隔的键
///
/// # 参数
/// - `prefix`: 当前键前缀
/// - `value`: JSON 值
/// - `store`: 配置存储
fn flatten_json_value(prefix: &str, value: &serde_json::Value, store: &mut ConfigStore) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, val) in map {
                let new_prefix = format!("{}.{}", prefix, key);
                flatten_json_value(&new_prefix, val, store);
            }
        }
        _ => {
            let config_value = json_to_config_value(value);
            store.insert(prefix.to_string(), config_value);
        }
    }
}

/// .env 配置文件解析器
pub struct EnvParser;

impl EnvParser {
    /// 创建新的 .env 解析器
    pub fn new() -> Self {
        EnvParser
    }

    /// 解析单行 .env 内容
    ///
    /// # 参数
    /// - `line`: 单行内容
    /// - `line_num`: 行号（用于错误提示）
    ///
    /// # 返回值
    /// 成功返回键值对，失败返回错误
    fn parse_line(line: &str, line_num: usize) -> ConfigResult<Option<(String, String)>> {
        // 去除首尾空白
        let trimmed = line.trim();
        
        // 跳过空行和注释行
        if trimmed.is_empty() || trimmed.starts_with('#') {
            return Ok(None);
        }
        
        // 查找等号位置
        let eq_pos = trimmed.find('=').ok_or_else(|| ConfigError::EnvParseError {
            message: format!("第 {} 行: 缺少等号分隔符", line_num),
        })?;
        
        // 提取键和值
        let key = trimmed[..eq_pos].trim().to_string();
        let mut value = trimmed[eq_pos + 1..].trim().to_string();
        
        // 验证键不为空
        if key.is_empty() {
            return Err(ConfigError::EnvParseError {
                message: format!("第 {} 行: 键不能为空", line_num),
            });
        }
        
        // 处理引号包围的值
        if (value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\''))
        {
            value = value[1..value.len() - 1].to_string();
        }
        
        Ok(Some((key, value)))
    }

    /// 推断字符串值的类型并转换为 ConfigValue
    ///
    /// # 参数
    /// - `value`: 字符串值
    ///
    /// # 返回值
    /// 返回推断后的配置值
    fn infer_value_type(value: String) -> ConfigValue {
        // 尝试解析为布尔值
        match value.to_lowercase().as_str() {
            "true" | "yes" | "on" => return ConfigValue::Boolean(true),
            "false" | "no" | "off" => return ConfigValue::Boolean(false),
            _ => {}
        }
        
        // 尝试解析为整数
        if let Ok(i) = value.parse::<i64>() {
            return ConfigValue::Integer(i);
        }
        
        // 尝试解析为浮点数
        if let Ok(f) = value.parse::<f64>() {
            return ConfigValue::Float(f);
        }
        
        // 默认作为字符串
        ConfigValue::String(value)
    }
}

impl Default for EnvParser {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigParser for EnvParser {
    fn parse(&self, content: &str) -> ConfigResult<ConfigStore> {
        let mut store = ConfigStore::new();
        
        for (line_num, line) in content.lines().enumerate() {
            if let Some((key, value)) = Self::parse_line(line, line_num + 1)? {
                let config_value = Self::infer_value_type(value);
                store.insert(key, config_value);
            }
        }
        
        Ok(store)
    }

    fn supported_extensions(&self) -> Vec<&'static str> {
        vec!["env"]
    }
}

/// 自动检测文件格式并解析
///
/// 根据文件扩展名自动选择合适的解析器
///
/// # 参数
/// - `path`: 文件路径
///
/// # 返回值
/// 成功返回配置存储，失败返回错误
pub fn parse_file_auto(path: &Path) -> ConfigResult<ConfigStore> {
    // 检查文件名是否为 .env
    if path.file_name().map(|name| name == ".env").unwrap_or(false) {
        return EnvParser::new().parse_file(path);
    }
    
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_lowercase())
        .unwrap_or_default();
    
    match extension.as_str() {
        "toml" => TomlParser::new().parse_file(path),
        "json" => JsonParser::new().parse_file(path),
        "env" => EnvParser::new().parse_file(path),
        _ => Err(ConfigError::InvalidPath {
            path: format!("不支持的配置文件格式: {}", extension),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_toml_parser() {
        let content = r#"
[database]
host = "localhost"
port = 5432
enabled = true
"#;
        let parser = TomlParser::new();
        let store = parser.parse(content).unwrap();
        
        // 扁平化后的键
        assert_eq!(store.get("database.host").unwrap().as_str().unwrap(), "localhost");
        assert_eq!(store.get("database.port").unwrap().as_integer().unwrap(), 5432);
        assert_eq!(store.get("database.enabled").unwrap().as_boolean().unwrap(), true);
    }

    #[test]
    fn test_json_parser() {
        let content = r#"{
    "database": {
        "host": "localhost",
        "port": 5432,
        "enabled": true
    }
}"#;
        let parser = JsonParser::new();
        let store = parser.parse(content).unwrap();
        
        // 扁平化后的键
        assert_eq!(store.get("database.host").unwrap().as_str().unwrap(), "localhost");
        assert_eq!(store.get("database.port").unwrap().as_integer().unwrap(), 5432);
        assert_eq!(store.get("database.enabled").unwrap().as_boolean().unwrap(), true);
    }

    #[test]
    fn test_env_parser() {
        let content = r#"
# Database configuration
DB_HOST=localhost
DB_PORT=5432
DB_ENABLED=true
DB_TIMEOUT=30.5
"#;
        let parser = EnvParser::new();
        let store = parser.parse(content).unwrap();
        
        assert_eq!(store.get("DB_HOST").unwrap().as_str().unwrap(), "localhost");
        assert_eq!(store.get("DB_PORT").unwrap().as_integer().unwrap(), 5432);
        assert_eq!(store.get("DB_ENABLED").unwrap().as_boolean().unwrap(), true);
        assert_eq!(store.get("DB_TIMEOUT").unwrap().as_float().unwrap(), 30.5);
    }

    #[test]
    fn test_env_parser_with_quotes() {
        let content = r#"
KEY1="value with spaces"
KEY2='another value'
"#;
        let parser = EnvParser::new();
        let store = parser.parse(content).unwrap();
        
        assert_eq!(store.get("KEY1").unwrap().as_str().unwrap(), "value with spaces");
        assert_eq!(store.get("KEY2").unwrap().as_str().unwrap(), "another value");
    }
}
