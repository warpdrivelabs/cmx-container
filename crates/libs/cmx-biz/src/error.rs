//! cmx-biz 错误类型定义

use thiserror::Error;

/// cmx-biz 统一错误类型
#[derive(Debug, Error)]
pub enum BizError {
    /// 数据库 CRUD 操作错误
    #[error("数据库操作错误: {0}")]
    Crud(#[from] cmx_database::crud::ServiceError),

    /// 数据库管理错误
    #[error("数据库管理错误: {0}")]
    Database(String),

    /// 业务逻辑错误
    #[error("业务错误: {0}")]
    Business(String),

    /// 数据未找到
    #[error("数据未找到: {0}")]
    NotFound(String),

    /// 资源冲突（乐观锁：单据已被他人修改）。映射 HTTP 409。
    #[error("{0}")]
    Conflict(String),

    /// JSON 序列化/反序列化错误
    #[error("JSON 解析错误: {0}")]
    SerdeJson(#[from] serde_json::Error),

    /// 插件函数调用错误
    #[error("插件函数调用错误: {0}")]
    PluginInvoke(String),

    /// 服务编排错误
    #[error("服务编排错误: {0}")]
    Orchestration(String),

    /// 内部错误
    #[error("内部错误: {0}")]
    Internal(String),
}

/// cmx-biz 统一结果类型别名
pub type Result<T> = core::result::Result<T, BizError>;

impl BizError {
    /// 创建业务错误
    pub fn business(msg: impl Into<String>) -> Self {
        Self::Business(msg.into())
    }

    /// 创建未找到错误
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::NotFound(msg.into())
    }

    /// 创建资源冲突错误（乐观锁，映射 HTTP 409）
    pub fn conflict(msg: impl Into<String>) -> Self {
        Self::Conflict(msg.into())
    }

    /// 批量保存里给错误加「第 N 单」定位前缀，**保留原变体**（如 Conflict 仍映射 409）。
    pub fn from_batch_item(index: usize, e: BizError) -> Self {
        let tag = format!("第 {} 单保存失败: ", index + 1);
        match e {
            BizError::Conflict(m) => BizError::Conflict(format!("{tag}{m}")),
            BizError::NotFound(m) => BizError::NotFound(format!("{tag}{m}")),
            BizError::Business(m) => BizError::Business(format!("{tag}{m}")),
            other => BizError::Internal(format!("{tag}{other}")),
        }
    }

    /// 创建内部错误
    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal(msg.into())
    }
}

/// 支持 BizError 到 [`cmx_traits::error::TraitError`] 的转换。
///
/// 使 cmx-biz 实现 `FunctionInvoker` trait 时，可将基础设施错误统一映射为
/// 抽象层错误类型（[`cmx_traits::error::TraitError`]），供 cmx-rpc 等基础设施层消费。
/// 满足孤儿规则：BizError 为本 crate 定义类型。
impl From<BizError> for cmx_traits::error::TraitError {
    fn from(e: BizError) -> Self {
        match e {
            BizError::Business(msg) => cmx_traits::error::TraitError::Business(msg),
            BizError::NotFound(msg) => cmx_traits::error::TraitError::NotFound(msg),
            // TraitError 无 Conflict 语义（rpc/wasm 层不区分）→ 归为 Business，保留文案。
            BizError::Conflict(msg) => cmx_traits::error::TraitError::Business(msg),
            BizError::PluginInvoke(msg) => cmx_traits::error::TraitError::WasmInvokeFailed(msg),
            BizError::Orchestration(msg) => cmx_traits::error::TraitError::OrchestrationFailed(msg),
            BizError::Crud(err) => {
                cmx_traits::error::TraitError::Internal(format!("数据库操作错误: {}", err))
            }
            BizError::Database(msg) => cmx_traits::error::TraitError::Internal(msg),
            BizError::SerdeJson(err) => {
                cmx_traits::error::TraitError::Internal(format!("JSON 解析错误: {}", err))
            }
            BizError::Internal(msg) => cmx_traits::error::TraitError::Internal(msg),
        }
    }
}

/// 支持 BizError 到 cmx_api_types::Error 的转换，
/// 使 cmx-api handler 中可以使用 `?` 操作符传播业务层错误。
impl From<BizError> for cmx_api_types::Error {
    fn from(e: BizError) -> Self {
        match e {
            BizError::Crud(e) => cmx_api_types::Error::from(e),
            BizError::Business(msg) => cmx_api_types::Error::business_error(msg),
            BizError::NotFound(msg) => cmx_api_types::Error::not_found(msg),
            BizError::Conflict(msg) => cmx_api_types::Error::conflict(msg),
            BizError::SerdeJson(e) => cmx_api_types::Error::from(e),
            BizError::Database(msg)
            | BizError::PluginInvoke(msg)
            | BizError::Orchestration(msg)
            | BizError::Internal(msg) => cmx_api_types::Error::internal_error(msg),
        }
    }
}
