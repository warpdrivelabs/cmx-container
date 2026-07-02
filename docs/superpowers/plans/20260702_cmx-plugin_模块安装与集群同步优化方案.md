# 模块安装与集群同步优化方案

> **分支:** `feat/module-centric-restructure`
> **日期:** 2026-07-02
> **背景:** 模块包安装(ModuleInstallService)在 deploy 流程、db_id 策略、集群同步、死字段四个方面存在问题，需优化。
 
---

## 一、改造目标

| # | 问题 | 根因 | 改造方向 |
|---|---|---|---|
| 1 | ModuleInstallService 用 `install_service.install`，已安装同版本会报错 | install 不做版本比较，遇到已安装直接报错 | 改用 `deploy_service.deploy`（自动判断 升级/安装/跳过） |
| 2 | db_id 全程用 `get_default_db_id()`，建表建到默认库 | `get_biz_db_id()` 全仓库零调用，业务库概念未启用 | 资源存 default 库，**建表用 `get_biz_db_id()`** |
| 3 | 模块安装元数据建表未建到业务库 | install_metadata 的 PgTableDefineExecutor 用 default db_id | 改用 biz_db_id 建表 |
| 4 | deploy 默认不上传 OSS，其他节点拉不到 zip | deploy handler 二分逻辑：发布市场才上传 OSS，否则 Local 路径存 DB | **deploy 内部统一上传 OSS**（非 handler 层，所有入口自动获益） |
| 5 | auto_activate 是死字段，激活功能未实现 | 安装链路从不读取 auto_activate，is_active 硬编码 false | 移除 auto_activate 字段及相关代码 |
 
---

## 二、调研结论(关键事实)

### 2.1 auto_activate — 完全死字段

- `InstallRequest.auto_activate` 在安装链路(install_persist → executor → runtime)中**从不被读取**
- 插件激活功能未实现: `traits_impl.rs` 的 `is_active` 硬编码返回 `Ok(false)`
- 全仓库 10 处赋值/透传，无一处条件判断读取
- 可安全移除

### 2.2 deploy 内部上传 OSS — 完全可行

- `deploy` 方法签名已是 `pub async fn deploy(&self, mut request: DeployRequest)`(deploy.rs L138)，request 是 mut，可直接覆盖 source
- 插入点: 步骤4 解析元数据后(L173，已拿到 plugin_id/version)、步骤5 查询已安装前(L179)
- 上传通过 `cmx_storage::GlobalStorageService::get().service().upload()` 全局单例，**无需改 DeployServiceDeps**(其 storage 字段是本地 FileStorage，不做 OSS)
- `marketplace_publisher.rs:69-83` 有可逐行照抄的现成实现
- 优势: **从 handler 下沉到 deploy 内部，所有调用 deploy 的入口(API deploy、模块安装、市场安装)都自动获益**

### 2.3 db_id 双库语义

| 库 | 用途 | 获取方式 |
|---|---|---|
| default 库 | 元数据登记表、模块资源(表单/菜单/权限/版本表) | `get_default_db_id()` |
| biz 库 | 业务数据表(create_or_upgrade_table 目标) | `get_biz_db_id()`(未注册时 fallback default) |

### 2.4 集群同步缺陷根因

```
当前流程(API deploy, publish_to_marketplace=false):
  上传 zip → 本节点临时路径 → Local{path} → DB 存 zip_source_url=本地路径
  其他节点收到通知 → sync_and_register → 从 DB 读 zip_source_url → LocalFetcher 找不到 → 失败
 
目标流程(deploy 内部统一上传):
  deploy 收到 Local{path} → 内部上传 OSS → 转为 Storage{file_id} → DB 存 zip_source_url=OSS 地址
  其他节点收到通知 → sync_and_register → 从 DB 读 storage type → StorageFetcher 下载 → 成功
```
 
---

## 三、改造点 1: 移除 auto_activate

**结论**: 安装链路从不读取此字段，插件激活功能未实现。

