//! 两族认证中间件的公共小件（401 响应体等）。

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

/// 401 响应（族 B 形态：`{code,msg}` Json 体，形状对齐各引擎 resp 的错误信封）。
///
/// 收编自 cmx-flow-app / cmx-rule-app auth.rs 中逐字相同的 `unauthorized()`——认证失败走
/// 裸响应、不经各仓错误枚举（故 P4 错误信封统一为 cmx-api-types 后，401 形态仍不变）。
/// 族 A（model/mdm）的 401 带 `WWW-Authenticate` 头、形态不同，见 [`super::delegated`] 内实现。
pub fn unauthorized(msg: &str) -> Response {
    (
        StatusCode::UNAUTHORIZED,
        axum::Json(serde_json::json!({ "code": 401, "msg": msg })),
    )
        .into_response()
}
