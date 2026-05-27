# 插件生命周期 — 基础服务中心数据分发方案

## 一、需求概述

在插件安装、升级、降级时，将插件安装目录下的业务数据子目录（`menudata/`、`permdata/`、`formdata/`、`flowdata/`）中的数据分别打包为 ZIP 并以 form-data 方式发送到对应的外部基础服务中心完成初始化；在卸载时通知各中心清理数据。

| 数据目录 | 目标服务中心 | 操作 |
|----------|-------------|------|
| `menudata/` | 门户中心 | 安装/升级/降级时推送 ZIP；卸载时清理 |
| `permdata/` | 权限中心 | 安装/升级/降级时推送 ZIP；卸载时清理 |
| `formdata/` | 表单中心 | 安装/升级/降级时推送 ZIP；卸载时清理 |
| `flowdata/` | 流程中心 | 安装/升级/降级时推送 ZIP；卸载时清理 |

### 核心约束

- 每个数据目录下的所有文件打包成一个 ZIP，以 **form-data（multipart）** 方式发送
- **各中心调用并行执行**，最终汇总结果（哪些成功、哪些失败）
- 调用外部接口必须获取明确结果
- 任一外部接口调用失败 → 整个生命周期操作失败，并回滚或清晰提示
- 当前阶段使用 Mock 实现，后续只需替换实现类即可对接真实服务
- 必须与 `persistence.rs` 解耦
- **所有 center_client 相关代码放在 cmx-plugin 内部的独立顶层模块 `center_client` 下**，不放在 cmx-traits
- **URL 直连模式的配置支持从 `dev.toml` 或环境变量读取**，有初始化入口

---

## 二、架构设计

### 2.1 整体分层

```
┌─────────────────────────────────────────────────────┐
│              PluginOperationExecutor                 │  ← 编排层（已有）
│  persistence → center_dispatch → runtime → audit    │  ← 新增 center_dispatch 步骤
└─────────────┬───────────────────────────────────────┘
              │
              ▼
┌─────────────────────────────────────────────────────┐
│           CenterDataDispatcher                       │  ← 新增：调度器
│  读取目录 → ZIP 打包 → 并行分发 → 汇总结果           │
│  (使用 futures::join_all 并行调用所有中心)            │
└─────────────┬───────────────────────────────────────┘
              │
              ▼
┌─────────────────────────────────────────────────────┐
│       ServiceCenterSender (trait)                    │  ← 新增：cmx-plugin/center_client 中定义
│  send_data(form-data: ZIP) / cleanup_data()          │
└─────────────┬───────────────────────────────────────┘
              │
       ┌──────┴──────┐
       ▼             ▼
  MockSender    HttpSender     ← 当前 Mock，后续替换为 Http
  (当前阶段)    (后续实现)
```

### 2.2 模块位置

```
cmx-plugin/src/
├── center_client/                  ← 新增独立顶层模块
│   ├── mod.rs                      模块导出
│   ├── types.rs                    DataCategory、DispatchContext、DispatchResult、请求/响应类型
│   ├── sender.rs                   ServiceCenterSender trait + CenterError 定义
│   ├── dispatcher.rs               CenterDataDispatcher 调度器（并行分发）
│   ├── packer.rs                   目录 → ZIP 打包工具
│   ├── mock_sender.rs              Mock 实现
│   └── config.rs                   CenterConfig + dev.toml / 环境变量加载
├── service/
│   ├── executor.rs                 ← 修改：集成 center_dispatch
│   └── ...
└── lib.rs                          ← 修改：新增 pub mod center_client
```

### 2.3 设计决策

