# cmx-plugin API 参考文档

## 一、概述

插件操作 API 挂载在 `/api/plugin/` 路由前缀下，提供插件的安装、卸载、升级、降级、部署及查询等完整生命周期管理能力。

每项操作执行完整流程：**DDL/DML 数据库操作 + 文件系统安装 + 运行时内存注册 + 审计日志 + 事件发布（GlobalEventBus + Redis 跨实例通知）**。

---

## 二、API 接口一览

| 路由 | 方法 | 功能 |
|------|------|------|
| `/api/plugin/deploy` | POST (multipart) | 智能部署（自动判断安装/升级/覆盖安装） |
| `/api/plugin/install` | POST (JSON) | 安装插件 |
| `/api/plugin/upgrade` | POST (JSON) | 升级插件 |
| `/api/plugin/downgrade` | POST (JSON) | 降级插件 |
| `/api/plugin/uninstall` | POST (JSON) | 卸载插件 |
| `/api/plugin/list` | POST (JSON) | 列表查询 |
| `/api/plugin/page` | POST (JSON) | 分页查询 |
| `/api/plugin/{plugin_id}` | GET | 详情查询 |
| `/api/plugin/exists` | GET | 存在性查询 |
| `/api/plugin/functions` | POST (JSON) | 批量函数查询 |

---

## 三、部署接口 `POST /api/plugin/deploy`（核心接口）

最复杂也最常用的接口，通过 multipart/form-data 上传插件 ZIP 包，系统自动判断操作类型。

### 请求字段

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `file` | File | 是 | 插件 ZIP 包 |
| `target_db_id` | String | 否 | 目标数据库 ID |
| `force_reinstall` | String | 否 | 是否覆盖安装，默认 false（`"true"` 或 `"1"`） |
| `build_type` | String | 否 | 构建类型 debug/release |
| `publish_to_marketplace` | String | 否 | 是否发布到插件市场 |

### 内部流程

```
上传 ZIP → 保存到本地 uploads 目录 → [可选] 发布到市场 → 构建 PluginSource →
DeployService.deploy() →
  1. 获取并解压插件包
  2. 安全验证（SecurityValidator）
  3. 解析元数据（plugin_id, version）
  4. 查询当前安装状态
  5. 版本比较决策：
     - 未安装 → Install
     - 新版本 > 旧版本 → Upgrade
     - 新版本 = 旧版本 && force_reinstall → Reinstall（先卸载再安装）
     - 新版本 = 旧版本 && !force_reinstall → AlreadyInstalled
     - 新版本 < 旧版本 → 错误（提示使用降级接口）
```

### 响应体 `PluginDeployResponse`

| 字段 | 类型 | 说明 |
|------|------|------|
| `pluginId` | String | 插件 ID |
| `action` | String | `"install"` / `"upgrade"` / `"reinstall"` / `"already_installed"` |
| `oldVersion` | Option\<String\> | 旧版本（升级/覆盖时有值） |
| `newVersion` | String | 新版本 |
| `installPath` | String | 安装路径 |
| `success` | bool | 是否成功 |
| `message` | Option\<String\> | 消息 |
| `marketplacePublish` | Option\<...\> | 市场发布信息（仅 publish_to_marketplace=true 时） |

---

## 四、安装接口 `POST /api/plugin/install`

从指定来源安装插件。

### 请求体 `PluginInstallRequest`

```json
{
  "source": { "type": "local", "path": "/path/to/plugin.wasm" },
  "target_db_id": "optional-db-id"
}
```

**PluginSourceRequest 枚举**（tag = "type"，小写）：

| 类型 | 字段 | 说明 |
|------|------|------|
| `local` | `path: String` | 本地文件路径 |
| `remote` | `url: String`, `checksum?: String` | 远程 URL |
| `marketplace` | `marketplace_url: String`, `plugin_id: String` | 插件市场 |

### 响应体 `InstallResponse`

| 字段 | 类型 | 说明 |
|------|------|------|
| `pluginId` | String | 插件 ID |
| `installPath` | String | 安装路径 |
| `version` | String | 插件版本 |
| `success` | bool | 是否成功 |
| `message` | Option\<String\> | 消息 |

---

## 五、卸载接口 `POST /api/plugin/uninstall`

### 请求体 `PluginUninstallRequest`

```json
{
  "plugin_id": "my-plugin",
  "force": false
}
```

### 响应体 `UninstallResponse`

| 字段 | 类型 | 说明 |
|------|------|------|
| `pluginId` | String | 插件 ID |
| `success` | bool | 是否成功 |
| `message` | Option\<String\> | 消息 |

