# Deploy 端点改造：集成插件市场发布

## 一、需求概述

改造 `POST /api/plugin/deploy` 端点，新增 `publish_to_marketplace` 参数（默认 `true`），
在部署插件时**先发布到插件市场，再执行安装/升级操作**。需向后兼容（不传参数时默认发布）。

核心难点：

1. **执行顺序**：先发布到市场 → 再执行安装/升级，确保市场记录先于本地部署
2. **判断市场新增 vs 版本升级**：市场已有该 plugin_id → 版本升级；不存在 → 新增
3. **文件上传到 cmx-storage**：deploy 只保存到本地，发布需额外上传到 cmx-storage
4. **元数据映射**：从 `PluginDefinition`（ZIP 解析）映射到 `MarketplacePluginForCreate` +
   `MarketplacePluginVersionForCreate`
5. **逻辑隔离**：发布逻辑放到独立源码文件中

## 二、当前架构

```
plugin_deploy (POST /api/plugin/deploy)
  ├── 上传 zip → 保存到本地 plugins/uploads/{uuid}.zip
  ├── 构建 PluginSource::Local
  └── PluginManager.deploy() → 自动判断 install/upgrade/reinstall

marketplace_plugin_publish (POST /api/marketplace/plugin/publish)
  ├── 上传 zip → cmx-storage（获取 file_id, url, size, hash）
  ├── 构建 MarketplacePluginForCreate + MarketplacePluginVersionForCreate
  └── MarketplaceService.publish_plugin() → 写入市场数据库
```

两者完全独立，互不调用。

## 三、改造方案

### 3.1 改造后的流程

```
plugin_deploy (POST /api/plugin/deploy)
  ├── 1. 上传 zip → 保存到本地 plugins/uploads/{uuid}.zip
  ├── 2. 解压 + 安全验证 + 解析 PluginDefinition
  ├── 3. [如果 publish_to_marketplace=true]
  │     ├── 3a. 读取 ZIP 字节 → 上传到 cmx-storage
  │     ├── 3b. 构建 MarketplacePluginForCreate + MarketplacePluginVersionForCreate
  │     └── 3c. 调用 MarketplaceService.publish_plugin() → 写入市场数据库
  │           └── 获取 marketplace_version_id（用于后续 marketplace_source_id 关联）
  ├── 4. 查询本地安装状态，判断 install/upgrade/reinstall
  ├── 5. 执行安装/升级/覆盖安装
  │     └── marketplace_source_id = 发布步骤返回的 marketplace_version_id
  └── 6. 返回 DeployResponse（含市场发布信息）
```

**关键变化**：发布在安装/升级**之前**执行，这样：

- 安装/升级时可以拿到 `marketplace_version_id`，写入 `marketplace_source_id` 字段
- 如果发布失败，整个 deploy 失败（因为市场是源头，发布不成功就不应该安装）
- 发布成功后安装失败，市场记录已存在（可接受，下次重试安装即可）

### 3.2 新增文件：`src/service/marketplace_publisher.rs`

创建独立的 `MarketplacePublisher` 服务，封装"从 ZIP 包发布到市场"的完整逻辑。

```rust
// crates/libs/cmx-plugin/src/service/marketplace_publisher.rs

/// 市场发布请求（从 ZIP 包发布）
pub struct PublishFromDeployRequest {
    pub plugin_id: String,
    pub version: String,
    pub plugin_def: PluginDefinition,
    pub zip_file_path: PathBuf,       // 本地 ZIP 包路径
}

/// 市场发布结果
pub struct PublishFromDeployResult {
    pub marketplace_plugin_id: String,  // 市场插件记录 ID
    pub marketplace_version_id: String, // 市场版本记录 ID（用于 marketplace_source_id）
    pub storage_file_id: String,        // cmx-storage 文件 ID
    pub is_new_plugin: bool,            // 是否为市场新增（vs 版本升级）
}

pub struct MarketplacePublisher { ... }

impl MarketplacePublisher {
    /// 从 ZIP 包发布到市场（在安装/升级之前调用）
    pub async fn publish_from_deploy(&self, req: &PublishFromDeployRequest) -> PluginResult<PublishFromDeployResult> {
        // 1. 读取 ZIP 文件字节
        // 2. 上传到 cmx-storage
        // 3. 从 PluginDefinition 构建 MarketplacePluginForCreate
        // 4. 从 PluginDefinition + cmx-storage 返回信息构建 MarketplacePluginVersionForCreate
        // 5. 调用 MarketplaceService.publish_plugin()
        // 6. 返回 PublishFromDeployResult（含 marketplace_version_id）
    }
}
```

### 3.3 修改 `DeployRequest`

