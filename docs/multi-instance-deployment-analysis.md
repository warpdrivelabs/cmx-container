# cmx-plugin 多实例部署问题与无状态化需求

## 一、背景

cmx-container 项目采用微服务架构部署，服务实例可能以多副本方式运行。在多实例部署场景下，插件的初始化安装、安装、卸载、升级等生命周期操作需要在多个实例之间保持一致性。

---

## 二、当前系统设计

### 2.1 核心数据库表

| 表名 | 用途 | 关键字段 |
|------|------|----------|
| `cmx_plugin` | 插件主记录（基线版本） | plugin_id, version, zip_source_url, zip_source_type |
| `cmx_plugin_versions` | 版本历史 | plugin_id, version, is_current |
| `cmx_plugin_deployments` | 节点部署记录 | plugin_id, node_id, version |

### 2.2 插件来源机制

插件文件不存储在本地，而是通过 `zip_source_type` 和 `zip_source_url` 动态获取：

```rust
pub enum PluginSource {
    Local { path: PathBuf },           // 本地路径
    Remote { url: String, checksum: Option<String> },  // 远程 URL
    Registry { registry_url: Option<String>, package_name: String },  // 注册表
}
```

### 2.3 生命周期服务

| 操作 | 影响范围 | 代码位置 |
|------|----------|----------|
| 安装 (install) | 仅当前节点 | `crates/libs/cmx-plugin/src/service/install.rs` |
| 升级 (upgrade) | 仅当前节点 | `crates/libs/cmx-plugin/src/service/upgrade.rs` |
| 卸载 (uninstall) | 仅当前节点 | `crates/libs/cmx-plugin/src/service/uninstall.rs` |
| 部署 (deploy) | 仅当前节点 | `crates/libs/cmx-plugin/src/service/deploy.rs` |

### 2.4 启动同步机制

`PluginInitializer` 负责启动时的插件同步：

```rust
// 1. 查询 cmx_plugin → 获取"期望安装的插件列表"
// 2. 查询 cmx_plugin_deployments（当前节点）→ 获取"已部署版本"
// 3. 对比生成操作计划：Install / Upgrade / Downgrade / Uninstall
// 4. 执行计划
// 5. 加载 contexts 到内存
```

---

## 三、存在的问题

### 问题 1：多实例部署时缺少主动跨节点同步

**现状**：
- 在节点 A 执行 install/upgrade/uninstall 时，只写当前节点的 `cmx_plugin_deployments` 记录
- **不主动通知**其他节点
- 其他节点只能等**下次启动时**才能发现变化

**影响**：
- 节点 A 安装插件后，节点 B 不会立刻知道
- 需要等节点 B 重启或手动触发同步

**代码证据**：

`sync.rs` 定义了完整的 Pub/Sub 同步机制，但生命周期服务中**未被调用**：

```rust
// crates/libs/cmx-plugin/src/cluster/sync.rs
pub async fn sync_to_remote(&self, plugin_id: &str) -> Result<(), String> {
    // 发布消息到 Redis Pub/Sub
    pubsub.publish(&self.sync_channel, &message_json).await
}
```

但 `InstallService`、`UpgradeService`、`UninstallService` 均未调用此方法。

### 问题 2：SyncManager 未集成

`sync.rs` 定义了消息类型但从未被使用：

```rust
pub enum SyncMessageType {
    StateUpdate,        // 状态更新
    StateDelete,        // 状态删除
    FullSyncRequest,    // 全量同步请求
    FullSyncResponse,   // 全量同步响应
}
```

### 问题 3：全量同步机制不完整

`sync_from_remote()` 发送请求后不等待响应：

```rust
// crates/libs/cmx-plugin/src/cluster/sync.rs#L155-176
pub async fn sync_from_remote(&self) -> Result<Vec<PluginStateRecord>, String> {
    pubsub.publish(&self.sync_channel, &message_json).await?;
    Ok(self.get_all_local_states().await)  // 直接返回本地状态，未等待远程响应
}
```

### 问题 4：部署策略未实现

虽然定义了 `DeploymentStrategy`，但实际生命周期服务中未被使用：

```rust
pub enum DeploymentStrategy {
    AllNodes,         // 所有节点
    SpecificNodes,    // 指定节点
    PrimaryOnly,      // 仅主节点
    RandomNodes(usize),
}
```

