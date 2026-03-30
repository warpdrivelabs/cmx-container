# cmx-api Plugin Handler 开发计划

## 需求概述

在 cmx-api 的 handlers 模块中新增 plugin 子模块，提供插件管理相关的 HTTP API，包括：
- 插件安装
- 插件卸载
- 插件升级
- 插件降级
- 已安装插件列表查询（支持分页）

这些 API 通过调用 cmx-plugin 模块的 `PluginManager` 实现具体操作。

---

## 需求细化

### 1. API 端点设计

| 操作 | HTTP 方法 | 路径 | 说明 |
|-----|---------|------|------|
| 安装插件 | POST | `/api/plugin/install` | Body: InstallRequest |
| 卸载插件 | POST | `/api/plugin/uninstall` | Body: UninstallRequest |
| 升级插件 | POST | `/api/plugin/upgrade` | Body: UpgradeRequest |
| 降级插件 | POST | `/api/plugin/downgrade` | Body: DowngradeRequest |
| 插件列表 | GET | `/api/plugin/list` | Query: PluginListQuery |
| 插件详情 | GET | `/api/plugin/{plugin_id}` | Path: plugin_id |
| 插件分页 | GET | `/api/plugin/page` | Query: PluginPageQuery |

### 2. 请求/响应结构

#### 2.1 安装请求 (InstallRequest)
```rust
struct InstallRequest {
    plugin_id: Option<String>,       // 插件ID（可选）
    source: PluginSource,             // 插件来源 (Local/Remote/Registry)
    target_db_id: Option<String>,   // 目标数据库ID
    target_db_type: Option<String>,
    target_nodes: Option<Vec<String>>,
    config: Option<serde_json::Value>,
    force: bool,                     // 是否强制安装
    skip_validation: bool,
    operator: String,               // 操作人
}
```

#### 2.2 卸载请求 (UninstallRequest)
```rust
struct UninstallRequest {
    plugin_id: String,   // 插件ID
    force: bool,         // 是否强制卸载
}
```

#### 2.3 升级请求 (UpgradeRequest)
```rust
struct UpgradeRequest {
    plugin_id: String,           // 插件ID
    source: PluginSource,        // 插件来源
    version_constraint: Option<String>,  // 版本约束
    force: bool,                 // 是否强制升级
    operator: String,            // 操作人
}
```

#### 2.4 降级请求 (DowngradeRequest)
```rust
struct DowngradeRequest {
    plugin_id: String,     // 插件ID
    target_version: String, // 目标版本
    operator: String,      // 操作人
}
```

#### 2.5 插件列表查询
```rust
struct PluginListQuery {
    pub status: Option<String>,       // 插件状态过滤
    pub domain_code: Option<String>,  // 域编码过滤
    pub application_code: Option<String>, // 应用编码过滤
}
```

#### 2.6 插件分页查询
```rust
struct PluginPageQuery {
    pub page: u64,        // 页码 (默认1)
    pub page_size: u64,   // 每页条数 (默认20)
    pub status: Option<String>,
    pub domain_code: Option<String>,
}
```

#### 2.7 统一响应
```rust
struct ApiResp<T> {
    code: u16,
    msg: String,
    data: Option<T>,
    pagination: Option<Pagination>,  // 分页信息
}
```

### 3. PluginSource 枚举（来自 cmx-plugin）
```rust
enum PluginSource {
    Local { path: PathBuf },
    Remote { url: String, checksum: Option<String> },
    Registry { registry_url: Option<String>, package_name: String },
}
```

---

## 开发任务

### 任务 1: 创建 plugin handler 模块结构

**文件**: `e:\rustspace\cmx\cmx-container\crates\libs\cmx-api\src\handlers\plugin\mod.rs`

创建 plugin 子模块，包含：
- `handler.rs` - HTTP Handler 实现
- `request.rs` - 请求结构体定义
- `response.rs` - 响应结构体定义
- `mod.rs` - 模块导出

### 任务 2: 定义请求结构体

**文件**: `e:\rustspace\cmx\cmx-container\crates\libs\cmx-api\src\handlers\plugin\request.rs`

