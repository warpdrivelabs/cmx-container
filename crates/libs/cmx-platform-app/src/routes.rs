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
use cmx_plugin_api::{
    MarketplaceModule, ModulePackageModule, PluginApiDoc, PluginModule, TableMetadataModule,
};
use cmx_iam_api::{AuthModule, IamApiDoc, IamModule};
use cmx_flow_api::FlowProxyModule;
use cmx_job_api::JobModule;
use cmx_service_rpc::Locator;
use cmx_rpt_api::ReportProxyModule;
use cmx_rule_api::RulesProxyModule;
use cmx_onto_api::OntoProxyModule;
use cmx_model_proxy::ModelProxyModule;
use cmx_mdm_proxy::MdmProxyModule;
use cmx_meta_proxy::MetaProxyModule;
use cmx_dataauth_proxy::DataAuthProxyModule;
use cmx_storage_api::{StorageApiDoc, StorageModule};
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

/// 服务定位键：流程引擎（`[service_rpc.services].flow`）。
const FLOW_UPSTREAM_KEY: &str = "flow";
/// 服务定位键：报表引擎（`[service_rpc.services].report`）。
const REPORT_UPSTREAM_KEY: &str = "report";
/// 服务定位键：决策规则引擎（`[service_rpc.services].rules`）。
const RULES_UPSTREAM_KEY: &str = "rules";
/// 服务定位键：本体平台（`[service_rpc.services].onto`）。
const ONTO_UPSTREAM_KEY: &str = "onto";
/// 服务定位键：模型中心（`[service_rpc.services].model`）。
const MODEL_UPSTREAM_KEY: &str = "model";
/// 服务定位键：主数据中心（`[service_rpc.services].mdm`）。
const MDM_UPSTREAM_KEY: &str = "mdm";
/// 服务定位键：元数据管理（`[service_rpc.services].meta`）。
const META_UPSTREAM_KEY: &str = "meta";
/// 服务定位键：数据权限（`[service_rpc.services].dataauth`）。
const DATAAUTH_UPSTREAM_KEY: &str = "dataauth";

/// 解析流程引擎反代目标（per-key 定位，见 `cmx_service_rpc::upstream`）。
///
/// 这是「后端一芯双壳」的切换点，对偶于前端一芯三壳：同一 `/api/flow/*` 前缀、同一
/// ModuleRoutes 契约，配了目标就转发、没配就不挂路由——**前端与其余装配全零改**。
pub(crate) fn flow_upstream() -> Option<Locator> {
    cmx_service_rpc::locator(FLOW_UPSTREAM_KEY)
}

/// 流程引擎是否在远程（代理态）。main 序列据此决定是否提示独立 flow-server 部署。
pub fn flow_is_proxied() -> bool {
    flow_upstream().is_some()
}

/// 解析报表引擎反代目标（per-key 定位）。
///
/// 与 [`flow_upstream`] 同构——报表侧的「后端一芯双壳」切换点。报表微服务对外 URL 与平台一致
/// （`/api/report-design/*` 等，无 `/v1`），配了目标就转发、没配就不挂路由，前端全零改。
pub(crate) fn report_upstream() -> Option<Locator> {
    cmx_service_rpc::locator(REPORT_UPSTREAM_KEY)
}

/// 解析决策规则引擎反代目标（per-key 定位）。
///
/// 规则引擎**无进程内嵌壳**（始终独立微服务），故与 flow/report 的差异：没配目标 = 门户不挂
/// 规则路由，而非回退内嵌。配了目标就转发 `/api/rules/*` + 规则拥有的 native 页，前端全零改。
pub(crate) fn rules_upstream() -> Option<Locator> {
    cmx_service_rpc::locator(RULES_UPSTREAM_KEY)
}

