use crate::middleware;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use derive_more::From;
use serde::Serialize;
use serde_json::Value;
use serde_with::{DisplayFromStr, serde_as};
use std::sync::Arc;
use tracing::{debug, warn};

/// 结果类型，包含成功值或错误
pub type Result<T> = core::result::Result<T, Error>;

/// Web 层错误类型
#[serde_as]
#[derive(Debug, Serialize, From)]
#[serde(tag = "type", content = "data")]
pub enum Error {
    // -- 外部模块错误
    #[from]
    SerdeJson(#[serde_as(as = "DisplayFromStr")] serde_json::Error),

	ReqStampNotInReqExt,
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        debug!("{:<12} - model::Error {self:?}", "INTO_RES");

        // 创建一个占位符 Axum 响应。
        let mut response = StatusCode::INTERNAL_SERVER_ERROR.into_response();
        // 将错误插入到响应中。
        response.extensions_mut().insert(Arc::new(self));
        response
    }
}

impl core::fmt::Display for Error {
    fn fmt(&self, fmt: &mut core::fmt::Formatter) -> core::result::Result<(), core::fmt::Error> {
        write!(fmt, "{self:?}")
    }
}

impl std::error::Error for Error {}