### 问题 5：node_id 配置不适合容器化部署

**现状**：`node_id` 通过配置文件或环境变量设置，每个实例需要不同的值。

**问题场景**：

#### 5.1 Kubernetes 部署

K8s Pod 名称对于无状态 Deployment 来说每次重启都可能变化，不适合手动配置不同的 node_id。

#### 5.2 微服务 + Nacos 配置中心

使用 Nacos 作为配置中心时，同一服务的所有实例共享一份配置文件，无法为每个实例设置不同的 node_id。

```yaml
# Nacos 配置（所有实例共享）
spring:
  application:
    name: cmx-server
plugin:
  node_id: ???  # 无法设置为不同值
```

#### 5.3 现有配置

```rust
// crates/libs/cmx-plugin/src/config/settings.rs
pub struct PluginManagerSettings {
    pub node_id: String,        // 需要手动配置
    pub node_name: Option<String>,
    pub node_type: Option<String>,
}
```

### 问题 6：数据库孤儿记录

当节点消失（如 Pod 删除重建但 node_id 变了）时，旧的 deployment 记录变成孤儿数据：

```rust
// crates/libs/cmx-plugin/src/service/uninstall.rs#L134-137
pub async fn delete_deployments_by_plugin_id(&self, plugin_id: &str, txn_id: Option<&str>) {
    // 只按 plugin_id 删除，不检查节点是否存在
}
```

---

## 四、无状态模式需求

### 4.1 目标

彻底去除 `node_id` 维度，实现无状态化设计：

1. **配置简化**：不需要配置 node_id
2. **数据库简化**：`cmx_plugin_deployments` 表可以退役
3. **同步机制简化**：所有实例共享插件状态，实例间无差异
4. **插件来源**：继续依赖 `zip_source_type` 和 `zip_source_url` 获取插件

### 4.2 设计原则

1. **插件状态以数据库为准**：所有实例共享 `cmx_plugin` 和 `cmx_plugin_versions` 表
2. **插件文件按需下载**：每个实例通过 `zip_source_url` 独立下载插件文件到本地
3. **启动时自动同步**：实例启动时检查本地插件版本与数据库版本，不一致则重新下载
4. **插件版本唯一性**：同一时刻只有一个"当前版本"（由 `cmx_plugin_versions.is_current` 标识）
5. **降级回退到安装逻辑**：降级时如果本地没有目标版本，需要走完整的安装流程
6. **操作加锁防重**：跨节点同步时需要加锁，防止同一插件操作并发执行
7. **事件防循环**：发布事件时需要标记来源，避免重复处理导致死循环

### 4.3 核心改动

#### 4.3.1 数据库设计变更

**退役表**：`cmx_plugin_deployments`

此表用于记录"哪个插件部署在哪个节点"，无状态模式下不再需要。

**保留表**：
- `cmx_plugin`：插件主记录
- `cmx_plugin_versions`：版本历史

**新增表**（可选，用于记录本地缓存状态）：

```sql
-- 本地插件缓存记录（可选，用于无状态化改造过渡期）
CREATE TABLE cmx_plugin_local_cache (
    plugin_id VARCHAR(255) PRIMARY KEY,
    version VARCHAR(50) NOT NULL,
    cached_at TIMESTAMP DEFAULT NOW(),
    source_url TEXT,
    source_type VARCHAR(50)
);
```

#### 4.3.2 启动同步逻辑变更

**当前逻辑**（有状态）：
```
1. 查询 cmx_plugin → 期望插件
2. 查询 cmx_plugin_deployments（当前节点）→ 已部署版本
3. 对比决定操作
```

**新逻辑**（无状态）：
```
1. 查询 cmx_plugin → 期望插件
2. 查询 cmx_plugin_versions 获取 is_current = true 的版本
3. 检查本地 ${plugin_root}/${plugin_id}/${version}/ 是否存在
4. 版本一致 → 跳过
5. 版本不一致 → 清理旧版本，重新下载安装
6. 本地无插件 → 下载安装
```

#### 4.3.3 安装/升级/卸载逻辑变更

**当前逻辑**（有状态）：
```rust
// 写入当前节点的 deployment 记录
insert_deployment(&plugin_id, &self.node_id, &version).await?;
```

