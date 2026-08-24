//! 页面投递内部中性错误：由调用方的泛型 `E` 渲染为各自历史错误体。

use axum::response::{IntoResponse, Response};

use cmx_api_types::Error;

/// 页面读取过程中的两类可预期错误（与五份副本的错误语义一一对应）。
///
/// 经 `From<PageServeError>` 折入各引擎自持错误类型：
/// - rule / flow → `business` / `not_found`（body code=1 / 4）；
/// - mdm / model / report → [`Error`]（bad_request=400 / not_found=404）。
#[derive(Debug, Clone, thiserror::Error)]
pub enum PageServeError {
    /// relPath 非法（越界 / 空段），对应各副本的 business/bad_request 分支。
    #[error("{0}")]
    BadRequest(String),
    /// 页面索引中不存在，或源文件缺失。
    #[error("{0}")]
    NotFound(String),
}

impl From<PageServeError> for Error {
    fn from(e: PageServeError) -> Self {
        match e {
            PageServeError::BadRequest(m) => Error::bad_request(m),
            PageServeError::NotFound(m) => Error::not_found(m),
        }
    }
}

impl IntoResponse for PageServeError {
    fn into_response(self) -> Response {
        Error::from(self).into_response()
    }
}
