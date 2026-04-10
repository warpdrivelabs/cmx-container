//! Extism 错误类型
//!
//! 定义 WASM 运行时引擎可能产生的错误类型。

/// Extism 错误类型
///
/// 封装 WASM 运行时操作过程中可能发生的各类错误。
#[derive(Debug, Clone, thiserror::Error)]
pub enum ExtismError {
    /// 插件加载失败
    ///
    /// WASM 模块加载、编译或实例化过程中发生错误
    #[error("插件加载失败: {0}")]
    PluginLoadFailed(String),
    
    /// 插件调用失败
    ///
    /// WASM 函数执行过程中发生错误
    #[error("插件调用失败: {0}")]
    PluginCallFailed(String),
    
    /// 配置错误
    ///
    /// 引擎配置参数无效或不完整
    #[error("配置错误: {0}")]
    ConfigError(String),
    
    /// 内部错误
    ///
    /// 其他未分类的内部错误
    #[error("内部错误: {0}")]
    InternalError(String),
}
