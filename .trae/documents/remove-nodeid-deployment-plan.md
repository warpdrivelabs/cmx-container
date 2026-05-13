# 移除 node_id 和 cmx_plugin_deployments 表依赖计划

## 背景

根据 `cmx-plugin-multi-instance-stateless-plan.md` 方案，多实例无状态化部署要求：
- 移除所有 `node_id` 依赖（实例不绑定节点身份）
- 移除所有 `cmx_plugin_deployments` 表操作（不再跟踪节点级部署状态）
- 确保安装/升级/卸载/降级逻辑正确无漏洞

## 当前代码问题清单

### A. deployment_repository 使用点（需移除）

| 文件 | 行号 | 当前用途 | 处理方式 |
|---|---|---|---|
| `install.rs` | 70 | `InstallServiceDeps.deployment_repository` 字段 | 删除字段 |
| `install.rs` | 196-223 | `find_deployment()` 检查是否已安装 | 改用 registry 检查 |
| `install.rs` | 534 | `Default` 实现中创建 | 删除 |
| `upgrade.rs` | 70 | `UpgradeServiceDeps.deployment_repository` 字段 | 删除字段 |
| `upgrade.rs` | 152-164 | `find_deployment()` 检查节点是否部署 | 删除检查（已通过 registry 确认） |
| `upgrade.rs` | 511 | `Default` 实现中创建 | 删除 |
| `uninstall.rs` | 50 | `UninstallServiceDeps.deployment_repository` 字段 | 删除字段 |
| `uninstall.rs` | 121-125 | `delete_deployments_by_plugin_id()` 删除部署记录 | 删除调用 |
| `uninstall.rs` | 223 | `Default` 实现中创建 | 删除 |
| `downgrade.rs` | 60 | `DowngradeServiceDeps.deployment_repository` 字段 | 删除字段 |
| `downgrade.rs` | 156-166 | 注释掉的 `update_deployment()` | 删除注释 |
| `deploy.rs` | 82 | `DeployServiceDeps.deployment_repository` 字段 | 删除字段 |
| `sync.rs` | 305-306 | `NodeSyncServiceDeps.deployment_repository` 字段 | 删除（整个向后兼容结构） |
| `initializer.rs` | 22,83,98,110 | `deployment_repository` 字段和参数 | 删除 |
| `manager.rs` | 290-293 | 创建 `DeploymentRepository` 实例 | 删除 |
| `manager.rs` | 343,367,403,418,446,461 | 传递给各服务 | 删除传递 |

### B. node_id 使用点（需移除或降级）

| 文件 | 行号 | 当前用途 | 处理方式 |
|---|---|---|---|
| `install.rs` | 94 | `InstallServiceDeps.node_id` 字段 | 删除字段 |
| `install.rs` | 436 | 审计日志 `with_node_id()` | 审计保留 node_id（可选信息），改为自动填充 |
| `install.rs` | 440 | 审计日志 details 中记录 | 删除 |
| `install.rs` | 546 | `Default` 实现中 `None` | 删除 |
| `upgrade.rs` | 95 | `UpgradeServiceDeps.node_id` 字段 | 删除字段 |
| `upgrade.rs` | 155 | `find_deployment()` 参数 | 随 deployment 移除 |
| `upgrade.rs` | 417 | 审计日志 details 中记录 | 删除 |
| `upgrade.rs` | 523 | `Default` 实现中 `None` | 删除 |
| `uninstall.rs` | 62 | `UninstallServiceDeps.node_id` 字段 | 删除字段 |
| `uninstall.rs` | 180 | 审计日志 details 中记录 | 删除 |
| `uninstall.rs` | 229 | `Default` 实现中 `None` | 删除 |
| `downgrade.rs` | 72 | `DowngradeServiceDeps.node_id` 字段 | 删除字段 |
| `downgrade.rs` | 301 | 审计日志 details 中记录 | 删除 |
| `deploy.rs` | 100 | `DeployServiceDeps.node_id` 字段 | 删除字段 |
| `manager.rs` | 315 | `AuditLoggerConfig` 中传入 | 审计保留，改为常量 "default" |
| `manager.rs` | 355,379,409,424,455 | 传递给各服务 | 删除传递 |
| `settings.rs` | 31 | `PluginSettings.node_id` 字段 | 保留为 Option（审计用） |
| `sync.rs` | 312 | `NodeSyncServiceDeps.node_id` | 删除（整个向后兼容结构） |
| `audit/logger.rs` | 25,33,38,48,77-79,93,103,141,173,202,223,228,233,370 | 审计日志 node_id | 保留审计字段，自动填充 |
| `audit/record.rs` | 72,114,163-164 | 审计记录 node_id | 保留审计字段 |

### C. 集群模块（需清理）

| 文件 | 处理方式 |
|---|---|
| `cluster/node.rs` (NodeManager) | 保留但标记为非核心，不强制使用 |
| `cluster/deployment.rs` (DeploymentCoordinator) | 保留类型定义，不强制使用 |
| `cluster/sync.rs` (SyncManager, PluginStateRecord, SyncMessage) | 删除向后兼容的废弃类型 |
| `infrastructure/database/deployment/` 整个模块 | 保留文件但标记为废弃，不再被服务层引用 |

### D. 逻辑漏洞修复

