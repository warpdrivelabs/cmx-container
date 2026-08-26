//! HTTP 出站操作相关类型（`cmx:http` 宿主函数，W4）。
//!
//! 定义宿主与 WASM 之间受控 egress 的请求和响应结构体。插件经 `http_fetch` 发起出站请求，
//! 宿主侧按 egress 策略（域名白名单 / SSRF 防护 / 超时 / 大小限制 / 配额）裁决后代为访问外部
//! HTTP(S) 资源。**能力受"仅声明命名空间可 import"约束**：未在 manifest 申请 `cmx:http` 的
//! 插件不会拿到此 import，无法出站。

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// HTTP 出站请求。
///
/// 用于 WASM 插件向宿主发起受控的 HTTP(S) 出站调用。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HttpRequest {
    /// 目标 URL（必须为 http/https；host 须命中 egress 白名单，且非内网/元数据地址）。
    pub url: String,
    /// HTTP 方法（GET/POST/PUT/PATCH/DELETE/HEAD；缺省 GET，大小写不敏感）。
    #[serde(default)]
    pub method: Option<String>,
    /// 请求头（键值对）。宿主可能剥离敏感/受控头（如 Host 由 URL 决定）。
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    /// 请求体（原样字节；宿主按 `max_body_bytes` 限制）。
    #[serde(default)]
    pub body: Option<Vec<u8>>,
    /// 单次请求超时（毫秒，可选；宿主按 policy 的 `timeout_ms` 取 min 兜底）。
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

/// HTTP 出站响应。
///
/// 宿主返回给 WASM 插件的出站结果；`success=false` 时 `error` 载明拒绝/失败原因
/// （策略拒绝 / SSRF 拦截 / 超时 / 上游错误）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpResponse {
    /// 是否成功（策略放行且上游返回）。
    pub success: bool,
    /// HTTP 状态码（成功时）。
    #[serde(default)]
    pub status: Option<u16>,
    /// 响应头（成功时）。
    #[serde(default)]
    pub headers: BTreeMap<String, String>,
    /// 响应体（成功时；受 `max_body_bytes` 截断，截断则置 `truncated=true`）。
    #[serde(default)]
    pub body: Option<Vec<u8>>,
    /// 响应体是否因超限被截断。
    #[serde(default)]
    pub truncated: bool,
    /// 错误信息（失败/被拒时）。
    #[serde(default)]
    pub error: Option<String>,
}

impl HttpResponse {
    /// 构造被拒绝/失败响应。
    pub fn err(msg: impl Into<String>) -> Self {
        Self {
            success: false,
            status: None,
            headers: BTreeMap::new(),
            body: None,
            truncated: false,
            error: Some(msg.into()),
        }
    }
}