**新逻辑**（无状态）：
```rust
// 不再写入 deployment 记录
// 只需要更新 cmx_plugin 和 cmx_plugin_versions 表
upsert_plugin(&db_record).await?;
upsert_version(&version_record).await?;
set_current_version(&plugin_id, &version).await?;
```

#### 4.3.4 本地文件存储

每个实例在本地存储插件文件：

```
${plugin_root}/
├── plugin-a/
│   ├── 1.0.0/          # version directory
│   │   ├── manifest.json
│   │   └── wasm/
│   └── 1.1.0/
├── plugin-b/
│   └── 2.0.0/
```

启动时对比本地版本与数据库版本，不一致则清理后重新下载。

---

## 五、跨节点同步详细设计

### 5.1 同步消息类型

扩展 `SyncMessageType` 枚举，使用 `source_instance_id` 替代 `source_node_id`：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SyncMessageType {
    /// 插件安装请求（广播）
    InstallRequest {
        plugin_id: String,
        version: String,
        source: PluginSource,
        source_instance_id: String,  // 请求来源实例 ID
        request_id: String,           // 请求唯一 ID，用于防重放
    },
    /// 插件安装完成通知（用于更新本地状态）
    InstallCompleted {
        plugin_id: String,
        version: String,
        source_instance_id: String,   // 安装完成的实例 ID
        request_id: String,           // 对应请求的 ID
    },
    /// 插件升级请求
    UpgradeRequest {
        plugin_id: String,
        old_version: String,
        new_version: String,
        source: PluginSource,
        source_instance_id: String,
        request_id: String,
    },
    /// 插件升级完成通知
    UpgradeCompleted {
        plugin_id: String,
        old_version: String,
        new_version: String,
        source_instance_id: String,
        request_id: String,
    },
    /// 插件卸载请求
    UninstallRequest {
        plugin_id: String,
        version: String,
        source_instance_id: String,
        request_id: String,
    },
    /// 插件卸载完成通知
    UninstallCompleted {
        plugin_id: String,
        source_instance_id: String,
        request_id: String,
    },
    /// 全量同步请求（节点启动时主动拉取）
    FullSyncRequest {
        source_instance_id: String,
    },
    /// 全量同步响应
    FullSyncResponse {
        plugins: Vec<PluginStateRecord>,
        source_instance_id: String,
    },
}
```

### 5.2 同步消息流转设计

```
                    节点 A (操作发起者)
                           │
                           ▼
              ┌────────────────────────┐
              │  1. 发起操作请求       │
              │  - 生成 request_id     │
              │  - 获取分布式锁        │
              │  - 写入数据库          │
              └───────────┬────────────┘
                          │
                          ▼
              ┌────────────────────────┐
              │  2. 发布请求消息       │
              │  (InstallRequest)      │
              └───────────┬────────────┘
                          │
          ┌───────────────┼───────────────┐
          ▼               ▼               ▼
      节点 A          节点 B          节点 C
    (自身跳过)     (处理请求)      (处理请求)
          │               │               │
          │               ▼               ▼
          │    ┌────────────────────────┐
          │    │  3. 检查本地状态        │
          │    │  - 是否已安装？        │
          │    │  - 版本是否一致？      │
          │    │  - 是否正在处理此请求？ │
          │    └───────────┬────────────┘
          │                │
          │                ▼
          │    ┌────────────────────────┐
          │    │  4. 执行安装/升级      │
          │    │  (下载、解压、验证)    │
          │    └───────────┬────────────┘
          │                │
          │                ▼
          │    ┌────────────────────────┐
          │    │  5. 发布完成消息        │
          │    │  (InstallCompleted)    │
          │    └───────────┬────────────┘
          │                │
          │                ▼
          │    ┌────────────────────────┐
          │    │  6. 释放分布式锁        │
          │    └────────────────────────┘
          │                │
          ▼                ▼
      节点 A          节点 B/C
    (收到完成)      (处理完成消息)
          │               │
          │               ▼
          │    ┌────────────────────────┐
          │    │  7. 更新本地缓存状态    │
          │    └────────────────────────┘
          │
          ▼
    节点 A 完成操作
```

### 5.3 分布式锁设计

使用 Redis 分布式锁防止并发操作：

```rust
/// 插件操作锁的 Key 格式
fn plugin_lock_key(plugin_id: &str) -> String {
    format!("cmx:plugin:lock:{}", plugin_id)
}

