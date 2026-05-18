# 插件市场与现有插件系统集成方案

## 一、现状分析

### 1.1 现有插件系统（cmx-plugin）

**核心表结构**（4 张表）：

| 表名                        | 用途                  |
|---------------------------|---------------------|
| `cmx_plugin`              | 插件注册主表，存储已安装插件的核心信息 |
| `cmx_plugin_versions`     | 版本历史表，记录插件的版本演进     |
| `cmx_plugin_dependencies` | 依赖关系表               |
| `cmx_plugin_audit_log`    | 审计日志表               |

**核心服务链路**：

* `InstallService` → 安装插件（支持 Local/Remote/Marketplace/Storage 四种来源）

* `UpgradeService` → 升级插件（需要指定 plugin\_id + 新版本 source）

* `DowngradeService` → 降级插件

* `DeployService` → 智能部署（自动判断安装/升级/覆盖安装）

* `ActivateService` → 激活/停用插件

* `UninstallService` → 卸载插件

* `RollbackService` → 回滚插件

**关键特征**：

* 插件来源通过 `PluginSource` 枚举定义（Local/Remote/Marketplace/Storage）

* `cmx_plugin` 表中 `zip_source_type` 字段记录来源类型：local/url/registry/storage

* `cmx_plugin` 表中 `zip_source_url` 字段记录来源地址

* 安装后的插件信息（版本、路径等）全部写入 `cmx_plugin` 主表

* 版本历史记录在 `cmx_plugin_versions` 表

### 1.2 已实现的插件市场模块（marketplace/）

**核心表结构**（4 张表）：

| 表名                               | 用途                      |
|----------------------------------|-------------------------|
| `cmx_marketplace_plugin`         | 市场插件主表（分类、评分、下载统计等）     |
| `cmx_marketplace_plugin_version` | 市场版本表（存储文件ID、兼容性、变更日志等） |
| `cmx_marketplace_download_stats` | 下载统计表                   |
| `cmx_marketplace_rating`         | 评分表                     |

**已实现的服务**：

* `MarketplaceService` → 发布、查询、评分、统计

* `StatsService` → 下载统计、热门推荐

* `MarketplaceRepository` → 复杂 SQL 数据访问

**关键特征**：

* 市场是独立的"目录"（Catalog），记录可用的插件和版本

* 版本表中有 `storage_file_id` 字段，关联 cmx-storage 的文件

* 版本表中有 `download_url` 字段，现有 handler 已实现 `storage_file_id` 优先、`download_url` 降级的下载逻辑

* 市场和本地安装表之间**目前没有关联字段**

### 1.3 Registry → Marketplace 重命名

**现状**：`PluginSource::Registry` 在代码中有完整实现（API 层、服务层、Fetcher 层），但领域层注释 `/// 远程注册表，可以认为是插件市场？`
明确暗示 Registry 原本就是为"插件市场"预留的概念。

**重命名方案**：将 `Registry` 统一重命名为 `Marketplace`。

| 重命名项            | 原名                                                      | 新名                                                         |
|-----------------|---------------------------------------------------------|------------------------------------------------------------|
| PluginSource 变体 | `PluginSource::Registry`                                | `PluginSource::Marketplace`                                |
| SourceType 枚举   | `SourceType::Registry`                                  | `SourceType::Marketplace`                                  |
| Fetcher 类       | `RegistryFetcher`                                       | `MarketplaceFetcher`                                       |
| Fetcher 辅助类     | `RegistryInfo`                                          | `MarketplaceSourceInfo`                                    |
| Fetcher 辅助类     | `RegistryPackageDetail`                                 | `MarketplacePackageDetail`                                 |
| Fetcher 辅助类     | `RegistrySearchResult`                                  | `MarketplaceSearchResult`                                  |
| Fetcher 辅助类     | `RegistryPackageVersion`                                | `MarketplacePackageVersion`                                |
| 数据库值            | `zip_source_type = 'registry'`                          | `zip_source_type = 'marketplace'`                          |
| 领域层枚举           | `PluginSource::Registry { registry_url, package_name }` | `PluginSource::Marketplace { marketplace_url, plugin_id }` |

**双** **`PluginSource`** **枚举分别处理**：

代码库中存在两个不同的 `PluginSource` 枚举，需分别重命名：

| 位置                  | 原名                       | 字段                                                                                   | 新名                                                                                                             |
|---------------------|--------------------------|--------------------------------------------------------------------------------------|----------------------------------------------------------------------------------------------------------------|
| `domain/plugin.rs`  | `PluginSource::Registry` | `registry_url: Option<String>`, `package_name: String`                               | `PluginSource::Marketplace { marketplace_url: Option<String>, plugin_id: String }`                             |
| `fetcher/source.rs` | `PluginSource::Registry` | `registry_url: String`, `package_name: String`, `version_constraint: Option<String>` | `PluginSource::Marketplace { marketplace_url: String, plugin_id: String, version_constraint: Option<String> }` |

注意差异：

