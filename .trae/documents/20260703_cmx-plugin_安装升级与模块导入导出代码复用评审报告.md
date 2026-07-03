# cmx-plugin 安装升级与模块导入导出 代码复用评审报告

> **评审日期**：2026-07-03
> **评审范围**：`crates/libs/cmx-plugin` 的插件安装/升级链路 + 模块导入/导出链路
> **评审重点**：功能与代码复用、`app_id` 字段必要性
> **参考 DDL**：`docs/sql/init/init_ddl.sql`

---

## 一、执行摘要

### 1.1 核心结论

| # | 维度 | 结论 |
|---|------|------|
| 1 | **插件安装与模块导入的融合度** | **部分融合**。模块导入已复用 `DeployService` 处理插件子包（✅ 好的设计）；但权限/DDL/元数据安装仍存在 **平行实现**，未充分复用。 |
| 2 | **`app_id` 字段必要性** | **在携带 `module_code` 的 4 张表上功能冗余**。`get_app_id()` 直接返回 `app.module_code` 配置值，且 `module_install.rs:87` 强制 `module.code == app_id`，两者恒等。仅在不带 `module_code` 的表（`cmx_plugin_versions`、`cmx_audit_log`、`cmx_model_*`）上仍有承载价值。 |
| 3 | **版本管理双轨** | 插件用**语义版本**（`1.0.0`），模块用 **14 位时间戳**（`yyyyMMddHHmmSS`），两套系统并存，设计合理但需文档化边界。 |
| 4 | **死代码负担** | `executor.rs` 5 处注释 dispatch 块 + 整段管控模式（496–782 行）、`utils.rs` 的 `execute_ddl_with_lock`/`create_plugin_tables` 已无活调用方，需清理。 |

### 1.2 最严重的 3 个问题

1. 🔴 **权限两阶段 upsert 重复 3 处**：`cmx-iam::import`、`cmx-iam::crud`、`module_install::install_permissions` 各自实现「先 INSERT parent_id=NULL → 回填 parent_id/parent_code/full_code_path/level/is_leaf」逻辑，存在维护漂移风险。
2. 🔴 **DDL + 元数据保存双路径**：`utils.rs::execute_ddl_with_lock`（插件路径，已失效）vs `module_install::install_metadata`（模块路径，在用），两套互不复用。
3. 🟠 **`install_persist` 与 `upgrade_persist` 80% 逻辑重复**：安全校验、元数据提取、依赖检查、目录创建、文件拷贝、事务、seed、记录构建、upsert、版本历史、服务解析等步骤几乎一致，未抽取共享 helper。

---

## 二、架构现状

### 2.1 调用拓扑

```
┌─────────────────────────── cmx-api Handlers ───────────────────────────┐
│  plugin/handler.rs (deploy)      module/package_handler.rs (import/    │
│    │                               export)                              │
│    ↓                                    ↓                                │
└──┬──────────────────────────────────────┬──────────────────────────────┘
   │                                      │
   ↓                                      ↓
┌──┴──────── DeployService ───────────────┴── ModuleInstallService ──────┐
│  deploy.rs:138 deploy()                   module_install.rs:72          │
│   ├─ 版本比对 → Install/Upgrade/Reinstall   ├─ app_id 守卫 (line 84)    │
│   ├─ OSS 上传 (内部 upload_to_storage)      ├─ 版本校验 validate_import │
│   └─ 委托 executor.execute_*                ├─ install_forms/menus      │
│        ↓                                     ├─ install_metadata (DDL)  │
│        ↓                                     ├─ install_permissions ⚠️  │
│ ┌──────────── Executor ───────────┐         │   （重复实现）            │
│ │ executor.rs execute_install/    │         ├─ record_version          │
│ │   upgrade/downgrade/reinstall   │         └─ 循环 plugin 子包:        │
│ │   ↓                             │              deploy_service.deploy │
│ │ ┌─── Persistence ─────────────┐ │                  ↑                 │
│ │ │ persistence.rs               │ │                  └──── 复用 ───────┤
│ │ │  install_persist ⚠️          │ │                                  │
│ │ │  upgrade_persist ⚠️(80%重复) │ │                                  │
│ │ │  └─ execute_seed_data        │ │                                  │
│ │ └──────────────────────────────┘ │                                  │
│ └──────────────────────────────────┘                                  │
└───────────────────────────────────────────────────────────────────────┘
```

