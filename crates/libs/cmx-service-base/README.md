# cmx-service-base

> 基础服务库：把「一个微服务起服前要初始化的基础设施」——配置加载、Redis 缓存/分布式锁、数据源注册、加密/调试/事件总线/存储/插件/WASM 运行时、Nacos 注册与配置中心、RPC 子系统——收成按 feature 门控的可复用原语，供门户 / 流程 / 报表 / 规则各微服务 main 按需调用。

[![Version](https://img.shields.io/badge/version-0.1.12-blue.svg)]()
[![Edition](https://img.shields.io/badge/rust--edition-2024-orange.svg)]()
[![Authors](https://img.shields.io/badge/authors-skylake%40pansoft.com-lightgrey.svg)]()

---

## 项目简介

`cmx-service-base` 是 CMX 微服务群的**起服基础设施公用包**。每个能力中心（portal / flow / report / rules）的 `main.rs` 在业务装配之前都有一段几乎相同的基础设施初始化：加载配置、起数据源、连 Redis、初始化各种全局单例。本 crate 把这些步骤从原 `web-server`（今 `cmx-platform-app`）逐文件提取，收成一个个独立 `init_*` 原语。

### 与 cmx-web-chassis 的分层

| crate | 定位 | 依赖约束 |
|-------|------|---------|
| `cmx-web-chassis` | 纯框架：日志 / 端口 / 中间件 / 优雅关闭 / banner | **零 infra 依赖**，能被任何 workspace 消费 |
| `cmx-service-base`（本 crate） | 基础服务：碰 Redis / DB / ConfigManager | feature 门控，不需要的服务可干净 opt-out |

刻意分成两个 crate 而非扩 chassis：chassis 的契约是零 infra 依赖（flowengine 等零 Redis/sqlx 的 workspace 也能用）；本 crate 用 feature 让不需要 Redis/sqlx/ConfigManager 的服务（如 flow）以 `default-features = false` 只拉轻量核。

### 关键拆分原则：通用 vs portal 专属

- **通用部分进本库**：如 `init_wasm` 注册 logging/db/buffer 三个通用 host-fn provider，portal 专属的 IAM/plugin provider 经 `extra_providers` 参数注入；`init_rpc` 的 `function_invoker` 由调用方组装层构造后传入。
- **portal 专属留在 portal**：`build_function_invoker()`（绑 cmx-biz）、`load_outgoing_credential()`（绑 cmx-plugin）、sqlx 数据源的持久化/迁移/回读（`cmx_sys_datasource` 相关）。因此本库**不依赖任何业务 crate**。

---

## 与其他 crate 的关系

### 上游依赖（按 feature）

| 依赖 | 拉入的 feature | 用途 |
|------|---------------|------|
| `cmx-database-pg` / `cmx-core` | default（轻量核） | tokio-postgres 数据源管理（`DbConfig`）与核心类型 |
| `cmx-buffer` | `redis` / `wasm` | `RedisClient` / `RedisConfig` / 缓存与分布式锁全局管理器 / buffer host-fn |
| `cmx-utils` | `config-manager` / `crypto` / `storage` / `services` / `plugins` / `registry-config` / `rpc` | ConfigManager、CryptoService |
| `cmx-debug` | `debug` | 调试会话管理器 |
| `cmx-traits` | `event-bus` / `services` / `wasm` / `rpc` | GlobalEventBus、HostFunctionProvider、AuthService/FunctionInvoker 等 trait |
| `cmx-database` | `db-sqlx` / `storage` / `services` / `plugins` / `wasm`（feature 声明） | sqlx DatabaseManager、DatabaseHostFunctions |
| `cmx-storage` | `storage` | StorageManager / GlobalStorageService |
| `cmx-service` | `services` | 服务仓储 / 注册中心 / 调用器 |
| `cmx-runtime` | `services` / `wasm` | Extism 引擎、GlobalExtismEngine |
| `cmx-plugin` | `services` / `plugins` | GlobalPluginManager、PluginManagerSettings |
| `cmx-registry-config` | `registry-config` / `rpc` | 注册中心 / 配置中心 SDK |
| `cmx-rpc` | `rpc` | gRPC 客户端/服务端、RpcServiceBundle |
| `tokio` / `local-ip-address` | `registry-config` / `rpc` | 后台任务与本机 IP 探测 |
| `toml` / `serde` / `tracing` / `anyhow` / `thiserror` | default | 基础设施 |

### 下游使用方（跨 workspace 实测）

| 使用方 | 引用方式 | 启用的 feature | 实际用途 |
|--------|---------|---------------|---------|
| `cmx-platform-app`（cmx-container） | `workspace = true, features = ["redis","config-manager","crypto","debug","event-bus","storage","plugins","services","wasm","registry-config","rpc"]` | 除 `db-sqlx` 外全开 | 门户总装配器：`run_platform` 的 20 步 init 中基础设施部分全部经本库 |
| `cmx-flowengine`（独立 ws） | `path = "../cmx-container/crates/libs/cmx-service-base", default-features = false`；flow-server 加 `features = ["config-manager"]` | 轻量核 + config-manager | flow-server 调 `init_config_manager` + `BaseConfig::from_toml_path` + `register_pg_datasources` |
| `cmx-report`（独立 ws） | 同上（`default-features = false`，rpt-server 加 `config-manager`） | 轻量核 + config-manager | report-server 起服配置链与 pg 数据源注册 |
| `cmx-rulesengine`（独立 ws） | 同上；rule-app 全关、rule-server 加 `config-manager` | 轻量核（+config-manager） | 规则微服务起服 |

---

## 核心功能与特性

| 原语 | feature | 一句话 |
|------|---------|--------|
| `BaseConfig::from_toml_path` | default | 轻量加载 `[[databases]]` + `[redis]`（文件缺失回退空配置） |
| `BaseConfig::from_config_manager` | config-manager | 重量加载：读全局 ConfigManager 的多源配置 |
| `init_config_manager` | config-manager | 所有能力中心共用的唯一一段 ConfigManager 装配（CONFIG_FILE → env） |
| `register_pg_datasources` | default | 把 pg 形态 DbConfig 注册到 tokio-postgres 全局管理器（失败仅 warn 不阻断） |
| `init_cache(cfg)` | redis | Redis 缓存 + 分布式锁（共享同一 client） |
| `init_crypto` | crypto | 全局加密服务（读 env `CMX_ENCRYPT_KEY`，幂等） |
| `init_debug` | debug | 全局调试会话管理器（起后台清理线程，幂等） |
| `init_event_bus` | event-bus | 全局事件总线 |
| `init_storage` | storage | 文件存储服务（读 `[storage]`，依赖 DB manager 已起） |
| `init_services` / `init_service_invoker` | services | 服务仓储/注册中心/生命周期监听器 + 全局服务调用器 |
| `init_plugins` | plugins | 插件管理器（读 `plugin.*`，须在 wasm 之后） |
| `init_wasm(extra_providers)` | wasm | Extism 引擎 + logging/db/buffer 三个通用 host-fn provider + 注入项 |
| `init_infra` / `shutdown_infra` | registry-config | Nacos 注册/配置中心 + 多源配置合并 + 热更新监听 + 服务实例注册/注销 |
| `init_rpc(...)` | rpc | gRPC 客户端 + 服务端启动（Bundle 注入式）+ 缓存预热 |
| `load_service_auth_config` | rpc | 读 `[service_auth]` 服务对外凭证 |

---

## 模块结构

```text
cmx-service-base
├── src
│   ├── lib.rs             # pub 导出汇总 + BaseError/Result
│   ├── config.rs          # BaseConfig 两个构造器 + init_config_manager
│   ├── datasource.rs      # register_pg_datasources（pg 新链路，flow+portal 共享）
│   ├── cache.rs           # init_cache（redis）：缓存 + 分布式锁共享 client
│   ├── crypto.rs          # init_crypto：CryptoService::init_from_env
│   ├── debug.rs           # init_debug：cmx_debug::init
│   ├── event_bus.rs       # init_event_bus：GlobalEventBus::initialize
│   ├── storage.rs         # init_storage：StorageManager → GlobalStorageService + 本地静态路由
│   ├── services.rs        # init_services / init_service_invoker（须在 wasm+plugins 后）
│   ├── plugins.rs         # init_plugins：PluginManagerSettings → GlobalPluginManager
│   ├── wasm.rs            # init_wasm：3 通用 provider + extra_providers 注入
│   ├── registry_config.rs # init_infra / shutdown_infra：注册/配置中心全链（376 行，最大模块）
│   └── rpc.rs             # init_rpc / load_rpc_config / load_service_auth_config
└── Cargo.toml             # feature 声明（default = []）
```

---

## 关键类型 / API

### 配置（`src/config.rs`）

```rust
/// 微服务基础资源配置（不含 Debug derive，因 cmx_database_pg::DbConfig 未实现 Debug）
pub struct BaseConfig {
    pub databases: Vec<cmx_database_pg::DbConfig>,      // toml [[databases]]
    #[cfg(feature = "redis")]
    pub redis: Option<cmx_buffer::RedisConfig>,          // toml [redis]
}

impl BaseConfig {
    /// 轻量加载：直接解析 toml 文件；文件不存在 → 返回 Default（各服务可 env 兜底）
    pub fn from_toml_path(path: impl AsRef<Path>) -> Result<Self>;
    /// 重量加载：从全局 ConfigManager 读（调用前 ConfigManager 须已 initialize）
    #[cfg(feature = "config-manager")]
    pub fn from_config_manager() -> Result<Self>;
}

/// 全局 ConfigManager 装配（CONFIG_FILE toml → env 覆盖；幂等）
#[cfg(feature = "config-manager")]
pub fn init_config_manager() -> Result<()>;
```

### 数据源与缓存

```rust
/// 注册一组 pg 数据源（tokio-postgres 链路）；非 Postgres 项跳过；单项失败仅 warn 不阻断
pub async fn register_pg_datasources(configs: &[cmx_database_pg::DbConfig]) -> Result<()>;

/// 用给定 RedisConfig 初始化全局缓存 + 分布式锁（共享同一 RedisClient）
#[cfg(feature = "redis")]
pub async fn init_cache(cfg: cmx_buffer::RedisConfig) -> Result<()>;
```

### WASM / 服务 / 插件（注入式 init）

```rust
/// 建 Extism 引擎 + 注册 logging/db/buffer 三个通用 provider + 调用方注入的额外 provider
#[cfg(feature = "wasm")]
pub async fn init_wasm(extra_providers: Vec<Arc<dyn cmx_traits::runtime::HostFunctionProvider>>) -> Result<()>;

/// 服务管理器（仓储 + 注册中心 + 生命周期监听器，延迟加载策略）
#[cfg(feature = "services")]
pub async fn init_services() -> Result<()>;
/// 全局服务调用器（组合 runtime + plugin + service query；须在 plugins 之后）
#[cfg(feature = "services")]
pub async fn init_service_invoker() -> Result<()>;

/// 插件管理器（读 plugin.* 配置；须在 wasm 之后）
#[cfg(feature = "plugins")]
pub async fn init_plugins() -> Result<()>;
```

### 注册/配置中心与 RPC

```rust
/// 微服务 main 的第一个 init 入口：多源配置合并（本地 TOML + 远程配置中心 + env）→
/// ConfigManager；创建注册/配置中心单例；注册服务实例；起服务列表同步（30s）与热更新监听
#[cfg(feature = "registry-config")]
pub async fn init_infra() -> Result<()>;
/// 优雅关闭：停同步器 + 从注册中心注销实例
#[cfg(feature = "registry-config")]
pub async fn shutdown_infra();

/// gRPC 子系统：客户端注册（bundles）+ 服务端启动 + warmup_services 预热。
/// 返回 Ok(Some(grpc_port)) 表示已启用；Err 或 Ok(None) 表示未启用/失败
#[cfg(feature = "rpc")]
pub async fn init_rpc(
    bundles: Vec<Box<dyn cmx_rpc::bundle::RpcServiceBundle>>,   // 组装层显式收集的领域 Bundle
    service_invoker: Arc<dyn cmx_traits::service::ServiceInvoker>,
    function_invoker: Arc<dyn cmx_traits::function_invoker::FunctionInvoker>, // 调用方构造后注入
    data_importer: Option<Arc<dyn cmx_traits::resource::ResourceDataImporter>>,
    auth_service: Option<Arc<dyn cmx_traits::auth::AuthService>>,              // None = 不启用 gRPC 鉴权
) -> Result<Option<u16>>;

/// [service_auth] 段：本服务作为调用方的服务级凭证（cmx_sk_xxx）
#[cfg(feature = "rpc")]
pub struct ServiceAuthConfig { pub outgoing_api_key: String }
#[cfg(feature = "rpc")]
pub fn load_service_auth_config() -> ServiceAuthConfig;
```

### 错误

```rust
pub enum BaseError {
    Config(String),    // 配置加载/解析失败
    Setup(String),     // 基础设施初始化失败（Redis/DB 等）
}
pub type Result<T> = std::result::Result<T, BaseError>;
```

---

## 使用示例

### 一、轻量微服务起服（flow / report / rules 模式，default-features = false）

```rust
// Cargo.toml: cmx-service-base = { workspace = true, default-features = false }
//             （需要 ConfigManager 时再加 features = ["config-manager"]）
async fn boot() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();

    // ① 全局配置装配（能力中心统一契约：CONFIG_FILE toml → env）
    cmx_service_base::init_config_manager()
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    // ② 轻量加载基础资源（文件缺失回退空配置，可 env 兜底）
    let cfg = cmx_service_base::BaseConfig::from_toml_path("flow-server.toml")
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    // ③ 注册 tokio-postgres 数据源（新链路；建池即首连验证，库不可达返回 Err——
    //    引擎启动钩子据此 fail-fast 终止启动）
    cmx_service_base::register_pg_datasources(&cfg.databases).await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(())
}
```

### 二、门户全量装配（cmx-platform-app 模式：20 步 init 的基础设施段）

```rust
use std::sync::Arc;

async fn boot_infra() -> cmx_service_base::Result<()> {
    // ① 第一个 init：注册中心 + 配置中心 + 多源配置合并 + 服务实例注册 + 热更新监听
    cmx_service_base::init_infra().await?;

    // ② 加密服务（数据源 db_url 解密依赖它 → 必须在 datasource 之前）
    cmx_service_base::init_crypto();

    // ③ WASM 运行时：3 个通用 provider + portal 注入的 iam/plugin provider
    cmx_service_base::init_wasm(vec![
        // Arc::new(IamHostFunctions::new(...)),      // portal 专属，由调用方构造注入
        // Arc::new(PluginHostFunctions::new(...)),
    ]).await?;

    // ④ 顺序敏感：services → plugins → service_invoker
    cmx_service_base::init_services().await?;
    cmx_service_base::init_plugins().await?;
    cmx_service_base::init_service_invoker().await?;

    // ⑤ RPC（[rpc] enabled=true 才真正启动；Bundle 与 invoker 均为注入式）
    let grpc_port = cmx_service_base::init_rpc(
        vec![Box::new(cmx_orchestrator_rpc::OrchestratorBundle)],
        global_service_invoker(),
        function_invoker,          // portal 侧 build_function_invoker()（绑 cmx-biz）
        None,
        Some(auth_service),
    ).await?;

    // …应用退出时：注销服务实例、停同步器
    cmx_service_base::shutdown_infra().await;
    let _ = grpc_port;
    Ok(())
}
```

### 三、Redis 缓存 + 分布式锁（本函数不吃 ConfigManager，配置自取）

```rust
use cmx_buffer::RedisConfig;

async fn boot_cache() -> cmx_service_base::Result<()> {
    // 从任意来源拿到 RedisConfig（门户侧是读 ConfigManager 的 [redis] 后再调本函数）
    let cfg = RedisConfig {
        url: "redis://127.0.0.1:6379".to_string(),
        ..Default::default()
    };
    // 内部：创建唯一 RedisClient，GlobalCacheManager 与 GlobalLockManager 共享，
    // 避免多个独立连接池
    cmx_service_base::init_cache(cfg).await
}
```

---

## Features 说明

`default = []`——**轻量核只有** `BaseConfig::from_toml_path` + `register_pg_datasources`（仅依赖 cmx-database-pg/cmx-core，跨 workspace 安全）。其余全部 opt-in：

| Feature | 拉入依赖 | 默认 | 说明 |
|---------|---------|:---:|------|
| `default` | cmx-database-pg / cmx-core | ✅ | 轻量核：toml 直读加载 + pg 数据源注册 |
| `redis` | `dep:cmx-buffer` | ❌ | Redis 缓存/分布式锁 init（portal 开、flow 不开）；`BaseConfig.redis` 字段随之出现 |
| `config-manager` | `dep:cmx-utils` | ❌ | `from_config_manager` 重量加载器 + `init_config_manager`（CONFIG_FILE+Nacos+env）。portal 专属；flow/rpt/rule-server 也用它做统一配置链 |
| `crypto` | `dep:cmx-utils` | ❌ | 加密服务 init（`CryptoService::init_from_env`，读 `CMX_ENCRYPT_KEY`）。纯全局、幂等 |
| `debug` | `dep:cmx-debug` | ❌ | 调试会话管理器 init（后台清理线程）。纯全局 |
| `event-bus` | `dep:cmx-traits` | ❌ | 全局事件总线 init。纯全局 |
| `db-sqlx` | `dep:cmx-database` | ❌ | **预留**：sqlx 数据源建池 + 迁移的通用部分。当前源码无 `#[cfg(feature = "db-sqlx")]` 引用——重逻辑（`cmx_sys_datasource` 持久化 / 迁移 / 从库回读 / 按部署模式过滤）留在 portal 侧 `cmx-platform-app::config::datasource` |
| `storage` | `dep:cmx-storage` + `dep:cmx-database` + `dep:cmx-utils` | ❌ | 文件存储服务 init（读 `[storage]`，依赖 DB manager 已起，须在 datasource 之后） |
| `services` | `dep:cmx-service` + `dep:cmx-runtime` + `dep:cmx-traits` + `dep:cmx-database` + `dep:cmx-utils` + `dep:cmx-plugin` | ❌ | 服务管理器 + 全局调用器 init（`init_service_invoker` 须在 wasm + plugins 之后） |
| `plugins` | `dep:cmx-plugin` + `dep:cmx-database` + `dep:cmx-utils` | ❌ | 插件管理器 init（读 `plugin.*`：install/backup/temp 根、auto_install、对账间隔） |
| `wasm` | `dep:cmx-runtime` + `dep:cmx-traits` + `dep:cmx-database` + `dep:cmx-utils` + `dep:cmx-buffer` | ❌ | WASM 运行时 init：logging/db/buffer 三个通用 host-fn provider；IAM/plugin provider 由调用方经 `extra_providers` 注入（本库不碰 cmx-iam/cmx-plugin） |
| `registry-config` | `dep:cmx-registry-config` + `dep:cmx-utils` + `dep:tokio` + `dep:local-ip-address` | ❌ | `init_infra`/`shutdown_infra`：Nacos 注册/配置中心、多源配置、服务实例注册/注销、30s 服务列表同步、配置热更新 |
| `rpc` | `dep:cmx-rpc` + `dep:cmx-registry-config` + `dep:cmx-traits` + `dep:cmx-utils` + `dep:tokio` | ❌ | `init_rpc`（gRPC 客户端 + 服务端 + 预热）。`function_invoker` 由 portal 注入（`build_function_invoker` 绑 cmx-biz 留 portal），本库不碰 cmx-biz |

---

## 关键设计决策

### 1. 为什么 feature 门控而不是全量依赖？

flow / rules 等微服务没有 Redis、不跑 sqlx、不用插件平台。若全量依赖，跨 workspace path 引用会拖入整棵门户依赖树（编译时间与产物体积爆炸）。`default-features = false` 后轻量核只依赖 cmx-database-pg + cmx-core。

### 2. trait/hook 拆分：init 函数为什么是「注入式」？

`init_wasm(extra_providers)` 与 `init_rpc(..., function_invoker, ...)` 把 portal 专属实现（`IamHostFunctions` / `BizFunctionInvoker`）作为参数由组装层构造后注入。本库因此只依赖 trait 所在的 `cmx-traits`，不依赖 `cmx-iam` / `cmx-biz` / `cmx-plugin`（出站凭证 `load_outgoing_credential` 绑 cmx-plugin，故留 portal）。

### 3. registry-config 与 rpc 的编译期交叉

注册实例的 metadata 需要带 `grpc_port`（RPC 端口），而 rpc 又依赖注册中心缓存。解法：`inject_rpc_metadata` 里 grpc_port 注入分支用 `#[cfg(feature = "rpc")]` 条件编译——`registry-config` 单开时不注入、不硬依赖 rpc 模块。

### 4. init 顺序约定（调用方负责）

本库不强制顺序，但以下顺序是 load-bearing 的（门户 20 步 init 的依据）：`init_crypto` 在 datasource 前（db_url 解密）；`init_storage` 在 datasource 后（文件元信息入库）；`init_wasm` 在 `init_plugins` / `init_services` 前（host-fn 先注册）；`init_service_invoker` 在 plugins 后；`init_rpc` 在 `init_infra`（提供共享缓存）后。

---

## 常见问题

### Q1: `db-sqlx` feature 为什么开了没效果？

它是**预留声明**：Cargo.toml 中声明并拉入 `cmx-database` 依赖，但源码没有任何 `#[cfg(feature = "db-sqlx")]` 分支。sqlx 数据源的完整注册逻辑（`cmx_sys_datasource` 持久化、迁移、从库回读、部署模式过滤）留在 `cmx-platform-app::config::datasource`。`lib.rs` 文档注释中提到的 `register_sqlx_datasources` 当前并不存在。

### Q2: `init_cache` 为什么自己带 RedisConfig 参数？

刻意设计：本函数**不读 ConfigManager**（保持 `redis` feature 轻依赖），配置从哪来是调用方的事。门户侧保留「读 ConfigManager 得 RedisConfig 再委托本函数」的包装。

### Q3: `init_infra` 之后配置热更新怎么订阅？

`init_infra` 内部经 `create_config_center` 注册了变更处理器：配置变更 → `ConfigReloader::reload` 原子替换全局 ConfigManager → `GlobalChangeNotifier::notify_listeners` 通知结构化监听器。业务模块用 `GlobalChangeNotifier::add_listener()` 订阅。

### Q4: flow 跨 workspace 引用会不会拖入 Redis/sqlx？

不会。flowengine 以 `default-features = false` 引用，上述可选依赖一个不拉；轻量核仅 cmx-database-pg / cmx-core / serde / toml / tracing 等，跨 workspace 消费不膨胀。
