# Extism 技术栈迁移方案

## 一、迁移背景与目标

### 1.1 迁移背景

当前系统采用 **wasmtime + rkyv + Arena** 技术栈实现 WASM 插件系统，虽然性能优异，但存在以下问题：

1. **开发复杂度高**：需要手动管理内存、序列化和宿主函数注册
2. **维护成本大**：Arena 内存管理、rkyv 版本兼容性等问题需要持续关注
3. **学习曲线陡峭**：新开发者需要理解零拷贝序列化、内存对齐等底层概念
4. **错误处理复杂**：P0 级别问题较多（内存安全、竞态条件等）

### 1.2 迁移目标

迁移至 **extism** 技术栈，实现：

1. **简化开发**：使用高级 API，减少底层细节处理
2. **提升可维护性**：由 extism 框架管理内存和序列化
3. **降低学习成本**：提供直观的 API 和完善的文档
4. **保持性能**：extism 基于 wasmtime 构建，性能有保障
5. **无需向后兼容**：可以大胆重构，不受历史包袱限制

### 1.3 技术栈对比

| 维度        | wasmtime + rkyv + Arena | extism                 |
| --------- | ----------------------- | ---------------------- |
| **底层运行时** | wasmtime                | wasmtime（extism 内置）    |
| **数据传递**  | rkyv 零拷贝序列化             | 字节流（JSON/MessagePack）  |
| **内存管理**  | 自定义 Arena               | extism 自动管理            |
| **宿主函数**  | 手动注册到 Linker            | `host_fn!` 宏简化定义       |
| **插件开发**  | 手动导出函数                  | `#[plugin_fn]` 宏自动导出   |
| **编译目标**  | wasm32-wasip1           | wasm32-unknown-unknown |
| **学习曲线**  | 陡峭（需理解底层概念）             | 平缓（高级抽象）               |
| **性能**    | 极高（零拷贝）                 | 高（基于 wasmtime）         |
| **开发效率**  | 低（需处理大量细节）              | 高（框架封装细节）              |

***

## 二、架构变更概览

### 2.1 模块架构对比

#### 现有架构（wasmtime + rkyv）

```
┌─────────────────────────────────────────────────────────────────┐
│                         web-server                               │
│  (应用入口，组装各组件)                                            │
└─────────────────────────────────────────────────────────────────┘
                                │
        ┌───────────────────────┼───────────────────────┐
        ▼                       ▼                       ▼
┌───────────────┐       ┌───────────────┐       ┌───────────────┐
│  cmx-service  │       │  cmx-plugin   │       │  cmx-runtime  │
│  (服务编排)    │◄──────│  (插件管理)    │       │  (WASM运行时) │
└───────────────┘       └───────────────┘       └───────────────┘
        │                       │                       │
        └───────────────────────┼───────────────────────┘
                                ▼
                        ┌───────────────┐
                        │  cmx-traits   │
                        │  (trait 抽象)  │
                        └───────────────┘
                                │
        ┌───────────────────────┼───────────────────────┐
        ▼                       ▼                       ▼
┌───────────────┐       ┌───────────────┐       ┌───────────────┐
│ cmx-database  │       │  cmx-buffer   │       │  cmx-utils    │
│ (数据库操作)   │       │  (缓存操作)    │       │  (日志等)      │
└───────────────┘       └───────────────┘       └───────────────┘
        │                       │                       │
        └───────────────────────┼───────────────────────┘
                                ▼
                        ┌───────────────┐
                        │   cmx-core    │
                        │ (基础数据类型) │
                        │ (rkyv 派生)   │  ← WASM 和 Host 共享
                        └───────────────┘
                                ▲
        ┌───────────────────────┴───────────────────────┐
        │                                               │
┌───────────────┐                               ┌───────────────┐
│ cmx-wasm-core │                               │ cmx-wasmdemo  │
│ (WASM端SDK)   │◄──────────────────────────────│ (WASM 模块)    │
│ - Arena       │                               │ target:       │
│ - 调用封装    │                               │ wasm32-wasip1 │
│ - 函数导出    │                               └───────────────┘
└───────────────┘
```

#### 新架构（extism）

```
┌─────────────────────────────────────────────────────────────────┐
│                         web-server                               │
│  (应用入口，组装各组件)                                            │
└─────────────────────────────────────────────────────────────────┘
                                │
        ┌───────────────────────┼───────────────────────┐
        ▼                       ▼                       ▼
┌───────────────┐       ┌───────────────┐       ┌───────────────┐
│  cmx-service  │       │  cmx-plugin   │       │ cmx-extism    │  ← 新模块
│  (服务编排)    │◄──────│  (插件管理)    │       │ (Extism运行时) │
└───────────────┘       └───────────────┘       └───────────────┘
        │                       │                       │
        └───────────────────────┼───────────────────────┘
                                ▼
                        ┌───────────────┐
                        │  cmx-traits   │
                        │  (trait 抽象)  │
                        └───────────────┘
                                │
        ┌───────────────────────┼───────────────────────┐
        ▼                       ▼                       ▼
┌───────────────┐       ┌───────────────┐       ┌───────────────┐
│ cmx-database  │       │  cmx-buffer   │       │  cmx-utils    │
│ (数据库操作)   │       │  (缓存操作)    │       │  (日志等)      │
└───────────────┘       └───────────────┘       └───────────────┘
        │                       │                       │
        └───────────────────────┼───────────────────────┘
                                ▼
                        ┌───────────────┐
                        │   cmx-core    │
                        │ (基础数据类型) │
                        │ (serde 派生)  │  ← WASM 和 Host 共享
                        └───────────────┘
                                ▲
        ┌───────────────────────┴───────────────────────┐
        │                                               │
┌───────────────┐                               ┌───────────────┐
│ cmx-plugin-sdk│  ← 新模块                      │ cmx-wasmdemo  │
│ (Extism PDK)  │◄──────────────────────────────│ (WASM 模块)    │
│ - 宿主函数封装│                               │ target:       │
│ - 类型转换    │                               │ wasm32-       │
│ - 工具函数    │                               │ unknown-      │
└───────────────┘                               │ unknown       │
                                                └───────────────┘
```

### 2.2 核心变更点