### 2.2 复用关系（好与坏）

| 复用点 | 状态 | 说明 |
|--------|------|------|
| `DeployService` 被 `ModuleInstallService` 调用 | ✅ 已复用 | `module_install.rs:163` 调 `deploy_service.deploy()`，插件子包安装不重复 |
| `FormService` / `MenuService` 被 import 复用 | ✅ 已复用 | 表单/菜单写入走 cmx-biz 标准服务 |
| `DeployService` 与 `InstallService` 共享 executor | ✅ 已复用 | `manager.rs:324` 同一个 `Arc<PluginOperationExecutor>` 注入两者 |
| 权限安装逻辑 | ❌ 未复用 | `module_install::install_permissions` 重写，未调 `cmx-iam::import_permissions` |
| DDL + 元数据保存 | ❌ 未复用 | `utils.rs` 旧路径失效，`module_install` 自建一套 |
| `install_persist` / `upgrade_persist` | ❌ 未复用 | 两者 80% 重复 |

---

## 三、代码复用问题清单

### 3.1 🔴 严重：权限两阶段 upsert 重复 3 处

**问题**：权限树的「先 INSERT（parent_id=NULL）→ 回填 parent_id/parent_code/full_code_path/level/is_leaf」逻辑在以下三处各自实现：

| 位置 | 文件:行号 | 实现 |
|------|-----------|------|
| 1 | `cmx-iam/src/permission/service/import.rs:171+` | `PermissionServiceImpl::import_permissions`，事务内 + diff（增/改/删） |
| 2 | `cmx-iam/src/permission/service/crud.rs` | 普通 CRUD 路径，计算 `full_code_path`/`level`/`parent_code` |
| 3 | `cmx-plugin/src/service/module_install.rs:532-733` | 手写 SQL 两阶段 upsert，**无事务**、**无 diff**、**无删除清理** |

**风险**：
- 三处 `full_code_path = parent_path + "/" + code`、`level = parent_level + 1`、父节点 `is_leaf = 0` 的计算必须手工保持同步。
- 模块导入的版本**没有事务保护**（`module_install.rs:601-733`），中途失败会留下 parent_id 为 NULL 的脏数据。
- 模块导入**不做 diff/删除**，旧权限残留无法清理。

**根因**：`cmx-iam::import_permissions` 需要注入式 `PermissionServiceImpl`（带 `mm`/`db_id`/audit 依赖）+ zip 输入，而模块安装器只有磁盘上的 `permissions/*.json` 文件和 `DatabaseManager` 句柄（见 `module_install.rs:531` 注释）。

### 3.2 🔴 严重：DDL + 元数据保存双路径

**问题**：表结构创建 + `cmx_meta_table_define` 保存存在两套实现：

| 路径 | 入口 | 状态 |
|------|------|------|
| 旧（插件路径） | `utils.rs::execute_ddl_with_lock` → `create_plugin_tables` → `save_plugin_table_metadata` | **已失效**：`persistence.rs:243-255`、`504-516` 已注释掉调用，`utils.rs:413/429/442` 的调用仅在 `execute_ddl_with_lock` 函数体内部（无外部活调用方） |
| 新（模块路径） | `module_install.rs:451-525 install_metadata` + `save_table_metadata:739-840` | **在用**：自带 `PgTableDefineExecutor::new(biz_db_id, None)`，**无分布式锁**（注释「module install is low-frequency」） |

**重复点**：
- `PgTableDefineExecutor::create_or_upgrade_table` 调用方式重复。
- `cmx_meta_table_define` upsert 逻辑重复：旧路径走 `TableMetadataService` 抽象，新路径走**裸 SQL**（`module_install.rs:794-840`）。
- 旧路径的 `save_plugin_table_metadata`（`utils.rs:300-374`）已无活调用方。

### 3.3 🟠 中等：`install_persist` 与 `upgrade_persist` 80% 重复