/// 分布式锁管理器
pub struct PluginLockManager {
    lock_manager: Arc<LockManager>,
}

impl PluginLockManager {
    /// 尝试获取插件操作锁
    pub async fn try_lock(&self, plugin_id: &str) -> PluginResult<Option<LockGuard>> {
        let lock_key = plugin_lock_key(plugin_id);
        let lock = self.lock_manager.lock(&lock_key).await
            .map_err(|e| PluginError::Lock(format!("获取锁失败: {}", e)))?;
        Ok(Some(lock))
    }

    /// 执行带锁的操作
    pub async fn with_lock<F, T>(&self, plugin_id: &str, operation: F) -> PluginResult<T>
    where
        F: Future<Output = PluginResult<T>>,
    {
        let lock = self.try_lock(plugin_id).await?
            .ok_or_else(|| PluginError::Lock(format!("插件 {} 正在被其他操作占用", plugin_id)))?;

        let result = operation.await?;

        // 锁在 drop 时自动释放
        Ok(result)
    }
}
```

### 5.4 防重放设计

每个同步请求携带唯一的 `request_id`，用于防止重复处理：

```rust
/// 已处理的请求记录（存储在 Redis 中，设置 TTL）
pub struct RequestDeduplicator {
    redis: Arc<RedisClient>,
}

impl RequestDeduplicator {
    const REQUEST_TTL_SECONDS: i64 = 300;  // 5 分钟内不重复处理

    /// 检查请求是否已处理
    pub async fn is_duplicate(&self, request_id: &str) -> PluginResult<bool> {
        let key = format!("cmx:plugin:request:{}", request_id);
        let exists = self.redis.exists(&key).await?;
        Ok(exists)
    }

    /// 标记请求已处理
    pub async fn mark_processed(&self, request_id: &str) -> PluginResult<()> {
        let key = format!("cmx:plugin:request:{}", request_id);
        self.redis.set_ex(&key, "1", Self::REQUEST_TTL_SECONDS).await?;
        Ok(())
    }
}
```

### 5.5 实例标识与事件防循环设计

**重要澄清**：无状态模式下不再需要**配置文件中的** `node_id`，但每个实例**运行时**仍需要一个唯一的标识（`instance_id`）来识别自身，用于事件防循环。

#### 5.5.1 实例标识的来源

```rust
/// 实例唯一标识（运行时自动生成，不依赖配置）
pub struct InstanceId {
    /// 标识符
    pub id: String,
    /// 生成方式
    pub source: InstanceIdSource,
}

pub enum InstanceIdSource {
    /// 从持久化文件读取（微服务场景）
    File,
    /// K8s Pod UID
    Kubernetes,
    /// 自动生成（临时实例）
    AutoGenerated,
}

/// 获取当前实例的唯一标识
pub fn get_instance_id() -> String {
    // 1. 尝试从持久化文件读取
    if let Ok(id) = std::fs::read_to_string("/data/cmx/instance_id") {
        if !id.trim().is_empty() {
            return id.trim().to_string();
        }
    }

    // 2. K8s 环境
    if let Ok(pod_uid) = std::env::var("NODE_ID") {  // K8s Downward API
        if !pod_uid.is_empty() {
            return pod_uid;
        }
    }

    // 3. 自动生成并持久化
    let auto_id = format!("instance-{}", uuid::Uuid::new_v4());
    if let Some(parent) = PathBuf::from("/data/cmx/instance_id").parent() {
        let _ = std::fs::create_dir_all(parent);
        let _ = std::fs::write("/data/cmx/instance_id", &auto_id);
    }
    auto_id
}
```

#### 5.5.2 事件防循环设计

**核心原则**：消息中携带来源实例 ID，本地实例接收到消息时检查是否是自己发的，如果是则跳过。

```rust
/// 同步消息结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncMessage {
    /// 消息类型
    pub msg_type: SyncMessageType,
    /// 来源实例 ID（用于防循环）
    pub source_instance_id: String,
    /// 请求唯一 ID（用于防重放）
    pub request_id: Option<String>,
    /// 时间戳
    pub timestamp: DateTime<Utc>,
}

