# cmx-plugin-api

> cmx 平台插件域的 HTTP 皮肤层：插件本地运行时（安装/部署/升降级）、插件市场、模块迁移包导入导出与表元数据查询的薄 axum handler，委托 cmx-plugin 服务。

[![Version](https://img.shields.io/badge/version-0.1.12-blue.svg)]()
[![Edition](https://img.shields.io/badge/rust--edition-2024-orange.svg)]()
[![Authors](https://img.shields.io/badge/authors-skylake%40pansoft.com-lightgrey.svg)]()

---

## 项目简介

`cmx-plugin-api` 是插件域的 HTTP 适配层，覆盖四块能力：**插件生命周期管理**（部署 / 安装 / 卸载 / 升级 / 降级 / 查询 / 数据导入导出）、**插件市场**（发布 / 版本 / 评分 / 下载 / 分类 / 热门统计）、**模块迁移包**（导入 / 导出，依赖 cmx-plugin 的 ModuleInstallService / ModuleExportService）与**表元数据查询**（cmx_meta_table_define 表的只读查询）。

### 皮肤 vs 领域：职责分工

按 AGENTS.md 第八章规范，本 crate 是**纯 HTTP 适配层**：业务逻辑在 cmx-plugin（`GlobalPluginManager` / `MarketplaceService` / `TableMetadataService` / `ModuleInstallService` / `ModuleExportService` / `MarketplacePublisher` 等）。本 crate 只做三件事：

1. **参数提取**：`Json<T>` / `Multipart`（文件上传）/ `Query` / `Path`；
2. **DTO 转换**：把 HTTP 请求 DTO（`request.rs`）转为 cmx-plugin 领域请求（如 `PluginSourceRequest` → `cmx_plugin::domain::plugin::PluginSource`，`ApiPluginFilter` → `PluginFilter`），把领域结果转为 HTTP 响应 DTO（`response.rs`）；
3. **信封封装**：统一 `ApiResp<T>` 响应与 `#[utoipa::path]` OpenAPI 注解。

### 四个独立 Module 的拆分逻辑

| Module | 路由前缀 | 依赖的 cmx-plugin 服务 |
|--------|---------|----------------------|
| `PluginModule` | `/api/plugin` | `GlobalPluginManager`（install / uninstall / upgrade / downgrade / deploy / list / page / functions / exists / get）+ `ResourceDataImporter`（data/import） |
| `MarketplaceModule` | `/api/marketplace` | `MarketplaceService` + `MarketplaceRepository` / `StatsService`（每请求构造）+ `GlobalPluginManager`（install / upgrade 联动） |
| `ModulePackageModule` | `/api/module/package` | `ModuleInstallService` / `ModuleExportService` / `PackageUtils` |
| `TableMetadataModule` | `/api/table-metadata` | `TableMetadataService`（静态服务方法） |

`handlers/plugin/control` 目录（启停控制）当前注释未编译，保留待启用。

---

## 与其他 crate 的关系

### 上游依赖（本 crate 依赖）

| 依赖 | 用途 |
|------|------|
| `cmx-api-core` | `CmxAppState` / `CmxSvrContext` / `ApiResp` / `Error` / `Result` / `ModuleRoutes` / `get_db_id_from_header` |
| `cmx-api-types` | 响应信封与文档类型源头 |
| `cmx-plugin` | `GlobalPluginManager` / `InstallRequest` / `UninstallRequest` / `UpgradeRequest` / `DowngradeRequest` / `DeployRequest` / `PluginSource` / `PluginFilter` / `MarketplaceService` / `TableMetadataService` / `ModuleInstallService` / `ModuleExportService` / `MarketplacePublisher` / `DefinitionUtils` |
| `cmx-storage` | 部署链路的存储支撑（deploy 内部上传 OSS 转为 Storage 源） |
| `cmx-core`（openapi feature） | `ListParams` / `PageParams` / `DataSet` |
| `cmx-database` | `get_default_db_manager()`（marketplace 每请求构造 service、target_db_id 回退 `get_biz_db_id`） |
| `cmx-traits` | `ResourceDataCategory` / `ResourceDataImportRequest`（通用数据导入） |
| `cmx-utils` | `ConfigManager`（plugin.upload_root 等配置） |
| `axum` / `serde` / `serde_json` / `tracing` / `utoipa` / `tokio` / `chrono` / `modql` / `uuid` / `bytes` | 常规 Web / 序列化 / 异步 / 文档依赖 |

### 下游使用者（谁依赖本 crate）

| 使用方 | 引用方式 | 实际用途 |
|--------|---------|---------|
| `cmx-platform-app` | `cmx-plugin-api = { workspace = true }` | 平台总装配器：`routes()` 中 `.merge(PluginModule.routes())` / `.merge(TableMetadataModule.routes())` / `.merge(MarketplaceModule.routes())` / `.merge(ModulePackageModule.routes())`；`merged_openapi()` 中 `doc.merge(PluginApiDoc::openapi())` |
| `cmx-portalservice`（跨 workspace） | path 引用 `cmx-platform-app` | 门户微服务 bin，间接获得插件域全部 HTTP 端点 |
| `cmx-flowengine`（跨 workspace） | 不直接依赖 | 流程微服务独立 workspace，仅引 cmx-core / cmx-database-pg 等基础设施 |

---

## 核心功能与特性（路由端点分组）

### PluginModule（`/api/plugin`，本地运行时管理）

| 端点 | 方法 | 说明 |
|------|------|------|
| `/plugin/deploy` | POST | 部署插件（multipart 上传 zip，自动判断安装 / 升级 / 覆盖安装；可选发布市场） |
| `/plugin/install` | POST | 从 Local / Remote / Marketplace 来源安装 |
| `/plugin/uninstall` | POST | 卸载（移除运行时实例，保留元数据，支持 force） |
| `/plugin/upgrade` | POST | 升级到更高版本 |
| `/plugin/downgrade` | POST | 降级到指定版本 |
| `/plugin/list` | POST | 列表查询已安装插件（`ListParams<ApiPluginFilter>`） |
| `/plugin/page` | POST | 分页查询已安装插件（含运行时状态） |
| `/plugin/exists` | GET | 判断指定 plugin_code 是否已安装 |
| `/plugin/functions` | POST | 列出插件对外暴露的可调用函数清单 |
| `/plugin/{plugin_id}` | GET | 查询插件详情（元数据 + 运行时状态） |
| `/plugin/data/import` | POST | 通用插件数据导入（multipart，按 category 路由 Form/Menu/Perm/Flow，供远程模式调用） |
| `/plugin/data/list` | GET | 通用插件资源数据列表查询（定义列表 base64 传输） |

### MarketplaceModule（`/api/marketplace`，插件市场）

| 端点 | 方法 | 说明 |
|------|------|------|
| `/marketplace/plugin/page` / `get` | POST / GET | 市场插件分页 / 详情 |
| `/marketplace/plugin/publish` / `update` / `delete` | POST | 发布 / 更新 / 逻辑删除 |
| `/marketplace/plugin/version/list` / `version/get` | POST / GET | 版本列表 / 版本详情 |
| `/marketplace/plugin/install` / `upgrade` | POST | 从市场安装 / 升级（联动 GlobalPluginManager） |
| `/marketplace/plugin/check-updates` | POST | 检查插件更新 |
| `/marketplace/plugin/download` | GET | 下载插件包 |
| `/marketplace/plugin/rate` / `rating/list` | POST | 评分 / 评分列表 |
| `/marketplace/category/list` | POST | 分类统计 |
| `/marketplace/stats/trending/list` | POST | 热门插件 |

### ModulePackageModule（`/api/module/package`，迁移包）

| 端点 | 方法 | 说明 |
|------|------|------|
| `/module/package/import` | POST | 导入模块迁移包（解包并写入表 / DAM 资产 / 配置） |
| `/module/package/export` | GET | 导出模块迁移包（聚合表 / DAM 资产 / 配置为 zip 流下载） |

### TableMetadataModule（`/api/table-metadata`，表元数据只读查询）

| 端点 | 方法 | 说明 |
|------|------|------|
| `/table-metadata/get` / `get-by-name` / `exists` | GET | 按主键 / 按表名查询、判断是否已登记 |
| `/table-metadata/list` / `page` | POST | 列表 / 分页查询（modql 过滤） |

---

## 模块结构

```text
cmx-plugin-api
├── src
│   ├── lib.rs                                  # 模块导出（PluginApiDoc + 四个 Module）
│   ├── openapi.rs                              # PluginApiDoc：本域 OpenApi 切片
│   └── handlers
│       ├── mod.rs                              #   子模块声明（marketplace / module / plugin / table_metadata）
│       ├── plugin
│       │   ├── mod.rs                          #   PluginModule 路由（nest "/plugin"）+ plugin_routes() 便捷函数
│       │   ├── handler.rs                      #   部署 / 安装 / 卸载 / 升降级 / 列表 / 分页 / 详情 / functions / exists
│       │   ├── data_handler.rs                 #   通用数据导入 / 查询（ResourceDataImporter 桥接）
│       │   ├── request.rs                      #   请求 DTO（PluginInstallRequest / PluginSourceRequest / ApiPluginFilter...）
│       │   ├── response.rs                     #   响应 DTO（InstallResponse / UpgradeResponse / PluginDeployResponse...）
│       │   └── control/                        #   启停控制（注释未编译，保留待启用）
│       │       ├── mod.rs
│       │       ├── handler.rs / request.rs / response.rs
│       ├── marketplace
│       │   ├── mod.rs                          #   MarketplaceModule 路由（nest "/marketplace"）+ 端点表注释
│       │   ├── handler.rs                      #   市场插件 / 版本 / 安装升级 / 下载 / 评分 / 分类 / 热门全量 handler
│       │   ├── request.rs                      #   市场请求 DTO
│       │   └── response.rs                     #   市场响应 DTO
│       ├── module
│       │   ├── mod.rs                          #   ModulePackageModule 路由（import / export）
│       │   └── package_handler.rs              #   迁移包导入（ModuleInstallService）/ 导出（ModuleExportService）
│       └── table_metadata
│           ├── mod.rs                          #   TableMetadataModule 路由（nest "/table-metadata"）
│           └── handler.rs                      #   get / get-by-name / exists / list / page
└── Cargo.toml
```

---

## 关键类型 / API

### 模块路由注册器（lib.rs 顶层导出）

```rust
pub struct PluginModule;         // impl ModuleRoutes：nest "/plugin"；prefix() = "plugin"
pub struct MarketplaceModule;    // nest "/marketplace"；prefix() = "/marketplace"
pub struct ModulePackageModule;  // 路径内建 "/module/package/*"；prefix() = "module-package"
pub struct TableMetadataModule;  // nest "/table-metadata"；prefix() = "table-metadata"

// 另有自由函数：plugin_routes() -> Router<CmxAppState>（不含前缀的内部路由，供按需挂载）
```

### 关键转换函数（handler 层 DTO ↔ 领域类型）

```rust
// PluginSourceRequest（HTTP 三来源）→ cmx_plugin::domain::plugin::PluginSource
pub fn convert_source(req: &PluginSourceRequest)
    -> cmx_plugin::domain::plugin::PluginSource;

// ApiPluginFilter（HTTP 查询过滤）→ cmx_plugin::domain::plugin::PluginFilter（list/page 内转换）
```

### 典型 handler 签名

```rust
// 生命周期：全局单例 manager（无状态转换 + 请求 DTO）
pub async fn plugin_install(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    _headers: HeaderMap,
    Json(req): Json<PluginInstallRequest>,
) -> Result<Json<ApiResp<InstallResponse>>>;

// 部署：multipart 文件上传
pub async fn plugin_deploy(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    mut multipart: Multipart,
) -> Result<Json<ApiResp<PluginDeployResponse>>>;

// 通用数据导入：trait 对象注入（CmxAppState.resource_data_importer()）
pub async fn import_resource_data(
    State(cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    mut multipart: Multipart,
) -> Result<Json<ApiResp<ImportResultDto>>>;
```

---

## 使用示例

### 一、cmx-platform-app 合并插件域路由（组装场景）

```rust
use cmx_plugin_api::{
    MarketplaceModule, ModulePackageModule, PluginApiDoc, PluginModule, TableMetadataModule,
};
use utoipa::OpenApi;

// 路由合并：四个 Module 一次挂全
let router = Router::new()
    .merge(PluginModule.routes())
    .merge(TableMetadataModule.routes())
    .merge(MarketplaceModule.routes())
    .merge(ModulePackageModule.routes());

// OpenAPI 聚合
let mut doc = ApiDoc::openapi();
doc.merge(PluginApiDoc::openapi());
```

### 二、插件安装：HTTP DTO → 领域请求的完整转换链（摘自 handler.rs 模式）

```rust
use cmx_api_core::{ApiResp, CmxAppState, Result};
use cmx_database::get_default_db_manager;

pub async fn plugin_install(
    State(_cmx_state): State<CmxAppState>,
    Json(req): Json<PluginInstallRequest>,       // source + target_db_id + ...
) -> Result<Json<ApiResp<InstallResponse>>> {
    // 1. 取全局插件管理器（组装层启动时初始化的单例）
    let manager = cmx_plugin::GlobalPluginManager::get();
    let app_id = manager.app_id().to_string();

    // 2. target_db_id 缺省时回退业务库（get_biz_db_id）
    let target_db_id = match req.target_db_id {
        Some(id) => Some(id),
        None => Some(get_default_db_manager().get_biz_db_id().await),
    };

    // 3. HTTP DTO → 领域 InstallRequest（source 经 convert_source 三来源转换）
    let install_req = cmx_plugin::service::install::InstallRequest {
        source: convert_source(&req.source),
        db_id: target_db_id,
        version_constraint: None,
        build_type: None,
        marketplace_source_id: None,
        app_id: Some(app_id),
    };

    // 4. 委托领域服务执行，领域错误映射为 HTTP 错误
    let result = manager.install(install_req).await
        .map_err(|e| cmx_api_core::Error::InternalError(format!("插件安装失败: {}", e)))?;

    Ok(Json(ApiResp::ok(InstallResponse {
        plugin_id: result.plugin_id,
        install_path: result.install_path.to_string_lossy().to_string(),
        version: result.version,
        success: result.success,
        message: Some(result.message),
    })))
}
```

### 三、通用数据导入：trait 对象注入协作（远程模式桥接，摘自 data_handler.rs 模式）

```rust
use cmx_traits::resource::{ResourceDataCategory, ResourceDataImportRequest};

pub async fn import_resource_data(
    State(cmx_state): State<CmxAppState>,        // 组装层注入 Arc<dyn ResourceDataImporter>
    mut multipart: Multipart,                    // file + category + domain/application/module_code + ...
) -> Result<Json<ApiResp<ImportResultDto>>> {
    // 1. 取注入的导入器（未初始化 → 500）
    let importer = cmx_state.resource_data_importer()
        .ok_or_else(|| cmx_api_core::Error::InternalError("ResourceDataImporter 未初始化".into()))?;

    // 2. 逐字段解析 multipart（file 二进制 + 元数据文本字段）
    //    ...（省略字段收集，见源码）

    // 3. category 字符串 → 枚举（menu/perm/form/flow），无效值 400
    let category = ResourceDataCategory::parse_from_str(&category_str)
        .ok_or_else(|| cmx_api_core::Error::BadRequest(
            format!("无效的 category: {category_str}（有效值: menu/perm/form/flow）")))?;

    // 4. 构造领域请求并委托 trait 对象（按 category 路由到对应实现）
    let req = ResourceDataImportRequest { /* file_data, category, module 元数据... */ };
    let result = importer.import(req).await?;

    Ok(Json(ApiResp::ok(ImportResultDto {
        success: result.success,
        message: result.message,
        created_count: result.created_count,
        updated_count: result.updated_count,
        deleted_count: result.deleted_count,
    })))
}
```

---

## 设计要点

1. **Module CRUD 与迁移包分家**：`cmx-biz-api::ModuleCrudModule` 只依赖 cmx-biz（纯 CRUD），本 crate 的 `ModulePackageModule` 依赖 cmx-plugin（包导入导出）——拆分原因见两边 mod.rs 注释：避免 biz⇄plugin 循环依赖。
2. **deploy 的来源策略**：multipart zip 先落本地上传目录（`plugin.upload_root` 配置），source 统一用 `Local`，由 deploy 内部上传 OSS 并转为 Storage 源，确保集群同步；`publish_to_marketplace` 仅控制是否额外发布市场展示 + 版本记录。
3. **marketplace service 每请求构造**：`get_marketplace_service()` 用当前 `DatabaseManager` + 默认库 ID 现建 `MarketplaceService`（repo + stats_service），与插件运行时的全局单例 `GlobalPluginManager` 形成两种获取模式并存。
4. **数据导入双端点分工**：`/api/plugin/data/import` 是通用端点（category 四类路由）；`/api/iam/permissions/import` 是权限专用端点——远程模式（http_url/http_discovery）的定义导入器调前者。
