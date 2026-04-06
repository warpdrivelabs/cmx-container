//! cmx-runtime 错误类型定义

/// WASM 运行时错误类型
#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    /// WASM 编译错误
    #[error("WASM 编译失败: {0}")]
    CompileFailed(String),

    /// WASM 实例化错误
    #[error("WASM 实例化失败: {0}")]
    InstantiationFailed(String),

    /// WASM 函数调用错误
    #[error("WASM 函数调用失败: {0}")]
    InvocationFailed(String),

    /// WASM 模块未找到
    #[error("WASM 模块未找到: {0}")]
    ModuleNotFound(String),

    /// WASM 导出函数未找到
    #[error("导出函数未找到: {module}/{name}")]
    ExportNotFound {
        /// 模块名
        module: String,
        /// 函数名
        name: String,
    },

    /// WASM 内存错误
    #[error("WASM 内存错误: {0}")]
    MemoryError(String),

    /// 宿主函数注册错误
    #[error("宿主函数注册失败: {0}")]
    HostFuncRegistrationFailed(String),

    /// 引擎配置错误
    #[error("引擎配置错误: {0}")]
    ConfigError(String),

    /// 内部错误
    #[error("内部错误: {0}")]
    Internal(String),
}

impl From<wasmtime::Error> for RuntimeError {
    fn from(err: wasmtime::Error) -> Self {
        Self::Internal(err.to_string())
    }
}