pub async fn handle_sync_message(&self, message: &SyncMessage) -> PluginResult<()> {
    // 1. 获取当前实例 ID
    let current_instance_id = self.get_current_instance_id();

    // 2. 检查是否是自身发送的消息（防循环）
    if message.source_instance_id == current_instance_id {
        tracing::debug!("忽略自身发送的消息: {:?}", message);
        return Ok(());
    }

    // 3. 检查请求是否已处理（防重放）
    if let Some(request_id) = &message.request_id {
        if self.deduplicator.is_duplicate(request_id).await? {
            tracing::debug!("忽略重复请求: {}", request_id);
            return Ok(());
        }
    }

    // 4. 根据消息类型处理
    match &message.msg_type {
        SyncMessageType::InstallRequest(msg) => {
            self.handle_install_request(msg).await?;
        }
        SyncMessageType::InstallCompleted(msg) => {
            self.handle_install_completed(msg).await?;
        }
        // ... 其他类型
    }
}
```

#### 5.5.3 消息类型中的实例标识

所有同步消息都需要携带 `source_instance_id`：

```rust
pub enum SyncMessageType {
    InstallRequest {
        plugin_id: String,
        version: String,
        source: PluginSource,
        source_instance_id: String,  // 来源实例
        request_id: String,           // 请求唯一 ID
    },
    InstallCompleted {
        plugin_id: String,
        version: String,
        source_instance_id: String,   // 哪个实例完成的
        request_id: String,
    },
    // ... 其他消息类型
}
```

#### 5.5.4 防循环 vs 防重放

| 机制 | 目的 | 实现方式 |
|------|------|----------|
| 防循环 | 避免处理自己发出的消息 | 比较 `source_instance_id` 与当前实例 ID |
| 防重放 | 避免重复处理同一请求 | Redis 存储 `request_id`，设置 TTL |

**关键区别**：
- **防循环**：基于实例 ID，始终有效
- **防重放**：基于请求 ID，有 TTL（建议 5 分钟），防止旧请求在队列中堆积后被重复处理

### 5.6 降级逻辑的边界处理

降级操作需要特别注意本地是否存在目标版本：

```rust
impl PluginInitializer {
    /// 同步单个插件（包含降级处理）
    async fn sync_single_plugin(&self, plugin_id: &str, expected_version: &str) -> PluginResult<SyncAction> {
        let local_version = self.get_local_cached_version(plugin_id).await?;

        match local_version {
            Some(local_ver) => {
                match local_ver.cmp(expected_version) {
                    Ordering::Equal => {
                        // 版本一致，跳过
                        Ok(SyncAction::Skip)
                    }
                    Ordering::Less => {
                        // 本地版本低于期望，升级
                        Ok(SyncAction::Upgrade)
                    }
                    Ordering::Greater => {
                        // 本地版本高于期望，降级
                        // 关键：检查本地是否有目标版本
                        if self.is_version_cached(plugin_id, expected_version).await? {
                            // 目标版本已缓存，执行降级
                            Ok(SyncAction::Downgrade)
                        } else {
                            // 目标版本未缓存，降级失败
                            // 方案 A：降级转为安装（重新下载目标版本）
                            tracing::warn!(
                                "插件 {} 目标版本 {} 未缓存，降级转为重新安装",
                                plugin_id, expected_version
                            );
                            Ok(SyncAction::Reinstall)
                        }
                    }
                }
            }
            None => {
                // 本地没有插件，执行安装
                Ok(SyncAction::Install)
            }
        }
    }
}

/// 同步操作类型
enum SyncAction {
    Skip,
    Install,
    Upgrade,
    Downgrade,
    Reinstall,
}
```

### 5.7 完整的跨节点同步流程

```rust
impl SyncManager {
    /// 处理安装请求（被其他节点通知）
    pub async fn handle_install_request(&self, msg: &InstallRequest) -> PluginResult<()> {
        let plugin_id = &msg.plugin_id;

        // 1. 防重放检查
        if self.deduplicator.is_duplicate(&msg.request_id).await? {
            return Ok(());
        }

        // 2. 检查本地是否已安装目标版本
        let local_version = self.get_local_cached_version(plugin_id).await?;
        if local_version.as_deref() == Some(&msg.version) {
            tracing::info!("节点 {} 插件 {} 版本 {} 已安装，跳过", self.current_node_id, plugin_id, msg.version);
            return Ok(());
        }

        // 3. 获取分布式锁
        let lock = match self.lock_manager.try_lock(plugin_id).await? {
            Some(lock) => lock,
            None => {
                tracing::info!("插件 {} 正在被其他操作占用，稍后重试", plugin_id);
                return Ok(());
            }
        };

        // 4. 标记请求已处理
        self.deduplicator.mark_processed(&msg.request_id).await?;

        // 5. 执行安装
        let install_result = self.do_install(plugin_id, &msg.version, &msg.source).await;

        // 6. 释放锁
        drop(lock);

        // 7. 发布安装完成事件（只有请求才需要发布完成通知）
        if install_result.is_ok() {
            self.publish_completed(InstallCompleted {
                plugin_id: plugin_id.clone(),
                version: msg.version.clone(),
                source_node_id: self.current_node_id.clone(),
                request_id: msg.request_id.clone(),
            }).await?;
        }

        Ok(())
    }

