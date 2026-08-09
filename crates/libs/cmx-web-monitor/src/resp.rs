//! 统一响应信封 `{code,msg,data}`（与 cmx-flow-app::resp 逐字节对齐，保证旧 `/clients` 端点不变）。

use serde::Serialize;

/// 统一 API 响应信封。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiResp<T> {
    /// 业务码（0 = 成功）。
    pub code: u16,
    /// 提示消息。
    pub msg: String,
    /// 数据体。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
}

impl<T> ApiResp<T> {
    /// 成功响应（code=0）。
    pub fn ok(data: T) -> Self {
        Self { code: 0, msg: "ok".to_string(), data: Some(data) }
    }
}
