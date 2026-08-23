//! cmx-platform-app —— 平台总装配器（原 cmx-portal-app）。装配全平台各业务域路由 + 20 步 init + serve。
//!
//! 平台聚合服务（原 web-server bin 的 20 步有序 init + CmxAppState + 路由 + serve）收成库，
//! 暴露 [`run_platform`]。各微服务 bin 薄壳传入自己的 banner 调它。对偶于 cmx-flow-app（流程微服务核）。

mod app_state;
mod config;
mod error;
mod router;
mod routes;

pub use self::error::{Error, Result};
use config::web_config;


use crate::app_state::build_app_state;
use crate::config::{
    build_audit_logger, build_function_invoker, finalize_iam_state, init_auth_service, init_cache,
    init_code_engine, init_datasources, init_iam_services, init_job_center, init_runtime,
    init_system_identity, init_web_config, run_permission_check,
};
// 基础设施改由公用包 cmx-service-base 提供：
// - crypto/debug/event-bus/storage/services/plugins（P1 纯全局 + 中等）
// - init_infra/shutdown_infra（Nacos 注册/配置中心）+ init_rpc（RPC 子系统）——通用微服务能力。
// 注：init_cache 仍走 portal 的 config::init_cache 包装（读 ConfigManager 得 RedisConfig 再委托 base）；
//    init_rpc 的 function_invoker 由 portal 的 build_function_invoker(绑 cmx-biz) 注入。
use cmx_service_base::{
    init_crypto, init_debug, init_event_bus, init_infra, init_plugins, init_rpc,
    init_service_invoker, init_services, init_storage, shutdown_infra,
};
use crate::router::build_router;
use cmx_utils::ConfigManager;
use std::time::Duration;
use tokio::net::TcpListener;
use tracing::info;

