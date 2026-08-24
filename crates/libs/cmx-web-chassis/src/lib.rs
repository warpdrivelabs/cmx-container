//! cmx-web-chassis —— 通用 HTTP 服务骨架（chassis）。
//!
//! 把「起一个 axum 微服务」的框架无关部分收成一份可复用代码：分层日志 + 配置 + 中间件栈 +
//! 有序启动钩子 + 优雅关闭 + banner。各服务填一个 [`ServiceSpec`]，调 [`run`] 即得同构服务。
//! 泛型 over 应用状态 `S`；零 cmx-api / 零平台依赖。抽自 web-server 的 main.rs 框架无关切片。
//!
//! ```ignore
//! use cmx_web_chassis::{run, ServiceSpec, ChassisConfig, InitHook};
//! #[tokio::main]
//! async fn main() -> cmx_web_chassis::Result<()> {
//!     let router = my_module::routes::<()>();               // Router<()>
//!     let spec = ServiceSpec::new("flow", ChassisConfig::load("flow", "flow-server.toml"))
//!         .router(router)
//!         .state(())
//!         .init("datasources", |_| Box::pin(async { register_dbs().await; Ok(()) }))
//!         .init("engine", |_| Box::pin(async { spawn_timer_poller().await?; Ok(()) }));
//!     run(spec).await
//! }
//! ```

pub mod banner;
pub mod config;
pub mod error;
pub mod format;
pub mod shutdown;

pub use config::ChassisConfig;
pub use banner::{BannerSpec, Rgb};
pub use error::{ChassisError, Result};

use std::future::Future;
use std::future::IntoFuture;
use std::pin::Pin;
use std::time::Duration;

use axum::extract::DefaultBodyLimit;
use axum::Router;
use tokio::net::TcpListener;
use tower_http::compression::CompressionLayer;
use tower_http::compression::predicate::{DefaultPredicate, NotForContentType, Predicate};
use tower_http::cors::CorsLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::trace::TraceLayer;
use tracing::{info, warn};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, registry, util::SubscriberInitExt};

/// 启动钩子：有序 async 步骤（注册数据源、spawn 后台任务…）。返回 Err 视为致命，中止启动。
///
/// 入参是 `&ServiceMeta`（服务名等），供钩子按需取用。返回 `anyhow::Result<()>`——服务钩子里
/// 各种错误 `?` 上来即可。
pub type InitHook = Box<
    dyn for<'a> FnOnce(&'a ServiceMeta) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send + 'a>>
        + Send,
>;

/// 传给钩子的服务元信息（只读）。
#[derive(Debug, Clone)]
pub struct ServiceMeta {
    pub name: String,
}

/// 请求体大小上限（100 MiB，与 web-server 一致）。
const BODY_LIMIT: usize = 100 * 1024 * 1024;

/// 服务描述：各服务填好后交给 [`run`]。
pub struct ServiceSpec<S = ()> {
    name: String,
    config: ChassisConfig,
    router: Router<S>,
    state: S,
    /// 是否把 router nest 到 `/api` 前缀下（默认 true，对齐平台 `/api/*`）。
    nest_api: bool,
    /// 是否启用通用技术监控（默认 true）：observe 遥测中间件 + 系统采样器 + `/_mon` 页。
    monitor: bool,
    /// 自定义 banner（字符画/标语/渐变配色；None 用默认）。
    banner: Option<BannerSpec>,
    init_hooks: Vec<(String, InitHook)>,
}