/// 本体平台**无进程内嵌壳**（始终独立微服务，与 rules 同构）：没配目标 = 门户不挂本体路由，
/// 而非回退内嵌。配了目标就转发 `/api/onto/*` + 本体拥有的 native 页，前端全零改。
pub(crate) fn onto_upstream() -> Option<Locator> {
    cmx_service_rpc::locator(ONTO_UPSTREAM_KEY)
}

/// 解析模型中心反代目标（per-key 定位）。
///
/// 与 flow/report 的差异：模型中心**保留进程内嵌兜底**（Dct/Doc/Model/Code 模块仍在 cmx-container，
/// 编译期保留，作平滑迁移期回退）。配了 `[service_rpc.services].model` = 反代到独立 cmx-model-server；
/// 没配 = 门户进程内嵌（现行为不变）。这是「后端一芯双壳」在模型中心的切换点。
pub(crate) fn model_upstream() -> Option<Locator> {
    cmx_service_rpc::locator(MODEL_UPSTREAM_KEY)
}

/// 解析主数据中心反代目标（per-key 定位）。
///
/// 主数据已抽独立微服务 cmx-mdm（:8095），容器内引擎源码已退役，**无进程内嵌兜底**
///（与 flow/report/rules/model 同构）：配了 `[service_rpc.services].mdm` = 反代到独立
/// cmx-mdm-server；没配 = 门户不挂 `/api/mdm/*` 路由。
pub(crate) fn mdm_upstream() -> Option<Locator> {
    cmx_service_rpc::locator(MDM_UPSTREAM_KEY)
}

/// 解析元数据管理反代目标（per-key 定位）。
///
/// 元数据管理是全新独立微服务 cmx-meta-data（:8096），**无进程内嵌兜底**（与 flow/report/rules/model
/// 抽出后同构）。配了 `[service_rpc.services].meta` = 反代到独立 cmx-meta-server；没配 = 门户不挂
/// `/api/meta/*` 路由。
pub(crate) fn meta_upstream() -> Option<Locator> {
    cmx_service_rpc::locator(META_UPSTREAM_KEY)
}

/// 解析数据权限引擎反代目标。配了 `[service_rpc.services].dataauth` = 反代到独立 cmx-dataauth-server；
/// 没配 = 门户不挂 `/api/dataauth/*` 与 `/console`。
pub(crate) fn dataauth_upstream() -> Option<Locator> {
    cmx_service_rpc::locator(DATAAUTH_UPSTREAM_KEY)
}