* 领域层的 `marketplace_url` 是 `Option<String>`，Fetcher 层是 `String`

* 领域层没有 `version_constraint`，Fetcher 层有

* `fetch_package()` 中需要将领域层枚举转换为 Fetcher 层枚举，`version_constraint` 需从
  `InstallRequest.version_constraint` 传入

**重命名影响范围**：

| 模块                                            | 影响文件                                       | 说明                             |
|-----------------------------------------------|--------------------------------------------|--------------------------------|
| fetcher/source.rs                             | 枚举定义、工厂方法、判断方法                             | 变体和字段重命名                       |
| fetcher/registry.rs → marketplace\_fetcher.rs | 整个文件重命名+内容重命名                              | Fetcher 实现                     |
| fetcher/mod.rs                                | `mod registry` → `mod marketplace_fetcher` | 模块声明                           |
| common/package.rs                             | match 分支                                   | Registry → Marketplace，含双枚举转换  |
| service/install.rs                            | `extract_source_info()`                    | `"registry"` → `"marketplace"` |
| service/upgrade.rs                            | `extract_source_info()`                    | `"registry"` → `"marketplace"` |
| service/initializer.rs                        | `build_plugin_source()`                    | `"registry"` → `"marketplace"` |
| service/auto\_install.rs                      | match 分支                                   | `"registry"` → `"marketplace"` |
| domain/plugin.rs                              | PluginSource 枚举                            | Registry → Marketplace         |
| lib.rs:109                                    | `FetcherPluginSource` 别名                   | 注释更新                           |
| lib.rs:112                                    | `RegistryFetcher` 等导出                      | 全部重命名                          |
| cmx-api/handlers/plugin/                      | handler.rs, request.rs, response.rs        | 请求/响应模型                        |

### 1.4 插件包下载流程分析

**当前已实现的下载路径**：

```
PluginSource::Storage { file_id }
    └─> StorageFetcher.fetch()
        └─> GlobalStorageService.get().service().download(file_id)
            └─> 从 cmx-storage 下载到本地临时目录

PluginSource::Remote { url, checksum }
    └─> RemoteFetcher.fetch()
        └─> HTTP GET url → 下载到本地临时目录

PluginSource::Local { path }
    └─> LocalFetcher.fetch()
        └─> 直接复制本地文件

PluginSource::Marketplace（原 Registry） { marketplace_url, plugin_id }
    └─> MarketplaceFetcher.fetch()
        └─> HTTP GET {marketplace_url}/api/marketplace/plugin/download?plugin_id=xxx&version=xxx
            → 下载到本地临时目录
```

**下载策略**：

* **优先使用** **`storage_file_id`**：一体化部署时直接通过 `StorageFetcher` → `GlobalStorageService.download()`，无需网络传输

* **降级使用** **`download_url`**：当 `storage_file_id` 不存在时，通过 `RemoteFetcher` → HTTP GET 下载（与现有 handler
  逻辑一致）

* 独立部署时：客户端通过下载 API（`GET /marketplace/plugin/download`）请求文件，该 API 内部通过 `storage_file_id` 调用
  cmx-storage 返回文件流

**市场版本表（`cmx_marketplace_plugin_version`）的下载相关字段**：

| 字段                | 类型           | 用途                                  |
|-------------------|--------------|-------------------------------------|
| `storage_file_id` | VARCHAR      | **优先下载标识**，关联 cmx-storage 文件        |
| `download_url`    | VARCHAR(512) | **降级下载路径**，`storage_file_id` 不存在时使用 |
| `checksum`        | VARCHAR(128) | SHA256 校验和，用于验证下载完整性                |

### 1.5 独立部署场景分析

**场景 A：一体化部署（当前模式）**

* 插件市场、cmx-storage、插件引擎在同一进程/集群

* 下载路径：`storage_file_id` → `StorageFetcher` → `GlobalStorageService.download()`

* 优点：无需网络传输，速度快

**场景 B：插件市场独立部署**

* 插件市场作为独立服务，对外提供 REST API

* 市场自身也引入 cmx-storage 作为文件存储后端

* 外部客户端通过以下方式获取插件：

    1. 调用市场的查询 API 获取插件列表和版本信息
    2. 调用市场的下载 API（`GET /marketplace/plugin/download`）获取插件 zip 包
    3. 下载 API 内部通过 `storage_file_id` 从 cmx-storage 获取文件并返回给客户端

**客户端安装流程（跨实例）**：

```
客户端（CMX 实例 B）                   插件市场（CMX 实例 A / 独立服务）
    │                                        │
    ├─ 1. POST /marketplace/plugin/install ─>│
    │     { plugin_id, version }             │
    │                                        ├─ 2. 查询版本信息（storage_file_id）
    │                                        ├─ 3. 构建 PluginSource::Marketplace
    │                                        │     { marketplace_url, plugin_id, version }
    │  <─ 返回安装结果 ────────────────────────┤
    │                                        │
    ├─ 4. MarketplaceFetcher.fetch() ───────>│
    │     GET /api/marketplace/plugin/        │
    │         download?plugin_id=xxx          │
    │     &version=1.0.0                      │
    │                                        ├─ 5. GlobalStorageService.download(file_id)
    │  <─ 文件流 ←────────────────────────────┤
    │                                        │
    └─ 6. 保存到本地临时目录，继续安装流程
```