impl<S> ServiceSpec<S>
where
    S: Clone + Send + Sync + 'static,
{
    /// 新建（需随后 `.router(..).state(..)`）。
    pub fn new(name: impl Into<String>, config: ChassisConfig) -> Self
    where
        S: Default,
    {
        Self {
            name: name.into(),
            config,
            router: Router::new(),
            state: S::default(),
            nest_api: true,
            monitor: true,
            banner: None,
            init_hooks: Vec::new(),
        }
    }

    /// 设置该服务的路由。
    pub fn router(mut self, router: Router<S>) -> Self {
        self.router = router;
        self
    }

    /// 设置应用状态（`with_state` 用）。
    pub fn state(mut self, state: S) -> Self {
        self.state = state;
        self
    }

    /// 是否 nest 到 `/api`（默认 true）。设 false 则路由挂在根。
    pub fn nest_api(mut self, yes: bool) -> Self {
        self.nest_api = yes;
        self
    }

    /// 是否启用通用技术监控（默认 true）。设 false 完全不挂 `/_mon`、不加 observe 层、不起采样器。
    ///
    /// 开启后（所有 chassis 服务默认）：
    /// - `GET /_mon` 技术监控页（系统 CPU/内存/网络/磁盘 + DB 连接池 + 请求遥测），免认证、挂根（逃 `/api`）；
    /// - `GET /_mon/tech-stats` 该页轮询的聚合 JSON；
    /// - observe 遥测中间件裹在服务路由外层（采集每请求 method/path/协议/身份/状态/耗时）；
    /// - 后台系统采样器（sysinfo，每 3s 刷新一次快照）。
    ///
    /// 身份维度需各服务在起服前 `cmx_web_monitor::set_identity_provider(..)` 注入；未注入则遥测里
    /// 身份列为匿名。服务名取自 `ServiceSpec.name`（用于页标题）。
    pub fn monitor(mut self, yes: bool) -> Self {
        self.monitor = yes;
        self
    }

    /// 自定义 banner（字符画 + 标语 + 渐变配色）。不设则用默认（CMX 字样 + 青→蓝→紫→品红）。
    ///
    /// 各服务可给自己的字符画和配色，例如：
    /// `.banner(BannerSpec::defaults("flow").art(FLOW_ART).stops(vec![(0,255,180),(0,120,255)]))`
    pub fn banner(mut self, banner: BannerSpec) -> Self {
        self.banner = Some(banner);
        self
    }

    /// 追加一个有序启动钩子。
    pub fn init<F>(mut self, name: impl Into<String>, hook: F) -> Self
    where
        F: for<'a> FnOnce(&'a ServiceMeta) -> Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send + 'a>>
            + Send
            + 'static,
    {
        self.init_hooks.push((name.into(), Box::new(hook)));
        self
    }
}

/// 装配并运行服务：日志 → 钩子 → 中间件 → serve（优雅关闭）。
///
/// 与 web-server main 的框架无关序列一致，泛型 over 应用状态 `S`。
pub async fn run<S>(spec: ServiceSpec<S>) -> Result<()>
where
    S: Clone + Send + Sync + 'static,
{
    dotenvy::dotenv().ok();

    // 1) 分层日志（控制台 CompactFormatter + 滚动文件 JSON）。guard 保活至 run 结束。
    let _guard = init_tracing(&spec.config);

    let meta = ServiceMeta { name: spec.name.clone() };

    // 2) 有序启动钩子（致命 Err 中止）。
    for (hook_name, hook) in spec.init_hooks {
        info!(service = %spec.name, hook = %hook_name, "启动钩子执行中…");
        hook(&meta).await.map_err(|source| ChassisError::InitHook {
            name: hook_name.clone(),
            source,
        })?;
    }

    // 3) 路由 + 中间件栈（框架无关下半段：trace/body-limit/CORS/compression）。
    let app_router = if spec.nest_api {
        Router::new().nest("/api", spec.router)
    } else {
        spec.router
    };
    // 3b) 通用技术监控（默认开）：`/_mon` 页 + `/_mon/tech-stats` 挂在根（逃 `/api`）、免认证；
    //     后台起系统采样器（sysinfo：CPU/内存/网络/磁盘，每 3s）。系统 + DB 连接池维度对所有
    //     chassis 服务（flow/report/mdm/…）零配置生效——这是「通用监控」的地基。请求遥测（observe
    //     中间件 + 身份）由各服务按其鉴权顺序自行 `.layer(cmx_web_monitor::observe)` 接入，喂同一
    //     进程级环形缓冲；未接入则监控页请求面板留空、系统/DB 面板照常。
    let app_router = if spec.monitor {
        cmx_web_monitor::set_service_name(spec.name.clone());
        cmx_web_monitor::spawn_system_sampler();
        // 服务依赖拓扑活体探测器（对 proxy 目标周期性打 /_mon/tech-stats）。拓扑来源由各服务
        // 经 set_topology_provider 注入；未注入则探测器空转（廉价）、拓扑面板显「无依赖」。
        cmx_web_monitor::spawn_topology_prober();
        app_router.merge(cmx_web_monitor::monitor_routes())
    } else {
        app_router
    };
    let app = default_layers(app_router.with_state(spec.state));

    // 4) 绑定 + banner。
    let addr = format!("{}:{}", spec.config.host, spec.config.port);
    let listener = TcpListener::bind(&addr)
        .await
        .map_err(|e| ChassisError::ServerSetup(format!("绑定 {addr} 失败: {e}")))?;

    let banner_spec = spec
        .banner
        .clone()
        .unwrap_or_else(|| BannerSpec::defaults(&spec.name));
    print_startup_banner(&spec.name, &spec.config, &listener, &banner_spec);

    // 5) serve + 优雅关闭。
    serve_with_shutdown(listener, app, spec.config.graceful_timeout_secs).await
}