/// 平台服务依赖拓扑：枚举各已挂载能力当前挂的是「进程内内嵌」还是「反代独立微服务」。
///
/// 供通用监控 [`cmx_web_monitor`] 的拓扑面板/活体探测消费——真实反映路由装配决策，而非猜测。
/// flow/report/rules 各按 `[service_rpc.services]` 服务定位配置（per-key）决定 embedded/proxy；
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
        // flow：按 [service_rpc] 服务定位配置决定 embedded/proxy。
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
    // report：按 [service_rpc] 服务定位配置决定 embedded/proxy。
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
    // 模型中心四能力（doc/dct/model/code）按 [service_rpc.services].model 决定 embedded/proxy；
    // 配了 = 四者都反代到独立 cmx-model-server，没配 = 四者进程内嵌。
    match model_upstream() {
        Some(upstream) => {
            let target = upstream.resolve();
            for (key, label) in [
                ("doc", "业务单据"),
                ("dct", "数据字典"),
                ("model", "模型中心"),
                ("code", "编码引擎"),
            ] {
                deps.push(cmx_web_monitor::ServiceDep {
                    key: key.into(),
                    label: label.into(),
                    mode: "proxy".into(),
                    target: target.clone(),
                    proxiable: true,
                });
            }
        }
        None => {
            deps.push(embedded("doc", "业务单据"));
            deps.push(embedded("dct", "数据字典"));
            deps.push(embedded("model", "模型中心"));
            deps.push(embedded("code", "编码引擎"));
        }
    }
    // 主数据：独立主数据微服务——配置了目标才挂（proxy），没配则不在拓扑里（与 merge_mdm
    // 「没配不挂 /api/mdm/* 路由」一致；引擎源码已退役，无进程内嵌形态）。
    if let Some(upstream) = mdm_upstream() {
        deps.push(cmx_web_monitor::ServiceDep {
            key: "mdm".into(),
            label: "主数据".into(),
            mode: "proxy".into(),
            target: upstream.resolve(),
            proxiable: true,
        });
    }
    // 元数据管理：全新独立微服务——配置了目标才挂（proxy），没配则不在拓扑里（无进程内嵌形态）。
    if let Some(upstream) = meta_upstream() {
        deps.push(cmx_web_monitor::ServiceDep {
            key: "meta".into(),
            label: "元数据管理".into(),
            mode: "proxy".into(),
            target: upstream.resolve(),
            proxiable: true,
        });
    }
    deps.push(embedded("job", "异步任务中心"));
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
            let api_key = crate::config::rpc::load_outgoing_credential();
            tracing::info!(upstream = %upstream.describe(), "流程引擎：独立微服务模式（FlowProxy 转发 /api/flow/* + 页面反代 native/html）");
            let resolver = upstream.resolver_fn();
            let router = router.merge(
                FlowProxyModule::with_resolver(resolver.clone(), api_key.clone()).routes(),
            );
            cmx_flow_api::with_flow_page_proxy(router, resolver, api_key)
        }
        None => {
            tracing::warn!("流程引擎：未配置反代目标（[service_rpc.services] 未配 flow 键或 url/discovery 均空）→ 门户不挂 /api/flow/* 路由；请启动独立 cmx-flow-server 并配置其地址");
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
            let api_key = crate::config::rpc::load_outgoing_credential();
            tracing::info!(upstream = %upstream.describe(), "报表引擎：独立微服务模式（ReportProxy 转发 /api/report-design/* + 页面反代 native/html）");
            let resolver = upstream.resolver_fn();
            let router = router.merge(
                ReportProxyModule::with_resolver(resolver.clone(), api_key.clone()).routes(),
            );
            cmx_rpt_api::with_report_page_proxy(router, resolver, api_key)
        }
        None => {
            tracing::warn!("报表引擎：未配置反代目标（[service_rpc.services] 未配 report 键或 url/discovery 均空）→ 门户不挂报表路由；请启动独立 cmx-rpt-server 并配置其地址");
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
            let api_key = crate::config::rpc::load_outgoing_credential();
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

/// 按配置产出本体平台路由：配置了反代目标 → OntoProxyModule（转发 `/api/onto/*`）+ 页面反代
/// （本体拥有的 `portal.onto.*` native 页转发到 onto-server）；没配 → 不挂本体路由。
/// 与 [`merge_rules`] 同构——本体始终独立微服务，无 embedded 分支。
fn merge_onto(router: Router<CmxAppState>) -> Router<CmxAppState> {
    match onto_upstream() {
        Some(upstream) => {
            let api_key = crate::config::rpc::load_outgoing_credential();
            tracing::info!(upstream = %upstream.describe(), "本体平台：独立微服务模式（OntoProxy 转发 /api/onto/* + 页面反代 native）");
            let resolver = upstream.resolver_fn();
            let router = router.merge(
                OntoProxyModule::with_resolver(resolver.clone(), api_key.clone()).routes(),
            );
            cmx_onto_api::with_onto_page_proxy(router, resolver, api_key)
        }
        None => router,
    }
}

/// 按配置产出模型中心路由：配置了反代目标 → ModelProxyModule（转发 `/api/{dct,dict,doc,model,
/// definitions,flexible-combination,code}/*`）+ 页面反代（模型中心拥有的 native/html 单页取页请求
/// 转发到 cmx-model-server）；没配 → 不挂模型中心路由。
///
/// 模型中心已抽独立微服务 cmx-model（:8093），容器内引擎源码已退役，**无进程内嵌兜底**（与
/// flow/report/rules 同构）。前端零改：浏览器请求同源 `/api/dct/*` 等，切换只看
/// `[service_rpc.services].model`。
///
/// ⚠ MDM（主数据）不在此列——见 [`merge_mdm`]（另一独立微服务 cmx-mdm）。
fn merge_model(router: Router<CmxAppState>) -> Router<CmxAppState> {
    match model_upstream() {
        Some(upstream) => {
            let api_key = crate::config::rpc::load_outgoing_credential();
            tracing::info!(upstream = %upstream.describe(), "模型中心：独立微服务模式（ModelProxy 转发 /api/{{dct,dict,doc,model,definitions,flexible-combination,code}}/* + 页面反代 native/html）");
            let resolver = upstream.resolver_fn();
            let router = router.merge(
                ModelProxyModule::with_resolver(resolver.clone(), api_key.clone()).routes(),
            );
            cmx_model_proxy::with_model_page_proxy(router, resolver, api_key)
        }
        None => {
            tracing::warn!("模型中心：未配置反代目标（[service_rpc.services] 未配 model 键或 url/discovery 均空）→ 门户不挂模型中心路由（/api/dct、/api/doc、/api/model、/api/code 等）；请启动独立 cmx-model-server 并配置其地址");
            router
        }
    }
}

/// 按配置产出主数据路由：配置了反代目标 → MdmProxyModule（转发 `/api/mdm/*`）+ 页面反代（MDM 拥有的
/// `portal.mdm.*` native 页转发到 cmx-mdm-server）；没配 → 不挂主数据路由。
///
/// 主数据已抽独立微服务 cmx-mdm（:8095），容器内引擎源码已退役，**无进程内嵌兜底**（与
/// flow/report/rules/model 同构）。前端零改：浏览器请求同源 `/api/mdm/*`，切换只看
/// `[service_rpc.services].mdm`。
fn merge_mdm(router: Router<CmxAppState>) -> Router<CmxAppState> {
    match mdm_upstream() {
        Some(upstream) => {
            let api_key = crate::config::rpc::load_outgoing_credential();
            tracing::info!(upstream = %upstream.describe(), "主数据中心：独立微服务模式（MdmProxy 转发 /api/mdm/* + 页面反代 native）");
            let resolver = upstream.resolver_fn();
            let router = router.merge(
                MdmProxyModule::with_resolver(resolver.clone(), api_key.clone()).routes(),
            );
            cmx_mdm_proxy::with_mdm_page_proxy(router, resolver, api_key)
        }
        None => {
            tracing::warn!("主数据中心：未配置反代目标（[service_rpc.services] 未配 mdm 键或 url/discovery 均空）→ 门户不挂 /api/mdm/* 路由；请启动独立 cmx-mdm-server 并配置其地址");
            router
        }
    }
}

/// 按配置产出元数据管理路由：配置了反代目标 → MetaProxyModule（转发 `/api/meta/*`）+ 页面反代
/// （`meta.*` native/html 页转发到 cmx-meta-server）；没配 → 不挂元数据路由。
///
/// 元数据管理是全新独立微服务 cmx-meta-data（:8096），**无进程内嵌兜底**（与 flow/report/rules/model/
/// mdm 同构）。前端零改：浏览器请求同源 `/api/meta/*`，切换只看 `[service_rpc.services].meta`。
fn merge_meta(router: Router<CmxAppState>) -> Router<CmxAppState> {
    match meta_upstream() {
        Some(upstream) => {
            let api_key = crate::config::rpc::load_outgoing_credential();
            tracing::info!(upstream = %upstream.describe(), "元数据管理：独立微服务模式（MetaProxy 转发 /api/meta/* + 页面反代 meta.*）");
            let resolver = upstream.resolver_fn();
            let router = router.merge(
                MetaProxyModule::with_resolver(resolver.clone(), api_key.clone()).routes(),
            );
            cmx_meta_proxy::with_meta_page_proxy(router, resolver, api_key)
        }
        None => {
            tracing::warn!("元数据管理：未配置反代目标（[service_rpc.services] 未配 meta 键或 url/discovery 均空）→ 门户不挂 /api/meta/* 路由；请启动独立 cmx-meta-server 并配置其地址");
            router
        }
    }
}

/// 数据权限：按 `[service_rpc.services].dataauth` 反代到独立 cmx-dataauth-server（无进程内嵌兜底，与
/// flow/report/rules/meta 同构）。前端零改：浏览器请求同源 `/api/dataauth/*`。`/console` 工作台整页
/// 由 router.rs 顶层单独反代（非 `/api`）。
fn merge_dataauth(router: Router<CmxAppState>) -> Router<CmxAppState> {
    match dataauth_upstream() {
        Some(upstream) => {
            let api_key = crate::config::rpc::load_outgoing_credential();
            tracing::info!(upstream = %upstream.describe(), "数据权限：独立微服务模式（DataAuthProxy 转发 /api/dataauth/*）");
            router.merge(
                DataAuthProxyModule::with_resolver(upstream.resolver_fn(), api_key).routes(),
            )
        }
        None => {
            tracing::warn!("数据权限：未配置反代目标（[service_rpc.services] 未配 dataauth）→ 门户不挂 /api/dataauth/* 路由；请启动独立 cmx-dataauth-server 并配置其地址");
            router
        }
    }
}
///
/// 直接调用 cmx-api 的统一路由注册，返回配置好的 Axum Router。
/// 外部模块路由（报表 ReportProxyModule、流程 FlowProxyModule、规则 RulesProxyModule、业务单据
/// DocModule、数据字典 DctModule、主数据 MdmModule、异步任务中心 JobModule、模型中心 ModelModule、
/// 编码引擎 CodeModule）在此合并——cmx-api 不依赖它们，避免循环依赖。
///
/// 流程/报表/规则三引擎均为**独立微服务**：各按 `[service_rpc.services]` 的服务定位配置（per-key：
/// `url` 静态基址优先，`discovery` Nacos 选例）决定——
/// 配了=反代到独立微服务，没配=不挂该模块路由（三者无进程内嵌，编译期均不依赖引擎源码）。
///
/// # Returns
///
/// 配置完成的 Axum Router 实例，已挂载所有 API 端点。
pub fn routes() -> Router<CmxAppState> {
    let base = api_routes()
        .merge(AuthModule.routes())
        .merge(IamModule.routes())
        .merge(JobModule.routes())
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
    // 报表、流程各按 [service_rpc.services].{report,flow} 二选一：配了=反代到独立微服务，没配=进程内嵌。
    // 规则按 [service_rpc.services].rules：配了=反代到独立 cmx-rule-server，没配=不挂（规则无内嵌）。
    // 模型中心按 [service_rpc.services].model：配了=反代到独立 cmx-model-server，没配=进程内嵌
    // （Dct/Doc/Model/Code 四模块，见 merge_model 的 None 分支——故已从 base 移出）。
    // 主数据按 [service_rpc.services].mdm：配了=反代到独立 cmx-mdm-server，没配=不挂
    // /api/mdm/* 路由（无进程内嵌，见 merge_mdm 的 None 分支——故已从 base 移出）。
    merge_dataauth(merge_meta(merge_mdm(merge_model(merge_flow(merge_report(merge_rules(merge_onto(base))))))))
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
    // 模型中心（DCT/DOC）与主数据（MDM）的 OpenAPI 切片随引擎迁至独立微服务
    // （cmx-model-server / cmx-mdm-server 各自暴露 /openapi.json），门户主文档不再合并。
    doc.merge(PortalApiDoc::openapi());
    doc
}

pub fn get_swagger_routes() -> Router {
    Router::new().merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", merged_openapi()))
}