需要定义：
- `PluginInstallRequest`
- `PluginUninstallRequest`
- `PluginUpgradeRequest`
- `PluginDowngradeRequest`
- `PluginListQuery`
- `PluginPageQuery`

### 任务 3: 定义响应结构体

**文件**: `e:\rustspace\cmx\cmx-container\crates\libs\cmx-api\src\handlers\plugin\response.rs`

需要定义：
- `PluginInfoResponse` - 插件信息响应
- `PluginListResponse` - 插件列表响应
- `InstallResponse`, `UninstallResponse`, `UpgradeResponse`, `DowngradeResponse`

### 任务 4: 实现 HTTP Handler

**文件**: `e:\rustspace\cmx\cmx-container\crates\libs\cmx-api\src\handlers\plugin\handler.rs`

实现以下处理函数：
- `plugin_install` - 安装插件
- `plugin_uninstall` - 卸载插件
- `plugin_upgrade` - 升级插件
- `plugin_downgrade` - 降级插件
- `plugin_list` - 插件列表
- `plugin_get` - 获取单个插件
- `plugin_page` - 分页查询

### 任务 5: 更新 handlers/mod.rs

**文件**: `e:\rustspace\cmx\cmx-container\crates\libs\cmx-api\src\handlers\mod.rs`

添加：
```rust
pub mod plugin;
```

### 任务 6: 在 routes 中注册插件路由

**文件**: `e:\rustspace\cmx\cmx-container\crates\libs\cmx-api\src\routes\routes.rs`

添加插件相关路由：
```rust
.plugin("/plugin", plugin_routes())
```

**文件**: `e:\rustspace\cmx\cmx-container\crates\libs\cmx-api\src\handlers\plugin\mod.rs`

添加路由创建函数：
```rust
pub fn plugin_routes() -> Router {
    Router::new()
        .route("/install", post(plugin_install))
        .route("/uninstall", post(plugin_uninstall))
        .route("/upgrade", post(plugin_upgrade))
        .route("/downgrade", post(plugin_downgrade))
        .route("/list", get(plugin_list))
        .route("/:plugin_id", get(plugin_get))
        .route("/page", get(plugin_page))
}
```

### 任务 7: 获取 PluginManager 实例

由于 cmx-api 是库，PluginManager 需要通过 State 或全局单例获取：

**方案 A**: 通过 web-server 传入 State
- 在 `CmxAppState` 中添加 `PluginManager`

**方案 B**: 使用全局单例（cmx-plugin 已有的 GlobalPluginManager）
- 使用 `cmx_plugin::GlobalPluginManager::get()`

推荐方案 B，因为 cmx-plugin 模块已经设计了全局单例。

### 任务 8: 编译检查

```bash
cargo check -p cmx-api
```

---

## 文件清单

### 新建文件

1. `e:\rustspace\cmx\cmx-container\crates\libs\cmx-api\src\handlers\plugin\mod.rs`
2. `e:\rustspace\cmx\cmx-container\crates\libs\cmx-api\src\handlers\plugin\handler.rs`
3. `e:\rustspace\cmx\cmx-container\crates\libs\cmx-api\src\handlers\plugin\request.rs`
4. `e:\rustspace\cmx\cmx-container\crates\libs\cmx-api\src\handlers\plugin\response.rs`

### 修改文件

1. `e:\rustspace\cmx\cmx-container\crates\libs\cmx-api\src\handlers\mod.rs`
2. `e:\rustspace\cmx\cmx-container\crates\libs\cmx-api\src\routes\routes.rs`
3. `e:\rustspace\cmx\cmx-container\crates\libs\cmx-api\src\handlers\plugin\mod.rs` (路由注册)

---

## 注意事项

1. **PluginSource 序列化**: `PluginSource` 是枚举，需要设计好 JSON 序列化格式（使用 `tag` 属性）
2. **错误处理**: 将 `PluginError` 转换为 `cmx_api::Error`
3. **操作人**: 从请求中获取操作人信息
4. **分页**: 使用 `Pagination` 结构
5. **并发安全**: PluginManager 内部已经是线程安全的