### 1.6 核心矛盾

| 维度   | 现有插件系统                             | 插件市场                                     |
|------|------------------------------------|------------------------------------------|
| 定位   | 本地插件生命周期管理                         | 插件目录与分发中心                                |
| 安装方式 | 直接指定 PluginSource                  | 从市场选择版本安装                                |
| 版本管理 | `cmx_plugin_versions` 记录本地版本历史     | `cmx_marketplace_plugin_version` 记录可分发版本 |
| 来源追踪 | `zip_source_type`/`zip_source_url` | 无关联到 `cmx_plugin`                        |
| 升级触发 | 手动指定新版本 source                     | 应支持"检查市场更新"                              |
| 下载路径 | Storage/Remote/Local/Marketplace   | storage\_file\_id 优先，download\_url 降级    |

***

## 二、需求清单

### 需求 1：Registry 重命名为 Marketplace

**描述**：将 `PluginSource::Registry` 及其所有相关类型重命名为 `Marketplace`，使命名与业务概念一致。

### 需求 2：`zip_source_type` 值更新

**描述**：将 `zip_source_type` 的值从 `url` → `remote`、`registry` → `marketplace`，统一命名。不新增字段，直接修改现有字段值。
**向后兼容**：代码中 `build_plugin_source()` 和 `build_source()` 同时支持新旧值。

### 需求 3：从市场安装插件（install\_from\_marketplace）

**描述**：用户在插件市场浏览后，选择一个插件版本，一键安装到本地。
**向后兼容**：现有的 InstallService + PluginSource::Remote/Local/Storage 安装方式不受影响。

### 需求 4：从市场升级插件（upgrade\_from\_marketplace）

**描述**：已安装的插件，检测到市场有新版本时，从市场获取新版本并升级。
**向后兼容**：现有的手动指定 source 升级方式不受影响。

### 需求 5：安装后关联市场信息（marketplace linkage）

**描述**：从市场安装/升级的插件，在 `cmx_plugin` 表中记录其市场来源，便于后续查询"是否从市场安装"、"是否有市场更新"。
**向后兼容**：非市场安装的插件，市场关联字段为 NULL，不影响现有逻辑。

### 需求 6：检查插件更新（check\_updates）

**描述**：查询已安装的插件在市场中是否有新版本可用。使用 `SemanticVersion` 进行版本比较。
**向后兼容**：纯新增功能，不影响现有代码。

### 需求 7：本地插件发布到市场（publish\_to\_marketplace）

**描述**：已安装的本地插件可以发布到市场，供其他用户安装。
**向后兼容**：纯新增功能。

### 需求 8：市场统计与本地安装联动（stats\_integration）

**描述**：从市场安装成功后，更新市场的下载/安装统计。
**向后兼容**：纯新增功能，不影响现有安装流程的核心路径。

### 需求 9：插件包下载 API（独立部署支持）

**描述**：提供 `GET /marketplace/plugin/download` API，支持外部客户端通过 HTTP 下载插件 zip 包。API 内部通过
`storage_file_id` 调用 cmx-storage 获取文件。
**向后兼容**：纯新增功能。

***

## 三、方案设计

### 3.1 架构原则

1. **市场作为"来源层"（Source Layer）**：市场是插件的一个来源渠道，与 Local/Remote/Storage 并列，不替代现有安装机制
2. **安装流程复用**：市场安装最终调用 InstallService/UpgradeService/DeployService，不另起炉灶
3. **数据关联而非合并**：`cmx_plugin` 和 `cmx_marketplace_plugin` 保持独立，通过 `plugin_id` 逻辑关联
4. **向后兼容**：所有新增字段为可空（NULL），代码同时支持新旧值
5. **优先 cmx-storage，降级 download\_url**：`storage_file_id` 优先，不存在时降级到 `download_url`
6. **Registry 重命名为 Marketplace**：使代码命名与业务概念一致
7. **直接修改 zip\_source\_type 值**：不新增字段，修改现有字段值映射

### 3.2 数据库变更方案

#### 3.2.1 `cmx_plugin` 表新增字段（仅 1 个）

```sql
ALTER TABLE cmx_plugin ADD COLUMN marketplace_source_id VARCHAR(64);
COMMENT ON COLUMN cmx_plugin.marketplace_source_id IS '市场版本来源ID，关联 cmx_marketplace_plugin_version.id，非市场安装时为 NULL';
```

#### 3.2.2 `cmx_plugin_versions` 表新增字段（仅 1 个）

```sql
ALTER TABLE cmx_plugin_versions ADD COLUMN marketplace_source_id VARCHAR(64);
COMMENT ON COLUMN cmx_plugin_versions.marketplace_source_id IS '市场版本来源ID，关联 cmx_marketplace_plugin_version.id';
```

