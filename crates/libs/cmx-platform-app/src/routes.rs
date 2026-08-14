//! 路由配置模块
//!
//! 负责配置应用程序的所有 HTTP 路由，包括 API 路由和 Swagger 文档路由。

use axum::Router;
use cmx_common_api::CmxAppState;
use cmx_common_api::openapi::{ApiDoc, PortalApiDoc};
use cmx_common_api::routes::routes_impl::api_routes;
use cmx_common_api::routes::traits::ModuleRoutes;
use cmx_ai_api::{AiApiDoc, AiModule};
use cmx_biz_api::{
    ApplicationModule, BizApiDoc, DomainModule, FormModule, MenuModule, ModuleCrudModule,
    SysDatasourceModule,
};
use cmx_code_api::CodeModule;
use cmx_plugin_api::{
    MarketplaceModule, ModulePackageModule, PluginApiDoc, PluginModule, TableMetadataModule,
};
use cmx_iam_api::{AuthModule, IamApiDoc, IamModule};
use cmx_dct_api::{DctApiDoc, DctModule};
use cmx_doc_api::{DocApiDoc, DocModule};
use cmx_flow_api::{FlowModule, FlowProxyModule};
use cmx_job_api::JobModule;
use cmx_mdm_api::{MdmApiDoc, MdmModule};
use cmx_model_api::ModelModule;
use cmx_rpt_api::{ReportModule, ReportProxyModule};
use cmx_storage_api::{StorageApiDoc, StorageModule};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