    /// 处理安装完成通知（不需要再次执行安装）
    pub async fn handle_install_completed(&self, msg: &InstallCompleted) -> PluginResult<()> {
        // 更新本地缓存状态（如果需要）
        // 注意：这里不需要执行实际的安装操作
        tracing::info!(
            "节点 {} 已完成插件 {} 版本 {} 的安装",
            msg.source_node_id, msg.plugin_id, msg.version
        );
        Ok(())
    }
}
```

---

## 六、实现方案

### 6.1 统一节点标识解析（兼容过渡期）

如果短期内无法完全去除 node_id，至少需要统一标识解析逻辑：

```rust
/// 统一节点 ID 解析，兼容多种部署环境
pub async fn resolve_node_id() -> String {
    // 1. 显式配置优先
    if let Ok(id) = std::env::var("CMX_NODE_ID") {
        if !id.is_empty() && id != "auto" {
            return id;
        }
    }

    // 2. K8s Pod Name
    if let Ok(pod_name) = std::env::var("POD_NAME") {
        if !pod_name.is_empty() {
            return pod_name;
        }
    }

    // 3. 本地持久化文件（微服务场景）
    let id_file = PathBuf::from(std::env::var("CMX_NODE_ID_FILE")
        .unwrap_or_else(|_| "/data/cmx/node_id".to_string()));
    if let Ok(content) = tokio::fs::read_to_string(&id_file).await {
        let id = content.trim().to_string();
        if !id.is_empty() {
            return id;
        }
    }

    // 4. Nacos 实例元数据
    if let Some(nacos_id) = try_get_nacos_instance_id().await {
        return nacos_id;
    }

    // 5. 自动生成并持久化
    let auto_id = generate_node_id();
    if let Some(parent) = id_file.parent() {
        let _ = tokio::fs::create_dir_all(parent).await;
        let _ = tokio::fs::write(&id_file, &auto_id).await;
    }
    auto_id
}
```

### 6.2 无状态化数据库改动

#### 6.2.1 退役 deployment 表

可选方案：

**方案 A：直接删除**
```sql
DROP TABLE cmx_plugin_deployments;
```

**方案 B：保留用于监控（推荐过渡期使用）**

添加清理策略，定期删除孤儿记录，或添加 trigger，实例启动时清理自己的旧记录。

#### 6.2.2 版本表增加标识字段

确保 `cmx_plugin_versions` 能够唯一标识当前版本：

```sql
ALTER TABLE cmx_plugin_versions
ADD COLUMN IF NOT EXISTS locally_cached BOOLEAN DEFAULT false;