| 决策点 | 选择 | 理由 |
|--------|------|------|
| Trait 定义位置 | `cmx-plugin/src/center_client/` | 仅 cmx-plugin 内部使用，无需跨 crate 共享 |
| 模块位置 | 独立顶层模块 `center_client` | 与 service、domain 等平级，职责清晰 |
| 集成位置 | `PluginOperationExecutor`（编排层） | 不修改 persistence.rs，保持其只做 DB+文件职责 |
| 传输方式 | 每个数据目录打包为 ZIP → form-data 上传 | 企业级接口标准，支持批量数据传输 |
| **并行策略** | **futures::join_all 并行调用** | 四个中心互不依赖，并行可大幅减少总耗时 |
| 参数传递 | **结构体 DispatchContext** | 方便后续扩展参数，避免方法签名膨胀 |
| 配置来源 | **dev.toml `[center_client]` 节 + 环境变量** | 遵循项目现有 ConfigManager 配置模式 |
| 调用时机 | 持久化之后、运行时注册之前 | DB 已提交，若中心调用失败需补偿回滚 |
| 失败策略 | 返回错误 + 补偿卸载 | 清晰提示优于自动回滚（自动回滚本身也可能失败） |

### 2.4 生命周期交互流程

#### 安装流程（install）

```
Executor.execute_install()
  │
  ├─ 1. persistence.install_persist()      → PersistResult (DB 事务已提交)
  │
  ├─ 2. center_dispatcher.dispatch_install(ctx)               ← NEW
  │     │
  │     ├─ 构造 DispatchContext（从 PersistResult 映射）
  │     │
  │     ├─ 并行打包 ZIP：
  │     │   task1: menudata/  → packer.zip()
  │     │   task2: permdata/  → packer.zip()
  │     │   task3: formdata/  → packer.zip()
  │     │   task4: flowdata/  → packer.zip()
  │     │
  │     ├─ 并行发送到中心（futures::join_all）：
  │     │   task1: sender.send_data(Menu, zip_bytes)
  │     │   task2: sender.send_data(Perm, zip_bytes)
  │     │   task3: sender.send_data(Form, zip_bytes)
  │     │   task4: sender.send_data(Flow, zip_bytes)
  │     │
  │     └─ 汇总 DispatchResult：
  │         { Menu: Ok, Perm: Ok, Form: Err("权限中心不可用"), Flow: Ok }
  │         → 任一失败 → 返回错误 → 补偿卸载
  │
  ├─ 3. runtime.register_plugin()
  ├─ 4. audit_logger.log()
  └─ 5. event_publisher.publish_installed()
```

#### 卸载流程（uninstall）

```
Executor.execute_uninstall()
  │
  ├─ 1. persistence.uninstall_persist()    → PersistResult (DB 事务已提交)
  │
  ├─ 2. center_dispatcher.dispatch_cleanup(ctx)               ← NEW
  │     ├─ 并行清理（futures::join_all）：
  │     │   task1: sender.cleanup_data(Menu, ...)
  │     │   task2: sender.cleanup_data(Perm, ...)
  │     │   task3: sender.cleanup_data(Form, ...)
  │     │   task4: sender.cleanup_data(Flow, ...)
  │     └─ 汇总 DispatchResult
  │         → 任一失败 → 返回错误（DB 已清理但中心数据残留）
  │
  ├─ 3. runtime.unregister_plugin()
  ├─ 4. audit_logger.log()
  └─ 5. event_publisher.publish_uninstalled()
```

---

## 三、核心代码结构

### 3.1 center_client/types.rs — 数据类型定义