/// 初始化分层日志：控制台 `CompactFormatter`（带色）+ 滚动文件 JSON（按天、非阻塞）。
///
/// 返回 `WorkerGuard`——**调用方必须持有它直到进程退出**，否则文件日志后台线程提前 drop、丢日志。
/// web-server 等平台服务可直接调本函数替代自建 tracing 初始化（形状完全一致）。
pub fn init_tracing(config: &ChassisConfig) -> tracing_appender::non_blocking::WorkerGuard {
    let file_appender = RollingFileAppender::new(Rotation::DAILY, &config.log_dir, &config.log_file);
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
    let file_layer = fmt::layer()
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_target(true)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .with_thread_names(true)
        .json();
    let console_layer = fmt::layer()
        .event_format(format::CompactFormatter)
        .with_writer(std::io::stdout)
        .with_ansi(true);
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.log_level));
    registry()
        .with(env_filter)
        .with(console_layer)
        .with(file_layer)
        .init();
    guard
}

/// 给 router 叠加 chassis 默认中间件栈（trace/body-limit/CORS/compression）。
///
/// 返回带全部中间件的 `Router`。平台服务若要在此基础上再叠加自己的中间件（认证/权限/cookie），
/// 可**不**调本函数、自己组装——本函数只是「框架无关下半段」的便捷封装。
pub fn default_layers(router: Router) -> Router {
    router
        .layer(TraceLayer::new_for_http())
        .layer(RequestBodyLimitLayer::new(BODY_LIMIT))
        .layer(DefaultBodyLimit::max(BODY_LIMIT))
        .layer(CorsLayer::permissive())
        .layer(CompressionLayer::new().compress_when(
            DefaultPredicate::new().and(NotForContentType::const_new("application/octet-stream")),
        ))
}

/// 打印启动信息框 + 渐变 banner（供 run 与平台服务共用）。
pub fn print_startup_banner(
    name: &str,
    config: &ChassisConfig,
    listener: &TcpListener,
    banner: &BannerSpec,
) {
    let actual_port = listener.local_addr().map(|a| a.port()).unwrap_or(config.port);
    info!("{}", "=".repeat(60));
    info!("🚀 {} 服务启动成功", name);
    info!("{}", "=".repeat(60));
    info!("   监听地址：{}:{}", config.host, actual_port);
    info!("   日志目录：{}", config.log_dir);
    info!("{}", "-".repeat(60));
    banner::print(banner);
}

/// serve + 优雅关闭（SIGINT/SIGTERM 触发 → 等待活动连接，超时兜底继续退出）。
///
/// 平台服务把自己**完全组装好的** `app`（含平台中间件/静态托管/fallback）+ listener 交进来，
/// 即可复用与 flow-server 完全一致的优雅关闭语义。`graceful_secs` 为超时兜底秒数。
pub async fn serve_with_shutdown(
    listener: TcpListener,
    app: Router,
    graceful_secs: u64,
) -> Result<()> {
    let graceful = Duration::from_secs(graceful_secs);
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let server = axum::serve(listener, app.into_make_service())
        .with_graceful_shutdown(async move {
            shutdown::shutdown_signal().await;
            let _ = shutdown_tx.send(());
        })
        .into_future();

    tokio::pin!(server);
    tokio::select! {
        result = &mut server => {
            result.map_err(|e| ChassisError::ServerSetup(format!("服务器运行失败: {e}")))?;
        }
        _ = async {
            let _ = shutdown_rx.await;
            tokio::time::sleep(graceful).await;
        } => {
            warn!("优雅关闭等待超过 {} 秒，仍有活动连接，继续退出", graceful.as_secs());
        }
    }
    info!("服务已优雅关闭");
    Ok(())
}