```rust
// crates/libs/cmx-plugin/src/service/deploy.rs

pub struct DeployRequest {
    pub source: PluginSource,
    pub db_id: Option<String>,
    pub force_reinstall: bool,
    pub build_type: Option<String>,
    pub publish_to_marketplace: bool,  // 新增，默认 true
}
```

### 3.4 修改 `DeployResponse`

```rust
pub struct DeployResponse {
    pub plugin_id: String,
    pub action: DeployAction,
    pub old_version: Option<String>,
    pub new_version: String,
    pub install_path: PathBuf,
    pub success: bool,
    pub message: String,
    pub marketplace_publish: Option<MarketplacePublishInfo>,  // 新增
}

/// 市场发布信息
pub struct MarketplacePublishInfo {
    pub marketplace_plugin_id: String,
    pub marketplace_version_id: String,
    pub storage_file_id: String,
    pub is_new_plugin: bool,
}
```

### 3.5 修改 `DeployService.deploy()`

**核心改造**：将 deploy 流程拆分为"发布 → 安装/升级"两阶段。

```
原流程：
  解析 ZIP → 查询安装状态 → 安装/升级/覆盖

新流程：
  解析 ZIP → [发布到市场] → 查询安装状态 → 安装/升级/覆盖（含 marketplace_source_id）
```

关键改动点：

1. 在步骤4（解析元数据）之后、步骤5（查询安装状态）之前，插入发布逻辑
2. 如果 `publish_to_marketplace == true`，调用 `MarketplacePublisher::publish_from_deploy()`
3. 发布成功后，将返回的 `marketplace_version_id` 传递给后续的安装/升级请求
4. **发布失败则整个 deploy 失败**（市场是源头，发布不成功不应继续安装）
5. `publish_to_marketplace == false` 时，跳过发布步骤，行为与原来一致

### 3.6 修改 API 层

#### 3.6.1 Handler (`plugin/handler.rs`)

在 `plugin_deploy` 的 multipart 解析中新增 `publish_to_marketplace` 字段：

- 从 multipart 中读取，类型为 `Option<String>`
- 默认值为 `true`（向后兼容：不传时默认发布）

#### 3.6.2 Request (`plugin/request.rs`)

`PluginDeployRequest` 新增字段：

```rust
pub publish_to_marketplace: Option<bool>,
```

#### 3.6.3 Response (`plugin/response.rs`)

`PluginDeployResponse` 新增字段：

```rust
pub marketplace_publish: Option<MarketplacePublishInfoResponse>,
```

## 四、详细实现步骤

### 步骤 1：创建 `marketplace_publisher.rs`

**文件**: `crates/libs/cmx-plugin/src/service/marketplace_publisher.rs`

1. 定义 `PublishFromDeployRequest`、`PublishFromDeployResult`、`MarketplacePublishInfo` 结构体
2. 定义 `MarketplacePublisher` struct
3. 实现 `publish_from_deploy()` 方法：
    - 读取 ZIP 文件字节
    - 上传到 cmx-storage（`GlobalStorageService`）
    - 从 `PluginDefinition` 构建 `MarketplacePluginForCreate`（映射字段）
    - 从 `PluginDefinition` + cmx-storage 返回信息构建 `MarketplacePluginVersionForCreate`
    - 调用 `MarketplaceService.publish_plugin()`
    - 从返回的 `MarketplacePlugin` 中提取版本 ID（需查询最新版本获取）
    - 返回 `PublishFromDeployResult`

### 步骤 2：修改 `deploy.rs`

1. `DeployRequest` 新增 `publish_to_marketplace: bool`
2. `DeployResponse` 新增 `marketplace_publish: Option<MarketplacePublishInfo>`
3. `DeployService.deploy()` 方法改造：
    - 在解析元数据（步骤4）之后、查询安装状态（步骤5）之前，插入发布阶段
    - 如果 `publish_to_marketplace == true`，调用 `MarketplacePublisher::publish_from_deploy()`
    - 发布失败 → 返回错误，终止 deploy
    - 发布成功 → 保存 `marketplace_version_id`，传递给后续安装/升级请求
4. `execute_install()`、`execute_upgrade()`、`execute_reinstall()` 方法修改：
    - 新增 `marketplace_source_id: Option<&str>` 参数
    - 传递给 `InstallRequest` / `UpgradeRequest` 的 `marketplace_source_id` 字段

### 步骤 3：修改 `mod.rs`

在 `crates/libs/cmx-plugin/src/service/mod.rs` 中添加 `pub mod marketplace_publisher;`

### 步骤 4：修改 API 层

1. `plugin/request.rs`：`PluginDeployRequest` 新增 `publish_to_marketplace: Option<bool>`
2. `plugin/response.rs`：`PluginDeployResponse` 新增 `marketplace_publish` 字段 + `MarketplacePublishInfoResponse` 结构体
3. `plugin/handler.rs`：
    - multipart 解析新增 `publish_to_marketplace` 字段读取
    - 构建 `DeployRequest` 时传入 `publish_to_marketplace`
    - 映射 `DeployResponse.marketplace_publish` 到响应