```rust
use std::collections::HashMap;
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DataCategory {
    Menu,
    Perm,
    Form,
    Flow,
}

impl DataCategory {
    pub fn dir_name(&self) -> &str {
        match self {
            Self::Menu => "menudata",
            Self::Perm => "permdata",
            Self::Form => "formdata",
            Self::Flow => "flowdata",
        }
    }

    pub fn center_name(&self) -> &str {
        match self {
            Self::Menu => "门户中心",
            Self::Perm => "权限中心",
            Self::Form => "表单中心",
            Self::Flow => "流程中心",
        }
    }

    pub fn all() -> &'static [DataCategory] {
        &[Self::Menu, Self::Perm, Self::Form, Self::Flow]
    }
}

/// dispatch_install / dispatch_cleanup 的统一入参结构体
///
/// 从 PersistResult 映射而来，方便后续扩展字段
#[derive(Debug, Clone)]
pub struct DispatchContext {
    pub install_path: PathBuf,
    pub plugin_id: String,
    pub app_id: String,
    pub version: String,
    pub domain_code: String,
    pub application_code: String,
    pub module_code: String,
}

/// 单个中心的分发结果
#[derive(Debug)]
pub struct CategoryResult {
    pub category: DataCategory,
    pub result: Result<CenterResponse, CenterError>,
}

/// 整体分发结果汇总
#[derive(Debug)]
pub struct DispatchResult {
    pub results: Vec<CategoryResult>,
}

impl DispatchResult {
    pub fn is_all_success(&self) -> bool {
        self.results.iter().all(|r| r.result.is_ok() && r.result.as_ref().unwrap().success)
    }

    pub fn failed_categories(&self) -> Vec<&CategoryResult> {
        self.results.iter().filter(|r| r.result.is_err() || !r.result.as_ref().map(|r| r.success).unwrap_or(false)).collect()
    }

    pub fn success_categories(&self) -> Vec<&CategoryResult> {
        self.results.iter().filter(|r| r.result.is_ok() && r.result.as_ref().unwrap().success).collect()
    }
}

/// 发送数据到服务中心的请求
pub struct CenterSendRequest {
    pub plugin_id: String,
    pub app_id: String,
    pub version: String,
    pub category: DataCategory,
    pub zip_data: Vec<u8>,
    pub zip_file_name: String,
    pub domain_code: String,
    pub application_code: String,
    pub module_code: String,
}

/// 清理服务中心数据的请求
pub struct CenterCleanupRequest {
    pub plugin_id: String,
    pub app_id: String,
    pub version: Option<String>,
    pub category: DataCategory,
}

/// 服务中心响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CenterResponse {
    pub success: bool,
    pub message: String,
    pub center_id: Option<String>,
}
```

### 3.2 center_client/sender.rs — Trait 定义

```rust
use async_trait::async_trait;
use super::types::{CenterSendRequest, CenterCleanupRequest, CenterResponse};

#[derive(Debug, thiserror::Error)]
pub enum CenterError {
    #[error("{center}调用失败: {message}")]
    CallFailed { center: String, message: String },
    #[error("{center}不可用: {url}")]
    Unavailable { center: String, url: String },
    #[error("数据打包失败: {0}")]
    PackError(String),
    #[error("配置错误: {0}")]
    Config(String),
    #[error("网络错误: {0}")]
    Network(String),
    #[error("超时: {center} 响应超时 ({timeout_ms}ms)")]
    Timeout { center: String, timeout_ms: u64 },
}

/// 服务中心数据发送器 trait
#[async_trait]
pub trait ServiceCenterSender: Send + Sync {
    /// 发送数据到服务中心（form-data 方式，ZIP 包含整个数据目录）
    async fn send_data(&self, request: CenterSendRequest) -> Result<CenterResponse, CenterError>;

    /// 清理服务中心中与指定插件相关的数据
    async fn cleanup_data(&self, request: CenterCleanupRequest) -> Result<CenterResponse, CenterError>;
}
```

### 3.3 center_client/dispatcher.rs — 调度器（并行分发）

