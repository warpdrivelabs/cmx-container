//! Extism 错误类型

/// Extism 错误类型
#[derive(Debug, Clone, thiserror::Error)]
pub enum ExtismError {
    #[error("插件加载失败: {0}")]
    PluginLoadFailed(String),
    
    #[error("插件调用失败: {0}")]
    PluginCallFailed(String),
    
    #[error("配置错误: {0}")]
    ConfigError(String),
    
    #[error("内部错误: {0}")]
    InternalError(String),
}
