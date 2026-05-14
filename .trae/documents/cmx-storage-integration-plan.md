# cmx-storage 模块集成与存储配置计划

## 一、现状分析

### 1.1 cmx-storage 初始化在哪里完成？

**当前状态：cmx-storage 模块尚未集成到主应用中。**

* `web-server/src/main.rs` 中没有调用任何 cmx-storage 初始化函数

* `web-server/src/config.rs` 中没有 `init_storage()` 函数

* `web-server/Cargo.toml` 中没有 `cmx-storage` 依赖

* `dev.toml` 中没有 `[storage]` 配置段

* `CmxAppState` 中没有 storage\_service 字段

* `routes_impl.rs` 中没有注册 storage 路由

### 1.2 是否有全局的 StorageService？

**没有。** `DefaultStorageService` 需要手动创建。计划添加 `GlobalStorageService` 全局单例。

### 1.3 是否有全局默认的存储平台配置？

**配置基础设施已完备**（`StorageManagerConfig` 支持多平台 + `default_platform`），但 dev.toml 中还没有添加配置。

### 1.4 配置结构问题

当前 `StorageManagerConfig` 的字段名 `storage` 与 TOML section `[storage]` 冲突，会导致 TOML 中出现 `[[storage.storage]]`
。需要重命名为 `instances`，使 TOML 配置结构更合理。

***

## 二、实施计划

### 步骤 1：重构 StorageManagerConfig 字段名

将 `StorageManagerConfig.storage` 重命名为 `StorageManagerConfig.instances`。

**修改前**：

```rust
pub struct StorageManagerConfig {
    pub storage: Vec<StorageInstanceConfig>,
    pub default_platform: Option<String>,
}
```

**修改后**：

```rust
pub struct StorageManagerConfig {
    pub instances: Vec<StorageInstanceConfig>,
    pub default_platform: Option<String>,
}
```

同步修改 `enabled_instances()` 方法中对 `self.storage` 的引用改为 `self.instances`。

**涉及文件**：`crates/libs/cmx-infra/cmx-storage/src/config.rs`

### 步骤 2：在 dev.toml 中添加存储配置

在 `dev.toml` 文件末尾添加 `[storage]` 配置段，整合所有存储相关配置到一个结构下：

```toml
# ============================================
# 文件存储配置
# ============================================
[storage]
default_platform = "local-1"

# 本地文件系统存储
[[storage.instances]]
platform = "local-1"
storage_type = "local"
enable_storage = true
domain = "http://localhost:8080/files/"
base_path = "uploads"
storage_path = "/data/cmx/storage"
enable_access = true
path_patterns = "**/*"

# MinIO (S3 兼容) 存储
[[storage.instances]]
platform = "amazon-s3-1"
storage_type = "s3"
enable_storage = true
access_key = "jRszdym59WToFPfmvr7O"
secret_key = "6C5a6ZRWhdilLu7JLq43IbTST41rpT0XV1aanuIG"
region = "us-east-1"
endpoint = "http://192.168.254.204:9000/"
bucket_name = "gateway-core-data"
domain = "http://192.168.254.204:9001/"
base_path = "portalcenter/"
```

**涉及文件**：`dev.toml`

### 步骤 3：在 web-server/Cargo.toml 中添加 cmx-storage 依赖

```toml
# 内部依赖 - 存储
cmx-storage = { workspace = true }
```

**涉及文件**：`crates/web/web-server/Cargo.toml`

### 步骤 4：创建 GlobalStorageService 全局单例

在 `cmx-storage` 中新建 `global.rs`，参照项目中其他全局单例（如 `GlobalCacheManager`、`GlobalExtismEngine`）的模式：

```rust
use std::sync::{Arc, OnceLock};
use crate::service::StorageService;

pub struct GlobalStorageService {
    service: Arc<dyn StorageService>,
}

impl GlobalStorageService {
    pub fn initialize(service: Arc<dyn StorageService>) -> Result<(), Arc<dyn StorageService>> { ... }
    pub fn get() -> &'static GlobalStorageService { ... }
    pub fn service(&self) -> &Arc<dyn StorageService> { ... }
}
```