/// 平台服务入口：装配并运行平台聚合服务。
///
/// 从原 web-server `main()` 提取——20 步有序 init（顺序 load-bearing）+ CmxAppState 组装 +
/// 路由 + serve + 优雅关闭。各微服务 bin 薄壳传入自己的 `banner` 调 `cmx_platform_app::run_platform(banner).await`。
///
/// # 初始化顺序
///
/// 1. 日志系统（控制台 + 文件双输出）
/// 2. 全局配置（含 Nacos 远程配置覆盖）
/// 3. 加密服务
/// 4. Redis 缓存
/// 5. 数据库数据源
/// 6. 文件存储
/// 7. 调试会话
/// 8. WASM 运行时
/// 9. 全局事件总线
/// 10. 服务管理器
/// 11. 插件管理器
///
/// # Errors
///
/// * `Error::ConfigError` - 配置加载或解析失败
/// * `Error::ServerSetup` - 服务器设置失败（如地址绑定）
/// * 其他初始化错误 - 各子系统初始化失败
pub async fn run_platform(banner: cmx_web_chassis::BannerSpec) -> Result<()> {
    dotenvy::dotenv().ok();

    // 分层日志初始化下沉到通用骨架 cmx-web-chassis（控制台 CompactFormatter + 滚动文件 JSON），
    // 与 flow-server / report-server / mdm-server 完全一致。日志目录 logs、文件名 cmx-server.log
    // （沿用原值，行为不变）。_guard 必须持有到 main 结束，确保文件日志后台线程 flush。
    let log_cfg = cmx_web_chassis::ChassisConfig {
        log_dir: "logs".to_string(),
        log_file: "cmx-server.log".to_string(),
        log_level: "info".to_string(),
        ..cmx_web_chassis::ChassisConfig::defaults("cmx-server")
    };
    let _guard = cmx_web_chassis::init_tracing(&log_cfg);

    // ── 基础设施（顺序敏感：审计依赖数据源、IAM 依赖审计、系统身份在 finalize 之前）──
    init_infra()
        .await
        .map_err(|e| Error::ConfigError(format!("基础设施初始化失败: {e}")))?;
    // 服务定位配置快照（补偿 map 键拼写错误静默不挂路由的可见性）+ 服务发现目标订阅预热
    //（无 discovery 定位键或注册中心未启用时为 no-op，不产生网络行为）。
    cmx_plugin::center_client::log_center_client_snapshot();
    cmx_plugin::center_client::warm_proxy_upstreams().await;
    init_crypto();

    init_cache().await?;
    init_datasources().await?;
    init_storage()
        .await
        .map_err(|e| Error::StorageInit(format!("存储初始化失败: {e}")))?;

    // 通用技术监控（/_mon）：注册平台身份读取器（把 AuthContext 映射为 monitor 中性 Identity，
    // 供 observe 遥测记录租户/用户/角色）+ 起系统采样器（CPU/内存/网络/磁盘，3s 一采）。
    // 数据源已就绪，故 DB 连接池状态可被 /_mon/tech-stats 聚合。与 flow/report/mdm-server 同一 crate。
    cmx_web_monitor::set_service_name("cmx-server 平台");
    cmx_web_monitor::set_identity_provider(web_identity);
    cmx_web_monitor::spawn_system_sampler();
    // 服务依赖拓扑：各能力 embedded/proxy 真源来自 routes 装配决策（flow 读 center_client 配置）。
    cmx_web_monitor::set_topology_provider(crate::routes::service_topology);
    // 活体探测器：对 proxy 目标（如独立 flow-server）周期打 /_mon/tech-stats 判可达/延迟/版本。
    cmx_web_monitor::spawn_topology_prober();

    // 流程引擎：**独立微服务**（引擎核 cmx-flow-app 在独立 ws ../cmx-flowengine，由 cmx-flow-server
    // 承载）。门户不再进程内嵌引擎/poller——只按 [center_client] 的服务定位配置反代 /api/flow/*
    //（见 routes.rs merge_flow）。故此处不再调 spawn_timer_poller（那条依赖已随壳瘦身移除）。
    if routes::flow_is_proxied() {
        info!("流程引擎：独立微服务模式，本进程不启动内嵌引擎 poller（转发到远程 flow-server）");
    } else {
        info!("流程引擎：未配置反代目标（[center_client] mode=local 或未配 flow 键）→ 门户无流程路由；请启动独立 cmx-flow-server 并配置其地址");
    }

    init_web_config().map_err(|e| Error::ConfigError(format!("加载 Web 配置失败: {}", e)))?;
    let web_config =
        web_config().map_err(|e| Error::ConfigError(format!("获取 Web 配置失败: {}", e)))?;

    init_debug();

    init_runtime().await?;

    init_event_bus().map_err(|e| Error::ServerSetup(format!("初始化全局事件总线失败: {e}")))?;

    init_services()
        .await
        .map_err(|e| Error::ServiceInit(format!("服务管理器初始化失败: {e}")))?;
    init_plugins()
        .await
        .map_err(|e| Error::PluginInit(format!("插件管理器初始化失败: {e}")))?;
    init_service_invoker()
        .await
        .map_err(|e| Error::ServiceInit(format!("服务调用器初始化失败: {e}")))?;

    // 编码引擎全局注入（供 DCT/DOC 钩子调用，未注入则钩子跳过=现状零影响）。
    init_code_engine();

    // ── IAM + 认证（审计→IAM→认证→系统身份→finalize→权限校验）──
    // 审计日志器依赖 DatabaseManager，必须在 init_datasources 之后。
    let audit_logger = build_audit_logger().await?;

    // init_iam_services 产出 ResourceDataImporter 和 DefinitionImporterBundle，供 HTTP/gRPC 统一
    // 调用权限导入逻辑，以及模块导入/导出复用统一导入器。
    let (iam_state, user_auth_query, iam_config, resource_data_importer, definition_importers) =
        init_iam_services(audit_logger.clone()).await?;

    let auth_service = init_auth_service(user_auth_query, audit_logger.clone()).await?;

    // 全局系统身份（供后台任务经 system_auth() 获取），必须在 finalize_iam_state 之前。
    init_system_identity();

    // 用 auth_service 完成 IamState 的最终组装（注入 UserServiceImpl）。
    let iam_state =
        finalize_iam_state(&iam_state, auth_service.clone(), iam_config, audit_logger).await?;

    // 权限一致性校验 + 权限列表日志（不写 DB，仅校验代码声明权限与 DB 是否一致）。
    run_permission_check().await?;

    // ── RPC + AppState + 后台子系统 ──
    // RPC 默认关闭（需 [rpc] enabled = true）。BizFunctionInvoker 在组装层构造后注入 cmx-rpc，
    // 使基础设施层 cmx-rpc 无需直接依赖业务层 cmx-biz。
    // ★ 主应用提供的 RPC 服务 = 此处显式收集的皮肤 crate Bundle 列表：
    //   依赖哪个域的 *-rpc crate 并在此注册其 Bundle，即对外提供哪个 gRPC 服务；
    //   裁剪能力（精简版/独立微服务形态）只需增删本列表，cmx-rpc 与皮肤 crate 零改动。
    let rpc_bundles: Vec<Box<dyn cmx_rpc::bundle::RpcServiceBundle>> = vec![
        Box::new(cmx_orchestrator_rpc::OrchestratorBundle),
        Box::new(cmx_resource_rpc::ResourceDataBundle),
    ];
    let grpc_port = init_rpc(
        rpc_bundles,
        cmx_traits::service::GlobalServiceInvoker::get().clone(),
        build_function_invoker(),
        resource_data_importer.clone(),
        Some(auth_service.clone()),
    )
    .await
    .map_err(|e| Error::ServerSetup(format!("RPC 初始化失败: {e}")))?;

    let app_state = build_app_state(
        auth_service,
        iam_state,
        resource_data_importer,
        definition_importers,
    );

    // AI 子系统（薄代理）：加载 OpenCode 配置、构建全局客户端、拉起后台 SSE relay task。
    // 幂等；配置缺失时以默认值（http://127.0.0.1:4096）启动，/api/ai/* 接口仍可调用。
    cmx_ai::init_ai_subsystem().await;

    // 异步任务中心（M3 分布式态）：PG 持久化 + 终态告警 + claim/heartbeat/reaper 三循环。
    init_job_center().await;

    // M5 主数据分发引擎已随主数据抽取迁至独立 cmx-mdm-server（../cmx-mdm），
    // 由其启动钩子按同一配置开关（[mdm.distribution].enabled）拉起——门户不再进程内嵌。

    // ── 路由 + 监听 + 服务 ──
    let routes_all = build_router(app_state, web_config);

    let server_host = ConfigManager::global()
        .get_string("server.host")
        .unwrap_or_else(|_| "0.0.0.0".to_string());
    let server_port: u16 = ConfigManager::global()
        .get_string("server.port")
        .unwrap_or_else(|_| "8080".to_string())
        .parse()
        .unwrap_or(8080);

    let listener = TcpListener::bind(format!("{}:{}", server_host, server_port))
        .await
        .map_err(|e| Error::ServerSetup(format!("绑定地址失败: {}", e)))?;

    let actual_port = listener.local_addr()?.port();

    info!("{}", "=".repeat(60));
    info!("🚀 {:<44} 🚀", "Web 服务器启动成功");
    info!("{}", "=".repeat(60));
    info!("   监听地址：{}:{}", server_host, actual_port);
    info!("   (配置端口：{})", server_port);
    info!("   静态文件目录：{}", web_config.web_folder);
    info!("   日志目录：{}", "logs");
    if let Some(port) = grpc_port {
        info!("   gRPC 端口：{}", port);
    }
    info!("{}", "-".repeat(60));

    // 启动字符画 Logo（直接打印到 stdout，避免被日志格式化器加上时间戳/级别前缀，
    // 仿照 Redis 启动时的 ASCII 艺术字效果）。放在所有启动日志之后，作为最后输出。
    // banner 由调用方（各微服务 bin）传入——门户/报表/主数据各打印自己个性化的字符画/配色，
    // 经通用骨架 cmx-web-chassis 的 BannerSpec 定制（非终端时降级纯文本，避免 ANSI 污染日志）。
    cmx_web_chassis::banner::print(&banner);

    // serve + 优雅关闭（SIGINT/SIGTERM + 超时兜底）下沉到通用骨架 cmx-web-chassis，
    // 与 flow-server 完全一致的关闭语义。平台的 routes_all（含平台中间件/静态托管/fallback）
    // 与 listener 已在上面组装好，此处只把「serve 循环」交给骨架。
    cmx_web_chassis::serve_with_shutdown(
        listener,
        routes_all,
        graceful_shutdown_timeout().as_secs(),
    )
    .await
    .map_err(|e| Error::ServerSetup(e.to_string()))?;

    // serve 返回即已收到关闭信号（或服务器自然结束）：执行平台侧退出清理。
    info!("开始优雅关闭...");
    shutdown_infra().await;
    info!("服务已优雅关闭");

    Ok(())
}

