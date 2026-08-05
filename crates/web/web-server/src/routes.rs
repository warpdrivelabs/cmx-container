//! 路由配置模块
//!
//! 负责配置应用程序的所有 HTTP 路由，包括 API 路由和 Swagger 文档路由。

use axum::Router;
use cmx_api::CmxAppState;
use cmx_api::routes::routes_impl::{api_routes, swagger_routes};
use cmx_api::routes::traits::ModuleRoutes;
use cmx_code_api::CodeModule;
use cmx_dct_api::DctModule;
use cmx_doc_api::DocModule;
use cmx_flow_api::FlowModule;
use cmx_job_api::JobModule;
use cmx_model_api::ModelModule;
use cmx_rpt_api::ReportModule;

/// 配置所有 API 路由
///
/// 直接调用 cmx-api 的统一路由注册，返回配置好的 Axum Router。
/// 外部模块路由（报表 ReportModule、流程 FlowModule、业务单据 DocModule、数据字典 DctModule、
/// 异步任务中心 JobModule、模型中心 ModelModule、编码引擎 CodeModule）在此合并——cmx-api
/// 不依赖它们，避免循环依赖。
///
/// # Returns
///
/// 配置完成的 Axum Router 实例，已挂载所有 API 端点。
pub fn routes() -> Router<CmxAppState> {
    api_routes()
        .merge(ReportModule.routes())
        .merge(FlowModule.routes())
        .merge(DocModule.routes())
        .merge(DctModule.routes())
        .merge(JobModule.routes())
        .merge(ModelModule.routes())
        .merge(CodeModule.routes())
}

/// 获取 Swagger 文档路由
///
/// 返回 Swagger UI 和 OpenAPI 规范的路由。
///
/// # Returns
///
/// Axum Router 实例，包含 Swagger 文档相关端点。
pub fn get_swagger_routes() -> Router {
    swagger_routes()
}