**涉及文件**：

* `crates/libs/cmx-infra/cmx-storage/src/global.rs`（新建）

* `crates/libs/cmx-infra/cmx-storage/src/lib.rs`（添加 `pub mod global;`）

### 步骤 5：在 web-server/src/config.rs 中添加 init\_storage() 函数

参照 `init_cache()` 和 `init_services()` 的模式：

```rust
pub async fn init_storage() {
    use cmx_storage::config::StorageManagerConfig;
    use cmx_storage::manager::StorageManager;
    use cmx_storage::service::DefaultStorageService;
    use cmx_storage::global::GlobalStorageService;

    info!("初始化存储服务...");

    let config = ConfigManager::global();
    let storage_config = StorageManagerConfig::from_config(config)
        .expect("存储配置加载失败");

    let manager = Arc::new(StorageManager::new(&storage_config)
        .expect("存储管理器初始化失败"));

    let service = Arc::new(DefaultStorageService::new(manager));
    GlobalStorageService::initialize(service)
        .expect("存储服务全局初始化失败");

    info!("存储服务初始化完成");
}
```

**涉及文件**：`crates/web/web-server/src/config.rs`

### 步骤 6：在 main.rs 启动流程中调用 init\_storage()

在 `init_datasources()` 之后添加：

```rust
// 初始化数据库数据源
init_datasources().await;

// 初始化文件存储服务（必须在 init_datasources 之后）
init_storage().await;
```

**涉及文件**：`crates/web/web-server/src/main.rs`

### 步骤 7：注册 storage 路由到主应用

需要将 cmx-storage 的 REST API 路由集成到主应用中。由于 cmx-storage 使用独立的 `AppState`（持有 `Arc<dyn StorageService>`
），需要创建适配层。

**7a. cmx-api 添加 cmx-storage 依赖**

在 `cmx-api/Cargo.toml` 中添加：

```toml
cmx-storage = { workspace = true }
```

**7b. CmxAppState 添加 storage\_service 字段**

```rust
pub struct CmxAppState {
    // ... 现有字段 ...
    storage_service: Option<Arc<dyn StorageService>>,
}
```

添加 `with_storage_service()` 和 `storage_service()` 方法。

**7c. 创建 storage handler 适配模块**

在 `cmx-api/src/handlers/storage/` 下创建适配模块，实现 `ModuleRoutes` trait，内部通过 `CmxAppState.storage_service` 创建
cmx-storage 的 `AppState` 并调用 `create_router`。

**7d. 注册路由**

在 `routes_impl.rs` 中注册 storage 模块路由。

**7e. main.rs 注入**

在 `main.rs` 中通过 `.with_storage_service(...)` 注入到 `CmxAppState`。

**涉及文件**：

* `crates/libs/cmx-api/Cargo.toml`

* `crates/libs/cmx-api/src/app_state.rs`

* `crates/libs/cmx-api/src/handlers/storage/mod.rs`（新建）

* `crates/libs/cmx-api/src/handlers/mod.rs`

* `crates/libs/cmx-api/src/routes/routes_impl.rs`

* `crates/web/web-server/src/main.rs`

***

## 三、文件变更清单

| #  | 文件                                                | 操作 | 说明                                          |
|----|---------------------------------------------------|----|---------------------------------------------|
| 1  | `crates/libs/cmx-infra/cmx-storage/src/config.rs` | 编辑 | `storage` → `instances` 字段重命名               |
| 2  | `dev.toml`                                        | 编辑 | 添加 `[storage]` + `[[storage.instances]]` 配置 |
| 3  | `crates/web/web-server/Cargo.toml`                | 编辑 | 添加 `cmx-storage` 依赖                         |
| 4  | `crates/libs/cmx-infra/cmx-storage/src/global.rs` | 新建 | `GlobalStorageService` 全局单例                 |
| 5  | `crates/libs/cmx-infra/cmx-storage/src/lib.rs`    | 编辑 | 导出 `global` 模块                              |
| 6  | `crates/web/web-server/src/config.rs`             | 编辑 | 添加 `init_storage()` 函数                      |
| 7  | `crates/web/web-server/src/main.rs`               | 编辑 | 调用 `init_storage()`                         |
| 8  | `crates/libs/cmx-api/Cargo.toml`                  | 编辑 | 添加 `cmx-storage` 依赖                         |
| 9  | `crates/libs/cmx-api/src/app_state.rs`            | 编辑 | 添加 `storage_service` 字段                     |
| 10 | `crates/libs/cmx-api/src/handlers/storage/mod.rs` | 新建 | storage handler 适配模块                        |
| 11 | `crates/libs/cmx-api/src/handlers/mod.rs`         | 编辑 | 添加 `pub mod storage`                        |
| 12 | `crates/libs/cmx-api/src/routes/routes_impl.rs`   | 编辑 | 注册 storage 路由                               |