/// 读 `[center_client.urls].flow`：非空=独立流程微服务部署（反代到它），空=进程内嵌引擎（默认）。
///
/// 这是「后端一芯双壳」的切换点，对偶于前端一芯三壳：同一 `/api/flow/*` 前缀、同一 ModuleRoutes
/// 契约，配了远程地址就转发、没配就本进程跑引擎——**前端与其余装配全零改**。
pub(crate) fn flow_remote_base() -> Option<String> {
    let cfg = cmx_plugin::center_client::CenterClientConfig::load();
    cfg.urls
        .flow
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// 流程引擎是否在远程（代理态）。main.rs 据此决定是否起本进程引擎 poller。
pub fn flow_is_proxied() -> bool {
    flow_remote_base().is_some()
}

/// 读 `[center_client.urls].report`：非空=独立报表微服务部署（反代到它），空=进程内嵌（默认）。
///
/// 与 [`flow_remote_base`] 同构——报表侧的「后端一芯双壳」切换点。报表微服务对外 URL 与平台一致
/// （`/api/report-design/*` 等，无 `/v1`），配了远程地址就转发、没配就本进程跑引擎，前端全零改。
pub(crate) fn report_remote_base() -> Option<String> {
    let cfg = cmx_plugin::center_client::CenterClientConfig::load();
    cfg.urls
        .report
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// 平台服务依赖拓扑：枚举各已挂载能力当前挂的是「进程内内嵌」还是「反代独立微服务」。
///
/// 供通用监控 [`cmx_web_monitor`] 的拓扑面板/活体探测消费——真实反映路由装配决策，而非猜测。
/// 今天只有 flow 有 embedded/proxy 双壳开关（读同一 `flow_remote_base()` 真源）；其余模块均进程内嵌
/// （无反代变体，`proxiable=false` 表示暂未接入独立部署）。这份清单与本文件的 `routes()` 装配一一对应。
pub fn service_topology() -> Vec<cmx_web_monitor::ServiceDep> {
    let embedded = |key: &str, label: &str| cmx_web_monitor::ServiceDep {
        key: key.into(),
        label: label.into(),
        mode: "embedded".into(),
        target: None,
        proxiable: false,
    };
    let mut deps = vec![
        // flow：唯一「一芯双壳」能力——按 [center_client.urls].flow 决定 embedded/proxy。
        match flow_remote_base() {
            Some(base) => cmx_web_monitor::ServiceDep {
                key: "flow".into(),
                label: "流程引擎".into(),
                mode: "proxy".into(),
                target: Some(base),
                proxiable: true,
            },
            None => cmx_web_monitor::ServiceDep {
                key: "flow".into(),
                label: "流程引擎".into(),
                mode: "embedded".into(),
                target: None,
                proxiable: true,
            },
        },
    ];
    // 其余已挂载模块（routes() 里无条件 merge，全进程内嵌）。
    // report：第二个「一芯双壳」能力——按 [center_client.urls].report 决定 embedded/proxy。
    deps.push(match report_remote_base() {
        Some(base) => cmx_web_monitor::ServiceDep {
            key: "report".into(),
            label: "报表引擎".into(),
            mode: "proxy".into(),
            target: Some(base),
            proxiable: true,
        },
        None => cmx_web_monitor::ServiceDep {
            key: "report".into(),
            label: "报表引擎".into(),
            mode: "embedded".into(),
            target: None,
            proxiable: true,
        },
    });
    deps.push(embedded("doc", "业务单据"));
    deps.push(embedded("dct", "数据字典"));
    deps.push(embedded("mdm", "主数据"));
    deps.push(embedded("job", "异步任务中心"));
    deps.push(embedded("model", "模型中心"));
    deps.push(embedded("code", "编码引擎"));
    deps
}

/// 按配置产出流程模块路由：远程基址非空 → FlowProxyModule（转发）；否则 FlowModule（内嵌）。
///
/// F3a：反代模式下**同时**叠加页面反代层（`with_flow_page_proxy`）——流程拥有的 native/html
/// 单页取页请求（`/api/native-pages/portal.flow.*`、`/api/html-pages/fi.cmxfico.gl.flow-*`）
/// 转发到 flow-server（它自暴同款字节对齐 API），其余页请求落回门户内嵌 handler。前端零改。
fn merge_flow(router: Router<CmxAppState>) -> Router<CmxAppState> {
    match flow_remote_base() {
        Some(base) => {
            let api_key = crate::config::rpc::load_outgoing_credential().map(|c| c.value);
            tracing::info!(flow_base = %base, "流程引擎：独立微服务模式（FlowProxy 转发 /api/flow/* + 页面反代 native/html）");
            let router = router.merge(FlowProxyModule::new(base.clone(), api_key.clone()).routes());
            cmx_flow_api::with_flow_page_proxy(router, base, api_key)
        }
        None => router.merge(FlowModule.routes()),
    }
}

/// 按配置产出报表模块路由：远程基址非空 → ReportProxyModule（转发到独立 cmx-rpt-server）；
/// 否则 ReportModule（进程内嵌）。与 [`merge_flow`] 同构。
///
/// F3a：反代模式下**同时**叠加页面反代层（`with_report_page_proxy`）——报表拥有的 native/html
/// 单页取页请求（`/api/native-pages/portal.rpt.*`、`/api/html-pages/fi.cmxfico.gl.rpt-*designer-*`）
/// 转发到 report-server（它自暴同款字节对齐 API），其余页请求落回门户内嵌 handler。前端零改。
fn merge_report(router: Router<CmxAppState>) -> Router<CmxAppState> {
    match report_remote_base() {
        Some(base) => {
            let api_key = crate::config::rpc::load_outgoing_credential().map(|c| c.value);
            tracing::info!(report_base = %base, "报表引擎：独立微服务模式（ReportProxy 转发 /api/report-design/* + 页面反代 native/html）");
            let router = router.merge(ReportProxyModule::new(base.clone(), api_key.clone()).routes());
            cmx_rpt_api::with_report_page_proxy(router, base, api_key)
        }
        None => router.merge(ReportModule.routes()),
    }
}

/// 配置所有 API 路由
///
/// 直接调用 cmx-api 的统一路由注册，返回配置好的 Axum Router。
/// 外部模块路由（报表 ReportModule、流程 FlowModule/FlowProxyModule、业务单据 DocModule、
/// 数据字典 DctModule、主数据 MdmModule、异步任务中心 JobModule、模型中心 ModelModule、
/// 编码引擎 CodeModule）在此合并——cmx-api 不依赖它们，避免循环依赖。
///
/// 流程、报表模块各按 `[center_client.urls].{flow,report}` 二选一：配了=反代到独立微服务，没配=进程内嵌。
///
/// # Returns
///
/// 配置完成的 Axum Router 实例，已挂载所有 API 端点。
pub fn routes() -> Router<CmxAppState> {
    let base = api_routes()
        .merge(AuthModule.routes())
        .merge(IamModule.routes())
        .merge(DocModule.routes())
        .merge(DctModule.routes())
        .merge(MdmModule.routes())
        .merge(JobModule.routes())
        .merge(ModelModule.routes())
        .merge(CodeModule.routes())
        .merge(StorageModule.routes())
        .merge(AiModule.routes())
        .merge(DomainModule.routes())
        .merge(ApplicationModule.routes())
        .merge(MenuModule.routes())
        .merge(SysDatasourceModule.routes())
        .merge(FormModule.routes())
        .merge(ModuleCrudModule.routes())
        .merge(PluginModule.routes())
        .merge(TableMetadataModule.routes())
        .merge(MarketplaceModule.routes())
        .merge(ModulePackageModule.routes());
    // 报表、流程各按 [center_client.urls].{report,flow} 二选一：配了=反代到独立微服务，没配=进程内嵌。
    merge_flow(merge_report(base))
}

/// 获取 Swagger 文档路由
///
/// 返回 Swagger UI 和 OpenAPI 规范的路由。
///
/// # Returns
///
/// Axum Router 实例，包含 Swagger 文档相关端点。
/// 聚合 OpenApi 文档：以 cmx-api 的主 ApiDoc 为基底，merge 各域 `*-api` crate 的切片。
///
/// 随 handler 逐步迁出 cmx-api，各域自带 ApiDoc 切片，此处统一合并，保证 Swagger 覆盖不丢。
/// 注：`OpenApi::merge` 对同名 schema 静默丢弃后者，迁移时注意 schema 命名不冲突。
fn merged_openapi() -> utoipa::openapi::OpenApi {
    let mut doc = ApiDoc::openapi();
    doc.merge(AiApiDoc::openapi());
    doc.merge(StorageApiDoc::openapi());
    doc.merge(BizApiDoc::openapi());
    doc.merge(PluginApiDoc::openapi());
    doc.merge(IamApiDoc::openapi());
    doc.merge(DctApiDoc::openapi());
    doc.merge(DocApiDoc::openapi());
    doc.merge(MdmApiDoc::openapi());
    doc.merge(PortalApiDoc::openapi());
    doc
}

pub fn get_swagger_routes() -> Router {
    Router::new().merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", merged_openapi()))
}