ALTER TABLE cmx_plugin_versions
ADD COLUMN IF NOT EXISTS locally_cached_at TIMESTAMP;
```

### 6.3 无状态启动同步实现

```rust
impl PluginInitializer {
    pub async fn sync_plugins(&self) -> PluginResult<PluginSyncResult> {
        // 1. 查询 cmx_plugin 获取所有期望插件
        let expected_plugins = self.repository.list_plugins(&Default::default()).await?;

        // 2. 遍历期望插件，决定操作
        for plugin in expected_plugins {
            let expected_version = &plugin.version;

            // 检查本地版本状态
            let local_path = self.plugin_root
                .join(&plugin.plugin_id)
                .join(expected_version);

            if local_path.exists() {
                // 本地已存在此版本，检查是否需要更新
                let local_meta = self.load_plugin_metadata(&local_path)?;
                if local_meta.version == *expected_version {
                    result.skipped.push(plugin.plugin_id.clone());
                    continue;
                }
            }

            // 需要安装或升级
            match self.decide_sync_action(&plugin.plugin_id, expected_version).await? {
                SyncAction::Skip => {
                    result.skipped.push(plugin.plugin_id.clone());
                }
                SyncAction::Install => {
                    self.install_plugin(&plugin, expected_version).await?;
                    result.installed.push(plugin.plugin_id.clone());
                }
                SyncAction::Upgrade => {
                    self.upgrade_plugin(&plugin, expected_version).await?;
                    result.upgraded.push(plugin.plugin_id.clone());
                }
                SyncAction::Downgrade | SyncAction::Reinstall => {
                    // 降级或重新安装都走相同流程
                    self.reinstall_plugin(&plugin, expected_version).await?;
                    result.upgraded.push(plugin.plugin_id.clone());
                }
            }
        }

        // 3. 加载 contexts
        self.load_contexts().await?;
        Ok(result)
    }

    /// 决定同步操作类型
    async fn decide_sync_action(&self, plugin_id: &str, expected_version: &str) -> PluginResult<SyncAction> {
        let local_version = self.get_local_version(plugin_id).await?;

        match local_version {
            None => Ok(SyncAction::Install),
            Some(local_ver) => {
                match local_ver.cmp(expected_version) {
                    Ordering::Equal => Ok(SyncAction::Skip),
                    Ordering::Less => Ok(SyncAction::Upgrade),
                    Ordering::Greater => {
                        // 降级：检查目标版本是否已缓存
                        if self.is_version_cached(plugin_id, expected_version).await? {
                            Ok(SyncAction::Downgrade)
                        } else {
                            // 目标版本未缓存，需要重新下载安装
                            tracing::warn!(
                                "插件 {} 降级目标版本 {} 未缓存，将重新下载安装",
                                plugin_id, expected_version
                            );
                            Ok(SyncAction::Reinstall)
                        }
                    }
                }
            }
        }
    }
}
```

### 6.4 生命周期服务改动

#### 6.4.1 InstallService

移除 `node_id` 相关逻辑，增加同步事件发布：

```rust
// After (无状态模式)
impl InstallService {
    pub async fn install(&self, request: InstallRequest) -> PluginResult<InstallResponse> {
        // 1. 执行安装（写入 cmx_plugin, cmx_plugin_versions）
        let result = self.do_install(request).await?;

        // 2. 发布安装完成事件（用于跨节点同步）
        let event = PluginLifecycleEvent::Installed {
            plugin_id: result.plugin_id.clone(),
            version: result.version.clone(),
            source_node_id: self.current_node_id.clone(),
        };
        self.event_bus.publish(plugin_events::INSTALLED, serde_json::to_value(&event)?).await;

        Ok(result)
    }
}
```

#### 6.4.2 UpgradeService

类似移除 `node_id` 相关逻辑。

#### 6.4.3 UninstallService

类似移除 `node_id` 相关逻辑。

---

## 七、待解决问题清单

| 序号 | 问题 | 优先级 | 状态 |
|------|------|--------|------|
| 1 | 生命周期服务缺少主动跨节点同步 | 高 | 待修复 |
| 2 | SyncManager 未集成 | 高 | 待修复 |
| 3 | node_id 配置不适合容器化部署 | 高 | 待修复 |
| 4 | 数据库孤儿记录 | 中 | 待修复 |
| 5 | 部署策略未实现 | 中 | 待设计 |
| 6 | 无状态化改造 | 高 | 待实现 |
| 7 | 降级时本地无目标版本的处理 | 高 | 待实现 |
| 8 | 跨节点同步需要加锁防并发 | 高 | 待实现 |
| 9 | 事件通知需要防死循环 | 高 | 待实现 |

---

## 八、建议实施步骤

### Phase 1：短期修复（不破坏兼容性）

1. 统一 node_id 解析逻辑，兼容 K8s、微服务等多种部署环境
2. 实现启动时孤儿记录清理
3. 定义完整的同步消息类型（包含 request_id 防重）
4. 实现分布式锁基础架构
5. 实现 SyncManager 消息处理（包含自身消息过滤）

### Phase 2：中期改造（向无状态化过渡）

1. 在 `cmx_plugin_versions` 表增加 `locally_cached` 字段
2. 启动同步改为对比本地缓存版本与数据库版本
3. 生命周期服务移除对 `cmx_plugin_deployments` 的写入
4. 实现降级时的安装回退逻辑
5. 集成 SyncManager 到生命周期服务

### Phase 3：长期目标（完全无状态化）

1. 退役 `cmx_plugin_deployments` 表
2. 简化配置，彻底移除 node_id 配置项
3. 优化插件文件存储和清理策略

---

## 九、附录

### A. 相关文件路径

| 文件 | 用途 |
|------|------|
| `crates/libs/cmx-plugin/src/service/install.rs` | 安装服务 |
| `crates/libs/cmx-plugin/src/service/upgrade.rs` | 升级服务 |
| `crates/libs/cmx-plugin/src/service/uninstall.rs` | 卸载服务 |
| `crates/libs/cmx-plugin/src/service/initializer.rs` | 启动同步 |
| `crates/libs/cmx-plugin/src/cluster/sync.rs` | 同步管理器（未使用） |
| `crates/libs/cmx-plugin/src/cluster/node.rs` | 节点管理器 |
| `crates/libs/cmx-plugin/src/cluster/deployment.rs` | 部署协调器（未使用） |
| `crates/libs/cmx-plugin/src/config/settings.rs` | 配置结构 |
| `crates/libs/cmx-plugin/src/infrastructure/database/deployment/repository.rs` | 部署记录仓库 |

### B. Nacos 配置示例

```yaml
spring:
  cloud:
    nacos:
      discovery:
        enabled: true
        service: cmx-server
        metadata:
          node-id: ${NODE_ID:auto}

