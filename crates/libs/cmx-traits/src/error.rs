//! cmx-traits 错误类型定义。
//!
//! 提供跨模块共享的统一错误类型，包括 trait 调用错误和宿主函数错误。

/// 全局系统身份重复初始化错误。
///
/// 由 [`crate::auth::context_scope::init_system_auth`] 在重复调用时返回。
#[derive(Debug, thiserror::Error)]
#[error("全局系统身份已初始化，禁止重复设置")]
pub struct SetSystemAuthError;

/// cmx-traits 统一错误类型。
///
/// 用于 trait 方法返回值的错误场景定义。
#[derive(Debug, thiserror::Error)]
pub enum TraitError {
    /// 插件未找到。
    #[error("插件未找到: {0}")]
    PluginNotFound(String),

    /// 插件未激活。
    #[error("插件未激活: {0}")]
    PluginNotActive(String),

    /// WASM 模块加载失败。
    #[error("WASM 模块加载失败: {0}")]
    WasmLoadFailed(String),

    /// WASM 函数调用失败。
    #[error("WASM 函数调用失败: {0}")]
    WasmInvokeFailed(String),

    /// WASM 模块未加载。
    #[error("WASM 模块未加载: {0}")]
    WasmNotLoaded(String),

    /// 编排执行失败。
    #[error("编排执行失败: {0}")]
    OrchestrationFailed(String),

    /// 内部错误。
    #[error("内部错误: {0}")]
    Internal(String),

    /// 资源未找到。
    #[error("资源未找到: {0}")]
    NotFound(String),

    /// 业务逻辑错误。
    #[error("业务错误: {0}")]
    Business(String),

    /// 权限不足。
    #[error("权限不足: {0}")]
    Forbidden(String),

    /// 资源已初始化。
    #[error("{0}")]
    AlreadyInitialized(String),
}

/// 宿主函数错误类型。
///
/// 用于 WASM 宿主函数注册和执行过程中的错误定义。
#[derive(Debug, thiserror::Error)]
pub enum HostFuncError {
    /// 函数注册失败。
    #[error("函数注册失败 [{namespace}/{name}]: {reason}")]
    RegistrationFailed {
        /// 命名空间。
        namespace: String,
        /// 函数名。
        name: String,
        /// 失败原因。
        reason: String,
    },

    /// 函数执行失败。
    #[error("函数执行失败 [{namespace}/{name}]: {reason}")]
    ExecutionFailed {
        /// 命名空间。
        namespace: String,
        /// 函数名。
        name: String,
        /// 失败原因。
        reason: String,
    },

    /// WASM 内存越界访问。
    #[error("WASM 内存越界访问 (offset={offset}, len={len})")]
    MemoryOutOfBounds {
        /// 偏移量。
        offset: u32,
        /// 长度。
        len: u32,
    },

    /// 无效参数。
    #[error("无效参数: {0}")]
    InvalidParam(String),
}

impl HostFuncError {
    /// 创建注册失败错误。
    ///
    /// # Arguments
    ///
    /// * `namespace` - 函数命名空间。
    /// * `name` - 函数名。
    /// * `reason` - 失败原因。
    ///
    /// # Returns
    ///
    /// 返回 [`HostFuncError::RegistrationFailed`] 变体。
    pub fn registration_failed(namespace: &str, name: &str, reason: impl Into<String>) -> Self {
        Self::RegistrationFailed {
            namespace: namespace.to_string(),
            name: name.to_string(),
            reason: reason.into(),
        }
    }

    /// 创建执行失败错误。
    ///
    /// # Arguments
    ///
    /// * `namespace` - 函数命名空间。
    /// * `name` - 函数名。
    /// * `reason` - 失败原因。
    ///
    /// # Returns
    ///
    /// 返回 [`HostFuncError::ExecutionFailed`] 变体。
    pub fn execution_failed(namespace: &str, name: &str, reason: impl Into<String>) -> Self {
        Self::ExecutionFailed {
            namespace: namespace.to_string(),
            name: name.to_string(),
            reason: reason.into(),
        }
    }

    /// 创建无效函数错误。
    ///
    /// # Arguments
    ///
    /// * `name` - 不存在的函数名。
    ///
    /// # Returns
    ///
    /// 返回 [`HostFuncError::ExecutionFailed`] 变体，命名空间为空，原因为"函数不存在"。
    pub fn invalid_function(name: &str) -> Self {
        Self::ExecutionFailed {
            namespace: String::new(),
            name: name.to_string(),
            reason: "函数不存在".to_string(),
        }
    }
}