**位置**：`persistence.rs:116-382`（install）vs `389-628`（upgrade）

**重复步骤**（按编号）：
1. fetch 包 → extract 到临时目录
2. 安全校验
3. 解析 plugin_definition 元数据
4. 依赖检查
5. 创建安装目录 `{plugin_root}/{app_id}/{plugin_id}/{version}`
6. 拷贝文件
7. 开启事务
8. 执行 seed data（`execute_seed_data`）
9. 构建记录、upsert plugin/version
10. `set_current_version`
11. 解析 + 保存服务定义
12. commit

**唯一差异**：upgrade 多了「检查已存在 + 版本必须更大 + 记录 old_version」前置校验。

**建议**：抽取 `persist_common(PersistContext)` helper，install/upgrade 只传入差异化的「前置校验闭包」和「是否记录 old_version」。

### 3.4 🟠 中等：`PermissionDefinition` 契约 3 份副本

**问题**：权限定义结构体（8 字段：code/name/resource_type/parent_code/sort_order/description/extension/status）存在 3 份定义：

| # | 位置 | 形式 |
|---|------|------|
| 1 | `cmx-iam/src/permission/service/import.rs:27-50` | 规范结构体 `PermissionDefinition`（带文档级语义） |
| 2 | `module_export.rs:395-404` | **临时 JSON**：`serde_json::json!({...8 字段...})` |
| 3 | `module_install.rs:554-569` | **私有内联结构体** `PermDef`（`#[serde(rename_all = "snake_case")]`） |

**风险**：字段重命名/增删时三处必须手工同步，且导出用 `json!` 宏绕过了类型检查，导入用独立 struct 无法被编译器对齐。

### 3.5 🟡 轻微问题集

| # | 问题 | 位置 | 建议 |
|---|------|------|------|
| 1 | `delete_by_code` 表单/菜单重复 | `form/service.rs:55-66` + `menu/service.rs:203-214`（结构完全一致） | 提升为 `GenericCrudService::delete_by_field` 泛型 helper |
| 2 | JSONB「string-or-object」强制转换重复 ~6 处 | export forms（179-184）、menus（224-228）、metadata（288-298）、menu tree（service.rs:319-324） | 抽 `cmx_utils::jsonb::coerce_to_object(Value)` |
| 3 | 临时目录 + 时间戳格式重复 | `module_export.rs:51-55` + `module_install.rs:190-194` + `migrate_to_module_packages.rs:239` | 收敛到 `PackageUtils::new_temp_dir(prefix)` |
| 4 | 14 位时间戳格式 `%Y%m%d%H%M%S` 重复 3 处 | export（110）+ migrate（239）+ 隐含于版本校验 | 抽常量 `MODULE_PACKAGE_VERSION_FORMAT` |
| 5 | `install_forms` 与 `install_menus` 几乎逐行相同 | `module_install.rs:306-370` vs `376-444`（读目录→`.json`→`{module}:{stem}` code→delete_by_code→create） | 泛型化 `install_definition_files<T>(...)` |

---

## 四、`app_id` 字段专项评估

### 4.1 设计意图 vs 现实

**设计意图**（来自列注释）：`app_id` 用于多租户/多应用隔离，`DEFAULT 'default'`。

**现实实现**（`cmx-utils/src/config/config_impl.rs:348-375`）：

```rust
pub fn get_app_id(&self) -> String {
    // 1. 读配置键 app.module_code   ← 直接取 module_code 的值
    if let Ok(v) = self.get_string("app.module_code") && !v.is_empty() { return v; }
    // 2-4. 环境变量 APP_ID / SERVICE_REGISTRY_NAME / NACOS_NAMING_SERVICE_NAME
    ...
    // 5. 兜底 "default"
    "default".to_string()
}
```

**关键事实**：`app_id` 的**第一优先来源就是 `app.module_code` 配置项**，即 `app_id ≡ module_code`（配置层面）。

### 4.2 强制 1:1 的证据链

