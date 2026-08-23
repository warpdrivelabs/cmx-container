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
use cmx_flow_api::FlowProxyModule;
use cmx_job_api::JobModule;
use cmx_mdm_api::MdmProxyModule;
use cmx_model_api::ModelModule;
use cmx_plugin::center_client::ProxyUpstream;
use cmx_rpt_api::ReportProxyModule;
use cmx_rule_api::RulesProxyModule;
use cmx_storage_api::{StorageApiDoc, StorageModule};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

/// 服务定位键：流程引擎（`[center_client.services].flow`）。
const FLOW_UPSTREAM_KEY: &str = "flow";
/// 服务定位键：报表引擎（`[center_client.services].report`）。
const REPORT_UPSTREAM_KEY: &str = "report";
/// 服务定位键：决策规则引擎（`[center_client.services].rules`）。
const RULES_UPSTREAM_KEY: &str = "rules";
/// 服务定位键：主数据治理引擎（`[center_client.services].mdm`）。
const MDM_UPSTREAM_KEY: &str = "mdm";

/// 解析流程引擎反代目标（per-key 定位，见 `cmx_plugin::center_client::upstream`）。
///
/// 这是「后端一芯双壳」的切换点，对偶于前端一芯三壳：同一 `/api/flow/*` 前缀、同一
/// ModuleRoutes 契约，配了目标就转发、没配就不挂路由——**前端与其余装配全零改**。
pub(crate) fn flow_upstream() -> Option<ProxyUpstream> {
    cmx_plugin::center_client::proxy_upstream(FLOW_UPSTREAM_KEY)
}

/// 流程引擎是否在远程（代理态）。main 序列据此决定是否提示独立 flow-server 部署。
pub fn flow_is_proxied() -> bool {
    flow_upstream().is_some()
}

/// 解析报表引擎反代目标（per-key 定位）。
///
/// 与 [`flow_upstream`] 同构——报表侧的「后端一芯双壳」切换点。报表微服务对外 URL 与平台一致
/// （`/api/report-design/*` 等，无 `/v1`），配了目标就转发、没配就不挂路由，前端全零改。
pub(crate) fn report_upstream() -> Option<ProxyUpstream> {
    cmx_plugin::center_client::proxy_upstream(REPORT_UPSTREAM_KEY)
}

/// 解析决策规则引擎反代目标（per-key 定位）。
///
/// 规则引擎**无进程内嵌壳**（始终独立微服务），故与 flow/report 的差异：没配目标 = 门户不挂
/// 规则路由，而非回退内嵌。配了目标就转发 `/api/rules/*` + 规则拥有的 native 页，前端全零改。
pub(crate) fn rules_upstream() -> Option<ProxyUpstream> {
    cmx_plugin::center_client::proxy_upstream(RULES_UPSTREAM_KEY)
}

/// 解析主数据治理引擎反代目标（per-key 定位）。
///
/// 主数据治理**无进程内嵌壳**（始终独立微服务，引擎核 cmx-mdm-app 在独立 workspace ../cmx-mdm，
/// 由 cmx-mdm-server 承载），与 rules 同形：没配目标 = 门户不挂主数据路由。配了目标就转发
/// `/api/mdm/*` + 主数据拥有的 native 页（`portal.mdm.*`），前端全零改。
pub(crate) fn mdm_upstream() -> Option<ProxyUpstream> {
    cmx_plugin::center_client::proxy_upstream(MDM_UPSTREAM_KEY)
}

