//! 校验失败响应构造（doc/dct 等 API handler 共用）。
//!
//! 把 `Vec<Violation>` 统一封装成结构化 422 响应信封，消除各 handler 重复手写。

use serde_json::{Value, json};

use crate::ApiResp;
use cmx_biz::errcode::Violation;

/// 构造校验失败响应：`{code:422, msg, data:{violations:[...]}}`（结构化，前端逐行逐列高亮）。
///
/// 各回存 handler（dct/doc 等）在 changeset 校验失败时统一调用，保证 422 信封形态一致。
pub fn validation_fail_resp(violations: &[Violation]) -> ApiResp<Value> {
    ApiResp::fail_with_data(
        422,
        format!("数据校验未通过（{} 处）", violations.len()),
        json!({ "violations": violations }),
    )
}
