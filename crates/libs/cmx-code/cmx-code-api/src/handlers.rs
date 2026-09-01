//! 编码引擎共享类型。
//!
//! HTTP handler 已迁至 `cmx-model-app`（平台中立应用层）。本模块只保留被
//! `engine` / `store::rule_store` 复用的 [`Dam`]（域/应用/模块三维标识），
//! 供铸号时 `Dam::default()` 与规则库按维度过滤共用（迁出的 handler 经
//! `cmx_code_api::handlers::Dam` 引用同一类型）。

// ═══════════════════════════════════════════════════════════════════════════════
// 辅助
// ═══════════════════════════════════════════════════════════════════════════════

/// 域/应用/模块三维标识（从请求头取，可选：前端带了就按此过滤/补全，不带时逐键为空）。
#[derive(Debug, Clone, Default)]
pub struct Dam {
    pub domain_code: String,
    pub application_code: String,
    pub module_code: String,
}