/// 平台身份读取器：把平台请求级 [`AuthContext`]（context_scope task_local）映射为
/// cmx-web-monitor 的中性 `Identity`，供 observe 遥测记录租户/用户/角色。
/// 无认证上下文（未登录 / 静态资源）时返回 None → 记为匿名。
/// 租户：平台 `AuthContext` 无单一租户串，取最接近的 `org_id`（缺省 default）。
fn web_identity() -> Option<cmx_web_monitor::Identity> {
    cmx_traits::auth::context_scope::current_auth().map(|a| cmx_web_monitor::Identity {
        tenant: a.org_id.clone().unwrap_or_else(|| "default".to_string()),
        user: Some(if a.username.is_empty() {
            a.user_id.clone()
        } else {
            a.username.clone()
        }),
        roles: a.roles.clone(),
    })
}

/// 优雅关闭超时（秒）。优先级：`CMX_GRACEFUL_SHUTDOWN_TIMEOUT_SECS` 环境变量 >
/// `server.graceful_shutdown_timeout_secs` 配置 > 默认 10s。0 视为无效回退默认。
fn graceful_shutdown_timeout() -> Duration {
    const DEFAULT_SECS: u64 = 10;

    let secs = std::env::var("CMX_GRACEFUL_SHUTDOWN_TIMEOUT_SECS")
        .ok()
        .and_then(|raw| raw.trim().parse::<u64>().ok())
        .or_else(|| {
            ConfigManager::global()
                .get_int("server.graceful_shutdown_timeout_secs")
                .ok()
                .and_then(|value| u64::try_from(value).ok())
        })
        .filter(|secs| *secs > 0)
        .unwrap_or(DEFAULT_SECS);

    Duration::from_secs(secs)
}

