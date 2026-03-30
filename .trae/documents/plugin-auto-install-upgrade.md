# 插件自动安装/升级 API 开发方案

## 一、功能设计说明

### 1.1 需求概述

新增一个统一的插件部署 API 接口（`/api/plugin/deploy`），该接口支持 **上传 zip 文件**，服务端将文件保存到项目根目录的 `uploads` 文件夹中，然后构建 `PluginSource::Local` 源自动判断操作类型（安装或升级）并执行相应流程。同时支持 `force_reinstall`（覆盖安装）参数，当已安装版本与待安装版本相同时，先执行卸载再重新安装。

### 1.2 核心设计思路

```
HTTP 请求(multipart: zip文件 + JSON参数) → 保存zip到 ./uploads/ → 构建 PluginSource::Local
    → deploy API → 获取插件元数据(版本号) → 查询当前安装状态
        ├─ 未安装 → 调用 install 流程
        ├─ 已安装 且 新版本 > 旧版本 → 调用 upgrade 流程
        ├─ 已安装 且 新版本 = 旧版本 且 force_reinstall=true → 先 uninstall 再 install
        ├─ 已安装 且 新版本 = 旧版本 且 force_reinstall=false → 返回提示"已安装相同版本"
        └─ 已安装 且 新版本 < 旧版本 → 返回错误提示（降级需使用专门的降级接口）
```

### 1.3 文件上传处理

* 使用 `axum::extract::Multipart` 处理 multipart/form-data 请求

* zip 文件保存到项目根目录下的 `./uploads/plugins/` 文件夹

* 文件名使用 UUID 重命名（避免冲突）：`{uuid}.zip`

* 保存后构建 `PluginSource::Local { path: PathBuf }` 传入 deploy 逻辑

* 需要在 workspace 的 axum 依赖中添加 `"multipart"` feature

### 1.4 与现有接口的关系

| 接口                    | 路径     | Content-Type        | 定位                      |
| --------------------- | ------ | ------------------- | ----------------------- |
| `/api/plugin/install` | 安装     | application/json    | 保留，用于指定本地路径/远程URL/注册表来源 |
| `/api/plugin/upgrade` | 升级     | application/json    | 保留，用于指定本地路径/远程URL/注册表来源 |
| `/api/plugin/deploy`  | 部署（新增） | multipart/form-data | **上传zip文件 + 智能判断**      |

`deploy` 接口是 `install` 和 `upgrade` 的上层封装，内部复用现有的 `InstallService` 和 `UpgradeService`，不修改现有服务的核心逻辑。

***

## 二、接口定义规范

### 2.1 API 路由

* **路径**: `POST /api/plugin/deploy`

* **Content-Type**: `multipart/form-data`

### 2.2 请求格式

使用 `multipart/form-data` 提交，包含以下字段：

| 字段名               | 类型           | 必填 | 说明              |
| ----------------- | ------------ | -- | --------------- |
| `file`            | binary (zip) | 是  | 插件 zip 包文件      |
| `target_db_id`    | string       | 否  | 目标数据库ID         |
| `force_reinstall` | boolean      | 否  | 是否覆盖安装，默认 false |

> 注意：不使用 JSON 请求体，因为需要上传二进制文件。参数通过 multipart form fields 传递。

### 2.3 API 层请求参数解析