| 变更项          | 现有实现                         | 新实现                         | 影响范围                               |
| ------------ | ---------------------------- | --------------------------- | ---------------------------------- |
| **运行时模块**    | cmx-runtime (wasmtime)       | cmx-extism (extism)         | cmx-runtime 删除，新建 cmx-extism       |
| **WASM SDK** | cmx-wasm-core (Arena + rkyv) | cmx-plugin-sdk (extism-pdk) | cmx-wasm-core 删除，新建 cmx-plugin-sdk |
| **数据类型**     | rkyv 派生                      | serde 派生                    | cmx-core 修改                        |
| **编译目标**     | wasm32-wasip1                | wasm32-unknown-unknown      | 所有 WASM 模块                         |
| **宿主函数**     | 手动注册到 Linker                 | `host_fn!` 宏                | 所有宿主函数模块                           |
| **插件函数**     | 手动导出                         | `#[plugin_fn]` 宏            | 所有插件模块                             |
| **数据传递**     | rkyv 零拷贝                     | JSON/MessagePack            | 所有接口                               |

***

## 三、详细迁移方案

### 3.1 cmx-core：数据类型模块

#### 3.1.1 变更内容

**删除内容：**

* 所有 rkyv 相关的 `Archive`, `Serialize`, `Deserialize` 派生

* `wasm_types.rs` 中的 rkyv 相关代码

**保留内容：**

* 所有 serde 相关的 `Serialize`, `Deserialize` 派生

* 基础数据类型定义

* 枚举定义

* 常量定义

**新增内容：**

* Extism 兼容的请求/响应类型

* 使用 `extism-convert` 特性标记

#### 3.1.2 Cargo.toml 修改

```toml
[package]
name = "cmx-core"
version.workspace = true
edition.workspace = true

[dependencies]
# ============================================
# 序列化框架
# ============================================
# serde: 传统序列化（JSON）
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# ============================================
# Extism 类型转换（可选，用于 WASM 模块）
# ============================================
extism-convert = { version = "0.2", optional = true }

# ============================================
# 基础类型
# ============================================
chrono = { version = "0.4", features = ["serde"] }
smol_str = { version = "0.3", features = ["serde"] }
rust_decimal = "1"
uuid = { version = "1.21", features = ["v4", "serde"] }
base64 = "0.22"

# ============================================
# 错误处理
# ============================================
thiserror = "2"

# ============================================
# 枚举增强
# ============================================
strum = "0.27"
strum_macros = "0.27"

[features]
default = ["std"]
std = []
# Extism 支持（WASM 模块启用）
extism = ["extism-convert"]
```

#### 3.1.3 数据类型定义示例

```rust
// cmx-core/src/wasm_types.rs

use serde::{Deserialize, Serialize};

/// 数据库查询请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbQueryRequest {
    /// SQL 语句
    pub sql: String,
    /// SQL 参数（JSON 字符串）
    #[serde(default)]
    pub params: Option<String>,
    /// 数据集ID（可选）
    #[serde(default)]
    pub dataset_id: Option<String>,
}

/// 数据库操作响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbResponse {
    /// 是否成功
    pub success: bool,
    /// 影响行数（写操作返回）
    pub affected_rows: Option<u64>,
    /// 查询结果数据集（查询操作返回，JSON 字符串）
    pub dataset: Option<String>,
    /// 事务ID（事务操作返回）
    pub txn_id: Option<String>,
    /// 错误信息
    pub error: Option<String>,
}

/// 缓存读取请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheGetRequest {
    /// 缓存键
    pub key: String,
}

/// 缓存写入请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheSetRequest {
    /// 缓存键
    pub key: String,
    /// 缓存值
    pub value: String,
    /// 过期时间（秒）
    #[serde(default)]
    pub ttl_seconds: Option<u64>,
}

/// 缓存操作响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheResponse {
    /// 是否成功
    pub success: bool,
    /// 缓存值（读取操作返回）
    pub value: Option<String>,
    /// 是否存在
    pub exists: Option<bool>,
    /// 错误信息
    pub error: Option<String>,
}

/// 插件调用请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginCallRequest {
    /// 目标插件ID
    pub target_plugin_id: String,
    /// 目标函数名
    pub function_name: String,
    /// 输入数据（JSON 字符串）
    pub input: String,
}

/// 插件调用响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginCallResponse {
    /// 是否成功
    pub success: bool,
    /// 输出数据（JSON 字符串）
    pub output: Option<String>,
    /// 执行耗时（微秒）
    pub elapsed_us: Option<u64>,
    /// 错误信息
    pub error: Option<String>,
}

/// 插件信息响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfoResponse {
    /// 当前插件ID
    pub plugin_id: String,
    /// 数据库ID
    pub db_id: String,
    /// 当前事务ID
    pub txn_id: Option<String>,
    /// 请求ID
    pub request_id: String,
    /// 租户ID
    pub tenant_id: Option<String>,
}

/// 通用 WASM 函数请求
/// 
/// 用于 Host 调用 WASM 函数时的通用请求包装。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmFunctionRequest<T> {
    /// 调用上下文
    pub context: WasmContext,
    /// 业务请求数据
    pub data: T,
}

/// 通用 WASM 函数响应
/// 
/// 用于 WASM 函数返回时的通用响应包装。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmFunctionResponse<T> {
    /// 是否成功
    pub success: bool,
    /// 业务响应数据
    pub data: Option<T>,
    /// 错误信息
    pub error: Option<String>,
}

/// WASM 调用上下文
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmContext {
    /// 请求ID
    pub request_id: String,
    /// 租户ID
    pub tenant_id: Option<String>,
    /// 数据库ID
    pub db_id: String,
    /// 事务ID
    pub txn_id: Option<String>,
    /// 插件ID
    pub plugin_id: String,
}
```

#### 3.1.4 文件修改清单

| 文件                    | 修改类型 | 说明                               |
| --------------------- | ---- | -------------------------------- |
| `Cargo.toml`          | 修改   | 移除 rkyv 依赖，添加 extism-convert（可选） |
| `src/wasm_types.rs`   | 修改   | 移除所有 rkyv 派生，保留 serde 派生         |
| `src/model/cell.rs`   | 修改   | 移除 rkyv 派生                       |
| `src/model/data/*.rs` | 修改   | 移除 rkyv 派生                       |

