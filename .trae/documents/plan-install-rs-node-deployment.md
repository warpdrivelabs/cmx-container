# install.rs 节点部署功能完善计划

## 一、需求分析

### 1.1 三张核心表的关系

| 表名                       | 用途        | 说明                                        |
| ------------------------ | --------- |-------------------------------------------|
| `cmx_plugin`             | **基线版本表** | 记录插件的**基线版本**，作为节点同步的目标版本                 |
| `cmx_plugin_versions`    | 版本历史表     | 记录插件**所有历史版本**，包含安装路径，可用于回滚或者降级           |
| `cmx_plugin_deployments` | 节点部署表     | 记录**各节点**实际安装的版本，node\_id + plugin\_id 唯一 |

其他表结构参考 `plugin_lifecycle_schemanojson.sql`

#
### 1.3 安装目录结构修改


```
插件安装目录结构:
plugin_root/
  └── example_plugin/           # 插件ID目录
      ├── 1.0.0/              # 版本目录（包含 WASM 和配置文件）
      │   ├── main.wasm
      │   └── manifest.json
      ├── 1.1.0/              # 升级后新增版本目录
      │   ├── main.wasm
      │   └── manifest.json

```

**优势**:

* 升级/降级只是切换目录，不涉及文件拷贝

* 保留历史版本目录和数据库记录，便于快速回退

### 1.4 节点同步逻辑（节点启动时）

```
节点启动时检查插件同步状态:
  1. 查询 cmx_plugin_deployments，获取节点当前安装的版本
  2. 查询 cmx_plugin，获取插件的当前基线版本

  比较逻辑:
  - deployed_version < baseline_version → 节点需要升级（追赶基线）
  - deployed_version > baseline_version → 节点需要降级（回退到基线）
  - deployed_version == baseline_version → 无需同步
```

***

## 三、详细设计

### 3.1 核心概念

| 概念                  | 定义                                           |
| ------------------- |----------------------------------------------|
| **基线版本 (Baseline)** | cmx\_plugin.version，插件的标准版本，所有节点应尽量保持与此版本一致  |
| **部署版本 (Deployed)** | cmx\_plugin\_deployments.version，某个节点实际安装的版本 |
| **版本历史 (History)**  | cmx\_plugin\_versions，某插件的所有版本记录，包含安装路径      |
| **安装路径**            | plugin\_root / plugin\_id / version，每个版本独立目录 |

## 四、升级/降级/卸载逻辑联动

### 4.1 升级 (upgrade.rs)

**触发条件**: 节点版本 < 基线版本

```
升级流程:
  1. 查询 cmx_plugin_deployments，确认节点当前版本
  2. 查询 cmx_plugin，获取目标基线版本
  3. 【修改】直接切换到目标版本目录（不拷贝文件）：
     - 旧版本目录保留
     - 新版本目录已存在（基线升级时创建）
  4. 更新 cmx_plugin_versions：旧版本 is_current=false，新版本 is_current=true
  5. 更新 cmx_plugin_deployments：节点版本更新为目标版本
  6. 注意：cmx_plugin 主表基线版本不变
```

### 4.2 降级 (downgrade.rs)

**触发条件**: 节点版本 > 基线版本

```
降级流程:
  1. 查询 cmx_plugin_versions，找到目标版本的目录
  2. 【修改】直接切换到目标版本目录（不拷贝文件）：
     - 当前版本目录保留
     - 目标版本目录已存在
  3. 更新 cmx_plugin_versions：目标版本 is_current=true，旧版本 is_current=false
  4. 更新 cmx_plugin_deployments：节点版本更新为目标版本
  5. 注意：cmx_plugin 主表基线版本不变
```

### 4.3 卸载 (uninstall.rs)

```
卸载流程:
  1. 查询 cmx_plugin_deployments，检查是否还有其他节点安装此插件
  2. 删除 cmx_plugin_deployments 当前节点记录
  3. 如果没有其他节点：
     - 标记 cmx_plugin_versions 当前版本 uninstalled_at
     - 保留 cmx_plugin 主表记录
     - 【可选】删除版本目录（保留基线版本目录）
  4. 如果有其他节点：
     - 找到这些节点中的最高版本
     - 更新 cmx_plugin 主表基线版本
     - 更新 cmx_plugin_versions：将该版本标记为 is_current=true
```

***

## 五、节点同步服务（新增）

### 5.1 NodeSyncService