| 问题 | 文件 | 修复方式 |
|---|---|---|
| install 检查已安装依赖 deployment 表 | `install.rs` | 改用 registry + 数据库 plugin 表检查 |
| upgrade 检查节点部署依赖 deployment 表 | `upgrade.rs` | 删除检查，已通过 `find_plugin()` 确认 |
| uninstall 删除 deployment 记录 | `uninstall.rs` | 删除调用 |
| initializer 持有 deployment_repository | `initializer.rs` | 删除 |

---

## 实施步骤

### 步骤1: install.rs — 移除 deployment + node_id，修复已安装检查逻辑

1. 删除 `InstallServiceDeps` 中的 `deployment_repository` 和 `node_id` 字段
2. 将步骤4的 `find_deployment()` 检查替换为：
   - 先查 registry（内存中是否有记录）
   - 再查 `repository.find_plugin()`（数据库中是否有记录）
   - 如果已存在且版本 = 要安装版本 → 返回已安装
   - 如果已存在且版本 > 要安装版本 → 返回错误提示用降级
   - 如果已存在且版本 < 要安装版本 → 正常走安装流程（覆盖安装/升级场景）
3. 删除审计日志中的 `with_node_id()` 和 details 中的 `node_id`
4. 删除 `Default` 实现中的 `deployment_repository` 和 `node_id`

### 步骤2: upgrade.rs — 移除 deployment + node_id

1. 删除 `UpgradeServiceDeps` 中的 `deployment_repository` 和 `node_id` 字段
2. 删除步骤2的 `find_deployment()` 检查（已有 `find_plugin()` 确认插件存在）
3. 删除审计日志 details 中的 `node_id`
4. 删除 `Default` 实现中的 `deployment_repository` 和 `node_id`

### 步骤3: uninstall.rs — 移除 deployment + node_id

1. 删除 `UninstallServiceDeps` 中的 `deployment_repository` 和 `node_id` 字段
2. 删除步骤5的 `delete_deployments_by_plugin_id()` 调用
3. 删除审计日志 details 中的 `node_id`
4. 删除 `Default` 实现中的 `deployment_repository` 和 `node_id`

### 步骤4: downgrade.rs — 移除 deployment + node_id

1. 删除 `DowngradeServiceDeps` 中的 `deployment_repository` 和 `node_id` 字段
2. 删除注释掉的 `update_deployment()` 代码块
3. 删除审计日志 details 中的 `node_id`

### 步骤5: deploy.rs — 移除 deployment + node_id

1. 删除 `DeployServiceDeps` 中的 `deployment_repository` 和 `node_id` 字段

### 步骤6: initializer.rs — 移除 deployment_repository

1. 删除 `PluginInitializer` 中的 `deployment_repository` 字段和构造参数
2. 删除 import

### 步骤7: sync.rs — 清理向后兼容类型

1. 删除 `NodeSyncServiceDeps` 结构体（含 `deployment_repository` 和 `node_id`）
2. 删除 `SyncState` 枚举

### 步骤8: manager.rs — 移除 deployment_repository 创建和传递

1. 删除 `DeploymentRepository` 的 import 和创建
2. 删除所有服务构造中的 `deployment_repository:` 传递
3. 删除所有服务构造中的 `node_id:` 传递
4. `AuditLoggerConfig` 的 node_id 改为常量 `"default"`
5. 删除 `PluginInitializer` 构造中的 `deployment_repository` 传递

### 步骤9: cluster/sync.rs — 清理废弃类型

1. 删除 `PluginStateRecord`（含 `node_id`）
2. 删除 `SyncMessage`（含 `source_node_id`）
3. 删除 `SyncMessageType` 枚举
4. 删除 `SyncManager`（含 `local_node_id`）
5. 保留 `PluginNotifier`、`PluginChangeNotification`、`PluginChangeAction`

### 步骤10: audit 模块 — node_id 降级为可选自动填充

1. `AuditRecord.node_id` 保留为 `Option<String>`（审计需要记录操作来源）
2. `AuditLoggerConfig.node_id` 保留，默认 `"default"`
3. 删除各服务中手动调用 `with_node_id()` 的代码，由 AuditLogger 自动填充

### 步骤11: lib.rs — 清理导出

1. 删除 `SyncManager`、`PluginStateRecord` 的导出
2. 保留 `PluginNotifier`、`PluginChangeNotification`、`PluginChangeAction` 导出

### 步骤12: 编译检查 + clippy

1. `cargo check -p cmx-plugin` 确保编译通过
2. `cargo clippy -p cmx-plugin` 确保无新增警告

---

## 不删除的文件/模块

以下文件/模块**保留**但不再被服务层引用：
- `infrastructure/database/deployment/` — 整个目录保留，以防其他 crate 引用
- `cluster/node.rs` — NodeManager 保留，作为可选功能
- `cluster/deployment.rs` — DeploymentCoordinator 保留，作为可选功能
- `settings.rs` 的 `node_id: Option<String>` — 保留，审计和集群可选使用
- `audit/record.rs` 的 `node_id: Option<String>` — 保留，审计需要记录来源

## 风险点

1. **install 已安装检查**：移除 deployment 检查后，需要确保 registry + plugin 表的检查逻辑能正确覆盖所有场景
2. **审计追溯**：node_id 从手动填充改为自动填充 "default"，历史审计记录可能不一致
3. **向后兼容**：`NodeSyncServiceDeps` 等类型删除后，外部代码如果引用会编译失败