***

### 3.2 cmx-extism：Extism 运行时模块（新建）

#### 3.2.1 模块职责

**cmx-extism 包含：**

1. Extism 运行时封装
2. Host → WASM 调用封装（实现 RuntimeInvoker trait）
3. Host 函数注册（使用 `host_fn!` 宏）
4. 插件生命周期管理
5. 错误类型定义

**编译目标：** `native` only

#### 3.2.2 Cargo.toml 配置

```toml
[package]
name = "cmx-extism"
version.workspace = true
edition.workspace = true

[dependencies]
# ============================================
# Extism 运行时
# ============================================
extism = "1.4"

# ============================================
# CMX 内部依赖
# ============================================
cmx-core = { path = "../cmx-core" }
cmx-traits = { path = "../cmx-traits" }

# ============================================
# 异步运行时
# ============================================
tokio = { version = "1", features = ["full"] }
async-trait = "0.1"

# ============================================
# 日志和错误处理
# ============================================
tracing = "0.1"
thiserror = "2"

# ============================================
# 序列化
# ============================================
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

[dev-dependencies]
tokio-test = "0.4"
```

#### 3.2.3 核心类型定义

```rust
// cmx-extism/src/lib.rs

pub mod engine;
pub mod error;
pub mod host_functions;

pub use engine::ExtismEngine;
pub use error::ExtismError;
```

```rust
// cmx-extism/src/engine.rs

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use async_trait::async_trait;
use tokio::sync::RwLock;
use tracing;

use cmx_traits::{CallerData, RuntimeInvoker, TraitError, WasmInvokeResult};
use extism::{Manifest, Plugin, Wasm};

use crate::error::ExtismError;

/// Extism 引擎配置
#[derive(Debug, Clone)]
pub struct ExtismEngineConfig {
    /// 是否启用 WASI，默认 true
    pub enable_wasi: bool,
    
    /// 内存限制（字节），默认 256MB
    pub memory_limit: u64,
}

impl Default for ExtismEngineConfig {
    fn default() -> Self {
        Self {
            enable_wasi: true,
            memory_limit: 256 * 1024 * 1024,
        }
    }
}

/// Extism 运行时引擎
///
/// 核心组件，负责：
/// - 管理 Extism 插件实例
/// - 调用 WASM 导出函数
/// - 实现 RuntimeInvoker trait
pub struct ExtismEngine {
    /// 已加载的插件实例映射 (plugin_id -> Plugin)
    plugins: Arc<RwLock<HashMap<String, Plugin>>>,
    
    /// 引擎配置
    config: ExtismEngineConfig,
}

impl ExtismEngine {
    /// 创建新的 Extism 引擎
    pub fn new(config: ExtismEngineConfig) -> Result<Self, ExtismError> {
        Ok(Self {
            plugins: Arc::new(RwLock::new(HashMap::new())),
            config,
        })
    }
    
    /// 使用默认配置创建引擎
    pub fn default() -> Result<Self, ExtismError> {
        Self::new(ExtismEngineConfig::default())
    }
}

#[async_trait]
impl RuntimeInvoker for ExtismEngine {
    async fn invoke(
        &self,
        plugin_id: &str,
        function_name: &str,
        input: &[u8],
        caller_data: &CallerData,
    ) -> Result<WasmInvokeResult, TraitError> {
        let start = std::time::Instant::now();
        
        let mut plugins = self.plugins.write().await;
        let plugin = plugins
            .get_mut(plugin_id)
            .ok_or_else(|| TraitError::WasmNotLoaded(plugin_id.to_string()))?;
        
        // 调用 WASM 函数
        let result = plugin
            .call::<&[u8], Vec<u8>>(function_name, input)
            .map_err(|e| TraitError::WasmInvokeFailed(e.to_string()))?;
        
        let elapsed_us = start.elapsed().as_micros() as u64;
        
        Ok(WasmInvokeResult {
            output: result,
            elapsed_us,
            fuel_consumed: None,
        })
    }
    
    async fn load_module(&self, plugin_id: &str, wasm_path: &Path) -> Result<(), TraitError> {
        // 检查是否已加载
        {
            let plugins = self.plugins.read().await;
            if plugins.contains_key(plugin_id) {
                tracing::warn!("插件 {} 的 WASM 模块已加载，跳过", plugin_id);
                return Ok(());
            }
        }
        
        // 读取 WASM 文件
        let wasm_bytes = std::fs::read(wasm_path)
            .map_err(|e| TraitError::WasmLoadFailed(format!(
                "读取 WASM 文件 {:?} 失败: {}",
                wasm_path, e
            )))?;
        
        // 创建 Manifest
        let wasm = Wasm::data(wasm_bytes);
        let manifest = Manifest::new([wasm])
            .with_wasi(self.config.enable_wasi)
            .with_memory_max(self.config.memory_limit);
        
        // 创建插件实例
        let mut plugin = Plugin::new(&manifest, [], true)
            .map_err(|e| TraitError::WasmLoadFailed(format!(
                "创建 Extism 插件失败: {}",
                e
            )))?;
        
        tracing::info!(
            "插件 {} WASM 模块加载成功",
            plugin_id
        );
        
        // 保存插件实例
        let mut plugins = self.plugins.write().await;
        plugins.insert(plugin_id.to_string(), plugin);
        
        Ok(())
    }
    
    async fn unload_module(&self, plugin_id: &str) -> Result<(), TraitError> {
        let mut plugins = self.plugins.write().await;
        if plugins.remove(plugin_id).is_some() {
            tracing::info!("插件 {} WASM 模块已卸载", plugin_id);
        } else {
            tracing::warn!("插件 {} WASM 模块未加载，无法卸载", plugin_id);
        }
        Ok(())
    }
    
    async fn is_loaded(&self, plugin_id: &str) -> bool {
        let plugins = self.plugins.read().await;
        plugins.contains_key(plugin_id)
    }
}
```