#### 3.2.3 `zip_source_type` 值迁移

直接修改现有 `zip_source_type` 字段值：

```sql
UPDATE cmx_plugin SET zip_source_type = 'marketplace' WHERE zip_source_type = 'registry';
UPDATE cmx_plugin SET zip_source_type = 'remote' WHERE zip_source_type = 'url';
```

代码中 `build_plugin_source()` 同时支持新旧值：

```rust
// initializer.rs
Some("url") | Some("remote") => { /* PluginSource::Remote */ }
Some("registry") | Some("marketplace") => { /* PluginSource::Marketplace */ }
```

```rust
// auto_install.rs
"url" | "remote" => { /* PluginSource::Remote */ }
"registry" | "marketplace" => { /* PluginSource::Marketplace */ }
```

#### 3.2.4 无需新增表

**设计决策**：不新增关联表。`cmx_plugin` 和 `cmx_marketplace_plugin` 通过 `plugin_id` 自然关联，`marketplace_source_id`
字段足以记录"从市场哪个版本安装"。

### 3.3 Rust 代码变更方案

#### 3.3.1 PluginSource::Registry → PluginSource::Marketplace 重命名

**fetcher/source.rs**：

```rust
pub enum PluginSource {
    Local { path: PathBuf },
    Remote { url: String, checksum: Option<String> },
    Marketplace {
        marketplace_url: String,
        plugin_id: String,
        version_constraint: Option<String>,
    },
    Storage { file_id: String, checksum: Option<String> },
}
```

**domain/plugin.rs**：

```rust
pub enum PluginSource {
    Local { path: PathBuf },
    Remote { url: String, checksum: Option<String> },
    Marketplace {
        marketplace_url: Option<String>,
        plugin_id: String,
    },
    Storage { file_id: String, checksum: Option<String> },
}
```

**fetcher/registry.rs → marketplace\_fetcher.rs**：

* `RegistryFetcher` → `MarketplaceFetcher`

* `RegistryInfo` → `MarketplaceSourceInfo`

* `RegistryPackageDetail` → `MarketplacePackageDetail`

* `RegistrySearchResult` → `MarketplaceSearchResult`

* `RegistryPackageVersion` → `MarketplacePackageVersion`

* `fetch_package()` 中 `domain::PluginSource::Marketplace` 转换为 `fetcher::PluginSource::Marketplace` 时，
  `version_constraint` 从 `InstallRequest` 传入

* 下载 URL 改为 `{marketplace_url}/api/marketplace/plugin/download?plugin_id=xxx&version=xxx`

#### 3.3.2 PluginRecord / PluginCreateParams 扩展

**infrastructure/database/plugin/model.rs**：

```rust
pub struct PluginRecord {
    // ... 现有字段 ...
    pub marketplace_source_id: Option<String>,
}

pub struct PluginCreateParams {
    // ... 现有字段 ...
    pub marketplace_source_id: Option<String>,
}

pub struct PluginUpdateParams {
    // ... 现有字段 ...
    pub marketplace_source_id: Option<String>,
}
```

**infrastructure/database/version\_history/model.rs**：

```rust
pub struct VersionRecord {
    // ... 现有字段 ...
    pub marketplace_source_id: Option<String>,
}

pub struct VersionCreateParams {
    // ... 现有字段 ...
    pub marketplace_source_id: Option<String>,
}
```

#### 3.3.3 InstallService 修改

**service/install.rs**：

1. `InstallRequest` 新增字段：

```rust
pub struct InstallRequest {
    // ... 现有字段 ...
    pub marketplace_source_id: Option<String>,
}
```

1. `extract_source_info()` 更新值映射：

```rust
// 旧值
PluginSource::Remote { .. } => (Some("url".to_string()), Some(url)),
PluginSource::Registry { .. } => (Some("registry".to_string()), ...),

// 新值
PluginSource::Remote { .. } => (Some("remote".to_string()), Some(url)),
PluginSource::Marketplace { .. } => (Some("marketplace".to_string()), ...),
```

1. `build_plugin_create_params()` 新增 `marketplace_source_id` 参数，在事务内写入

#### 3.3.4 UpgradeService 修改

**service/upgrade.rs**：

与 InstallService 相同的修改。

#### 3.3.5 `build_plugin_source()` 修复（initializer.rs + auto\_install.rs）

现有代码缺少 `storage` 类型处理，需要修复：

```rust
// initializer.rs
pub fn build_plugin_source(zip_source_url: Option<&str>, zip_source_type: Option<&str>) -> PluginSource {
    match zip_source_type {
        Some("local") => { PluginSource::Local { path: ... } }
        Some("url") | Some("remote") => { PluginSource::Remote { url: ..., checksum: None } }
        Some("registry") | Some("marketplace") => { PluginSource::Marketplace { ... } }
        // 新增：修复 storage 类型缺失
        Some("storage") => { PluginSource::Storage { file_id: zip_source_url.unwrap_or("").to_string(), checksum: None } }
        _ => { PluginSource::Local { path: ... } }
    }
}
```