plugin:
  shared: false
  plugin_root: /data/cmx/plugins
```

### C. K8s Deployment 示例（过渡期使用）

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: cmx-server
spec:
  replicas: 3
  template:
    spec:
      containers:
        - name: cmx
          env:
            - name: POD_NAME
              valueFrom:
                fieldRef:
                  fieldPath: metadata.name
          volumeMounts:
            - name: cmx-data
              mountPath: /data
      volumes:
        - name: cmx-data
          emptyDir: {}
```

### D. Redis 键设计

```
# 分布式锁
cmx:plugin:lock:{plugin_id}                    # 插件操作锁

# 请求防重放
cmx:plugin:request:{request_id}                # 已处理请求记录 (TTL: 5min)

# 插件状态同步频道
cmx:plugin:sync                                 # Pub/Sub 频道
```

### E. 时序图：完整安装同步流程

```
┌────────┐         ┌────────┐         ┌────────┐
│ Client │         │ Node A │         │ Node B │
└───┬────┘         └───┬────┘         └───┬────┘
    │                   │                   │
    │ install request   │                   │
    │──────────────────>│                   │
    │                   │                   │
    │                   │ 1. 获取分布式锁    │
    │                   │───────────────────│
    │                   │<───────────────────│
    │                   │                   │
    │                   │ 2. 写入数据库       │
    │                   │                   │
    │                   │ 3. 发布 InstallRequest │
    │                   │                   │
    │                   │───────────────────│
    │                   │<───────────────────│
    │                   │                   │
    │                   │ 4. (Node B) 接收消息 │
    │                   │      检查本地状态    │
    │                   │      获取分布式锁    │
    │                   │      下载安装插件    │
    │                   │      释放锁         │
    │                   │                   │
    │                   │ 5. 发布 InstallCompleted │
    │                   │                   │
    │   install response│<───────────────────│
    │<──────────────────│                   │
    │                   │                   │
    │                   │ 6. (Node B) 收到完成 │
    │                   │      更新本地缓存   │
    │                   │                   │
```

---

## 十、变更记录

| 日期 | 版本 | 变更内容 |
|------|------|----------|
| 2026-05-08 | v1.0 | 初始版本，描述多实例部署问题和无状态化需求 |
| 2026-05-08 | v1.1 | 新增跨节点同步详细设计（消息类型、锁、防重放、事件防循环） |
| 2026-05-08 | v1.2 | 新增降级逻辑边界处理（本地无目标版本时回退到安装） |
| 2026-05-08 | v1.3 | 修正事件防循环设计：使用 `instance_id` 替代 `node_id`，区分配置 vs 运行时标识 |