```rust
// cmx-extism/src/error.rs

/// Extism 错误类型
#[derive(Debug, Clone, thiserror::Error)]
pub enum ExtismError {
    #[error("插件加载失败: {0}")]
    PluginLoadFailed(String),
    
    #[error("插件调用失败: {0}")]
    PluginCallFailed(String),
    
    #[error("配置错误: {0}")]
    ConfigError(String),
    
    #[error("内部错误: {0}")]
    InternalError(String),
}
```

#### 3.2.4 宿主函数注册

```rust
// cmx-extism/src/host_functions.rs

use extism::{host_fn, UserData, CurrentPlugin};
use cmx_core::wasm_types::*;

/// 数据库查询宿主函数
/// 
/// # 参数
/// - `plugin`: Extism 插件上下文
/// - `request`: 数据库查询请求（JSON）
/// 
/// # 返回值
/// 返回数据库响应（JSON）
pub fn register_database_host_functions() {
    // 数据库查询函数
    host_fn!(db_query(plugin: CurrentPlugin; request: String) -> String {
        // 解析请求
        let query_request: DbQueryRequest = serde_json::from_str(&request)
            .map_err(|e| format!("解析请求失败: {}", e))?;
        
        // 执行数据库查询（需要注入数据库管理器）
        // let response = db_manager.query(&query_request).await?;
        
        // 返回响应
        let response = DbResponse {
            success: true,
            affected_rows: None,
            dataset: Some(r#"[{"id": 1, "name": "test"}]"#.to_string()),
            txn_id: None,
            error: None,
        };
        
        let response_json = serde_json::to_string(&response)
            .map_err(|e| format!("序列化响应失败: {}", e))?;
        
        Ok(response_json)
    });
    
    // 数据库执行函数
    host_fn!(db_execute(plugin: CurrentPlugin; request: String) -> String {
        let execute_request: DbQueryRequest = serde_json::from_str(&request)
            .map_err(|e| format!("解析请求失败: {}", e))?;
        
        // 执行数据库操作
        let response = DbResponse {
            success: true,
            affected_rows: Some(1),
            dataset: None,
            txn_id: None,
            error: None,
        };
        
        let response_json = serde_json::to_string(&response)
            .map_err(|e| format!("序列化响应失败: {}", e))?;
        
        Ok(response_json)
    });
}

/// 缓存操作宿主函数
pub fn register_cache_host_functions() {
    host_fn!(cache_get(plugin: CurrentPlugin; request: String) -> String {
        let cache_request: CacheGetRequest = serde_json::from_str(&request)
            .map_err(|e| format!("解析请求失败: {}", e))?;
        
        // 执行缓存读取
        let response = CacheResponse {
            success: true,
            value: Some("cached_value".to_string()),
            exists: Some(true),
            error: None,
        };
        
        let response_json = serde_json::to_string(&response)
            .map_err(|e| format!("序列化响应失败: {}", e))?;
        
        Ok(response_json)
    });
    
    host_fn!(cache_set(plugin: CurrentPlugin; request: String) -> String {
        let cache_request: CacheSetRequest = serde_json::from_str(&request)
            .map_err(|e| format!("解析请求失败: {}", e))?;
        
        // 执行缓存写入
        let response = CacheResponse {
            success: true,
            value: None,
            exists: None,
            error: None,
        };
        
        let response_json = serde_json::to_string(&response)
            .map_err(|e| format!("序列化响应失败: {}", e))?;
        
        Ok(response_json)
    });
}

/// 日志宿主函数
pub fn register_logging_host_functions() {
    host_fn!(log_info(plugin: CurrentPlugin; message: String) {
        tracing::info!("[WASM] {}", message);
        Ok(())
    });
    
    host_fn!(log_error(plugin: CurrentPlugin; message: String) {
        tracing::error!("[WASM] {}", message);
        Ok(())
    });
}
```

#### 3.2.5 文件结构

```
cmx-extism/
├── Cargo.toml
├── src/
│   ├── lib.rs              # 模块入口
│   ├── engine.rs           # ExtismEngine 核心引擎
│   ├── error.rs            # ExtismError 错误类型
│   └── host_functions.rs   # 宿主函数注册
└── tests/
    └── engine_test.rs      # 单元测试
```

***

### 3.3 cmx-plugin-sdk：插件开发 SDK（新建）

#### 3.3.1 模块职责

**cmx-plugin-sdk 包含：**

1. Extism PDK 封装
2. 宿主函数调用封装
3. 插件函数导出宏
4. 错误类型定义
5. 工具函数

**编译目标：** `wasm32-unknown-unknown` only

#### 3.3.2 Cargo.toml 配置

```toml
[package]
name = "cmx-plugin-sdk"
version.workspace = true
edition.workspace = true

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
# ============================================
# Extism PDK
# ============================================
extism-pdk = "1.1"

# ============================================
# CMX 内部依赖
# ============================================
cmx-core = { path = "../cmx-core", features = ["extism"] }

# ============================================
# 序列化
# ============================================
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# ============================================
# 错误处理
# ============================================
thiserror = "2"
```

#### 3.3.3 宿主函数调用封装