```rust
use std::sync::Arc;
use futures::future::join_all;
use super::sender::{ServiceCenterSender, CenterError};
use super::types::*;
use super::packer::pack_directory_to_zip;
use crate::error::{PluginError, PluginResult};

pub struct CenterDataDispatcher {
    sender: Arc<dyn ServiceCenterSender>,
}

impl CenterDataDispatcher {
    pub fn new(sender: Arc<dyn ServiceCenterSender>) -> Self {
        Self { sender }
    }

    /// 安装/升级/降级：并行读取 → 打包 → 推送到各中心 → 汇总结果
    pub async fn dispatch_install(&self, ctx: &DispatchContext) -> PluginResult<DispatchResult> {
        let mut futures = Vec::new();

        for category in DataCategory::all() {
            let dir = ctx.install_path.join(category.dir_name());
            if !dir.exists() {
                tracing::info!("插件 {} 无 {} 数据目录，跳过", ctx.plugin_id, category.dir_name());
                continue;
            }

            // 打包为 ZIP（同步 IO，在 spawn 之前完成）
            let zip_data = pack_directory_to_zip(&dir).map_err(|e| {
                PluginError::CenterData(format!(
                    "{}数据目录打包失败: {}", category.center_name(), e
                ))
            })?;

            let request = CenterSendRequest {
                plugin_id: ctx.plugin_id.clone(),
                app_id: ctx.app_id.clone(),
                version: ctx.version.clone(),
                category: *category,
                zip_data,
                zip_file_name: format!("{}.zip", category.dir_name()),
                domain_code: ctx.domain_code.clone(),
                application_code: ctx.application_code.clone(),
                module_code: ctx.module_code.clone(),
            };

            let sender = self.sender.clone();
            futures.push(async move {
                let result = sender.send_data(request).await;
                CategoryResult {
                    category: *category,
                    result,
                }
            });
        }

        // 并行执行所有发送任务
        let results = join_all(futures).await;
        let dispatch_result = DispatchResult { results };

        // 日志汇总
        for r in &dispatch_result.results {
            match &r.result {
                Ok(resp) if resp.success => tracing::info!(
                    "插件 {} {} 数据推送成功: {}",
                    ctx.plugin_id, r.category.center_name(), resp.message
                ),
                Ok(resp) => tracing::error!(
                    "插件 {} {} 数据推送被拒绝: {}",
                    ctx.plugin_id, r.category.center_name(), resp.message
                ),
                Err(e) => tracing::error!(
                    "插件 {} {} 数据推送失败: {}",
                    ctx.plugin_id, r.category.center_name(), e
                ),
            }
        }

        Ok(dispatch_result)
    }

    /// 卸载：并行通知各中心清理数据 → 汇总结果
    pub async fn dispatch_cleanup(&self, ctx: &DispatchContext) -> PluginResult<DispatchResult> {
        let mut futures = Vec::new();

        for category in DataCategory::all() {
            let request = CenterCleanupRequest {
                plugin_id: ctx.plugin_id.clone(),
                app_id: ctx.app_id.clone(),
                version: Some(ctx.version.clone()),
                category: *category,
            };

            let sender = self.sender.clone();
            futures.push(async move {
                let result = sender.cleanup_data(request).await;
                CategoryResult {
                    category: *category,
                    result,
                }
            });
        }

        let results = join_all(futures).await;
        let dispatch_result = DispatchResult { results };

        for r in &dispatch_result.results {
            match &r.result {
                Ok(resp) if resp.success => tracing::info!(
                    "插件 {} {} 数据清理成功", ctx.plugin_id, r.category.center_name()
                ),
                Ok(resp) => tracing::error!(
                    "插件 {} {} 数据清理被拒绝: {}",
                    ctx.plugin_id, r.category.center_name(), resp.message
                ),
                Err(e) => tracing::error!(
                    "插件 {} {} 数据清理失败: {}",
                    ctx.plugin_id, r.category.center_name(), e
                ),
            }
        }

        Ok(dispatch_result)
    }
}
```

### 3.4 center_client/packer.rs — ZIP 打包工具

```rust
use std::path::Path;
use std::io::{Write, Cursor};
use zip::ZipWriter;
use zip::write::SimpleFileOptions;
use crate::error::{PluginError, PluginResult};

/// 将指定目录下的所有文件打包为 ZIP 字节
///
/// ZIP 内保持相对路径结构：
/// menudata/menu1.json, menudata/menu2.json
pub fn pack_directory_to_zip(dir: &Path) -> PluginResult<Vec<u8>> {
    let buf = Cursor::new(Vec::new());
    let mut writer = ZipWriter::new(buf);
    let options = SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);

    let dir_name = dir.file_name()
        .ok_or_else(|| PluginError::CenterData("目录名无效".to_string()))?
        .to_string_lossy();

    pack_dir_recursive(&mut writer, dir, &dir_name, &options)?;

    let buf = writer.finish()
        .map_err(|e| PluginError::CenterData(format!("ZIP 写入失败: {}", e)))?;
    Ok(buf.into_inner())
}

fn pack_dir_recursive<W: Write + std::io::Seek>(
    writer: &mut ZipWriter<W>,
    base_dir: &Path,
    prefix: &str,
    options: &SimpleFileOptions,
) -> PluginResult<()> {
    for entry in std::fs::read_dir(base_dir)
        .map_err(|e| PluginError::CenterData(format!("读取目录失败: {}", e)))?
    {
        let entry = entry.map_err(|e| PluginError::CenterData(format!("读取目录条目失败: {}", e)))?;
        let path = entry.path();
        let name = format!("{}/{}", prefix, entry.file_name().to_string_lossy());

        if path.is_dir() {
            pack_dir_recursive(writer, &path, &name, options)?;
        } else {
            writer.start_file(&name, *options)
                .map_err(|e| PluginError::CenterData(format!("ZIP 添加文件失败: {}", e)))?;
            let data = std::fs::read(&path)
                .map_err(|e| PluginError::CenterData(format!("读取文件失败 {}: {}", path.display(), e)))?;
            writer.write_all(&data)
                .map_err(|e| PluginError::CenterData(format!("ZIP 写入失败: {}", e)))?;
        }
    }
    Ok(())
}
```

