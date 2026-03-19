//! 配置加载模块
//! 
//! 加载和解析配置文件

use std::path::Path;

use super::settings::PluginManagerSettings;

/// 配置加载器
pub struct ConfigLoader;

impl ConfigLoader {
    /// 从文件加载配置
    pub fn from_file(path: &Path) -> Result<PluginManagerSettings, String> {
        if !path.exists() {
            return Err(format!("配置文件不存在: {:?}", path));
        }
        
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("读取配置文件失败: {}", e))?;
        
        let extension = path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        
        match extension {
            "json" => Self::parse_json(&content),
            "yaml" | "yml" => Self::parse_yaml(&content),
            "toml" => Self::parse_toml(&content),
            _ => Err(format!("不支持的配置文件格式: {}", extension)),
        }
    }
    
    /// 解析JSON配置
    fn parse_json(content: &str) -> Result<PluginManagerSettings, String> {
        serde_json::from_str(content)
            .map_err(|e| format!("解析JSON配置失败: {}", e))
    }
    
    /// 解析YAML配置
    fn parse_yaml(content: &str) -> Result<PluginManagerSettings, String> {
        serde_yaml::from_str(content)
            .map_err(|e| format!("解析YAML配置失败: {}", e))
    }
    
    /// 解析TOML配置
    fn parse_toml(content: &str) -> Result<PluginManagerSettings, String> {
        toml::from_str(content)
            .map_err(|e| format!("解析TOML配置失败: {}", e))
    }
    
    /// 保存配置到文件
    pub fn save_to_file(settings: &PluginManagerSettings, path: &Path) -> Result<(), String> {
        let extension = path.extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        
        let content = match extension {
            "json" => serde_json::to_string_pretty(settings)
                .map_err(|e| format!("序列化JSON配置失败: {}", e))?,
            "yaml" | "yml" => serde_yaml::to_string(settings)
                .map_err(|e| format!("序列化YAML配置失败: {}", e))?,
            "toml" => toml::to_string_pretty(settings)
                .map_err(|e| format!("序列化TOML配置失败: {}", e))?,
            _ => return Err(format!("不支持的配置文件格式: {}", extension)),
        };
        
        std::fs::write(path, content)
            .map_err(|e| format!("写入配置文件失败: {}", e))
    }
}
