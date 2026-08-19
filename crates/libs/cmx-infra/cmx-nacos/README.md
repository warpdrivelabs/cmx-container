# cmx-nacos

> ⚠️ **非活跃 member（已注释）** —— 本 crate 目前已从 cmx-container workspace 的 `members` 与 `workspace.dependencies` 中注释掉，未被编译，也无可用的下游依赖方；其活跃替代者为抽象层 crate `cmx-registry-config`。

> 基于 nacos-sdk-rust 封装的 Nacos 微服务集成库：服务注册/发现 + 配置中心（远程配置覆盖本地 TOML），与 cmx-utils 的 config 框架深度集成。

[![Version](https://img.shields.io/badge/version-0.1.12-blue.svg)]()
[![Edition](https://img.shields.io/badge/rust--edition-2024-orange.svg)]()
[![Authors](https://img.shields.io/badge/authors-skylake%40pansoft.com-lightgrey.svg)]()
[![Status](https://img.shields.io/badge/status-非活跃%20member（已注释）-red.svg)]()

---

## 项目简介

`cmx-nacos` 基于 nacos-sdk 0.8 封装 Nacos 的两大能力：

- **服务注册/发现**：通过命名服务实现微服务实例自动注册（`register_service`）、注销（`deregister_service`）与健康实例查询（`query_instances`）；
- **配置中心**：拉取远程 TOML 配置（`get_config`），并经 `NacosConfigSource`（实现底层 `config` crate 的 `Source` trait）注入 `ConfigBuilder::add_source()`，实现**远程配置自动覆盖本地同名配置项**；配合 `RemoteConfigChangeListener` 监听变更并经 `GlobalConfigChangeNotifier` 广播给已注册回调。

配置优先级（从高到低，见 `src/lib.rs` 文档）：**环境变量 > Nacos 远程配置 > 本地 TOML 配置文件 > 代码默认值**。

### 当前状态说明（重要）

- 根 `Cargo.toml` 的 workspace `members` 列表中本 crate 条目被注释（约第 16 行），`[workspace.dependencies]` 中的 `cmx-nacos` 条目同样被注释（约第 145 行）—— 即本 crate **不参与 workspace 编译**；
- 全仓库（cmx-container / cmx-flowengine / cmx-report / cmx-rulesengine / cmx-portalservice）反查 `Cargo.toml`，**无任何 crate 依赖 `cmx-nacos`**；
- `cmx-platform-app/src/config/nacos.rs` 中残留 `use cmx_nacos::...` 引用，但该文件是**孤儿文件**：未挂载进 `config/mod.rs` 的 mod 树，且 `cmx-platform-app` 的 Cargo.toml 已无此依赖，不参与编译；
- 注册中心/配置中心的现行方案是 **`cmx-registry-config`**（可扩展抽象层，支持 Nacos、Mock 及后续扩展），需要新接入 Nacos 的代码应优先使用它，而非恢复本 crate。

### 设计要点

- **零成本禁用**：`NacosConfig.enabled = false`（默认）时 `NacosClient::new` 直接返回空客户端（naming/config 均为 `None`），调用相应方法返回 `NacosError::NamingDisabled` / `ConfigDisabled`；
- **配置模型分层**：`NacosConfig`（连接）内嵌 `NamingConfig`（注册）与 `ConfigCenterConfig`（配置 + 监听列表），均带 serde 默认值，支持 TOML 反序列化与环境变量加载（`NacosConfig::from_env`）；
- **回调隔离**：`ConfigChangeNotifier::notify` 使用 `catch_unwind` 防止单个 handler panic 拖垮广播循环。

---

## 与其他 crate 的关系

### 上游依赖

| 依赖 | 用途 |
|------|------|
| `cmx-utils` | 配置管理框架（远程配置覆盖本地配置的宿主） |
| `nacos-sdk` 0.8 | Nacos 官方 SDK（NamingService / ConfigService / ClientProps） |
| `config`（第三方） | `Source` trait 实现（`NacosConfigSource` 注入 ConfigBuilder） |
| `tokio` / `serde` / `serde_json` / `toml` / `thiserror` / `tracing` | 运行时 / 序列化 / 错误 / 日志 |

### 下游使用者

**无活跃依赖方**（全仓库 Cargo.toml 反查为空）。历史调用入口为 `cmx-platform-app/src/config/nacos.rs`（现为未挂载的孤儿文件）。抽象层替代者：`cmx-registry-config`。

---

## 核心功能与特性

| 功能 | 说明 | 关键入口 |
|------|------|----------|
| 统一客户端 | 按 `enabled`/`naming.enabled`/`config.enabled` 惰性初始化两大子服务 | `NacosClient::new` |
| 服务注册 | 注册/注销实例（service_name / group / cluster / weight / metadata） | `NacosClient::register_service` |
| 服务发现 | 查询健康实例列表 | `NacosClient::query_instances` |
| 配置拉取 | 获取远程配置文本，或解析为 `NacosConfigSource` | `NacosClient::get_config(_source)` |
| 配置注入 | `NacosConfigSource` 实现 `config::Source`，`add_source()` 后覆盖本地同名项 | `NacosConfigSource::from_toml_str` |
| 变更监听 | nacos SDK 监听 → `RemoteConfigChangeListener` → 全局通知器 | `NacosClient::listen_config` |
| 全局通知 | 静态注册表 + `catch_unwind` 广播（`Arc<dyn Fn(&str)>` 回调） | `GlobalConfigChangeNotifier::register/notify` |
| 环境变量 | `NACOS_*` 前缀 13 个变量加载配置 | `NacosConfig::from_env` |

---

## 模块结构

```text
src/
├── lib.rs            # 导出面 + 配置优先级文档（环境变量 > Nacos > 本地 TOML > 代码默认值）
├── client.rs         # NacosClient：统一入口（new / register_service / deregister_service /
│                     #   get_config / get_config_source / listen_config / query_instances）
├── config.rs         # NacosConfig / NamingConfig / ConfigCenterConfig / ConfigListener + from_env
├── config_service.rs # ConfigClient：配置中心薄封装（get_config / get_config_source / add_listener）
├── config_source.rs  # NacosConfigSource：TOML → config::Value 树，impl config::Source
├── naming.rs         # NamingClient：命名服务薄封装（register_instance / deregister_instance /
│                     #   select_instances）
├── listener.rs       # RemoteConfigChangeListener：nacos SDK 监听 → 全局通知器
├── notifier.rs       # ConfigChangeCallback / ConfigChangeNotifier / GlobalConfigChangeNotifier
└── error.rs          # NacosError（9 变体）/ NacosResult
```

---

## 关键类型 / API

```rust
// —— 客户端（src/client.rs）——
pub struct NacosClient { /* naming: Option<_>, config: Option<_>, nacos_config */ }
impl NacosClient {
    pub async fn new(nacos_config: NacosConfig) -> Result<Self, NacosError>;  // enabled=false → 空客户端
    pub async fn register_service(&self, ip: &str, port: u16) -> Result<(), NacosError>;
    pub async fn deregister_service(&self, ip: &str, port: u16) -> Result<(), NacosError>;
    pub async fn query_instances(&self, service_name: &str, group_name: Option<&str>,
        clusters: Vec<String>) -> Result<Vec<nacos_sdk::api::naming::ServiceInstance>, NacosError>;
    pub async fn get_config(&self, data_id: &str, group: &str) -> Result<String, NacosError>;
    pub async fn get_config_source(&self, data_id: &str, group: &str)
        -> Result<NacosConfigSource, NacosError>;
    pub async fn listen_config(&self, data_id: &str, group: &str,
        listener: Arc<dyn nacos_sdk::api::config::ConfigChangeListener>) -> Result<(), NacosError>;
    pub fn nacos_config(&self) -> &NacosConfig;
    pub fn is_naming_enabled(&self) -> bool;
    pub fn is_config_enabled(&self) -> bool;
}

// —— 配置模型（src/config.rs，serde 默认值 + 环境变量加载）——
pub struct NacosConfig {          // server_addr 默认 "127.0.0.1:8848"、app_name 默认 "cmx-container"、
    pub server_addr: String,      // enabled 默认 false（整体开关）
    pub namespace: String,        // 默认 ""（public 命名空间）
    pub app_name: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub enabled: bool,
    pub naming: NamingConfig,
    pub config: ConfigCenterConfig,
}
impl NacosConfig {
    pub fn from_env() -> Self;    // NACOS_ENABLED/SERVER_ADDR/NAMESPACE/APP_NAME/USERNAME/PASSWORD/
}                                 //   NAMING_ENABLED/NAMING_SERVICE_NAME/NAMING_GROUP_NAME/
                                  //   CONFIG_ENABLED/CONFIG_DATA_ID/CONFIG_GROUP
pub struct NamingConfig { pub service_name: String /* 默认 "cmx-server" */,
    pub group_name: String /* "DEFAULT_GROUP" */, pub cluster_name: String /* "DEFAULT" */,
    pub weight: f64 /* 1.0 */, pub enabled: bool /* true */, pub metadata: HashMap<String, String> }
pub struct ConfigCenterConfig { pub enabled: bool /* false */, pub listeners: Vec<ConfigListener> }
pub struct ConfigListener { pub data_id: String, pub group: String }

// —— 配置注入（src/config_source.rs）——
pub struct NacosConfigSource { /* HashMap<String, config::Value> */ }
impl NacosConfigSource {
    pub fn from_toml_str(content: &str) -> Result<Self, NacosError>;
}
// impl config::Source for NacosConfigSource —— 经 ConfigBuilder::add_source() 注入

// —— 变更通知（src/notifier.rs）——
pub type ConfigChangeCallback = Arc<dyn Fn(&str) + Send + Sync>;
pub struct ConfigChangeNotifier;      // 实例级：register(key, cb) / unregister(key) / notify(content)
pub struct GlobalConfigChangeNotifier; // 静态：initialize() / register(key, cb) / notify(content)

// —— 错误（src/error.rs）——
pub enum NacosError { InitFailed, NamingDisabled, ConfigDisabled, RegisterFailed,
                      DeregisterFailed, ConfigGetFailed, ConfigParseFailed,
                      ConfigListenFailed, QueryFailed }
```

---

## 使用示例

> 前提：需先在 workspace 根 `Cargo.toml` 中恢复 `members` 与 `workspace.dependencies` 的对应条目（当前均被注释）。新代码建议优先评估 `cmx-registry-config` 抽象层。

### 场景 1：启动客户端并注册服务实例

```rust
use cmx_nacos::{NacosClient, NacosConfig};

async fn bootstrap() -> Result<(), cmx_nacos::NacosError> {
    // 连接配置（enabled=false 时 new 返回空客户端，后续调用报 NamingDisabled/ConfigDisabled）
    let config = NacosConfig {
        enabled: true,
        server_addr: "127.0.0.1:8848".into(),
        namespace: String::new(),
        app_name: "cmx-container".into(),
        username: None,
        password: None,
        naming: Default::default(),   // service_name="cmx-server"、group="DEFAULT_GROUP"、enabled=true
        config: Default::default(),   // 配置中心默认关闭
    };
    let client = NacosClient::new(config).await?;

    // 把本进程注册为服务实例（临时实例，SDK 自动心跳保活）
    client.register_service("10.0.0.5", 8080).await?;
    Ok(())
}
```

### 场景 2：拉取远程配置并覆盖本地 TOML（config::Source 注入）

```rust
use cmx_nacos::{NacosClient, NacosConfig};
use config::ConfigBuilder;

async fn load_config() -> anyhow::Result<config::Config> {
    let client = NacosClient::new(NacosConfig::from_env()).await?;  // 或显式构造
    // 远程配置内容要求 TOML 格式，解析为 config::Value 树
    let source = client.get_config_source("cmx-app.toml", "DEFAULT_GROUP").await?;

    // 注入顺序决定优先级：远程 source 在本地 FileSource 之后 → 覆盖同名配置项
    let settings = ConfigBuilder::new()
        .add_source(config::File::with_name("config/app").required(false))
        .add_source(source)              // Nacos 远程覆盖本地
        .build()?;
    Ok(settings)
}
```

### 场景 3：监听配置变更并注册全局回调

```rust
use cmx_nacos::{GlobalConfigChangeNotifier, NacosClient, NacosConfig, RemoteConfigChangeListener};
use std::sync::Arc;

async fn watch_config() -> Result<(), cmx_nacos::NacosError> {
    let client = NacosClient::new(NacosConfig::from_env()).await?;

    // 注册本模块的变更回调（key 用于注销；notify 用 catch_unwind 隔离 panic）
    GlobalConfigChangeNotifier::register("my-module", Arc::new(|content: &str| {
        tracing::info!("Nacos 配置已变更，新内容长度: {}", content.len());
        // 此处可触发配置热更新
    }));

    // 挂载 SDK 监听器：变更 → RemoteConfigChangeListener::notify → 全局通知器广播
    client.listen_config(
        "cmx-app.toml",
        "DEFAULT_GROUP",
        Arc::new(RemoteConfigChangeListener),
    ).await
}
```

### 场景 4：服务发现（查询健康实例）

```rust
use cmx_nacos::{NacosClient, NacosConfig};

async fn discover(client: &NacosClient) -> Result<(), cmx_nacos::NacosError> {
    // 只返回 healthy=true 的实例；clusters 传空 Vec 表示不按集群过滤
    let instances = client
        .query_instances("cmx-server", Some("DEFAULT_GROUP"), vec![])
        .await?;
    for inst in instances {
        println!("可用实例: {}:{} (weight={})", inst.ip, inst.port, inst.weight);
    }
    Ok(())
}
```

---

## Features 说明

本 crate 的 `Cargo.toml` 未定义 `[features]` 段，无可选特性。功能开关在配置层完成：`NacosConfig.enabled`（总开关）、`naming.enabled`（注册）、`config.enabled`（配置中心）。
