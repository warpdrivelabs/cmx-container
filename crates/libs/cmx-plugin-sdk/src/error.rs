//! 插件错误类型

/// 插件错误类型
#[derive(Debug, Clone, thiserror::Error)]
pub enum PluginError {
    #[error("宿主函数调用失败: {0}")]
    HostCallFailed(String),
    
    #[error("序列化失败: {0}")]
    SerializationError(String),
    
    #[error("反序列化失败: {0}")]
    DeserializationError(String),
    
    #[error("参数错误: {0}")]
    ArgumentError(String),
    
    #[error("内部错误: {0}")]
    InternalError(String),
}