### 3.5 center_client/mock_sender.rs — Mock 实现

```rust
use async_trait::async_trait;
use super::sender::{ServiceCenterSender, CenterError};
use super::types::*;

pub struct MockServiceCenterSender;

#[async_trait]
impl ServiceCenterSender for MockServiceCenterSender {
    async fn send_data(&self, request: CenterSendRequest) -> Result<CenterResponse, CenterError> {
        tracing::info!(
            "[Mock] 向{}推送数据: plugin={}, zip={}, size={}bytes",
            request.category.center_name(),
            request.plugin_id,
            request.zip_file_name,
            request.zip_data.len(),
        );
        Ok(CenterResponse {
            success: true,
            message: format!("Mock: {}数据接收成功", request.category.center_name()),
            center_id: Some(format!("mock-{}", request.category.dir_name())),
        })
    }

    async fn cleanup_data(&self, request: CenterCleanupRequest) -> Result<CenterResponse, CenterError> {
        tracing::info!(
            "[Mock] 通知{}清理数据: plugin={}, app={}",
            request.category.center_name(),
            request.plugin_id,
            request.app_id,
        );
        Ok(CenterResponse {
            success: true,
            message: format!("Mock: {}数据清理成功", request.category.center_name()),
            center_id: None,
        })
    }
}
```

### 3.6 center_client/config.rs — 配置 + 初始化

```rust
use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use super::types::DataCategory;

/// 服务中心配置（从 dev.toml [center_client] 节反序列化）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CenterClientConfig {
    /// 访问模式："url" | "discovery" | "mock"
    #[serde(default = "default_mode")]
    pub mode: String,

    /// URL 直连模式：各中心的服务地址
    #[serde(default)]
    pub urls: CenterUrlsConfig,

    /// 服务发现模式配置
    #[serde(default)]
    pub discovery: CenterDiscoveryConfig,

    /// 请求超时（毫秒）
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CenterUrlsConfig {
    #[serde(default)]
    pub menu: Option<String>,
    #[serde(default)]
    pub perm: Option<String>,
    #[serde(default)]
    pub form: Option<String>,
    #[serde(default)]
    pub flow: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CenterDiscoveryConfig {
    #[serde(default)]
    pub nacos_group: Option<String>,
    #[serde(default)]
    pub menu_service: Option<String>,
    #[serde(default)]
    pub perm_service: Option<String>,
    #[serde(default)]
    pub form_service: Option<String>,
    #[serde(default)]
    pub flow_service: Option<String>,
}

fn default_mode() -> String { "mock".to_string() }
fn default_timeout() -> u64 { 30000 }

impl CenterClientConfig {
    /// 从全局 ConfigManager 加载配置
    ///
    /// 配置优先级（从低到高）：
    /// 1. dev.toml [center_client] 节
    /// 2. 环境变量 CENTER_CLIENT__MODE, CENTER_CLIENT__URLS__MENU 等
    pub fn load() -> Self {
        let config_manager = match cmx_utils::config::ConfigManager::try_global() {
            Some(cm) => cm,
            None => {
                tracing::warn!("ConfigManager 未初始化，使用默认 center_client 配置 (mock)");
                return Self::default();
            }
        };

        match config_manager.deserialize::<Self>("center_client") {
            Ok(config) => {
                tracing::info!(
                    "center_client 配置加载成功: mode={}",
                    config.mode
                );
                config
            }
            Err(e) => {
                tracing::warn!("加载 center_client 配置失败: {}，使用默认 mock 模式", e);
                Self::default()
            }
        }
    }

    /// 解析 URL 配置为 HashMap
    pub fn resolve_urls(&self) -> HashMap<DataCategory, String> {
        let mut urls = HashMap::new();
        if let Some(ref url) = self.urls.menu { urls.insert(DataCategory::Menu, url.clone()); }
        if let Some(ref url) = self.urls.perm { urls.insert(DataCategory::Perm, url.clone()); }
        if let Some(ref url) = self.urls.form { urls.insert(DataCategory::Form, url.clone()); }
        if let Some(ref url) = self.urls.flow { urls.insert(DataCategory::Flow, url.clone()); }
        urls
    }
}

impl Default for CenterClientConfig {
    fn default() -> Self {
        Self {
            mode: default_mode(),
            urls: CenterUrlsConfig::default(),
            discovery: CenterDiscoveryConfig::default(),
            timeout_ms: default_timeout(),
        }
    }
}
```

