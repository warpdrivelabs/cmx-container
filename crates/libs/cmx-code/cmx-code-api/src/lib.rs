//! cmx-code-api —— 编码引擎模块的 HTTP 层。
//!
//! 薄 axum handler：提取参数 → 调 `store` 服务（规则库 CRUD / 反查 max 铸号）→ `ApiResp` 信封。
//! `CodeModule` 实现 cmx-api 的 `ModuleRoutes`，聚合编码引擎路由。由 web-server（而非 cmx-api）
//! 合并 `CodeModule.routes()`，故 cmx-api 不反向依赖本 crate（无环）。
//!
//! 端点路径 `/code/*`，`/api` 前缀由 web-server nest 加。

pub mod engine;
pub mod handlers;
pub mod store;

use axum::Router;
use axum::routing::{get, post};

use cmx_api_core::routes::traits::ModuleRoutes;
use cmx_api_core::CmxAppState;

use handlers as code;

/// 编码引擎模块路由聚合（实现 cmx-api 的 ModuleRoutes，由 web-server 合并进主路由）。
pub struct CodeModule;

impl ModuleRoutes for CodeModule {
    fn routes(self) -> Router<CmxAppState> {
        Router::new()
            // 规则库 CRUD
            .route("/code/rules", get(code::rule_list).post(code::rule_create))
            .route(
                "/code/rules/{ruleCode}",
                get(code::rule_get).put(code::rule_update).delete(code::rule_delete),
            )
            // 预览编码（不落库不占号）
            .route("/code/preview", post(code::preview))
            .route("/code/preview/batch", post(code::preview_batch))
            // 权威生成（事务内铸号落库）
            .route("/code/generate", post(code::generate))
            .route("/code/generate/batch", post(code::generate_batch))
            // manual pattern 校验
            .route("/code/validate", post(code::validate))
            // 断号查询 / 手动取号（C6）
            .route("/code/gaps", get(code::gap_list))
            .route("/code/gaps/take", post(code::gap_take))
    }

    fn prefix() -> &'static str {
        "code"
    }

    fn module_name(&self) -> &'static str {
        "code"
    }
}