```rust
// auto_install.rs - 同步修复
"storage" => { PluginSource::Storage { file_id: ..., checksum: None } }
```

#### 3.3.6 MarketplaceService 增强

**marketplace/service.rs**：

**依赖注入**：`MarketplaceService` 通过 `GlobalPluginManager::get()` 获取安装服务（与现有 handler 模式一致），而非通过构造函数注入。新增方法不接收
`InstallService` 参数。

```rust
impl MarketplaceService {
    pub async fn install_from_marketplace(
        &self,
        request: MarketInstallRequest,
    ) -> PluginResult<InstallResponse>;

    pub async fn upgrade_from_marketplace(
        &self,
        plugin_id: &str,
        target_version: Option<&str>,
    ) -> PluginResult<UpgradeResponse>;

    pub async fn check_updates(
        &self,
        installed_plugins: &[PluginRecord],
    ) -> PluginResult<Vec<PluginUpdateInfo>>;

    pub async fn publish_installed_plugin(
        &self,
        plugin_id: &str,
        plugin_record: &PluginRecord,
        version_info: PublishVersionInfo,
    ) -> PluginResult<MarketplacePlugin>;
}
```

**构建 PluginSource 的逻辑**（保留 download\_url 降级）：

```rust
fn build_plugin_source(version: &MarketplacePluginVersion) -> PluginResult<PluginSource> {
    if let Some(ref file_id) = version.storage_file_id {
        Ok(PluginSource::storage(file_id.clone(), version.checksum.clone()))
    } else if let Some(ref url) = version.download_url {
        Ok(PluginSource::remote(url.clone(), version.checksum.clone()))
    } else {
        Err(PluginError::Plugin(format!(
            "市场版本 '{}' 缺少 storage_file_id 和 download_url，无法下载",
            version.version
        )))
    }
}
```

#### 3.3.7 `is_latest` 标志管理

**marketplace/service.rs** 的 `publish_plugin()` 创建新版本时，需先将同 `plugin_id` 的其他版本的 `is_latest` 重置为 0：

```rust
// 在创建新版本前
self.repo.reset_is_latest(plugin_id).await?;
// 然后创建新版本 with is_latest = 1
```

**marketplace/repository.rs** 新增方法：

```rust
pub async fn reset_is_latest(&self, plugin_id: &str) -> PluginResult<()>;
```

#### 3.3.8 ZIP 打包功能

**common/package.rs** 新增 `create_zip` 方法（或使用 `cmx_utils::zip` 模块扩展）：

```rust
pub fn create_zip(source_dir: &Path, target_path: &Path) -> PluginResult<()>;
```

此功能用于 `publish_installed_plugin` 将已安装插件目录打包上传到市场。

#### 3.3.9 新增数据模型

**marketplace/model.rs**：

```rust
pub struct PluginUpdateInfo {
    pub plugin_id: String,
    pub plugin_name: String,
    pub current_version: String,
    pub latest_version: String,
    pub latest_version_info: MarketplacePluginVersion,
    pub has_update: bool,
}

pub struct PublishVersionInfo {
    pub storage_file_id: Option<String>,
    pub package_size: Option<i64>,
    pub checksum: Option<String>,
    pub changelog: Option<String>,
    pub release_notes: Option<String>,
}
```

#### 3.3.10 MarketplaceRepository 新增方法

```rust
impl MarketplaceRepository {
    pub async fn get_latest_versions_batch(
        &self,
        plugin_ids: &[String],
    ) -> PluginResult<HashMap<String, MarketplacePluginVersion>>;
}
```

SQL 设计：

```sql
SELECT DISTINCT ON (plugin_id) *
FROM cmx_marketplace_plugin_version
WHERE plugin_id = ANY($1) AND status = 'published' AND archived = 0
ORDER BY plugin_id, version_rank DESC
```

#### 3.3.11 PluginRepository SQL 更新

所有涉及 `cmx_plugin` 表的 SQL 需要新增 `marketplace_source_id` 字段：

| 操作     | 位置                                                      | 说明                           |
|--------|---------------------------------------------------------|------------------------------|
| INSERT | `plugin/repository.rs` `upsert_plugin()`                | 新增 `marketplace_source_id` 列 |
| SELECT | `plugin/repository.rs` `get_plugin()`, `list_plugins()` | 新增返回字段                       |
| UPDATE | `plugin/repository.rs` `update_plugin()`                | 新增字段（COALESCE 模式，`None` 不覆盖） |

版本历史表同理：

| 操作     | 位置                              | 说明                           |
|--------|---------------------------------|------------------------------|
| INSERT | `version_history/repository.rs` | 新增 `marketplace_source_id` 列 |
| SELECT | `version_history/repository.rs` | 新增返回字段                       |

#### 3.3.12 `extract_source_info()` 提取到公共模块

**service/utils.rs**：