---

## 六、升级接口 `POST /api/plugin/upgrade`

### 请求体 `PluginUpgradeRequest`

```json
{
  "plugin_id": "my-plugin",
  "source": { "type": "remote", "url": "https://example.com/plugin-v2.wasm" },
  "version_constraint": null,
  "force": false,
  "operator": "admin"
}
```

### 响应体 `UpgradeResponse`

| 字段 | 类型 | 说明 |
|------|------|------|
| `pluginId` | String | 插件 ID |
| `oldVersion` | String | 旧版本 |
| `newVersion` | String | 新版本 |
| `success` | bool | 是否成功 |
| `message` | Option\<String\> | 消息 |

---

## 七、降级接口 `POST /api/plugin/downgrade`

降级只是切换版本目录，不涉及文件拷贝。

### 请求体 `PluginDowngradeRequest`

```json
{
  "plugin_id": "my-plugin",
  "target_version": "1.0.0",
  "operator": "admin"
}
```

### 响应体 `DowngradeResponse`

| 字段 | 类型 | 说明 |
|------|------|------|
| `pluginId` | String | 插件 ID |
| `oldVersion` | String | 旧版本 |
| `targetVersion` | String | 目标版本 |
| `success` | bool | 是否成功 |
| `message` | Option\<String\> | 消息 |

---

## 八、查询接口

### 8.1 列表查询 `POST /api/plugin/list`

请求体：`ListParams<ApiPluginFilter>`，支持按 status/name/domain_code/application_code/module_code/app_id 过滤。

响应体：`PluginListResponse { plugins: Vec<PluginInfoResponse> }`

### 8.2 分页查询 `POST /api/plugin/page`

请求体：`PageParams<ApiPluginFilter>`

响应体：`ApiResp<Vec<PluginInfoResponse>>`（带分页信息）

### 8.3 详情查询 `GET /api/plugin/{plugin_id}`

响应体：`PluginInfoResponse`

### 8.4 存在性查询 `GET /api/plugin/exists?plugin_id=xxx`

响应体：`"1"` 存在，`"0"` 不存在

### 8.5 批量函数查询 `POST /api/plugin/functions`

请求体：`{ "plugin_ids": ["plugin-a", "plugin-b"] }`

响应体：`HashMap<String, PluginFunctionsResponse>`，返回每个插件的 `api.json` 内容

---

## 九、调用链路

```
API Handler (cmx-api)
  → GlobalPluginManager (全局单例)
    → InstallService / DeployService / UpgradeService / DowngradeService / UninstallService
      → PluginOperationExecutor (统一编排器)
        → 1. PluginPersistence（数据库 + 文件系统）
        → 2. RuntimeOps（Registry + Contexts + Cache 内存注册）
        → 3. AuditLogger（审计日志）
        → 4. EventPublisher（GlobalEventBus 进程内事件 + Redis 跨实例通知）
```

---

## 十、关键文件路径

| 文件 | 作用 |
|------|------|
| [handler.rs](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/libs/cmx-api/src/handlers/plugin/handler.rs) | HTTP Handler（所有插件操作入口） |
| [request.rs](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/libs/cmx-api/src/handlers/plugin/request.rs) | API 请求结构体 |
| [response.rs](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/libs/cmx-api/src/handlers/plugin/response.rs) | API 响应结构体 |
| [mod.rs](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/libs/cmx-api/src/handlers/plugin/mod.rs) | 路由定义 |
| [executor.rs](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/libs/cmx-plugin/src/service/executor.rs) | 统一编排器 |
| [deploy.rs](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/libs/cmx-plugin/src/service/deploy.rs) | 部署服务（智能判断） |
| [install.rs](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/libs/cmx-plugin/src/service/install.rs) | 安装服务 |
| [upgrade.rs](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/libs/cmx-plugin/src/service/upgrade.rs) | 升级服务 |
| [downgrade.rs](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/libs/cmx-plugin/src/service/downgrade.rs) | 降级服务 |
| [uninstall.rs](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/libs/cmx-plugin/src/service/uninstall.rs) | 卸载服务 |
| [persistence.rs](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/libs/cmx-plugin/src/service/persistence.rs) | 持久化层（数据库 + 文件系统） |
| [runtime_ops.rs](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/libs/cmx-plugin/src/service/runtime_ops.rs) | 运行时操作层（纯内存） |
| [event_publisher.rs](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/libs/cmx-plugin/src/service/event_publisher.rs) | 事件发布层 |
