//! 服务层错误类型
//!
//! 定义 cmx-service 中所有操作可能产生的错误类型。

use std::string::String;

use thiserror::Error;

use cmx_traits::error::TraitError;

/// 服务层错误
#[derive(Debug, Error)]
pub enum ServiceError {
    /// 插件未找到
    #[error("插件未找到: {0}")]
    PluginNotFound(String),

    /// 插件未激活
    #[error("插件未激活: {0}")]
    PluginNotActive(String),

    /// 插件 WASM 模块未加载
    #[error("插件 {0} 的 WASM 模块未加载")]
    WasmNotLoaded(String),

    /// 运行时调用失败
    #[error("运行时调用失败: {0}")]
    InvokeFailed(String),

    /// 编排执行失败
    #[error("编排执行失败，步骤 {step_id}: {message}")]
    OrchestrationFailed {
        /// 失败的步骤ID
        step_id: String,
        /// 错误信息
        message: String,
    },

    /// 输入数据解析失败
    #[error("输入数据解析失败: {0}")]
    InputParseError(String),

    /// 输出数据序列化失败
    #[error("输出数据序列化失败: {0}")]
    OutputSerializeError(String),

    /// 数据库操作失败
    #[error("数据库操作失败: {0}")]
    DatabaseError(String),

    /// Trait 错误
    #[error("{0}")]
    TraitError(#[from] TraitError),

    /// 节点执行失败（携带步骤上下文）
    #[error("节点执行失败 [{node_type}] {node_name}({node_id}): {detail}")]
    NodeExecutionFailed {
        /// 失败的节点ID
        node_id: String,
        /// 失败的节点名称
        node_name: String,
        /// 失败的节点类型
        node_type: String,
        /// 具体错误信息
        detail: String,
    },

    /// 事务回滚
    #[error("事务回滚: txn_id={txn_id}, reason={reason}")]
    TransactionRolledBack {
        /// 事务ID
        txn_id: String,
        /// 回滚原因
        reason: String,
    },

    /// 内部错误
    #[error("内部错误: {0}")]
    InternalError(String),
}

impl ServiceError {
    /// 创建插件未找到错误
    pub fn plugin_not_found(plugin_id: &str) -> Self {
        Self::PluginNotFound(plugin_id.to_string())
    }

    /// 创建插件未激活错误
    pub fn plugin_not_active(plugin_id: &str) -> Self {
        Self::PluginNotActive(plugin_id.to_string())
    }

    /// 创建 WASM 未加载错误
    pub fn wasm_not_loaded(plugin_id: &str) -> Self {
        Self::WasmNotLoaded(plugin_id.to_string())
    }

    /// 创建编排失败错误
    pub fn orchestration_failed(step_id: &str, message: &str) -> Self {
        Self::OrchestrationFailed {
            step_id: step_id.to_string(),
            message: message.to_string(),
        }
    }
}