/// 平台服务依赖拓扑：枚举各已挂载能力当前挂的是「进程内内嵌」还是「反代独立微服务」。
///
/// 供通用监控 [`cmx_web_monitor`] 的拓扑面板/活体探测消费——真实反映路由装配决策，而非猜测。
/// flow/report/rules 各按 `[center_client.services]` 服务定位配置（per-key）决定 embedded/proxy；
/// 其余模块均进程内嵌（无反代变体，`proxiable=false` 表示暂未接入独立部署）。这份清单与本文件
/// 的 `routes()` 装配一一对应。proxy 目标的 `target` 每轮探测时现解析（服务发现模式下跟随
/// 实例变化；未解析出实例时为 `None`，面板显示为无目标而非误报不可达）。
pub fn service_topology() -> Vec<cmx_web_monitor::ServiceDep> {
    let embedded = |key: &str, label: &str| cmx_web_monitor::ServiceDep {
        key: key.into(),
        label: label.into(),
        mode: "embedded".into(),
        target: None,
        proxiable: false,
    };
    let mut deps = vec![
        // flow：按 [center_client] 服务定位配置决定 embedded/proxy。
        match flow_upstream() {
            Some(upstream) => cmx_web_monitor::ServiceDep {
                key: "flow".into(),
                label: "流程引擎".into(),
                mode: "proxy".into(),
                target: upstream.resolve(),
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
    // report：按 [center_client] 服务定位配置决定 embedded/proxy。
    deps.push(match report_upstream() {
        Some(upstream) => cmx_web_monitor::ServiceDep {
            key: "report".into(),
            label: "报表引擎".into(),
            mode: "proxy".into(),
            target: upstream.resolve(),
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
    // rules：独立规则微服务——配置了目标才挂（proxy），没配则不在拓扑里。
    if let Some(upstream) = rules_upstream() {
        deps.push(cmx_web_monitor::ServiceDep {
            key: "rules".into(),
            label: "决策规则引擎".into(),
            mode: "proxy".into(),
            target: upstream.resolve(),
            proxiable: true,
        });
    }
    // mdm：独立主数据微服务——配置了目标才挂（proxy），没配则不在拓扑里。
    if let Some(upstream) = mdm_upstream() {
        deps.push(cmx_web_monitor::ServiceDep {
            key: "mdm".into(),
            label: "主数据治理".into(),
            mode: "proxy".into(),
            target: upstream.resolve(),
            proxiable: true,
        });
    }
    deps.push(embedded("doc", "业务单据"));
    deps.push(embedded("dct", "数据字典"));
    deps.push(embedded("job", "异步任务中心"));
    deps.push(embedded("model", "模型中心"));
    deps.push(embedded("code", "编码引擎"));
    deps
}

/// 按配置产出流程模块路由：配置了反代目标 → FlowProxyModule（转发 `/api/flow/*`）+ 页面反代；
/// 没配 → 不挂流程路由（流程无进程内嵌，始终独立微服务）。与 [`merge_rules`] 同构。
///
/// F3a：反代模式下**同时**叠加页面反代层（`with_flow_page_proxy`）——流程拥有的 native/html
/// 单页取页请求（`/api/native-pages/portal.flow.*`、`/api/html-pages/fi.cmxfico.gl.flow-*`）
/// 转发到 flow-server（它自暴同款字节对齐 API），其余页请求落回门户内嵌 handler。前端零改。
///
/// 目标 resolver 由 `ProxyUpstream::resolver_fn` 构造（捕获启动期解析结果：静态基址固化返回，
/// 服务发现模式每请求查实例缓存），API 反代与页面反代共享同一 resolver / 连接池。
///
/// 引擎核 cmx-flow-app 在独立 workspace ../cmx-flowengine，由 cmx-flow-server 承载；门户只反代，
/// 编译期不再依赖引擎源码（本壳 cmx-flow-api 已瘦成纯反代，无 embedded 分支）。
fn merge_flow(router: Router<CmxAppState>) -> Router<CmxAppState> {
    match flow_upstream() {
        Some(upstream) => {
            let api_key = crate::config::rpc::load_outgoing_credential().map(|c| c.value);
            tracing::info!(upstream = %upstream.describe(), "流程引擎：独立微服务模式（FlowProxy 转发 /api/flow/* + 页面反代 native/html）");
            let resolver = upstream.resolver_fn();
            let router = router.merge(
                FlowProxyModule::with_resolver(resolver.clone(), api_key.clone()).routes(),
            );
            cmx_flow_api::with_flow_page_proxy(router, resolver, api_key)
        }
        None => {
            tracing::warn!("流程引擎：未配置反代目标（[center_client.services] 未配 flow 键或 url/discovery 均空）→ 门户不挂 /api/flow/* 路由；请启动独立 cmx-flow-server 并配置其地址");
            router
        }
    }
}

/// 按配置产出报表模块路由：配置了反代目标 → ReportProxyModule（转发到独立 cmx-rpt-server）+
/// 页面反代；没配 → 不挂报表路由（报表无进程内嵌，始终独立微服务）。与 [`merge_flow`]/
/// [`merge_rules`] 同构。目标 resolver 由 `ProxyUpstream::resolver_fn` 构造，API 与页面反代共享。
///
/// F3a：反代模式下**同时**叠加页面反代层（`with_report_page_proxy`）——报表拥有的 native/html
/// 单页取页请求（`/api/native-pages/portal.rpt.*`、`/api/html-pages/fi.cmxfico.gl.rpt-*designer-*`）
/// 转发到 report-server（它自暴同款字节对齐 API），其余页请求落回门户内嵌 handler。前端零改。
///
/// 中立核 cmx-rpt-app 在独立 workspace ../cmx-report，由 cmx-rpt-server 承载；门户只反代，
/// 编译期不再依赖报表引擎源码（本壳 cmx-rpt-api 已瘦成纯反代，无 embedded 分支）。
fn merge_report(router: Router<CmxAppState>) -> Router<CmxAppState> {
    match report_upstream() {
        Some(upstream) => {
            let api_key = crate::config::rpc::load_outgoing_credential().map(|c| c.value);
            tracing::info!(upstream = %upstream.describe(), "报表引擎：独立微服务模式（ReportProxy 转发 /api/report-design/* + 页面反代 native/html）");
            let resolver = upstream.resolver_fn();
            let router = router.merge(
                ReportProxyModule::with_resolver(resolver.clone(), api_key.clone()).routes(),
            );
            cmx_rpt_api::with_report_page_proxy(router, resolver, api_key)
        }
        None => {
            tracing::warn!("报表引擎：未配置反代目标（[center_client.services] 未配 report 键或 url/discovery 均空）→ 门户不挂报表路由；请启动独立 cmx-rpt-server 并配置其地址");
            router
        }
    }
}

/// 按配置产出规则模块路由：配置了反代目标 → RulesProxyModule（转发 `/api/rules/*`）+ 页面反代
/// （规则拥有的 `portal.rules.*` native 页转发到 rules-server）；没配 → 不挂规则路由（规则无内嵌）。
/// 与 [`merge_report`] 同构，但**无 embedded 分支**——规则始终独立微服务。
fn merge_rules(router: Router<CmxAppState>) -> Router<CmxAppState> {
    match rules_upstream() {
        Some(upstream) => {
            let api_key = crate::config::rpc::load_outgoing_credential().map(|c| c.value);
            tracing::info!(upstream = %upstream.describe(), "规则引擎：独立微服务模式（RulesProxy 转发 /api/rules/* + 页面反代 native）");
            let resolver = upstream.resolver_fn();
            let router = router.merge(
                RulesProxyModule::with_resolver(resolver.clone(), api_key.clone()).routes(),
            );
            cmx_rule_api::with_rules_page_proxy(router, resolver, api_key)
        }
        None => router,
    }
}

/// 按配置产出主数据模块路由：配置了反代目标 → MdmProxyModule（转发到独立 cmx-mdm-server）+
/// 页面反代（主数据拥有的 `portal.mdm.*` native 页转发到 mdm-server）；没配 → 不挂主数据路由
/// （主数据无进程内嵌，始终独立微服务）。与 [`merge_rules`] 同构。
fn merge_mdm(router: Router<CmxAppState>) -> Router<CmxAppState> {
    match mdm_upstream() {
        Some(upstream) => {
            let api_key = crate::config::rpc::load_outgoing_credential().map(|c| c.value);
            tracing::info!(upstream = %upstream.describe(), "主数据治理：独立微服务模式（MdmProxy 转发 /api/mdm/* + 页面反代 native）");
            let resolver = upstream.resolver_fn();
            let router = router.merge(
                MdmProxyModule::with_resolver(resolver.clone(), api_key.clone()).routes(),
            );
            cmx_mdm_api::with_mdm_page_proxy(router, resolver, api_key)
        }
        None => {
            tracing::warn!("主数据治理：未配置反代目标（[center_client.services] 未配 mdm 键或 url/discovery 均空）→ 门户不挂 /api/mdm/* 路由；请启动独立 cmx-mdm-server 并配置其地址");
            router
        }
    }
}

/// 配置所有 API 路由
///
/// 直接调用 cmx-api 的统一路由注册，返回配置好的 Axum Router。
/// 外部模块路由（流程 FlowProxyModule、报表 ReportProxyModule、规则 RulesProxyModule、主数据
/// MdmProxyModule、业务单据 DocModule、数据字典 DctModule、异步任务中心 JobModule、模型中心
/// ModelModule、编码引擎 CodeModule）在此合并——cmx-api 不依赖它们，避免循环依赖。
///
/// 流程/报表/规则/主数据四引擎均为**独立微服务**：各按 `[center_client.services]` 的服务定位配置
/// （per-key：`url` 静态基址优先，`discovery` Nacos 选例）决定——
/// 配了=反代到独立微服务，没配=不挂该模块路由（四者无进程内嵌，编译期均不依赖引擎源码）。
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
    // 报表/流程/规则/主数据各按 [center_client.services].{report,flow,rules,mdm} 定位：
    // 配了=反代到独立微服务，没配=不挂（四者均无进程内嵌）。
    merge_mdm(merge_flow(merge_report(merge_rules(base))))
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
    doc.merge(PortalApiDoc::openapi());
    doc
}

pub fn get_swagger_routes() -> Router {
    Router::new().merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", merged_openapi()))
}