### 步骤 5：修改所有 `DeployRequest` 构建处

搜索所有构建 `DeployRequest` 的地方，补充 `publish_to_marketplace` 字段：

- `cmx-api/src/handlers/plugin/handler.rs` — `plugin_deploy`：使用前端传入值
- 其他可能的调用点（`auto_install.rs`、`initializer.rs` 等）— 设为 `false`

### 步骤 6：编译验证

`cargo check` 确保编译通过。

## 五、字段映射表

### PluginDefinition → MarketplacePluginForCreate

| PluginDefinition 字段 | MarketplacePluginForCreate 字段 | 说明                |
|---------------------|-------------------------------|-------------------|
| `id`                | `plugin_id`                   | 业务 ID             |
| `name`              | `name`                        | 显示名称              |
| `description`       | `description`                 | 描述                |
| `vendor_name`       | `vendor_name`                 | 供应商               |
| `vendor_url`        | `vendor_url`                  | 供应商网址             |
| `vendor_contact`    | `vendor_contact`              | 供应商联系方式           |
| `domain_code`       | `domain_code`                 | 域编码               |
| `application_code`  | `application_code`            | 应用编码              |
| `module_code`       | `module_code`                 | 模块编码              |
| `r#type`            | `plugin_type`                 | 插件类型              |
| -                   | `status`                      | 硬编码 `"published"` |
| -                   | `is_featured`                 | `None`            |
| -                   | `is_official`                 | `None`            |
| -                   | `short_description`           | `None`            |
| -                   | `icon_url`                    | `None`            |
| -                   | `category`                    | `None`            |
| -                   | `tags`                        | `None`            |
| -                   | `license_type`                | `None`            |
| -                   | `homepage_url`                | `None`            |
| -                   | `documentation_url`           | `None`            |
| -                   | `repository_url`              | `None`            |

### PluginDefinition + cmx-storage → MarketplacePluginVersionForCreate

| 来源                      | MarketplacePluginVersionForCreate 字段 | 说明            |
|-------------------------|--------------------------------------|---------------|
| `id`                    | `plugin_id`                          | 业务 ID         |
| `version`               | `version`                            | 版本号           |
| -                       | `version_rank`                       | `Some(0)`     |
| -                       | `changelog`                          | `None`        |
| -                       | `release_notes`                      | `None`        |
| cmx-storage `url`       | `download_url`                       | 下载地址          |
| cmx-storage `id`        | `storage_file_id`                    | 存储文件 ID       |
| cmx-storage `size`      | `package_size`                       | 包大小           |
| cmx-storage `hash_info` | `checksum`                           | SHA256 校验     |
| -                       | `min_platform_version`               | `None`        |
| -                       | `max_platform_version`               | `None`        |
| -                       | `dependencies`                       | `None`        |
| -                       | `compatibility`                      | `None`        |
| -                       | `status`                             | `"published"` |
| -                       | `is_latest`                          | `Some(1)`     |
| -                       | `is_stable`                          | `Some(1)`     |

## 六、向后兼容性

| 场景                                   | 行为                    |
|--------------------------------------|-----------------------|
| 旧客户端不传 `publish_to_marketplace`      | 默认 `true`，先发布到市场再部署   |
| 新客户端传 `publish_to_marketplace=false` | 仅部署，不发布到市场（行为与改造前一致）  |
| 新客户端传 `publish_to_marketplace=true`  | 先发布到市场，再部署            |
| 市场发布失败                               | 整个 deploy 失败，不执行安装/升级 |

## 七、风险与注意事项

1. **执行顺序**：发布在安装之前，发布失败则整个 deploy 失败。这是设计决策——市场是插件来源的权威记录
2. **ZIP 文件生命周期**：deploy 流程中 `package_path` 指向原始 ZIP 文件，需确保在发布完成前不被清理
3. **cmx-storage 依赖**：`MarketplacePublisher` 需要访问 `GlobalStorageService`，这是 cmx-storage 的全局单例
4. **事务边界**：发布和安装是两个独立事务。发布成功后安装失败，市场记录已存在（可接受，下次重试安装即可）
5. **重复发布**：`MarketplaceService.publish_plugin()` 内部已有 upsert 逻辑（存在则更新+新版本），天然幂等
6. **版本冲突**：如果市场已有相同版本号，`GenericCrudService::create` 会插入新记录（需确认是否有唯一约束）
7. **marketplace_source_id 传递**：发布成功后获取的 `marketplace_version_id` 需要传递给安装/升级请求，确保 `cmx_plugin` 和
   `cmx_plugin_versions` 表正确记录来源
