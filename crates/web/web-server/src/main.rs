//! Web 服务器主模块
//!
//! 该模块是应用程序的入口点，负责初始化各种组件、配置路由并启动 HTTP 服务器。

// macOS Apple 链接器（ld-1267+）对超大 debug 二进制会报 `__eh_frame section too large
// (max 16MB)`：本 workspace 体量大 + 完整 debuginfo 使 DWARF 栈展开段超 16MB，compact unwind
// 表偏移量装不下。后果仅为「panic 展开性能*可能*下降」，不影响正确性/运行。Rust 1.97 起
// `linker_messages` lint 把链接器 stderr 抬成告警才使其显现（代码未变差）。此处按其良性静音。
#![allow(linker_messages)]

mod app_state;
mod config;
mod error;
mod router;
mod routes;

pub use self::error::{Error, Result};
use config::web_config;

/// 启动字符画 Logo（编译期嵌入二进制，无运行时文件路径依赖）
const BANNER: &str = include_str!("banner.txt");

use crate::app_state::build_app_state;
use crate::config::{
    build_audit_logger, build_function_invoker, finalize_iam_state, init_auth_service, init_cache,
    init_code_engine, init_datasources, init_infra, init_iam_services, init_job_center,
    init_plugins, init_rpc, init_runtime, init_service_invoker, init_services, init_storage,
    init_system_identity, init_web_config, run_permission_check, shutdown_infra,
};
use crate::router::build_router;
use cmx_utils::ConfigManager;
use std::time::Duration;
use tokio::net::TcpListener;
use tracing::{info, warn};

/// 应用程序主函数
///
/// 负责初始化配置、设置日志、初始化连接池、配置路由并启动服务器。
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
#[tokio::main]
async fn main() -> Result<()> {
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
    init_infra().await?;
    cmx_utils::crypto::CryptoService::init_from_env();
    info!("加密服务初始化完成");

    init_cache().await?;
    init_datasources().await?;
    init_storage().await?;

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

    // 流程引擎就绪：装载已发布定义 + 启动定时器 poller（依赖数据源，故在 init_datasources 之后）。
    // 非致命：流程 DB/schema 不可用时只 warn，不阻塞 web-server 启动。
    // S6：独立微服务模式（配了 [center_client.urls].flow）时引擎在远程，本进程不起引擎/poller，
    // /api/flow/* 由 FlowProxyModule 转发（见 routes.rs）。
    if routes::flow_is_proxied() {
        info!("流程引擎：独立微服务模式，本进程不启动内嵌引擎 poller（转发到远程 flow-server）");
    } else if let Err(e) = cmx_flow_api::spawn_timer_poller().await {
        warn!("流程引擎初始化失败（流程功能不可用，其余服务照常）: {}", e);
    }

    init_web_config().map_err(|e| Error::ConfigError(format!("加载 Web 配置失败: {}", e)))?;
    let web_config =
        web_config().map_err(|e| Error::ConfigError(format!("获取 Web 配置失败: {}", e)))?;

    cmx_debug::init();
    info!("调试会话管理器初始化完成");

    init_runtime().await?;

    cmx_traits::event_bus::GlobalEventBus::initialize()
        .map_err(|e| Error::ServerSetup(format!("初始化全局事件总线失败: {}", e)))?;
    info!("全局事件总线初始化完成");

    init_services().await?;
    init_plugins().await?;
    init_service_invoker().await?;

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
    let grpc_port = init_rpc(
        cmx_traits::service::GlobalServiceInvoker::get().clone(),
        build_function_invoker(),
        resource_data_importer.clone(),
        Some(auth_service.clone()),
    )
    .await?;

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
    print_banner();

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

/// 打印带渐变色的启动 Logo。
///
/// 对 [`BANNER`] 逐行施加「青 → 蓝 → 紫 → 品红」的纵向 24-bit 真彩渐变。
/// 当 stdout 不是终端时（如重定向到文件 / 日志收集管道）降级为纯文本输出，
/// 避免把 ANSI 转义码写进日志。
fn print_banner() {
    use std::io::IsTerminal;

    // Logo 下方的标语
    const TAGLINE: &str = "  Enterprise Business Container V1.1.6 ";

    // 非终端：输出纯文本，避免 ANSI 码污染日志
    if !std::io::stdout().is_terminal() {
        println!("{}", BANNER);
        println!("{}", TAGLINE);
        return;
    }

    // 渐变停靠点（RGB）：青 → 蓝 → 紫 → 品红
    const STOPS: [(u8, u8, u8); 4] = [
        (0, 229, 255),  // 青
        (41, 121, 255), // 蓝
        (124, 77, 255), // 紫
        (255, 64, 200), // 品红
    ];

    let lines: Vec<&str> = BANNER.lines().collect();
    // 仅按「有内容的行」计算渐变位置，空行不参与
    let total = lines.iter().filter(|l| !l.trim().is_empty()).count();
    let denom = total.saturating_sub(1).max(1) as f32;

    let mut content_idx = 0usize;
    for line in &lines {
        if line.trim().is_empty() {
            println!();
            continue;
        }
        let t = content_idx as f32 / denom;
        let (r, g, b) = gradient_color(&STOPS, t);
        // \x1b[1;38;2;R;G;Bm = 加粗 + 24-bit 前景色
        println!("\x1b[1;38;2;{};{};{}m{}\x1b[0m", r, g, b, line);
        content_idx += 1;
    }

    // 字符画之后换行，再打印标语（取渐变末端的品红色，加粗）
    let (r, g, b) = STOPS[STOPS.len() - 1];
    println!("\n\x1b[1;38;2;{};{};{}m{}\x1b[0m", r, g, b, TAGLINE);
}

/// 在多个 RGB 停靠点之间按 `t ∈ [0,1]` 线性插值，得到渐变色。
fn gradient_color(stops: &[(u8, u8, u8)], t: f32) -> (u8, u8, u8) {
    let seg = stops.len().saturating_sub(1);
    if seg == 0 {
        return stops.first().copied().unwrap_or((255, 255, 255));
    }
    let scaled = t.clamp(0.0, 1.0) * seg as f32;
    let i = (scaled.floor() as usize).min(seg - 1);
    let local = scaled - i as f32;
    let (r0, g0, b0) = stops[i];
    let (r1, g1, b1) = stops[i + 1];
    let lerp = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * local).round() as u8;
    (lerp(r0, r1), lerp(g0, g1), lerp(b0, b1))
}

/// HTTP 优雅关闭最长等待时间。
///
/// Axum 会等待所有活动连接结束；SSE、长轮询或调试页保持的连接可能让等待无限持续。
/// 可通过 `CMX_GRACEFUL_SHUTDOWN_TIMEOUT_SECS` 或 `server.graceful_shutdown_timeout_secs`
/// 调整，默认 10 秒。
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