将 `install.rs` 和 `upgrade.rs` 中重复的 `extract_source_info()` 函数提取到 `service/utils.rs` 公共模块。

### 3.4 API 变更方案

#### 3.4.1 新增 API 接口

| 方法   | 路径                                  | 说明                                         |
|------|-------------------------------------|--------------------------------------------|
| POST | `/marketplace/plugin/install`       | 从市场安装（现有 handler 重构为调用 MarketplaceService） |
| POST | `/marketplace/plugin/upgrade`       | 从市场升级（新增）                                  |
| POST | `/marketplace/plugin/check-updates` | 检查更新（新增）                                   |
| POST | `/marketplace/plugin/publish`       | 发布到市场（现有 handler 增强）                       |
| GET  | `/marketplace/plugin/download`      | 下载插件包（新增，独立部署时使用）                          |

#### 3.4.2 下载 API 设计

**`GET /marketplace/plugin/download`**：

用于外部客户端下载插件 zip 包。

```
GET /api/marketplace/plugin/download?plugin_id=xxx&version=1.0.0
    │
    ├─ 1. 查询 cmx_marketplace_plugin_version 获取版本信息
    ├─ 2. 获取 storage_file_id
    ├─ 3. 通过 GlobalStorageService.download(storage_file_id) 获取文件
    ├─ 4. 以 Streaming Body 返回文件流
    │     Content-Type: application/octet-stream
    │     Content-Disposition: attachment; filename="plugin-xxx-1.0.0.zip"
    └─ 5. 记录下载统计
```

**请求参数**：

| 参数         | 类型     | 必填 | 说明          |
|------------|--------|----|-------------|
| plugin\_id | String | 是  | 插件业务 ID     |
| version    | String | 否  | 版本号，默认最新稳定版 |

**错误响应**：

* 版本不存在：404

* `storage_file_id` 为空：503（版本文件暂不可用）

**安全设计**：

| 安全项  | 设计                       |
|------|--------------------------|
| 鉴权   | 复用现有 API 鉴权机制（JWT Token） |
| 频率限制 | 复用现有 API 限流中间件           |
| 文件大小 | 使用 Streaming Body，不加载到内存 |
| 断点续传 | 暂不支持，后续版本考虑              |

#### 3.4.3 现有 marketplace install handler 重构

**cmx-api/handlers/marketplace/handler.rs** 中 `marketplace_plugin_install` 已有完整实现。重构为：

* handler 仅负责参数解析和响应格式化

* 业务逻辑（查询版本 → 构建 PluginSource → 调用安装 → 更新关联 → 记录统计）下沉到
  `MarketplaceService.install_from_marketplace()`

* 保留现有的 `storage_file_id` 优先、`download_url` 降级逻辑

#### 3.4.4 现有 API 兼容性

所有现有 Plugin API 保持不变。新增接口使用新路由路径。

***

## 四、关键场景流程

### 4.1 从市场安装插件（一体化部署）

```
用户操作：在市场界面点击"安装"
    │
    ▼
POST /marketplace/plugin/install
    │ { plugin_id: "xxx", version: "1.2.0", db_id: null, auto_activate: true }
    │
    ▼
MarketplaceService.install_from_marketplace()
    │
    ├─ 1. 查询 cmx_marketplace_plugin WHERE plugin_id = 'xxx'
    ├─ 2. 查询 cmx_marketplace_plugin_version WHERE plugin_id = 'xxx' AND version = '1.2.0'
    │     → 获取 storage_file_id, download_url, checksum, 兼容性信息
    ├─ 3. 校验兼容性（min_platform_version / max_platform_version）
    ├─ 4. 构建 PluginSource
    │     → 优先 PluginSource::Storage { file_id: storage_file_id }
    │     → 降级 PluginSource::Remote { url: download_url }
    ├─ 5. 通过 GlobalPluginManager.get().install() 调用 InstallService
    │     → InstallRequest 中携带 marketplace_source_id
    │     → 复用现有完整安装流程（事务内写入 marketplace_source_id）
    ├─ 6. 安装成功后：
    │     ├─ 记录下载统计 → cmx_marketplace_download_stats
    │     └─ 增加安装量 → cmx_marketplace_plugin.install_count + 1
    └─ 7. 返回安装结果
```

### 4.2 从市场安装插件（跨实例/独立部署）

```
客户端 CMX 实例 B                       远程插件市场服务
    │                                        │
    ├─ POST /marketplace/plugin/install ────>│
    │  { plugin_id, version }                │
    │                                        ├─ 查询版本信息
    │                                        ├─ 构建 PluginSource::Marketplace
    │                                        │     { marketplace_url, plugin_id, version }
    │                                        ├─ InstallService.install()
    │                                        │     └─ MarketplaceFetcher.fetch()
    │                                        │         └─ GET {marketplace_url}/api/marketplace/
    │                                        │             plugin/download?plugin_id=xxx&version=1.0.0
    │  <────── 文件流（cmx-storage 代理）─────┤
    │                                        │
    ├─ 保存到本地临时目录                     │
    ├─ 继续安装流程（解析、校验、注册）        │
    └─ 安装完成
```