#### dev.toml 配置示例

在 `dev.toml` 末尾新增：

```toml
# ============================================
# 基础服务中心配置
# ============================================
[center_client]
# 模式：mock（默认，当前阶段）| url（直连）| discovery（服务发现）
mode = "mock"
# 请求超时（毫秒）
timeout_ms = 30000

# URL 直连模式（mode = "url" 时生效）
# 环境变量覆盖：CENTER_CLIENT__URLS__MENU, CENTER_CLIENT__URLS__PERM 等
[center_client.urls]
# menu = "http://portal-center:8080/api/plugin/menu/import"
# perm = "http://perm-center:8080/api/plugin/perm/import"
# form = "http://form-center:8080/api/plugin/form/import"
# flow = "http://flow-center:8080/api/plugin/flow/import"

# 服务发现模式（mode = "discovery" 时生效）
# [center_client.discovery]
# nacos_group = "DEFAULT_GROUP"
# menu_service = "cmx-portal-center"
# perm_service = "cmx-perm-center"
# form_service = "cmx-form-center"
# flow_service = "cmx-flow-center"
```

环境变量覆盖（优先级高于 dev.toml）：

```bash
CENTER_CLIENT__MODE=url
CENTER_CLIENT__URLS__MENU=http://portal-center:8080/api/plugin/menu/import
CENTER_CLIENT__TIMEOUT_MS=60000
```

### 3.7 executor.rs 集成变更（示意）