```rust
// cmx-plugin-sdk/src/host_calls.rs

use extism_pdk::*;
use cmx_core::wasm_types::*;
use serde::de::DeserializeOwned;
use serde::Serialize;

/// 宿主函数调用器
/// 
/// 用于 WASM 插件调用宿主函数
pub struct HostCaller;

impl HostCaller {
    /// 调用数据库查询
    /// 
    /// # 参数
    /// - `request`: 数据库查询请求
    /// 
    /// # 返回值
    /// 返回数据库响应
    pub fn db_query(request: DbQueryRequest) -> Result<DbResponse, Error> {
        let request_json = serde_json::to_string(&request)?;
        let response_json = unsafe {
            extism_pdk::call_host("cmx:database", "db_query", &request_json)?
        };
        let response: DbResponse = serde_json::from_str(&response_json)?;
        Ok(response)
    }
    
    /// 调用数据库执行
    pub fn db_execute(request: DbQueryRequest) -> Result<DbResponse, Error> {
        let request_json = serde_json::to_string(&request)?;
        let response_json = unsafe {
            extism_pdk::call_host("cmx:database", "db_execute", &request_json)?
        };
        let response: DbResponse = serde_json::from_str(&response_json)?;
        Ok(response)
    }
    
    /// 调用缓存读取
    pub fn cache_get(request: CacheGetRequest) -> Result<CacheResponse, Error> {
        let request_json = serde_json::to_string(&request)?;
        let response_json = unsafe {
            extism_pdk::call_host("cmx:buffer", "cache_get", &request_json)?
        };
        let response: CacheResponse = serde_json::from_str(&response_json)?;
        Ok(response)
    }
    
    /// 调用缓存写入
    pub fn cache_set(request: CacheSetRequest) -> Result<CacheResponse, Error> {
        let request_json = serde_json::to_string(&request)?;
        let response_json = unsafe {
            extism_pdk::call_host("cmx:buffer", "cache_set", &request_json)?
        };
        let response: CacheResponse = serde_json::from_str(&response_json)?;
        Ok(response)
    }
    
    /// 调用插件服务
    pub fn call_plugin(request: PluginCallRequest) -> Result<PluginCallResponse, Error> {
        let request_json = serde_json::to_string(&request)?;
        let response_json = unsafe {
            extism_pdk::call_host("cmx:plugin", "call_service", &request_json)?
        };
        let response: PluginCallResponse = serde_json::from_str(&response_json)?;
        Ok(response)
    }
    
    /// 记录信息日志
    pub fn log_info(message: &str) -> Result<(), Error> {
        unsafe {
            extism_pdk::call_host("cmx:log", "log_info", message)?;
        }
        Ok(())
    }
    
    /// 记录错误日志
    pub fn log_error(message: &str) -> Result<(), Error> {
        unsafe {
            extism_pdk::call_host("cmx:log", "log_error", message)?;
        }
        Ok(())
    }
}
```

#### 3.3.4 插件函数导出示例

````rust
// cmx-plugin-sdk/src/lib.rs

pub mod host_calls;
pub mod error;

pub use extism_pdk::*;
pub use host_calls::HostCaller;
pub use error::PluginError;

/// 插件开发示例
/// 
/// ```rust
/// use cmx_plugin_sdk::*;
/// use cmx_core::wasm_types::*;
/// use serde::{Deserialize, Serialize};
/// 
/// /// 定义请求类型
/// #[derive(Debug, Clone, Serialize, Deserialize)]
/// pub struct MyRequest {
///     pub name: String,
///     pub age: u32,
/// }
/// 
/// /// 定义响应类型
/// #[derive(Debug, Clone, Serialize, Deserialize)]
/// pub struct MyResponse {
///     pub message: String,
///     pub processed: bool,
/// }
/// 
/// /// 插件函数
/// #[plugin_fn]
/// pub fn process_data(request: MyRequest) -> FnResult<MyResponse> {
///     // 记录日志
///     HostCaller::log_info(&format!("处理请求: {:?}", request))?;
///     
///     // 调用数据库
///     let db_response = HostCaller::db_query(DbQueryRequest {
///         sql: "SELECT * FROM users WHERE name = ?".to_string(),
///         params: Some(request.name.clone()),
///         dataset_id: None,
///     })?;
///     
///     // 返回响应
///     Ok(MyResponse {
///         message: format!("Hello, {}!", request.name),
///         processed: true,
///     })
/// }
/// ```
````

#### 3.3.5 文件结构

```
cmx-plugin-sdk/
├── Cargo.toml
├── src/
│   ├── lib.rs              # 模块入口 + 插件开发示例
│   ├── host_calls.rs       # 宿主函数调用封装
│   └── error.rs            # PluginError 错误类型
└── tests/
    └── plugin_test.rs      # 单元测试
```

***

### 3.4 cmx-wasmdemo：示例插件模块（重构）

#### 3.4.1 变更内容

**删除内容：**

* 所有 Arena 相关代码

* 所有 rkyv 相关代码

* 手动导出函数代码

**新增内容：**

* 使用 `#[plugin_fn]` 宏导出函数

* 使用 `HostCaller` 调用宿主函数

* 使用 serde 序列化

#### 3.4.2 Cargo.toml 修改

```toml
[package]
name = "cmx-wasmdemo"
version.workspace = true
edition.workspace = true

[lib]
crate-type = ["cdylib"]

[dependencies]
# ============================================
# CMX 插件 SDK
# ============================================
cmx-plugin-sdk = { path = "../cmx-plugin-sdk" }

# ============================================
# CMX 核心类型
# ============================================
cmx-core = { path = "../cmx-core", features = ["extism"] }

# ============================================
# 序列化
# ============================================
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
```

#### 3.4.3 插件实现示例

```rust
// cmx-wasmdemo/src/lib.rs

use cmx_plugin_sdk::*;
use cmx_core::wasm_types::*;
use serde::{Deserialize, Serialize};

/// 示例请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DemoRequest {
    pub name: String,
    pub count: u32,
}

/// 示例响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DemoResponse {
    pub message: String,
    pub total: u32,
}

/// 示例插件函数
/// 
/// 统计字符串中的元音字母数量
#[plugin_fn]
pub fn count_vowels(input: String) -> FnResult<String> {
    let vowels = ['a', 'e', 'i', 'o', 'u', 'A', 'E', 'I', 'O', 'U'];
    let count = input.chars().filter(|c| vowels.contains(c)).count();
    
    let response = serde_json::json!({
        "count": count,
        "total": count,
        "input": input,
    });
    
    Ok(response.to_string())
}

/// 数据库查询示例
#[plugin_fn]
pub fn query_example(request: DemoRequest) -> FnResult<DemoResponse> {
    // 记录日志
    HostCaller::log_info(&format!("查询示例: {:?}", request))?;
    
    // 调用数据库
    let db_response = HostCaller::db_query(DbQueryRequest {
        sql: format!("SELECT * FROM demo WHERE name = '{}'", request.name),
        params: None,
        dataset_id: None,
    })?;
    
    // 返回响应
    Ok(DemoResponse {
        message: format!("查询成功: {:?}", db_response),
        total: request.count,
    })
}

/// 缓存操作示例
#[plugin_fn]
pub fn cache_example(request: DemoRequest) -> FnResult<DemoResponse> {
    // 写入缓存
    HostCaller::cache_set(CacheSetRequest {
        key: request.name.clone(),
        value: request.count.to_string(),
        ttl_seconds: Some(3600),
    })?;
    
    // 读取缓存
    let cache_response = HostCaller::cache_get(CacheGetRequest {
        key: request.name.clone(),
    })?;
    
    // 返回响应
    Ok(DemoResponse {
        message: format!("缓存操作成功: {:?}", cache_response),
        total: request.count,
    })
}

/// 插件间调用示例
#[plugin_fn]
pub fn call_plugin_example(request: DemoRequest) -> FnResult<DemoResponse> {
    // 调用其他插件
    let plugin_response = HostCaller::call_plugin(PluginCallRequest {
        target_plugin_id: "other-plugin".to_string(),
        function_name: "some_function".to_string(),
        input: serde_json::to_string(&request)?,
    })?;
    
    // 返回响应
    Ok(DemoResponse {
        message: format!("插件调用成功: {:?}", plugin_response),
        total: request.count,
    })
}
```

