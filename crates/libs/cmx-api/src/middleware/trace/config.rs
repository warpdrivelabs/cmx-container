//! 追踪配置模块。
//!
//! 定义请求追踪中间件的运行参数，包括 body 读取上限和预览截断长度。

/// 追踪模式。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraceMode {
    /// 轻量模式，仅记录方法、路径、查询参数、状态码、耗时。
    Lightweight,
    /// 详细模式，记录完整请求头、请求体、响应体（脱敏）。
    Verbose,
}

/// 追踪配置。
#[derive(Clone, Debug)]
pub struct TraceConfig {
    /// 请求体最大读取字节数，仅 [`TraceMode::Verbose`] 模式生效。
    pub max_request_body_size: usize,
    /// 响应体最大读取字节数，仅 [`TraceMode::Verbose`] 模式生效。
    pub max_response_body_size: usize,
    /// 响应体预览截断长度，仅 [`TraceMode::Verbose`] 模式生效。
    pub max_preview_length: usize,
}

impl Default for TraceConfig {
    fn default() -> Self {
        Self {
            max_request_body_size: 10 * 1024 * 1024,
            max_response_body_size: 5 * 1024 * 1024,
            max_preview_length: 2000,
        }
    }
}
