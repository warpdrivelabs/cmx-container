//! cmx-proxy-core —— 反向代理**转发核**（三反代壳 flow/rpt/rule 的公共实现）。
//!
//! 平台「后端一芯双壳」架构里，独立微服务域（流程/报表/规则）在门户侧各有一个反代薄壳
//! （`cmx-flow-api` / `cmx-rpt-api` / `cmx-rule-api`），把平台同源 API 透明转发到远程微服务。
//! 三壳的转发逻辑原本各持一份复制（拼 URL 后的流式转发 + 头处理 + 出站鉴权 + 错误兜底），
//! 现收敛到本 crate：**转发行为一处定义、一处修复**（超时语义、头卫生等 P0 修复不再改三遍）。
//!
//! 职责边界（壳与核的分工）：
//!   - **壳**（各域 crate）：`ModuleRoutes` 路由挂载 + **路径重写规则**（flow 升 `/v1`，
//!     rpt/rule 恒等映射）+ 页面归属判定（`portal.flow.*` 等按 id 反代单页）。
//!   - **核**（本 crate）：目标解析（[`UpstreamResolver`]）→ 出站请求头卫生 → 三层出站鉴权
//!     → 流式转发 → 响应构建（含 SSE 逐块透传）→ 502/503 兜底。
//!
//! 转发行为要点（P0 修复后的语义）：
//!   - **超时**：只设连接超时（5s）与读空闲超时（60s），**不设总超时**——SSE/长轮询等流式
//!     响应只要持续有数据就不被掐断（原 30s 总超时会硬切流）。
//!   - **出站头卫生**：剥除客户端可伪造的平台注入型头（`X-API-Key`/`X-Delegated-User-Token`/
//!     `X-Request-Id`）与 `Cookie`（门户会话不下发内部服务），随后由本核从可信源重新注入——
//!     防止外部请求伪造服务身份/委托令牌打穿到内部微服务。
//!   - **X-Forwarded-\***：补齐 `X-Forwarded-For`（append 直连客户端 IP，取不到则保留原值）、
//!     `X-Forwarded-Proto`（缺省 http）、`X-Forwarded-Host`（从入站 Host 补），供下游获取真实
//!     客户端信息。
//!   - **响应头 append 语义**：多值头（`Set-Cookie` 等）全保留，不因 `insert` 覆盖丢值。
//!
//! # Examples
//!
//! 壳侧典型用法（详见各反代壳）：
//!
//! ```no_run
//! use std::sync::Arc;
//! use cmx_proxy_core::{ProxyCore, UpstreamResolver};
//!
//! let resolver: UpstreamResolver = Arc::new(|| Some("http://127.0.0.1:8091".to_string()));
//! let core = Arc::new(ProxyCore::new(resolver, Some("svc-key".into())));
//! // handler 内：core.forward("流程服务", req, |base, uri| format!("{base}/api{}", uri.path())).await
//! # let _ = core;
//! ```

mod core;
mod headers;

pub use core::{ProxyCore, UpstreamResolver};