#### 3.4.4 编译命令

```bash
# 安装 WASM 目标
rustup target add wasm32-unknown-unknown

# 编译插件
cd crates/libs/cmx-wasmdemo
cargo build --release --target wasm32-unknown-unknown

# 输出文件
# target/wasm32-unknown-unknown/release/cmx_wasmdemo.wasm
```

***

### 3.5 cmx-service：服务编排模块（修改）

#### 3.5.1 变更内容

**修改内容：**

* 更新依赖：`cmx-runtime` → `cmx-extism`

* 保持 `RuntimeInvoker` trait 接口不变

* 保持编排逻辑不变

**无需修改：**

* 编排执行器（Orchestrator）

* 编排定义（Orchestration）

* HTTP Handler

#### 3.5.2 Cargo.toml 修改

```toml
[package]
name = "cmx-service"
version.workspace = true
edition.workspace = true

[dependencies]
# ============================================
# CMX 内部依赖
# ============================================
cmx-core = { path = "../cmx-core" }
cmx-traits = { path = "../cmx-traits" }
cmx-extism = { path = "../cmx-extism" }  # ← 修改：cmx-runtime → cmx-extism
cmx-database = { path = "../cmx-infra/cmx-database" }

# ============================================
# 异步运行时
# ============================================
tokio = { version = "1", features = ["full"] }
async-trait = "0.1"

# ============================================
# 序列化
# ============================================
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# ============================================
# 日志和错误处理
# ============================================
tracing = "0.1"
thiserror = "2"
```

#### 3.5.3 使用示例

```rust
// web-server/src/plugins.rs

use cmx_extism::ExtismEngine;
use cmx_service::{CmxService, ServiceConfig};
use cmx_traits::RuntimeInvoker;
use std::sync::Arc;

/// 初始化服务
pub async fn init_service() -> Arc<CmxService> {
    // 创建 Extism 引擎
    let engine = ExtismEngine::default()
        .expect("创建 Extism 引擎失败");
    
    // 包装为 trait 对象
    let runtime: Arc<dyn RuntimeInvoker> = Arc::new(engine);
    
    // 创建服务
    let service = CmxService::with_defaults(
        plugin_query,  // PluginQuery trait 对象
        runtime,        // RuntimeInvoker trait 对象
    );
    
    Arc::new(service)
}
```

***

### 3.6 cmx-traits：Trait 抽象层（保持不变）

#### 3.6.1 无需修改

cmx-traits 模块定义的 trait 接口保持不变：

* `RuntimeInvoker` — WASM 运行时调用接口

* `PluginQuery` — 插件状态查询接口

* `PluginLifecycleListener` — 生命周期监听接口

* `HostFunctionProvider` — 宿主函数注册接口（可能需要调整）

#### 3.6.2 可能的调整

如果需要适配 extism 的宿主函数注册方式，可能需要调整 `HostFunctionProvider` trait：

```rust
// cmx-traits/src/host_func.rs

use extism::{CurrentPlugin, Function};

/// 宿主函数提供者（Extism 版本）
pub trait HostFunctionProvider: Send + Sync {
    /// 获取命名空间
    fn namespace(&self) -> &str;
    
    /// 注册宿主函数到 Extism
    /// 
    /// # 参数
    /// - `builder`: Extism PluginBuilder
    fn register_functions(&self, builder: &mut PluginBuilder) -> Result<(), HostFuncError>;
    
    /// 获取提供的函数列表
    fn provided_functions(&self) -> Vec<&str> {
        Vec::new()
    }
}
```

***

## 四、数据传递机制变更

### 4.1 现有机制（wasmtime + rkyv）

#### Host → WASM

```
┌──────────────────────────────────────────────────────────────────┐
│                    Host (使用 cmx-runtime)                        │
│                                                                  │
│  1. rkyv 序列化请求                                                │
│     let input = rkyv::to_bytes(&request)?;                       │
│                                                                  │
│  2. 调用 WASM 分配函数                                             │
│     let input_ptr = wasm_alloc(input.len())?;                    │
│                                                                  │
│  3. 写入输入数据到 WASM 内存                                       │
│     memory[input_ptr..input_ptr+input.len()] = input;            │
│                                                                  │
│  4. 调用 WASM 业务函数                                             │
│     let result = invoker.invoke("process", &request)?;           │
│                                                                  │
│  5. 零拷贝读取响应                                                 │
│     let response = rkyv::archived_root::<R>(output);             │
│                                                                  │
│  6. 重置 WASM Arena                                               │
│     wasm_reset_arena();                                          │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
                                    │
                                    │ (input_ptr, input_len) -> i64(ptr, len)
                                    ▼
┌──────────────────────────────────────────────────────────────────┐
│                    WASM (使用 cmx-wasm-core)                      │
│                                                                  │
│  1. 零拷贝解析输入请求                                             │
│     let request = parse_input::<T>(input_ptr, input_len)?;       │
│                                                                  │
│  2. 执行业务逻辑                                                   │
│     let response = process_request(request)?;                    │
│                                                                  │
│  3. rkyv 序列化响应到 Arena                                        │
│     let (ptr, len) = serialize_to_arena(&response)?;             │
│                                                                  │
│  4. 返回 (ptr, len) 编码为 i64                                    │
│     return encode_ptr_len(ptr, len);                             │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
```

#### WASM → Host