### 4.3 检查插件更新

```
POST /marketplace/plugin/check-updates
    │
    ▼
MarketplaceService.check_updates()
    │
    ├─ 1. 获取已安装插件列表
    ├─ 2. 批量查询市场最新版本（get_latest_versions_batch）
    │     → SQL: SELECT DISTINCT ON (plugin_id) * FROM cmx_marketplace_plugin_version
    │       WHERE plugin_id = ANY($1) AND status = 'published' AND archived = 0
    │       ORDER BY plugin_id, version_rank DESC
    ├─ 3. 使用 SemanticVersion::parse() 逐个比较版本号
    └─ 4. 返回更新列表
```

### 4.4 现有安装流程不受影响

```
POST /plugin/install 或 POST /plugin/deploy
    │
    ▼
InstallService.install() / DeployService.deploy()
    │
    ├─ 正常安装流程不变
    └─ marketplace_source_id = NULL
```

***

## 五、向后兼容性保障

### 5.1 数据库层面

| 变更                                                 | 兼容性影响        | 应对措施           |
|----------------------------------------------------|--------------|----------------|
| `cmx_plugin` 新增 `marketplace_source_id` 列          | NULL，不影响现有数据 | 默认 NULL        |
| `cmx_plugin_versions` 新增 `marketplace_source_id` 列 | NULL，不影响现有数据 | 默认 NULL        |
| `zip_source_type` 值从 `url` → `remote`              | 现有数据需迁移      | 迁移脚本 + 代码兼容新旧值 |
| `zip_source_type` 值从 `registry` → `marketplace`    | 现有数据需迁移      | 迁移脚本 + 代码兼容新旧值 |
| 不新增外键约束                                            | 无影响          | 应用层关联          |

### 5.2 代码层面

| 变更                                      | 兼容性影响                              | 应对措施                          |
|-----------------------------------------|------------------------------------|-------------------------------|
| Registry → Marketplace 重命名              | API 请求中 `registry` → `marketplace` | 前端同步更新                        |
| `PluginRecord` 新增 Option 字段             | 不影响反序列化                            | 默认 None                       |
| `zip_source_type` 值更新                   | 旧数据仍可正常读取                          | `build_plugin_source()` 兼容新旧值 |
| `MarketplaceService` 新增方法               | 纯新增                                | 不影响现有方法                       |
| `build_plugin_source()` 新增 `storage` 分支 | 修复现有 bug                           | 不影响其他分支                       |

### 5.3 API 层面

| 变更                                | 兼容性影响              |
|-----------------------------------|--------------------|
| 现有 Plugin API 不修改                 | 无影响                |
| 现有 Marketplace install handler 重构 | 内部重构，API 接口不变      |
| 新增 5 个 API 接口                     | 纯新增，不影响现有          |
| 现有 API 返回 JSON 新增 null 字段         | 前端通常忽略 null 字段，无影响 |

***

## 六、实施步骤（按优先级排序）

### 第一阶段：Registry → Marketplace 重命名 + zip\_source\_type 值更新

1. **重命名 PluginSource::Registry → PluginSource::Marketplace**

    * `fetcher/source.rs`、`domain/plugin.rs` 枚举定义

    * `fetcher/registry.rs` → `fetcher/marketplace_fetcher.rs`

    * `fetcher/mod.rs` 模块声明

    * `lib.rs` 导出更新

2. **更新所有引用**

    * `common/package.rs` match 分支（含双枚举转换逻辑）

    * `service/install.rs`、`service/upgrade.rs` `extract_source_info()` → 值改为 `remote`/`marketplace`

    * `service/initializer.rs` `build_plugin_source()` → 兼容新旧值 + 新增 `storage` 分支

    * `service/auto_install.rs` → 兼容新旧值 + 新增 `storage` 分支

    * `cmx-api/handlers/plugin/` handler、request、response

3. **数据迁移脚本**

    * `UPDATE cmx_plugin SET zip_source_type = 'marketplace' WHERE zip_source_type = 'registry'`

    * `UPDATE cmx_plugin SET zip_source_type = 'remote' WHERE zip_source_type = 'url'`

4. **编译验证**：`rtk cargo check`

### 第二阶段：数据层打通

1. **数据库迁移脚本**

    * `cmx_plugin` 新增 `marketplace_source_id` 字段

    * `cmx_plugin_versions` 新增 `marketplace_source_id` 字段

2. **更新 Rust 数据模型**

    * `PluginRecord`、`PluginCreateParams`、`PluginUpdateParams` 新增 `marketplace_source_id`

    * `VersionRecord`、`VersionCreateParams` 新增 `marketplace_source_id`

    * `InstallRequest` 新增 `marketplace_source_id`

    * `build_plugin_create_params()` 新增 `marketplace_source_id` 参数

    * `PluginRepository` 的 INSERT/SELECT/UPDATE SQL 新增字段

    * `VersionHistoryRepository` 的 INSERT/SELECT SQL 新增字段