清理清单:
- `crates/libs/cmx-plugin/src/service/install.rs` — InstallRequest 移除字段
- `crates/libs/cmx-plugin/src/service/deploy.rs` L252 — execute_install 移除赋值
- `crates/libs/cmx-plugin/src/service/executor.rs` L519 — execute_control_install 移除
- `crates/libs/cmx-plugin/src/service/persistence.rs` L916 — reinstall_persist 移除
- `crates/libs/cmx-plugin/src/service/auto_install.rs` L242 — 移除
- `crates/libs/cmx-plugin/src/service/module_install.rs` L140 — 移除(改用 deploy 后重写)
- `crates/libs/cmx-plugin/src/marketplace/service.rs` L694 — 移除透传
- `crates/libs/cmx-plugin/src/marketplace/model.rs` L533 — 移除字段
- `crates/libs/cmx-api/src/handlers/plugin/handler.rs` L73 — 移除
- `crates/libs/cmx-api/src/handlers/marketplace/request.rs` L129 — 移除 API 入参
- `crates/libs/cmx-api/src/handlers/marketplace/handler.rs` L659 — 移除

---

## 四、改造点 2: deploy 内部统一上传 OSS

### 4.1 deploy 方法内插入上传逻辑

**文件**: `crates/libs/cmx-plugin/src/service/deploy.rs` deploy 方法(L138-239)

插入点: 步骤4 解析元数据后(L173)、步骤5 查询已安装前(L179):

```rust
// 步骤 4.5: 若 source 是 Local，上传 OSS 后转为 Storage(集群同步必需)
if let PluginSource::Local { ref path } = request.source {
    let zip_bytes = tokio::fs::read(path).await
        .map_err(|e| PluginError::Plugin(format!("读取插件 zip 失败: {e}")))?;
    let storage_service = cmx_storage::global::GlobalStorageService::get().service();
    let file_name = path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| format!("plugin-{plugin_id}-{new_version}.zip"));
    let upload_request = cmx_storage::types::UploadRequest {
        data: zip_bytes.into(),
        original_filename: Some(file_name),
        content_type: Some("application/zip".to_string()),
        object_type: Some("deployed_plugin".to_string()),
        object_id: Some(plugin_id.clone()),
        platform: None,
        user_metadata: None,
        acl: None,
    };
    let file_info = storage_service.upload(upload_request).await
        .map_err(|e| PluginError::Plugin(format!("上传插件包到存储失败: {e}")))?;
    tracing::info!(plugin_id = %plugin_id, file_id = %file_info.id, "插件包已上传到 cmx-storage");
    request.source = PluginSource::Storage {
        file_id: file_info.id,
        checksum: None,
    };
}
```

- request 已是 mut，直接覆盖 source
- 下游 execute_install/upgrade/reinstall 拿到的就是 Storage source
- DB 的 zip_source_url 自动变为 OSS 地址，其他节点 sync 正常

### 4.2 handler 简化

**文件**: `crates/libs/cmx-api/src/handlers/plugin/handler.rs` L325-370

```rust
// 移除 if publish_to_marketplace 二分上传逻辑
// source 统一用 Local{path}(deploy 内部会自动转 OSS)
source = cmx_plugin::domain::plugin::PluginSource::Local { path: abs_path };
 
// publish_to_marketplace 仅控制是否额外写 marketplace 表
```

### 4.3 模块安装自动获益

module_install.rs 构造 DeployRequest 时 source 用 `Local{path}`，deploy 内部自动转 OSS，无需模块安装层关心上传。
 
---

## 五、改造点 3: ModuleInstallService 改用 deploy

### 5.1 manager.rs 新增 deploy_service() 访问器

**文件**: `crates/libs/cmx-plugin/src/core/manager.rs` (L605 附近)

```rust
/// 获取部署服务(供 ModuleInstallService 复用，自动判断升级/安装)
pub fn deploy_service(&self) -> &crate::service::deploy::DeployService {
    &self.deploy_service
}
```

### 5.2 module_install.rs 改持有 DeployService

**文件**: `crates/libs/cmx-plugin/src/service/module_install.rs`

```rust
// 旧
install_service: std::sync::Arc<InstallService>,
// 新
deploy_service: std::sync::Arc<DeployService>,
```

构造函数:
```rust
pub fn new(package_utils: PackageUtils, deploy_service: std::sync::Arc<DeployService>) -> Self
```

第 6 步遍历插件子包:
```rust
let deploy_req = DeployRequest {
    source: PluginSource::Local { path: plugin_zip.clone() },
    db_id: None,
    force_reinstall: false,
    build_type: None,
    publish_to_marketplace: false,
    app_id: Some(app.clone()),
    marketplace_source_id: None,
    marketplace_publish_info: None,
};
match self.deploy_service.deploy(deploy_req).await {
    Ok(resp) => {
        info!(plugin_id = %resp.plugin_id, action = ?resp.action, "插件子包部署成功");
        plugin_count += 1;
    }
    Err(e) => warn!(package = %entry.package, error = %e, "插件子包部署失败"),
}
```