| 证据 | 位置 | 说明 |
|------|------|------|
| ① 导入守卫强制相等 | `module_install.rs:84-92` | `let res_app_id = &manifest.module.code;` 然后 `if res_app_id != &current_service_app_id { return Err(...) }`。**模块 code 被当作 app_id 使用**。 |
| ② 导出双过滤冗余 | `module_export.rs:434` | `WHERE module_code = $1 AND app_id = $2`，调用方 `export_module`（line 95）把 `application_code` 当作 `app_id` 传入，语义混淆。 |
| ③ deploy 内部回退 | `deploy.rs:214` | `request.app_id.as_deref().unwrap_or(&self.deps.app_id)`，而 `deps.app_id` 来自 `settings.app_id`（即 `get_app_id()`）。 |
| ④ 配置同源 | `config_template.toml` | `[app]` 段 `module_code = "default"`，而 `get_app_id()` 优先读它。 |

**结论**：在当前实现下 `app_id == module_code` **恒成立**，无法支持「一个 app_id 承载多个 module_code」的多租户场景。

### 4.3 各表 `app_id` 必要性判定

| 表 | 有 `module_code`? | `app_id` 判定 | 说明 |
|----|:--:|:--:|------|
| `cmx_plugin` | ✅ | 🟡 **冗余** | 唯一约束 `(app_id, plugin_id)` 可改为 `(module_code, plugin_id)`；但 `(idx_plugin_domain_app_module)` 已有三联索引 |
| `cmx_meta_table_define` | ✅ | 🟡 **冗余** | 已有 `(domain_code, application_code, module_code)` 三联，`app_id` 过滤无增量价值 |
| `cmx_meta_table_define_version` | ✅ | 🟡 **冗余** | 同上 |
| `cmx_service_define` | ✅ | 🟡 **冗余** | 同上 |
| `cmx_plugin_versions` | ❌ | 🟢 **必要** | 唯一承载租户隔离的列 |
| `cmx_plugin_audit_log` | ❌ | 🟢 **必要** | 同上 |
| `cmx_audit_log` | ❌ | 🟢 **必要** | 同上 |
| `cmx_model_meta/registry/source/...` | 部分 | 🟢 **必要** | 模型中心多租户隔离 |
| `cmx_module_current_version` | ✅ | — | **无 `app_id` 列**，用 `(module_code)` 唯一 |
| `cmx_module_version_history` | ✅ | — | **无 `app_id` 列**，用 `(module_code, package_version)` 唯一 |
| `cmx_form` / `cmx_menu` | ✅ | — | **无 `app_id` 列**，cmx-biz 全系不用 app_id |
| `cmx_permission` | ✅ | — | 用 `app_code`（**命名不一致**），无 `app_id`/`application_code` |

### 4.4 `app_id` 使用不一致风险点

| 风险 | 位置 | 说明 |
|------|------|------|
| 🔴 兜底值不一致 | `persistence.rs:118-121` 硬编码 `"default"` vs `deploy.rs:214` 用 `self.deps.app_id`（配置值） | 同一请求走不同路径会得到不同 app_id，导致 `find_plugin` 查不到记录 |
| 🟠 模块导入用 module.code 当 app_id | `module_install.rs:84, 157` | 语义混淆：`ModuleInfo.code` 字段被复用为 app_id 传入 DeployRequest |
| 🟠 导出 application_code 当 app_id | `module_export.rs:95 → export_plugins` | `application_code` 和 `app_id` 是不同概念，此处混用 |
| 🟠 元数据导入硬编码 `'default'` | `module_install.rs:796, 821` | 写入 `cmx_meta_table_define.app_id` 列时**不**取自 manifest，而是字面量 `'default'` |
| 🟡 `cmx_permission` 用 `app_code` | `init_ddl.sql:1752` | 命名与全局 `app_id`/`application_code` 不一致，易混淆 |

### 4.5 `app_id` 治理建议

**短期（低成本，消除风险）**：
1. 统一兜底逻辑：`persistence.rs:118-121` 改为读 `ConfigManager::global().get_app_id()`，与 `deploy.rs` 一致。
2. `module_install.rs:796,821` 的硬编码 `'default'` 改为取 `manifest`/配置的 app_id。
3. 在 `AGENTS.md` 或专门文档中明确约束：「当前 `app_id ≡ module_code`，多租户隔离尚未启用」。