```rust
pub struct NodeSyncService {
    deps: NodeSyncServiceDeps,
}

impl NodeSyncService {
    /// 同步节点上的所有插件
    pub async fn sync_node_plugins(&self, node_id: &str) -> PluginResult<SyncResult> {
        let deployments = self.deps.deployment_repository
            .list_node_deployments(node_id)
            .await?;
        let plugins = self.deps.repository.list_plugins().await?;

        let mut upgrades = Vec::new();
        let mut downgrades = Vec::new();
        let mut synced = Vec::new();

        for plugin in plugins {
            let deployment = deployments.iter().find(|d| d.plugin_id == plugin.plugin_id);

            match deployment {
                Some(d) => {
                    let baseline = SemanticVersion::parse(&plugin.version)?;
                    let current = SemanticVersion::parse(&d.version)?;

                    if current < baseline {
                        upgrades.push((plugin.plugin_id.clone(), d.version.clone(), plugin.version.clone()));
                    } else if current > baseline {
                        downgrades.push((plugin.plugin_id.clone(), d.version.clone(), plugin.version.clone()));
                    } else {
                        synced.push(plugin.plugin_id.clone());
                    }
                }
                None => {
                    upgrades.push((plugin.plugin_id.clone(), "none".to_string(), plugin.version.clone()));
                }
            }
        }

        Ok(SyncResult { upgrades, downgrades, synced })
    }
}
```

***

## 六、文件变更清单

| 文件                                           | 变更类型 | 说明              |
| -------------------------------------------- | ---- | --------------- |
| `sql/plugin_lifecycle_schemanojson.sql`      | 修改   | 增加标准字段          |
| `infrastructure/database/repository.rs`      | 修改   | 新增基线版本相关方法      |
| `infrastructure/database/mod.rs`             | 修改   | 导出新仓库           |
| `infrastructure/database/deployment.rs`      | 新增   | 部署记录仓库          |
| `infrastructure/database/version_history.rs` | 新增   | 版本历史仓库          |
| `service/install.rs`                         | 修改   | 集成节点部署逻辑，修改安装路径 |
| `service/upgrade.rs`                         | 修改   | 简化升级（只切换目录）     |
| `service/downgrade.rs`                       | 修改   | 简化降级（只切换目录）     |
| `service/uninstall.rs`                       | 修改   | 集成节点部署清理        |
| `service/sync.rs`                            | 新增   | 节点同步服务          |
| `service/mod.rs`                             | 修改   | 导出新服务           |
| `lib.rs`                                     | 修改   | 导出新增类型          |

***

## 七、开发步骤

### Step 2: 创建基础设施层

1. 创建 `infrastructure/database/deployment.rs`
2. 创建 `infrastructure/database/version_history.rs`
3. 修改 `infrastructure/database/mod.rs`
4. 修改 `repository.rs` 新增方法

### Step 3: 创建 NodeSyncService

1. 创建 `service/sync.rs`
2. 修改 `service/mod.rs`

### Step 4: 修改 InstallService

1. 修改 `InstallServiceDeps` 添加新字段
2. 修改安装路径计算逻辑 `plugin_id/version`
3. 添加节点部署检查逻辑
4. 添加版本历史记录逻辑
5. 添加主表更新逻辑

### Step 5: 修改 UpgradeService

1. 简化为只切换目录（不需要拷贝文件）
2. 修改版本历史记录逻辑
3. 修改节点部署记录逻辑

### Step 6: 修改 DowngradeService

1. 简化为只切换目录
2. 修改版本历史记录逻辑
3. 修改节点部署记录逻辑

### Step 7: 修改 UninstallService

1. 添加节点部署清理逻辑
2. 添加主表基线版本协调逻辑

### Step 8: 更新数据结构

1. 在各 Record 结构体中添加标准字段
2. 更新审计日志字段

### Step 9: 编译验证

1. 运行 `cargo check -p cmx-plugin`
2. 修复任何编译错误

***

## 八、关键设计决策

### 8.1 安装目录结构

* 路径: `plugin_root / plugin_id / version`

* 每个版本有独立目录，升级/降级只切换目录

* 历史版本目录保留，支持快速回滚

### 8.2 标准字段

所有表增加: `archived`, `create_by`, `create_name`, `update_by`, `update_name`

### 8.3 版本关系总结

| 场景        | cmx\_plugin     | cmx\_plugin\_deployments | 操作        |
| --------- | --------------- | ------------------------ | --------- |
| 首次安装      | 插入(version=新版本) | 插入(version=新版本)          | 创建版本目录    |
| 节点版本 < 基线 | 不变              | 更新为基线版本                  | 切换到基线版本目录 |
| 节点版本 > 基线 | 不变              | 更新为基线版本                  | 切换到基线版本目录 |
| 节点版本 = 基线 | 不变              | 不变                       | 无需操作      |

### 8.4 cmx\_plugin\_versions.is\_current

表示这是当前的基线版本，不是节点部署版本。

### 8.5 卸载时处理

不删除 cmx\_plugin 记录，版本目录保留便于审计追踪。