### 5.3 package_handler.rs 适配

**文件**: `crates/libs/cmx-api/src/handlers/module/package_handler.rs`

```rust
let deploy_svc = std::sync::Arc::new(manager.deploy_service().clone());
let module_install_svc = ModuleInstallService::new(package_utils, deploy_svc);
```
 
---

## 六、改造点 4: db_id 双库策略

### 6.1 获取双 db_id

**文件**: `crates/libs/cmx-plugin/src/service/module_install.rs` install_module_package(L92-93)

```rust
let mm = get_default_db_manager();
let default_db_id = mm.get_default_db_id().await;
let biz_db_id = mm.get_biz_db_id().await;
```

### 6.2 install_module_resources 传入双 db_id

方法签名增加 `biz_db_id: &str`:
- forms/menus/permissions/版本登记 用 default_db_id
- metadata 建表用 biz_db_id

### 6.3 install_metadata 用 biz_db_id 建表

```rust
// 建表用 biz_db_id(业务数据表建到业务库)
let executor = cmx_metadata::executor::PgTableDefineExecutor::new(biz_db_id, None);
// 元数据登记存 default 库
Self::save_table_metadata(mm, default_db_id, biz_db_id, table_def, ...).await;
```

### 6.4 save_table_metadata 修正

- SQL 执行库: default_db_id(元数据表在默认库)
- 记录 db_id 列: biz_db_id(标记业务表所在库，不再硬编码 'default')

---

## 七、影响范围

| 文件 | 改动类型 | 内容 |
|---|---|---|
| `cmx-plugin/src/service/install.rs` | 修改 | 移除 auto_activate 字段 |
| `cmx-plugin/src/service/deploy.rs` | 修改 | 内部上传 OSS + 移除 auto_activate |
| `cmx-plugin/src/service/executor.rs` | 修改 | 移除 auto_activate |
| `cmx-plugin/src/service/persistence.rs` | 修改 | 移除 auto_activate |
| `cmx-plugin/src/service/auto_install.rs` | 修改 | 移除 auto_activate |
| `cmx-plugin/src/service/module_install.rs` | 修改 | 改用 deploy + 双 db_id + save_table_metadata |
| `cmx-plugin/src/service/marketplace_publisher.rs` | 修改 | 移除 auto_activate 透传 |
| `cmx-plugin/src/marketplace/service.rs` | 修改 | 移除 auto_activate |
| `cmx-plugin/src/marketplace/model.rs` | 修改 | 移除 auto_activate 字段 |
| `cmx-plugin/src/core/manager.rs` | 修改 | 新增 deploy_service() |
| `cmx-api/src/handlers/plugin/handler.rs` | 修改 | 简化 deploy 分支 + 移除 auto_activate |
| `cmx-api/src/handlers/module/package_handler.rs` | 修改 | 适配 deploy_service |
| `cmx-api/src/handlers/marketplace/request.rs` | 修改 | 移除 auto_activate 入参 |
| `cmx-api/src/handlers/marketplace/handler.rs` | 修改 | 移除 auto_activate |
 
---

## 八、实施顺序

1. **移除 auto_activate**(全仓库清理死字段，降低后续改动复杂度)
2. **deploy.rs 内部上传 OSS**(核心改造，所有 deploy 入口自动获益)
3. **manager.rs 新增 deploy_service() 访问器**
4. **module_install.rs 改用 deploy + 双 db_id + save_table_metadata 修正**
5. **package_handler.rs 适配 deploy_service**
6. **plugin handler.rs 简化 deploy 分支**
7. **编译 + clippy 验证**

---

## 九、风险点

1. **GlobalStorageService 未初始化 panic**: deploy 内部调 `GlobalStorageService::get()` 需确保存储已初始化(正常启动流程会初始化)
2. **大包内存占用**: 上传需先 `tokio::fs::read` zip 到内存，超大包需评估
3. **get_biz_db_id 回退**: 未注册 biz 库时回退 default_db_id，不会报错(平滑兼容)
4. **auto_activate 移除范围**: 需全面清理，避免遗漏导致编译失败
5. **source_utils Storage 类型**: 需确认 extract_source_info 对 Storage 正确提取 file_id/file_url 写入 DB