3. **提取** **`extract_source_info()`** **到** **`service/utils.rs`**

### 第三阶段：从市场安装

1. **实现** **`install_from_marketplace`**

    * `MarketplaceService` 新增方法

    * `build_plugin_source()` 辅助方法（storage\_file\_id 优先，download\_url 降级）

    * 通过 `GlobalPluginManager::get().install()` 调用安装

2. **重构现有 marketplace install handler**

    * handler 改为调用 `MarketplaceService.install_from_marketplace()`

    * 保留 `storage_file_id` 优先、`download_url` 降级逻辑

3. **实现下载 API handler**

    * `GET /marketplace/plugin/download` — 通过 storage\_file\_id 调用 cmx-storage 返回文件流

### 第四阶段：升级与更新检测

1. **实现** **`upgrade_from_marketplace`**

2. **实现** **`check_updates`**

    * `MarketplaceRepository.get_latest_versions_batch()`

    * 使用 `SemanticVersion` 比较版本

3. **实现** **`is_latest`** **标志管理**

    * `MarketplaceRepository.reset_is_latest()`

    * 在 `publish_plugin()` 创建新版本前调用

4. **新增 API handler**

    * `POST /marketplace/plugin/upgrade`

    * `POST /marketplace/plugin/check-updates`

### 第五阶段：发布到市场

1. **实现 ZIP 打包功能**

    * `PackageUtils::create_zip()` 或 `cmx_utils::zip` 模块扩展

2. **实现** **`publish_installed_plugin`**

    * 打包插件目录 → 上传 cmx-storage → 获取 storage\_file\_id → 创建市场记录

3. **增强 publish API handler**

### 第六阶段：集成测试

1. **编写测试**

    * 从市场安装（一体化）→ 验证 storage\_file\_id → StorageFetcher

    * 从市场安装（降级）→ 验证 download\_url → RemoteFetcher

    * 从市场安装（跨实例）→ 验证 MarketplaceFetcher → 下载 API

    * 从市场升级 → 验证版本历史

    * 检查更新 → 验证 SemanticVersion 比较

    * 非市场安装 → 验证向后兼容

    * 发布到市场 → 验证 is\_latest 管理和 storage\_file\_id 关联

    * `build_plugin_source()` → 验证 `storage` 分支和旧值兼容

    * 下载 API → 验证文件流返回

***

## 七、风险与注意事项

### 7.1 数据一致性

* **风险**：从市场安装成功但统计更新失败

* **应对**：统计更新失败不影响安装结果，记录错误日志并异步重试

### 7.2 版本冲突

* **风险**：市场版本与本地已安装版本冲突

* **应对**：安装/升级前严格进行版本校验，使用 `SemanticVersion` 比较

### 7.3 并发安全

* **风险**：同时从市场安装和手动安装同一个插件

* **应对**：复用现有的 `LockManager` 机制，按 plugin\_id 加锁

### 7.4 市场不可用时的降级

* **风险**：市场服务不可用时，从市场安装/升级失败

* **应对**：返回明确错误信息，不影响现有的本地/远程安装方式

### 7.5 Registry 重命名的迁移

* **风险**：数据库中已有 `zip_source_type = 'registry'` 的记录

* **应对**：数据迁移脚本 + 代码兼容新旧值（`"registry" | "marketplace"`）

### 7.6 大文件下载性能

* **风险**：插件包较大时，下载 API 的内存占用高

* **应对**：使用 Streaming Body（axum 的 `StreamBody`），不将整个文件加载到内存。需确认 `cmx-storage` 的 download 接口支持流式返回

### 7.7 跨实例安装的错误处理

* **风险**：跨实例下载超时、网络中断

* **应对**：配置合理的超时时间（当前 `MarketplaceFetcher` 硬编码 60 秒），部分下载清理由 `TempDirCleanup` 处理

***

## 八、总结

本方案的核心思路是：

1. **统一通过 cmx-storage 下载**：`storage_file_id` 优先，`download_url` 降级。独立部署时通过下载 API 代理 cmx-storage
2. **Registry 重命名为 Marketplace**：使代码命名与业务概念一致
3. **直接修改 zip\_source\_type 值**：不新增冗余字段，`url` → `remote`，`registry` → `marketplace`，代码兼容新旧值
4. **市场是来源，不是替代**：插件市场作为插件的一个新来源渠道，与 Local/Remote/Storage 并列
5. **最小改动原则**：`cmx_plugin` 表仅新增 1 个可空字段（`marketplace_source_id`），不修改现有核心安装流程
6. **桥接层设计**：`MarketplaceService` 作为市场与安装系统的桥接层，通过 `GlobalPluginManager` 获取安装服务
7. **修复现有 bug**：`build_plugin_source()` 缺少 `storage` 类型处理、`is_latest` 标志管理缺失
8. **完全向后兼容**：所有变更对现有功能和数据无侵入性影响

