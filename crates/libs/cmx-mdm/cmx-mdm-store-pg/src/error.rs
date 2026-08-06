//! cmx-mdm-store-pg 错误助手（照抄 cmx-dct-store-pg/src/error.rs 的对外两个函数）。
//!
//! - [`api_err`]：普通业务错误 → cmx_api_types::Error（BusinessError，code!=0/HTTP 200）。
//! - [`api_err_db`]：DB 原始错误 → 已翻译的优雅错误（稳定错误码 + 中文），不暴露 PG 英文原文。
//!
//! 注：cmx-dct 的 `map_db_err` / `pg_detail` 依赖 DCT 特有的 `DictView` 类型，MDM 暂无等价 view，
//! M0 不引入；待 M1 激活器 view 类型确定后再按需补齐（target 改为 `cmx_mdm::db`）。

use cmx_api_types::Error;

/// 普通业务错误 → cmx_api_types::Error（照抄 cmx-dct-store-pg/src/error.rs:15-17）。
pub fn api_err(msg: &str) -> Error {
    cmx_biz::BizError::business(msg.to_string()).into()
}

/// DB 原始错误 → 已翻译的优雅错误（照抄 cmx-dct-store-pg/src/error.rs:20-22）。
///
/// 注意：底层函数名是 `from_db_error`（非 `from_db_message`），核实自 cmx-biz/src/error.rs:80。
pub fn api_err_db(raw: &str) -> Error {
    cmx_biz::BizError::from_db_error(raw).into()
}