在 [handler.rs](file:///e:/rustspace/cmx/cmx-container/crates/libs/cmx-api/src/handlers/plugin/handler.rs) 中，Handler 直接使用 `axum::extract::Multipart` 接收请求，无需定义额外的请求结构体。

### 2.4 响应结构体 (`PluginDeployResponse`)

在 [response.rs](file:///e:/rustspace/cmx/cmx-container/crates/libs/cmx-api/src/handlers/plugin/response.rs) 中新增：

```rust
/// 插件部署响应
#[derive(Debug, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PluginDeployResponse {
    /// 插件ID
    pub plugin_id: String,
    /// 操作类型: "install" | "upgrade" | "reinstall" | "already_installed"
    pub action: String,
    /// 旧版本（仅 upgrade/reinstall 时有值）
    pub old_version: Option<String>,
    /// 新版本
    pub new_version: String,
    /// 安装路径
    pub install_path: String,
    /// 是否成功
    pub success: bool,
    /// 消息
    pub message: Option<String>,
}
```

### 2.5 utoipa 文档注解

由于 multipart 上传的 utoipa 支持有限，`#[utoipa::path]` 注解中不指定 `request_body`（或使用描述性文本），手动在 tag 注释中说明请求格式：

```rust
#[utoipa::path(
    post,
    path = "/api/plugin/deploy",
    responses(
        (status = 200, description = "部署成功", body = ApiResp<PluginDeployResponse>),
        (status = 400, description = "请求参数错误"),
        (status = 500, description = "部署失败")
    ),
    tag = "Plugin"
)]
```

***

## 三、实现逻辑流程

### 3.1 整体流程（Handler 层）

在 [handler.rs](file:///e:/rustspace/cmx/cmx-container/crates/libs/cmx-api/src/handlers/plugin/handler.rs) 中新增 `plugin_deploy` 函数：

```
1. 使用 axum::extract::Multipart 接收请求
2. 遍历 multipart fields:
   a. file field → 读取字节 → 保存到 ./uploads/plugins/{uuid}.zip
   b. target_db_id field → 解析为 Option<String>
   c. force_reinstall field → 解析为 bool（默认 false）
3. 构建 PluginSource::Local { path: 保存的zip路径 }
4. 调用 PluginManager.deploy(DeployRequest { source, db_id, force_reinstall })
5. 将 DeployResponse 转换为 PluginDeployResponse 返回
```

### 3.2 PluginManager.deploy() 流程

在 [manager.rs](file:///e:/rustspace/cmx/cmx-container/crates/libs/cmx-plugin/src/core/manager.rs) 中新增 `deploy()` 方法：

```
1. 通过 PackageUtils 获取并解压插件包到临时目录
2. 通过 SecurityValidator 做安全验证
3. 通过 DefinitionUtils 解析元数据（plugin_id, version）
4. 查询 repository 获取当前安装状态和版本
5. 版本比较 + 分发:
   a. 未安装 → 调用 InstallService.install()
   b. 新版本 > 旧版本 → 调用 UpgradeService.upgrade()
   c. 新版本 = 旧版本 && force_reinstall → 先 UninstallService.uninstall() 再 InstallService.install()
   d. 新版本 = 旧版本 && !force_reinstall → 返回 AlreadyInstalled
   e. 新版本 < 旧版本 → 返回错误，提示使用降级接口
6. 统一封装为 DeployResponse 返回（包含 install_path）
```

### 3.3 新增 DeployService（service 层）

新增 [deploy.rs](file:///e:/rustspace/cmx/cmx-container/crates/libs/cmx-plugin/src/service/deploy.rs) 文件：

```
DeployService:
  - 复用 PackageUtils 获取/解压插件包
  - 复用 SecurityValidator 做安全验证
  - 复用 DefinitionUtils 解析元数据
  - 查询 repository 获取当前安装状态和版本
  - 调用 InstallService / UpgradeService / UninstallService 完成实际操作
```

### 3.4 DeployRequest / DeployResponse 定义（service 层）

```rust
/// 部署请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployRequest {
    /// 插件来源（Local 模式，指向上传的 zip 文件路径）
    pub source: PluginSource,
    /// 目标数据库ID（可选）
    pub db_id: Option<String>,
    /// 是否覆盖安装
    pub force_reinstall: bool,
}

/// 部署响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployResponse {
    /// 插件ID
    pub plugin_id: String,
    /// 操作类型
    pub action: DeployAction,
    /// 旧版本（仅 upgrade/reinstall 时有值）
    pub old_version: Option<String>,
    /// 新版本
    pub new_version: String,
    /// 安装路径
    pub install_path: PathBuf,
    /// 是否成功
    pub success: bool,
    /// 消息
    pub message: String,
}

/// 部署操作类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DeployAction {
    /// 全新安装
    Install,
    /// 升级安装
    Upgrade,
    /// 覆盖安装（卸载后重新安装）
    Reinstall,
    /// 已安装相同版本，无需操作
    AlreadyInstalled,
}
```

***

## 四、与现有模块的集成方式

### 4.1 依赖关系图

```
API Handler (plugin_deploy)
    ├─> Multipart 解析: 保存 zip 到 ./uploads/plugins/{uuid}.zip
    └─> PluginManager.deploy()
            └─> DeployService.deploy()
                    ├─> PackageUtils.fetch_package()        # 获取插件包（本地路径）
                    ├─> PackageUtils.prepare_package()       # 解压到临时目录
                    ├─> SecurityValidator.validate()         # 安全验证
                    ├─> DefinitionUtils.parse_definition()   # 解析元数据
                    ├─> PluginRepository.find_plugin()       # 查询安装状态
                    ├─> InstallService.install()             # 安装
                    ├─> UpgradeService.upgrade()             # 升级
                    └─> UninstallService.uninstall()         # 卸载（覆盖安装时）
```

### 4.2 不修改现有模块

* `InstallService`：不修改，保持现有 13 步安装流程

* `UpgradeService`：不修改，保持现有 16 步升级流程

* `UninstallService`：不修改，保持现有卸载流程

* `PluginSource`：不修改，复用现有定义

* `PluginManager`：仅新增 `deploy()` 方法，不修改现有方法

***

## 五、错误处理机制

### 5.1 错误场景

| 场景               | 处理方式                 | HTTP 状态码 |
| ---------------- | -------------------- | -------- |
| 未上传文件            | 返回错误，提示需要上传 zip 文件   | 400      |
| 文件保存失败           | 返回错误，含具体原因           | 500      |
| 插件包获取/解压失败       | 返回错误，含具体原因           | 500      |
| 安全验证失败           | 返回错误，含验证详情           | 400      |
| 元数据解析失败          | 返回错误                 | 400      |
| 降级场景（新版本 < 旧版本）  | 返回错误，提示使用降级接口        | 400      |
| 安装失败             | 透传 InstallService 错误 | 500      |
| 升级失败             | 透传 UpgradeService 错误 | 500      |
| 覆盖安装时卸载失败        | 返回错误，不继续安装           | 500      |
| 覆盖安装时卸载成功但重新安装失败 | 返回错误（注意：插件已被卸载）      | 500      |

### 5.2 覆盖安装的原子性考虑

覆盖安装（uninstall + install）不是原子操作。如果卸载成功但安装失败，插件将处于未安装状态。这种情况下：

* 响应中明确告知用户安装失败

* 日志中记录完整的操作链路

* 用户可以重新调用 deploy 接口进行安装

### 5.3 错误类型

复用现有 `PluginError` 枚举，新增 `Deploy` 变体：

```rust
PluginError::Deploy(String)  // 部署操作相关错误
```

***

## 六、实施步骤

### 步骤 1：启用 axum multipart feature

**文件**: `Cargo.toml`（workspace 根目录）

将 `axum` 的 features 从 `["macros"]` 改为 `["macros", "multipart"]`

### 步骤 2：新增 `DeployService`（service 层）

**文件**: `crates/libs/cmx-plugin/src/service/deploy.rs`（新建）

* 定义 `DeployRequest`：包含 `source: PluginSource`, `db_id: Option<String>`, `force_reinstall: bool`

* 定义 `DeployResponse`：包含 `plugin_id`, `action: DeployAction`, `old_version`, `new_version`, `install_path`, `success`, `message`

* 定义 `DeployAction` 枚举：`Install`, `Upgrade`, `Reinstall`, `AlreadyInstalled`

* 定义 `DeployServiceDeps` 依赖结构体（复用 InstallServiceDeps 类似的依赖）

* 实现 `DeployService::deploy()` 方法

### 步骤 3：注册 service 模块

**文件**: `crates/libs/cmx-plugin/src/service/mod.rs`

* 添加 `pub mod deploy;`

### 步骤 4：在 `PluginManager` 中新增 `deploy()` 方法

**文件**: `crates/libs/cmx-plugin/src/core/manager.rs`

* 新增 `deploy_service` 字段

* 在 `from_builder()` 中初始化 `DeployService`

* 新增 `pub async fn deploy(&self, request: DeployRequest) -> PluginResult<DeployResponse>` 方法

* 导出 `DeployRequest`, `DeployResponse`, `DeployAction`

### 步骤 5：导出类型

**文件**: `crates/libs/cmx-plugin/src/lib.rs`

* 导出 `service::deploy::{DeployRequest, DeployResponse, DeployAction}`

### 步骤 6：新增 API 层响应结构体

**文件**: `crates/libs/cmx-api/src/handlers/plugin/response.rs`

* 新增 `PluginDeployResponse` 结构体（含 `install_path` 字段）

### 步骤 7：实现 Handler

**文件**: `crates/libs/cmx-api/src/handlers/plugin/handler.rs`

* 新增 `plugin_deploy` handler 函数

* 使用 `axum::extract::Multipart` 接收请求

* 处理文件上传：保存 zip 到 `./uploads/plugins/{uuid}.zip`

* 解析 form fields（`target_db_id`, `force_reinstall`）

* 构建 `PluginSource::Local` 并调用 `manager.deploy()`

* 将 `DeployResponse` 转换为 `PluginDeployResponse` 返回

* 添加 `#[utoipa::path]` 文档注解

### 步骤 8：注册路由

**文件**: `crates/libs/cmx-api/src/handlers/plugin/mod.rs`

* 在 `plugin_routes()` 中新增 `.route("/deploy", post(plugin_deploy))`

* 在 `pub use handler::` 中导出 `plugin_deploy`

### 步骤 9：编译验证

* 运行 `cargo check` 确保编译通过

* 运行 `cargo clippy` 检查代码质量

***

## 七、单元测试策略

### 7.1 DeployService 单元测试

| 测试场景                                        | 预期结果                                      |
| ------------------------------------------- | ----------------------------------------- |
| 插件未安装 → deploy                              | 调用 install，返回 action=Install              |
| 新版本 > 旧版本 → deploy                          | 调用 upgrade，返回 action=Upgrade              |
| 新版本 = 旧版本 + force\_reinstall=true → deploy  | 先 uninstall 再 install，返回 action=Reinstall |
| 新版本 = 旧版本 + force\_reinstall=false → deploy | 返回 action=AlreadyInstalled                |
| 新版本 < 旧版本 → deploy                          | 返回错误                                      |
| 插件包获取失败 → deploy                            | 返回错误                                      |
| 安全验证失败 → deploy                             | 返回错误                                      |

### 7.2 Handler 测试

* 使用 axum 的测试工具发送 multipart 请求

* 验证 zip 文件正确保存到 uploads 目录

* 验证请求参数正确解析

* 验证响应格式符合预期（包含 install\_path）

***

## 八、涉及文件清单

| 文件                                                    | 操作     | 说明                                                 |
| ----------------------------------------------------- | ------ | -------------------------------------------------- |
| `Cargo.toml`                                          | 修改     | axum features 添加 `"multipart"`                     |
| `crates/libs/cmx-plugin/src/service/deploy.rs`        | **新建** | DeployService 核心实现                                 |
| `crates/libs/cmx-plugin/src/service/mod.rs`           | 修改     | 添加 `pub mod deploy;`                               |
| `crates/libs/cmx-plugin/src/core/manager.rs`          | 修改     | 新增 `deploy_service` 字段和 `deploy()` 方法              |
| `crates/libs/cmx-plugin/src/lib.rs`                   | 修改     | 导出 `DeployRequest`/`DeployResponse`/`DeployAction` |
| `crates/libs/cmx-plugin/src/error.rs`                 | 修改     | 新增 `PluginError::Deploy` 变体                        |
| `crates/libs/cmx-api/src/handlers/plugin/response.rs` | 修改     | 新增 `PluginDeployResponse`                          |
| `crates/libs/cmx-api/src/handlers/plugin/handler.rs`  | 修改     | 新增 `plugin_deploy` handler（含文件上传处理）                |
| `crates/libs/cmx-api/src/handlers/plugin/mod.rs`      | 修改     | 注册路由并导出                                            |