```rust
use crate::center_client::dispatcher::CenterDataDispatcher;
use crate::center_client::types::DispatchContext;

pub struct PluginOperationExecutor {
    persistence: PluginPersistence,
    runtime: Arc<RuntimeOps>,
    event_publisher: EventPublisher,
    audit_logger: Arc<AuditLogger>,
    center_dispatcher: Arc<CenterDataDispatcher>,  // ← 新增
}

impl PluginOperationExecutor {
    pub fn new(
        persistence: PluginPersistence,
        runtime: Arc<RuntimeOps>,
        event_publisher: EventPublisher,
        audit_logger: Arc<AuditLogger>,
        center_dispatcher: Arc<CenterDataDispatcher>,  // ← 新增参数
    ) -> Self {
        Self { persistence, runtime, event_publisher, audit_logger, center_dispatcher }
    }

    pub async fn execute_install(&self, request: InstallRequest) -> PluginResult<InstallResponse> {
        let start_time = std::time::Instant::now();

        // 1. 持久化
        let persist_result = self.persistence.install_persist(request).await?;

        // 1.5 中心数据分发（新增）
        let ctx = DispatchContext::from(&persist_result);
        let dispatch_result = self.center_dispatcher.dispatch_install(&ctx).await?;

        if !dispatch_result.is_all_success() {
            let failures: Vec<String> = dispatch_result.failed_categories().iter()
                .map(|r| format!("{}: {}", r.category.center_name(),
                    r.result.as_ref().map_err(|e| e.to_string()).unwrap_or_else(|e| e.clone())))
                .collect();
            let error_msg = format!("中心数据推送失败: {}", failures.join(", "));
            tracing::error!("{}", error_msg);

            // 补偿卸载
            tracing::error!("开始补偿卸载: {}", persist_result.plugin_id);
            let uninstall_req = UninstallRequest {
                plugin_id: persist_result.plugin_id.clone(),
                force: true,
                operator: "system-compensate".to_string(),
                app_id: Some(persist_result.app_id.clone()),
            };
            let _ = self.persistence.uninstall_persist(uninstall_req).await
                .map_err(|rollback_err| {
                    tracing::error!("补偿卸载也失败，需人工介入: {}", rollback_err);
                    rollback_err
                });
            return Err(PluginError::CenterData(error_msg));
        }

        // 2-5. 运行时注册、审计、事件发布（不变）
        self.runtime.register_plugin(&persist_result).await?;
        // ... audit + events ...
        Ok(InstallResponse { ... })
    }

    pub async fn execute_uninstall(&self, request: UninstallRequest) -> PluginResult<UninstallResponse> {
        // 1. 持久化
        let persist_result = self.persistence.uninstall_persist(request).await?;

        // 1.5 中心数据清理（新增）
        let ctx = DispatchContext::from(&persist_result);
        let dispatch_result = self.center_dispatcher.dispatch_cleanup(&ctx).await?;

        if !dispatch_result.is_all_success() {
            let failures: Vec<String> = dispatch_result.failed_categories().iter()
                .map(|r| format!("{}: {:?}", r.category.center_name(), r.result))
                .collect();
            return Err(PluginError::CenterData(format!(
                "中心数据清理失败（DB 已清理但中心数据残留）: {}", failures.join(", ")
            )));
        }

        // 2-5. 运行时注销、审计、事件发布（不变）
        ...
    }
}
```

---

## 四、新增/修改文件清单

### 4.1 新增文件

| 文件 | 用途 |
|------|------|
| `cmx-plugin/src/center_client/mod.rs` | 模块导出 |
| `cmx-plugin/src/center_client/types.rs` | DataCategory、DispatchContext、DispatchResult、请求/响应类型 |
| `cmx-plugin/src/center_client/sender.rs` | `ServiceCenterSender` trait + `CenterError` |
| `cmx-plugin/src/center_client/dispatcher.rs` | `CenterDataDispatcher`（并行分发） |
| `cmx-plugin/src/center_client/packer.rs` | 目录 → ZIP 打包工具 |
| `cmx-plugin/src/center_client/mock_sender.rs` | Mock 实现 |
| `cmx-plugin/src/center_client/config.rs` | 配置定义 + ConfigManager 加载 |
| `plugins/.../permdata/sample-perm.json` | 示例权限数据 |
| `plugins/.../flowdata/sample-flow.json` | 示例流程定义 |

### 4.2 修改文件

| 文件 | 修改内容 |
|------|----------|
| `cmx-plugin/src/lib.rs` | 新增 `pub mod center_client;` |
| `cmx-plugin/src/error.rs` | 新增 `CenterData(String)` 错误变体 + `error_code` |
| `cmx-plugin/src/service/executor.rs` | 注入 `CenterDataDispatcher`，各 execute 方法集成 |
| `cmx-plugin/src/service/install.rs` | `InstallServiceDeps` 新增 `center_sender` 字段 |
| `dev.toml` | 新增 `[center_client]` 配置节 |
| `web-server/src/main.rs` 或初始化链路 | 加载配置 + 创建 MockServiceCenterSender 并注入 |

**不修改的文件**：`persistence.rs`、`cmx-traits`（完全解耦）

---

## 五、实施步骤

### 步骤 1：创建 center_client 模块骨架
- 创建 `src/center_client/` 目录及 7 个文件
- `mod.rs` 导出子模块
- `lib.rs` 新增 `pub mod center_client;`

### 步骤 2：实现类型和 trait
- `types.rs`：DataCategory、DispatchContext、DispatchResult、CenterSendRequest 等
- `sender.rs`：CenterError、ServiceCenterSender trait

### 步骤 3：实现打包和调度器
- `packer.rs`：pack_directory_to_zip()
- `dispatcher.rs`：CenterDataDispatcher（并行 dispatch_install / dispatch_cleanup）
- `config.rs`：CenterClientConfig + ConfigManager 加载