**中期（消除语义混淆）**：
4. `module_install.rs:84` 的变量 `res_app_id` 重命名为 `module_code`，并在与 `app_id` 比较处加注释说明「当前设计下二者同源」。
5. `module_export.rs:95` 的 `export_plugins` 调用，不要把 `application_code` 当 `app_id` 传，应显式取 `get_app_id()`。

**长期（保留为多租户预留，不立即移除列）**：
6. `app_id` 列**保留**（未来多租户仍需要），但在携带 `module_code` 的表上**查询时不再单独过滤 `app_id`**（避免冗余条件）。
7. 若确认永不做多租户，可考虑在下次大版本迁移时移除 `app_id` 列，统一用 `(domain_code, application_code, module_code)` 三联。

---

## 五、版本管理体系评估

### 5.1 双轨现状

| 维度 | 插件版本 | 模块包版本 |
|------|----------|------------|
| 字段 | `cmx_plugin.version` / `cmx_plugin_versions.version` | `cmx_module_current_version.package_version` / `cmx_module_version_history.package_version` |
| 类型 | `VARCHAR(50)` 语义版本（`1.0.0`） | `VARCHAR(14)` 时间戳（`yyyyMMddHHmmSS`） |
| 粒度 | 单插件 | 整个模块包（含 N 个插件 + 资源） |
| 生成方 | 插件开发者声明（`plugin_definition.version`） | **导出时自动生成**（`module_export.rs:110` `Local::now().format("%Y%m%d%H%M%S")`） |
| 比较方式 | `semver` 语义比较 | **字符串字典序**（因定宽 14 位，字典序 = 时间序） |
| 唯一约束 | `(plugin_id, app_id, version)` | `(module_code, package_version)` |

### 5.2 合理性分析

✅ **双轨合理**：
- 插件版本是**开发语义**（API 兼容性），由开发者控制。
- 模块包版本是**发布快照**（环境迁移溯源），由导出时间戳决定，避免人工输入错误。
- 两者正交：同一个模块包可包含不同版本的插件。

⚠️ **需注意**：
- 模块导入时，内部插件的安装/升级由 `DeployService.deploy` 按**插件语义版本**判断；模块包整体是否导入由 **14 位时间戳**判断。两者**不耦合**——可能出现「模块包时间戳更新但某插件版本更旧」的情况（`force_reinstall: true` 在 `module_install.rs:153` 强制覆盖，规避了此问题，但语义需文档化）。
- 14 位时间戳格式重复 3 处（`module_export.rs:110`、`migrate_to_module_packages.rs:239`、隐含于校验逻辑），应抽常量。

### 5.3 版本校验逻辑（`module_install.rs:231-263`）

```
checksum 相等  → SkipSame（幂等跳过）
package_version > 当前 → AllowUpgrade
package_version < 当前 → RejectOldVersion（拒绝旧版本覆盖新版本）
package_version == 当前 且 force → AllowForceDowngrade
package_version == 当前 不 force → AllowSameSecondPatch（同秒补丁）
```

✅ 设计合理，覆盖了幂等、升级、降级、同秒补丁场景。

---

## 六、死代码清单（需清理）

| 文件 | 行号 | 内容 | 处置建议 |
|------|------|------|----------|
| `executor.rs` | 95-134, 186-213, 265-292, 343-370, 431-458 | 5 处注释 `dispatch_install`/`dispatch_cleanup` 块 | 删除（已迁移到模块流程） |
| `executor.rs` | 496-782 | 整段 `/* ... */` 管控模式（`execute_control_*`） | 删除（引用的 `request.send_event` 字段已不存在） |
| `executor.rs` | 46, 63, 70 | `center_dispatcher` 字段 + 注入 | 删除（无活调用） |
| `manager.rs` | 164, 352-366 | `control_service` 字段 + 构造（注释） | 删除 |
| `service/mod.rs` | 15 | `// pub mod control;` | 删除 |
| `persistence.rs` | 243-255, 504-516 | 注释的 `execute_ddl_with_lock` 调用 | 删除注释 |
| `utils.rs` | 392-455 | `execute_ddl_with_lock`（无活调用方） | 删除 |
| `utils.rs` | 89-198 | `create_plugin_tables`（仅被 `execute_ddl_with_lock` 内部调用） | 删除 |
| `utils.rs` | 300-374 | `save_plugin_table_metadata`（无活调用方） | 删除或并入 module_install |
| `manager.rs` | 613-615 | `install_service()` 访问器文档注释过期（称「供 ModuleInstallService 复用」，实际用的是 `deploy_service()`） | 修正注释 |