***

## 四、配置结构对比

### TOML 配置（整合后）

```toml
[storage]
default_platform = "local-1"

[[storage.instances]]
platform = "local-1"
storage_type = "local"
enable_storage = true
domain = "http://localhost:8080/files/"
base_path = "uploads"
storage_path = "/data/cmx/storage"
enable_access = true
path_patterns = "**/*"

[[storage.instances]]
platform = "amazon-s3-1"
storage_type = "s3"
enable_storage = true
access_key = "jRszdym59WToFPfmvr7O"
secret_key = "6C5a6ZRWhdilLu7JLq43IbTST41rpT0XV1aanuIG"
region = "us-east-1"
endpoint = "http://192.168.254.204:9000/"
bucket_name = "gateway-core-data"
domain = "http://192.168.254.204:9001/"
base_path = "portalcenter/"
```

### 对应 Rust 结构

```rust
StorageManagerConfig {
default_platform: Some("local-1"),
instances: vec![
    StorageInstanceConfig {
        platform: "local-1",
        storage_type: Local,
        enable_storage: true,
        domain: Some("http://localhost:8080/files/"),
        base_path: "uploads",
        storage_path: Some("/data/cmx/storage"),
        enable_access: true,
        path_patterns: Some("**/*"),
        ..Default::default()  // S3 字段为 None
    },
    StorageInstanceConfig {
        platform: "amazon-s3-1",
        storage_type: S3,
        enable_storage: true,
        access_key: Some("jRszdym59WToFPfmvr7O"),
        secret_key: Some("6C5a6ZRWhdilLu7JLq43IbTST41rpT0XV1aanuIG"),
        region: Some("us-east-1"),
        endpoint: Some("http://192.168.254.204:9000/"),
        bucket_name: Some("gateway-core-data"),
        domain: Some("http://192.168.254.204:9001/"),
        base_path: "portalcenter/",
        ..Default::default()  // Local 字段为 None/false
    },
],
}
```

### Spring Boot x-file-storage 对照

| Spring Boot                                  | cmx-storage                            | 说明    |
|----------------------------------------------|----------------------------------------|-------|
| `x-file-storage.default-platform`            | `[storage].default_platform`           | 默认平台  |
| `x-file-storage.thumbnail-suffix`            | 预留                                     | 缩略图后缀 |
| `x-file-storage.huawei-obs[].platform`       | `[[storage.instances]].platform`       | 平台标识  |
| `x-file-storage.huawei-obs[].enable-storage` | `[[storage.instances]].enable_storage` | 启用开关  |
| `x-file-storage.huawei-obs[].access-key`     | `[[storage.instances]].access_key`     | 访问密钥  |
| `x-file-storage.huawei-obs[].secret-key`     | `[[storage.instances]].secret_key`     | 秘密密钥  |
| `x-file-storage.huawei-obs[].region`         | `[[storage.instances]].region`         | 区域    |
| `x-file-storage.huawei-obs[].end-point`      | `[[storage.instances]].endpoint`       | 端点    |
| `x-file-storage.huawei-obs[].bucket-name`    | `[[storage.instances]].bucket_name`    | 桶名    |
| `x-file-storage.huawei-obs[].domain`         | `[[storage.instances]].domain`         | 域名    |
| `x-file-storage.huawei-obs[].base-path`      | `[[storage.instances]].base_path`      | 基础路径  |

