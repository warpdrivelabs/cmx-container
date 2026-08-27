//! 响应信封 re-export——`cmx-api-types` 唯一真源（决策 2-B：flow/rule 错误 code 值域一步迁 404/500）。
//!
//! 受控变更①：错误 code 从 flow/rule 自持值域（Business=1 / BadRequest=2 / NotFound=4 /
//! Internal=5）迁至 api-types 值域（Business=1 不变；BadRequest→400、NotFound→404、
//! Internal→500，HTTP 状态码均不变）。
//! 受控变更③：PageServeError 桥由 cmx-form 内置的 api-types 转换承担（引擎仓本地桥已删），
//! BadRequest 200/1→400/400、Io 200/1→500/500。
//! 成功体 `{code,msg,data}` 逐字节不变；401 为认证中间件裸响应、不经 Error（形态不变）。
//!
//! `FlowError` / `RuleError` 为过渡期别名（见 flow/rule 两仓 resp.rs shim）；终态：handlers
//! 直依 cmx-api-types 后本模块撤销。

pub use cmx_api_types::{ApiResp, Error, Result};