```
┌──────────────────────────────────────────────────────────────────┐
│                    WASM (使用 cmx-wasm-core)                      │
│                                                                  │
│  1. 构建请求对象（cmx-core 类型）                                  │
│     let request = DbQueryRequest { sql: "SELECT ...", ... };     │
│                                                                  │
│  2. rkyv 序列化请求                                               │
│     let bytes = rkyv::to_bytes(&request)?;                       │
│                                                                  │
│  3. 准备 Arena 输出缓冲区                                         │
│     arena.reset();                                               │
│                                                                  │
│  4. 调用宿主函数                                                   │
│     host_caller.call("cmx:database", "query_sql", &request)?;    │
│                                                                  │
│  5. rkyv 零拷贝反序列化响应                                        │
│     let archived = rkyv::archived_root::<R>(output_bytes);       │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
                                    │
                                    │ (input_ptr, input_len, output_ptr, capacity)
                                    ▼
┌──────────────────────────────────────────────────────────────────┐
│                    Host (使用 cmx-runtime)                        │
│                                                                  │
│  1. 从 WASM 线性内存读取 rkyv 归档数据                             │
│     let bytes = memory.data(&store)[ptr..ptr+len];               │
│                                                                  │
│  2. 零拷贝访问请求                                                 │
│     let archived = rkyv::archived_root::<T>(bytes);              │
│                                                                  │
│  3. 执行业务逻辑                                                   │
│     let response = handler(context, archived)?;                  │
│                                                                  │
│  4. rkyv 序列化响应                                                │
│     let output = rkyv::to_bytes(&response)?;                     │
│                                                                  │
│  5. 写入 WASM 线性内存                                             │
│     memory.data_mut(&store)[out_ptr..] = output;                 │
│                                                                  │
│  6. 返回写入字节数                                                 │
│     return output.len() as i32;                                  │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
```

### 4.2 新机制（extism）

#### Host → WASM

```
┌──────────────────────────────────────────────────────────────────┐
│                    Host (使用 cmx-extism)                         │
│                                                                  │
│  1. JSON 序列化请求                                                │
│     let input = serde_json::to_vec(&request)?;                   │
│                                                                  │
│  2. 调用 Extism 插件函数                                           │
│     let result = plugin.call::<&[u8], Vec<u8>>("process", input)?; │
│                                                                  │
│  3. JSON 反序列化响应                                              │
│     let response: R = serde_json::from_slice(&result)?;          │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
                                    │
                                    │ Extism 自动处理内存传递
                                    ▼
┌──────────────────────────────────────────────────────────────────┐
│                    WASM (使用 cmx-plugin-sdk)                     │
│                                                                  │
│  1. 自动解析输入请求                                               │
│     #[plugin_fn] 自动处理                                         │
│                                                                  │
│  2. 执行业务逻辑                                                   │
│     let response = process_request(request)?;                    │
│                                                                  │
│  3. 自动序列化响应                                                 │
│     #[plugin_fn] 自动处理                                         │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
```

#### WASM → Host

```
┌──────────────────────────────────────────────────────────────────┐
│                    WASM (使用 cmx-plugin-sdk)                     │
│                                                                  │
│  1. 构建请求对象                                                   │
│     let request = DbQueryRequest { sql: "SELECT ...", ... };     │
│                                                                  │
│  2. JSON 序列化请求                                                │
│     let request_json = serde_json::to_string(&request)?;         │
│                                                                  │
│  3. 调用宿主函数                                                   │
│     let response_json = call_host("cmx:database", "db_query", request_json)?; │
│                                                                  │
│  4. JSON 反序列化响应                                              │
│     let response: DbResponse = serde_json::from_str(&response_json)?; │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
                                    │
                                    │ Extism 自动处理内存传递
                                    ▼
┌──────────────────────────────────────────────────────────────────┐
│                    Host (使用 cmx-extism)                         │
│                                                                  │
│  1. 自动解析输入请求                                               │
│     host_fn! 宏自动处理                                           │
│                                                                  │
│  2. 执行业务逻辑                                                   │
│     let response = handler(request)?;                            │
│                                                                  │
│  3. 自动序列化响应                                                 │
│     host_fn! 宏自动处理                                           │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
```

### 4.3 性能对比

| 操作        | wasmtime + rkyv | extism + JSON | 性能差异       |
| --------- | --------------- | ------------- | ---------- |
| **序列化**   | 零拷贝，极快          | JSON 序列化，较快   | rkyv 快 10x |
| **反序列化**  | 零拷贝，无开销         | JSON 解析，有开销   | rkyv 快 10x |
| **内存管理**  | 手动 Arena        | 自动管理          | extism 更安全 |
| **开发复杂度** | 高               | 低             | extism 更简单 |
| **总体性能**  | 极高              | 高             | 差距可接受      |

**结论：** 虽然 extism + JSON 的序列化性能不如 rkyv，但：

1. 性能差距在可接受范围内（毫秒级）
2. 开发效率大幅提升
3. 维护成本显著降低
4. 安全性更好（无内存安全问题）

***

## 五、迁移步骤

### 5.1 阶段一：基础设施准备（1-2 天）

#### 任务清单

1. **创建 cmx-extism 模块**

   * [ ] 创建模块目录结构

   * [ ] 配置 Cargo.toml

   * [ ] 实现 ExtismEngine 核心引擎

   * [ ] 实现 RuntimeInvoker trait

   * [ ] 编写单元测试

2. **创建 cmx-plugin-sdk 模块**

   * [ ] 创建模块目录结构

   * [ ] 配置 Cargo.toml

   * [ ] 实现 HostCaller 封装

   * [ ] 编写插件开发示例

   * [ ] 编写单元测试

3. **修改 cmx-core 模块**

   * [ ] 移除 rkyv 依赖

   * [ ] 移除所有 rkyv 派生

   * [ ] 保留 serde 派生

   * [ ] 添加 extism-convert（可选）

   * [ ] 验证编译通过

### 5.2 阶段二：宿主函数迁移（2-3 天）

#### 任务清单

1. **迁移数据库宿主函数**

   * [ ] 使用 `host_fn!` 宏重写 db\_query

   * [ ] 使用 `host_fn!` 宏重写 db\_execute

   * [ ] 使用 `host_fn!` 宏重写事务相关函数

   * [ ] 测试数据库操作