---

## 七、改进建议矩阵

| # | 建议 | 优先级 | 工作量 | 风险 | 受益 |
|---|------|:--:|:--:|:--:|------|
| 1 | **抽取共享 `apply_permissions(mm, db_id, defs)` 到 cmx-iam**，让 `module_install::install_permissions` 和 `cmx-iam::import_permissions` 都调用 | 🔴 高 | 中 | 中 | 消除 3 处权限逻辑重复，统一事务保护 |
| 2 | **删除 `utils.rs` 失效的 DDL 路径**（`execute_ddl_with_lock`/`create_plugin_tables`/`save_plugin_table_metadata`） | 🔴 高 | 低 | 低 | 减 ~400 行死代码，消除双路径困惑 |
| 3 | **清理 `executor.rs` 死代码**（5 处 dispatch 注释 + 管控模式 + center_dispatcher） | 🟠 中 | 低 | 低 | 减 ~300 行，移除未用依赖 |
| 4 | **统一 `app_id` 兜底**：`persistence.rs:118-121` 改读 `get_app_id()` | 🟠 中 | 低 | 低 | 消除 find_plugin 查不到记录的隐患 |
| 5 | **`install_persist`/`upgrade_persist` 抽取 `persist_common` helper** | 🟠 中 | 中 | 中 | 消除 80% 重复，降低维护成本 |
| 6 | **`PermissionDefinition` 收敛到 cmx-core**（或 cmx-iam re-export），export/import 共用 | 🟠 中 | 低 | 低 | 消除契约漂移 |
| 7 | **`module_install.rs:796,821` 硬编码 `'default'` 改为配置/manifest 取值** | 🟡 低 | 低 | 低 | 元数据 app_id 与实际一致 |
| 8 | **泛型化 `install_definition_files<T>`** 合并 forms/menus 安装 | 🟡 低 | 低 | 低 | 消除 ~70 行重复 |
| 9 | **抽 `coerce_to_object(Value)` 工具函数** 消除 ~6 处 JSONB 转换重复 | 🟡 低 | 低 | 低 | 代码整洁 |
| 10 | **文档化 `app_id ≡ module_code` 约束**（写入 AGENTS.md 或专门 ADR） | 🟡 低 | 低 | 无 | 防止后续误用 |

### 7.1 建议实施顺序

**第一批（高收益低风险，可立即做）**：#2、#3、#4、#10
**第二批（中等工作量，需测试）**：#1、#5、#6
**第三批（代码整洁， opportunistically）**：#7、#8、#9

---

## 八、附录

### 8.1 受影响文件清单

| 文件 | 角色 |
|------|------|
| `crates/libs/cmx-plugin/src/service/deploy.rs` | 智能部署入口（含 OSS 上传） |
| `crates/libs/cmx-plugin/src/service/install.rs` | 薄包装，委托 executor |
| `crates/libs/cmx-plugin/src/service/upgrade.rs` / `downgrade.rs` / `uninstall.rs` | 薄包装 |
| `crates/libs/cmx-plugin/src/service/executor.rs` | 编排器（含死代码） |
| `crates/libs/cmx-plugin/src/service/persistence.rs` | DB + 文件持久化（含重复逻辑） |
| `crates/libs/cmx-plugin/src/service/utils.rs` | DDL/seed/metadata helper（含死代码） |
| `crates/libs/cmx-plugin/src/service/module_install.rs` | 模块导入（含权限重复实现） |
| `crates/libs/cmx-plugin/src/service/module_export.rs` | 模块导出 |
| `crates/libs/cmx-plugin/src/core/manager.rs` | 服务装配 + 访问器 |
| `crates/libs/cmx-biz/src/module/version/service.rs` | 模块版本服务 |
| `crates/libs/cmx-biz/src/form/service.rs` / `menu/service.rs` | 表单/菜单 CRUD |
| `crates/libs/cmx-iam/src/permission/service/import.rs` / `crud.rs` | 权限导入/CRUD（重复点） |
| `crates/libs/cmx-utils/src/config/config_impl.rs` | `get_app_id()` 实现 |
| `crates/libs/cmx-api/src/handlers/module/package_handler.rs` | 模块导入导出 Handler |