### 步骤 4：实现 Mock
- `mock_sender.rs`：MockServiceCenterSender

### 步骤 5：集成到 Executor
- `error.rs` 新增 `CenterData(String)` 变体
- `executor.rs` 新增 `center_dispatcher` 字段，修改 5 个 execute 方法
- `install.rs` 的 `InstallServiceDeps` 新增 `center_sender` 字段
- 初始化链路：加载 CenterClientConfig → 创建 MockServiceCenterSender → 注入

### 步骤 6：配置文件
- `dev.toml` 新增 `[center_client]` 配置节

### 步骤 7：补充示例数据
- 创建 `permdata/sample-perm.json`
- 创建 `flowdata/sample-flow.json`

### 步骤 8：编译验证
- `rtk cargo check`
- `rtk cargo clippy`

---

## 六、后续对接真实服务

只需在 `center_client/` 下新增 `http_sender.rs`，并在 `config.rs` 中根据 `mode` 选择实现：

```rust
// 初始化时（web-server main.rs 或 PluginManagerBuilder）
let config = CenterClientConfig::load();
let sender: Arc<dyn ServiceCenterSender> = match config.mode.as_str() {
    "url" => {
        Arc::new(HttpServiceCenterSender::new(reqwest::Client::new(), config))
    }
    "discovery" => {
        Arc::new(HttpServiceCenterSender::with_discovery(reqwest::Client::new(), config, nacos_client))
    }
    _ => Arc::new(MockServiceCenterSender),
};
```

`HttpServiceCenterSender` 实现：

```rust
pub struct HttpServiceCenterSender {
    http_client: reqwest::Client,
    config: CenterClientConfig,
}

impl HttpServiceCenterSender {
    async fn resolve_url(&self, category: DataCategory) -> Result<String, CenterError> {
        match self.config.mode.as_str() {
            "url" => self.config.resolve_urls().get(&category).cloned()
                .ok_or(CenterError::Config(format!("{} URL 未配置", category.center_name()))),
            "discovery" => {
                // 通过 cmx-nacos 服务发现获取实例地址
                ...
            }
            _ => Err(CenterError::Config("未知模式".to_string())),
        }
    }
}

#[async_trait]
impl ServiceCenterSender for HttpServiceCenterSender {
    async fn send_data(&self, request: CenterSendRequest) -> Result<CenterResponse, CenterError> {
        let url = self.resolve_url(request.category).await?;
        let form = reqwest::multipart::Form::new()
            .text("plugin_id", request.plugin_id)
            .text("app_id", request.app_id)
            .text("version", request.version)
            .text("domain_code", request.domain_code)
            .text("application_code", request.application_code)
            .text("module_code", request.module_code)
            .part("file", reqwest::multipart::Part::bytes(request.zip_data)
                .file_name(request.zip_file_name)
                .mime_str("application/zip").unwrap());
        let resp = self.http_client.post(&url)
            .timeout(Duration::from_millis(self.config.timeout_ms))
            .multipart(form)
            .send().await
            .map_err(|e| CenterError::Network(e.to_string()))?;
        let body: CenterResponse = resp.json().await
            .map_err(|e| CenterError::CallFailed {
                center: request.category.center_name().to_string(),
                message: e.to_string(),
            })?;
        Ok(body)
    }

    async fn cleanup_data(&self, request: CenterCleanupRequest) -> Result<CenterResponse, CenterError> {
        let url = self.resolve_url(request.category).await?;
        let resp = self.http_client.delete(&url)
            .query(&[
                ("plugin_id", &request.plugin_id),
                ("app_id", &request.app_id),
            ])
            .timeout(Duration::from_millis(self.config.timeout_ms))
            .send().await
            .map_err(|e| CenterError::Network(e.to_string()))?;
        let body: CenterResponse = resp.json().await
            .map_err(|e| CenterError::CallFailed {
                center: request.category.center_name().to_string(),
                message: e.to_string(),
            })?;
        Ok(body)
    }
}
```

**替换方式**：修改 `dev.toml` 中 `center_client.mode = "url"`，无需修改任何 Rust 代码。