2. **迁移缓存宿主函数**

   * [ ] 使用 `host_fn!` 宏重写 cache\_get

   * [ ] 使用 `host_fn!` 宏重写 cache\_set

   * [ ] 使用 `host_fn!` 宏重写其他缓存操作

   * [ ] 测试缓存操作

3. **迁移日志宿主函数**

   * [ ] 使用 `host_fn!` 宏重写 log\_info

   * [ ] 使用 `host_fn!` 宏重写 log\_error

   * [ ] 测试日志功能

4. **迁移插件宿主函数**

   * [ ] 使用 `host_fn!` 宏重写 call\_service

   * [ ] 测试插件间调用

### 5.3 阶段三：插件模块迁移（1-2 天）

#### 任务清单

1. **重构 cmx-wasmdemo**

   * [ ] 移除所有 Arena 相关代码

   * [ ] 移除所有 rkyv 相关代码

   * [ ] 使用 `#[plugin_fn]` 宏重写所有导出函数

   * [ ] 使用 HostCaller 调用宿主函数

   * [ ] 编译为 wasm32-unknown-unknown

   * [ ] 测试插件功能

2. **更新编译脚本**

   * [ ] 修改编译目标为 wasm32-unknown-unknown

   * [ ] 更新构建脚本

   * [ ] 更新部署脚本

### 5.4 阶段四：服务集成（1 天）

#### 任务清单

1. **更新 cmx-service**

   * [ ] 修改依赖：cmx-runtime → cmx-extism

   * [ ] 验证 RuntimeInvoker trait 兼容性

   * [ ] 测试服务编排功能

2. **更新 web-server**

   * [ ] 修改初始化代码

   * [ ] 注册 Extism 宿主函数

   * [ ] 测试 HTTP 接口

### 5.5 阶段五：测试与优化（1-2 天）

#### 任务清单

1. **功能测试**

   * [ ] 测试所有宿主函数

   * [ ] 测试所有插件函数

   * [ ] 测试服务编排

   * [ ] 测试插件生命周期

2. **性能测试**

   * [ ] 测试单次调用延迟

   * [ ] 测试吞吐量

   * [ ] 对比迁移前后性能

3. **文档更新**

   * [ ] 更新 README

   * [ ] 更新 API 文档

   * [ ] 编写迁移指南

***

## 六、风险评估与缓解

### 6.1 技术风险

| 风险               | 影响 | 概率 | 缓解措施                |
| ---------------- | -- | -- | ------------------- |
| **性能下降**         | 中  | 中  | 进行性能测试，优化热点路径       |
| **Extism 版本兼容性** | 低  | 低  | 锁定 Extism 版本，定期更新   |
| **宿主函数迁移错误**     | 高  | 中  | 充分测试，保留回滚方案         |
| **插件编译问题**       | 中  | 低  | 提供详细的编译指南           |
| **依赖冲突**         | 低  | 低  | 使用 workspace 统一管理依赖 |

### 6.2 业务风险

| 风险          | 影响 | 概率 | 缓解措施          |
| ----------- | -- | -- | ------------- |
| **现有插件不兼容** | 高  | 高  | 无需向后兼容，重新开发插件 |
| **开发进度延迟**  | 中  | 中  | 预留缓冲时间，分阶段实施  |
| **团队学习成本**  | 低  | 低  | 提供培训和文档       |

### 6.3 回滚方案

如果迁移失败，可以快速回滚：

1. **保留原有代码**：在迁移期间，保留 `cmx-runtime` 和 `cmx-wasm-core` 模块
2. **分支管理**：在独立分支进行迁移，失败可删除分支
3. **配置切换**：通过配置文件切换运行时实现

***

## 七、验收标准

### 7.1 功能验收

* [ ] 所有宿主函数正常工作

* [ ] 所有插件函数正常工作

* [ ] 服务编排正常工作

* [ ] 插件生命周期管理正常

* [ ] HTTP 接口正常

### 7.2 性能验收

* [ ] 单次调用延迟 < 10ms

* [ ] 编排执行吞吐量 > 500 steps/s

* [ ] 内存占用合理，无内存泄漏

### 7.3 代码质量验收

* [ ] 所有单元测试通过

* [ ] 所有集成测试通过

* [ ] 代码覆盖率 > 80%

* [ ] 无编译警告

### 7.4 文档验收

* [ ] README 更新完成

* [ ] API 文档更新完成

* [ ] 迁移指南编写完成

* [ ] 示例代码编写完成

***

## 八、总结

### 8.1 迁移收益

1. **开发效率提升**：使用高级 API，减少 50% 以上的代码量
2. **维护成本降低**：无需关注内存管理、序列化细节
3. **学习曲线平缓**：新开发者可快速上手
4. **安全性提升**：无内存安全问题
5. **生态完善**：Extism 提供多语言 PDK 支持

### 8.2 性能权衡

虽然 JSON 序列化性能不如 rkyv，但：

* 性能差距在可接受范围内

* 开发效率和维护性的收益远超性能损失

* 可通过优化热点路径弥补性能差距

### 8.3 建议

1. **优先迁移**：建议尽快开始迁移，享受 Extism 的便利
2. **充分测试**：迁移过程中要充分测试，确保功能正确
3. **保留文档**：编写详细的迁移文档，方便后续维护
4. **持续优化**：迁移完成后，持续优化性能和体验

***

## 九、参考资料

### 9.1 Extism 官方文档

* [Extism 官网](https://extism.org/)

* [Extism GitHub](https://github.com/extism/extism)

* [Extism Rust SDK](https://github.com/extism/extism/tree/main/runtime)

* [Extism Rust PDK](https://github.com/extism/rust-pdk)

* [Extism 文档](https://extism.org/docs/)

### 9.2 相关技术文档

* [wasmtime 文档](https://docs.wasmtime.dev/)

* [serde 文档](https://serde.rs/)

* [WebAssembly 规范](https://webassembly.org/)

### 9.3 性能对比

* [WebAssembly 运行时性能对比](https://blog.csdn.net/gitblog_00101/article/details/153718226)

* [Extism vs wasmtime](https://www.libhunt.com/compare-wasmtime-vs-extism)

