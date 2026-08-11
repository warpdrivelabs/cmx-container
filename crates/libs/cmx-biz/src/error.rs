//! cmx-biz 错误类型定义

use thiserror::Error;

use crate::errcode::{CmxErrCode, Violation};

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

    /// 落库前列级校验失败（结构化 violations，一次回报全部）。映射 HTTP 422。
    #[error("数据校验未通过（{} 处）", .0.len())]
    Validation(Vec<Violation>),

    /// 数据库约束错误（PG 原始错误已翻译成优雅提示 + 稳定错误码）。
    #[error("{message}")]
    DbConstraint { code: CmxErrCode, message: String },

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

    /// 创建列级校验失败错误（结构化 violations）。
    pub fn validation(violations: Vec<Violation>) -> Self {
        Self::Validation(violations)
    }

    /// 从 PG 原始错误串创建「已翻译的约束错误」——不再暴露英文原文给前端。
    pub fn from_db_error(raw: &str) -> Self {
        let code = crate::errcode::classify_db_error(raw);
        let detail = crate::errcode::brief_db_detail(raw);
        // NOT NULL 错误模板含 {caption} 占位（「{caption}」不能为空），用提取的列名填充，
        // 避免前端看到未渲染的 「{caption}」字面量。其他模板用 {detail}。
        let params: Vec<(&str, String)> = match code {
            crate::errcode::CmxErrCode::NotNullDbViolation => {
                vec![("caption", detail.clone()), ("detail", detail)]
            }
            _ => vec![("detail", detail)],
        };
        let message = crate::errcode::render(code.message_template(), &params);
        Self::DbConstraint { code, message }
    }

    /// 取结构化 violations（仅 Validation 变体有）。供 handler 组装 `data.violations`。
    pub fn violations(&self) -> Option<&[Violation]> {
        match self {
            Self::Validation(v) => Some(v),
            _ => None,
        }
    }

    /// 批量保存里给错误加「第 N 单」定位前缀，**保留原变体**（如 Conflict 仍映射 409）。
    pub fn from_batch_item(index: usize, e: BizError) -> Self {
        let tag = format!("第 {} 单保存失败: ", index + 1);
        match e {
            BizError::Conflict(m) => BizError::Conflict(format!("{tag}{m}")),
            BizError::NotFound(m) => BizError::NotFound(format!("{tag}{m}")),
            BizError::Business(m) => BizError::Business(format!("{tag}{m}")),
            // Validation/DbConstraint 已是结构化/优雅错误，不加前缀污染（保留原样）。
            v @ BizError::Validation(_) => v,
            c @ BizError::DbConstraint { .. } => c,
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
            // 校验/约束错误在 rpc/wasm 层归为 Business，保留优雅文案。
            BizError::Validation(vs) => cmx_traits::error::TraitError::Business(
                vs.first()
                    .map(|v| v.message.clone())
                    .unwrap_or_else(|| "数据校验未通过".into()),
            ),
            BizError::DbConstraint { message, .. } => {
                cmx_traits::error::TraitError::Business(message)
            }
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
            // 列级校验失败：铺平成消息列表（走 422）。handler 通常直接返回结构化 data.violations，
            // 不经此路径；此为 `?` 冒泡时的兜底。
            BizError::Validation(vs) => {
                let msgs: Vec<String> = vs.into_iter().map(|v| v.message).collect();
                cmx_api_types::Error::validation_error(msgs)
            }
            // 已翻译的约束错误：按错误码的 HTTP 类别映射，绝不暴露 PG 原文。
            BizError::DbConstraint { code, message } => {
                use cmx_api_types::ErrCode;
                match code.http_code() {
                    ErrCode::Conflict => cmx_api_types::Error::conflict(message),
                    ErrCode::ValidationError => {
                        cmx_api_types::Error::validation_error(vec![message])
                    }
                    ErrCode::BadRequest => cmx_api_types::Error::bad_request(message),
                    _ => cmx_api_types::Error::business_error(message),
                }
            }
            BizError::SerdeJson(e) => cmx_api_types::Error::from(e),
            BizError::Database(msg)
            | BizError::PluginInvoke(msg)
            | BizError::Orchestration(msg)
            | BizError::Internal(msg) => cmx_api_types::Error::internal_error(msg),
        }
    }
}

// ============================================================================
// 公共 DB 错误助手（dct/doc/mdm/rpt/code 等 store-pg 共用，消除各 crate 复刻）
// ============================================================================

/// 普通业务错误 → `cmx_api_types::Error`（BusinessError，code!=0/HTTP 200）。
pub fn api_err(msg: &str) -> cmx_api_types::Error {
    BizError::business(msg.to_string()).into()
}

/// DB 原始错误字符串 → 已翻译的优雅错误（稳定错误码 + 中文），不暴露 PG 英文原文。
///
/// `raw` 应为 `cmx_database_pg::pg_detail` 抽取的真实明细（含 SQLSTATE/DETAIL/constraint），
/// 经 [`BizError::from_db_error`] 归类成 `CmxErrCode`。
///
/// 注：`pg_detail` 原位于本模块，因入参为 `cmx_database_pg::Error`，已下沉至
/// `cmx_database_pg::pg_detail`（归属地更自然，供 code/dct/mdm/rpt 等直接引用）。
pub fn api_err_db(raw: &str) -> cmx_api_types::Error {
    BizError::from_db_error(raw).into()
}