### 8.2 `app_id` SQL 过滤点清单

| 文件:行号 | SQL | 说明 |
|-----------|-----|------|
| `infrastructure/database/plugin/repository.rs:251` | `UPDATE cmx_plugin ... WHERE plugin_id = ? AND app_id = ?` | update_plugin |
| `infrastructure/database/plugin/repository.rs:274` | `DELETE FROM cmx_plugin WHERE plugin_id = $1 AND app_id = $2` | delete_plugin |
| `infrastructure/database/plugin/repository.rs:383` | `OnConflict::columns([app_id, plugin_id])` | upsert 唯一性 |
| `infrastructure/database/version_history/repository.rs:299` | `DELETE FROM cmx_plugin_versions WHERE plugin_id = $1 AND app_id = $2` | 删版本 |
| `service/module_export.rs:434` | `SELECT ... FROM cmx_plugin WHERE module_code = $1 AND app_id = $2 AND archived = 0` | 导出插件（**双过滤冗余**） |
| `service/module_export.rs:257` | `JOIN ... ON d.app_id = v.app_id WHERE d.module_code = $1 AND d.application_code = $2` | 导出元数据 |
| `core/registry.rs:111-112` | 内存过滤 `p.app_id == *app_id` | 注册表过滤 |

### 8.3 表结构 `app_id` / `module_code` 字段矩阵

| 表 | `app_id` | `module_code` | `application_code` | `app_code` | 唯一键 |
|----|:--:|:--:|:--:|:--:|------|
| `cmx_plugin` | ✅ | ✅ | ✅ | — | `(app_id, plugin_id)` |
| `cmx_plugin_versions` | ✅ | ❌ | — | — | `(plugin_id, app_id, version)` |
| `cmx_meta_table_define` | ✅ | ✅ | ✅ | — | `id` |
| `cmx_meta_table_define_version` | ✅ | ✅ | ✅ | — | `id` |
| `cmx_module_current_version` | ❌ | ✅ | ✅ | — | `(module_code)` |
| `cmx_module_version_history` | ❌ | ✅ | ✅ | — | `(module_code, package_version)` |
| `cmx_form` | ❌ | ✅ | ✅ | — | `(code)` |
| `cmx_menu` | ❌ | ✅ | ✅ | — | `(code)` |
| `cmx_permission` | ❌ | ✅ | ❌ | ✅ | `(code)` |
| `cmx_service_define` | ✅ | ✅ | ✅ | — | `id` |

> **注**：`cmx_permission` 用 `app_code` 而非 `app_id`/`application_code`，是全局命名不一致点。

---

## 九、结论

cmx-plugin 的模块化重构（将表单/菜单/元数据/权限上提到模块层）方向正确，`DeployService` 被 `ModuleInstallService` 复用是亮点。但**资源安装层（权限/DDL/元数据）的复用尚不充分**，存在 3 处权限逻辑重复和 2 条 DDL 路径，长期会带来维护负担。

`app_id` 字段在当前「单租户 + `get_app_id()` 读 `module_code`」的实现下，于携带 `module_code` 的表上**功能冗余**，但不建议立即删列（保留多租户演进空间）。**当务之急是消除兜底值不一致（`persistence.rs` 硬编码 `"default"`）和语义混淆（module.code 当 app_id、application_code 当 app_id）**，这些是真实的缺陷而非理论问题。

建议按第七节的优先级矩阵推进，第一批（#2/#3/#4/#10）可立即执行，零风险清除死代码并修复 `app_id` 兜底缺陷。
