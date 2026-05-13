# cmx-plugin 多实例无状态化改造方案

## 一、对现有分析文档的评审

### 1.1 问题识别评估

| 文档识别的问题 | 评估 | 说明 |
|---|---|---|
| 问题1：缺少跨节点同步 | ✅ 正确 | `cluster/sync.rs` 的 `SyncManager` 已定义但未被 `PluginManager` 集成 |
| 问题2：SyncManager 未集成 | ✅ 正确 | `from_builder()` 中未创建 `SyncManager` 实例 |
| 问题3：全量同步不完整 | ✅ 正确 | `sync_from_remote()` 直接返回本地状态，未等待远程响应 |
| 问题4：部署策略未实现 | ✅ 正确 | `DeploymentStrategy` 枚举已定义但未使用 |
| 问题5：node_id 不适合容器化 | ✅ 正确 | Nacos 共享配置、K8s Pod 名称不可预测 |
| 问题6：数据库孤儿记录 | ✅ 正确 | Pod 重建后旧 deployment 记录无人清理 |

### 1.2 解决方案评估

| 文档提出的方案 | 评估 | 问题描述 |
|---|---|---|
| Redis Pub/Sub 携带完整数据同步 | ⚠️ 过于复杂 | 在消息中传递完整操作数据，各节点独立执行安装，引入了分布式一致性问题 |
| 7+ 种 SyncMessageType | ⚠️ 过度设计 | InstallRequest/InstallCompleted/UpgradeRequest/UpgradeCompleted 等消息类型过多 |
| 分布式锁 + 请求去重 + 防循环 | ⚠️ 机制冗余 | 三套防护机制叠加，复杂度极高 |
| instance_id 自动生成 | ⚠️ 引入新问题 | 文件持久化 `/data/cmx/instance_id` 在 K8s emptyDir 中 Pod 重启后丢失 |
| 新增 `cmx_plugin_local_cache` 表 | ❌ 不必要 | 本地文件系统已能标识缓存状态（目录是否存在），无需额外的数据库表 |

### 1.3 核心问题总结

现有文档的方案本质上是**通过消息传递实现分布式数据同步**——在 Pub/Sub 中传递完整操作上下文，让每个节点独立执行安装/升级。这种方案存在以下根本性缺陷：

1. **一致性风险**：节点 B 接收到 InstallRequest 后执行安装可能失败，导致集群状态不一致
2. **重复劳动**：N 个节点各自下载、解压、安装同一插件，浪费网络和计算资源
3. **复杂度爆炸**：为保证一致性引入的锁、去重、防循环机制相互纠缠
4. **违背无状态原则**：名义上消除 node_id，实际引入了 instance_id，本质上没有简化

---

## 二、推荐方案：数据库通知模式

### 2.1 核心思想

> **数据库是唯一的真相来源（Single Source of Truth），Redis Pub/Sub 只传递轻量级通知信号，不传递业务数据。**

```
┌──────────────────────────────────────────────────────┐
│                     核心原则                          │
├──────────────────────────────────────────────────────┤
│ 1. 数据库（cmx_plugin + cmx_plugin_versions）是权威  │
│ 2. 通知消息只携带 plugin_id + action，不携带业务数据  │
│ 3. 收到通知后从数据库读取最新状态，再决定本地操作      │
│ 4. 本地操作天然幂等（目录已存在则跳过）               │
└──────────────────────────────────────────────────────┘
```

### 2.2 为什么这个方案更好

| 对比维度 | 现有文档方案 | 推荐方案 |
|---|---|---|
| 消息复杂度 | 7+ 种消息类型，携带完整业务数据 | 1 种通知消息，只含 plugin_id + action |
| 一致性保证 | 需要分布式锁 + 请求去重 | 数据库事务保证，通知只是触发信号 |
| 失败处理 | 需要重试、回滚、完成确认 | 幂等操作：失败后下次启动自然修复 |
| 节点负载 | 每个节点独立下载安装 | 同样的负载，但无需分布式协调 |
| instance_id | 必须（防循环） | 可选（重复处理幂等，无害） |
| 代码改动量 | 大（重写 SyncManager + 所有生命周期服务） | 中（修改通知机制 + 修改启动同步） |

### 2.3 架构对比

**现有文档方案（消息携带数据）**：
```
节点A 安装插件 → 写DB → 发布 InstallRequest{plugin_id, version, source, ...} 
                                    ↓
节点B 收到消息 → 检查去重 → 获取锁 → 下载安装 → 发布 InstallCompleted → 释放锁
节点C 收到消息 → 检查去重 → 获取锁 → 下载安装 → 发布 InstallCompleted → 释放锁
```

**推荐方案（数据库通知）**：
```
节点A 安装插件 → 写DB → 发布 PluginChanged{plugin_id, action: "install"}
                                    ↓
节点B 收到通知 → 查询DB获取最新状态 → 对比本地 → 执行安装（幂等）
节点C 收到通知 → 查询DB获取最新状态 → 对比本地 → 执行安装（幂等）
```

---

## 三、详细设计

### 3.1 通知消息设计

极简消息结构，只携带标识信息：

```rust
/// 插件变更通知（极简设计，不携带业务数据）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginChangeNotification {
    /// 插件ID
    pub plugin_id: String,
    /// 变更动作
    pub action: PluginChangeAction,
    /// 通知时间
    pub timestamp: DateTime<Utc>,
}

/// 插件变更动作
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PluginChangeAction {
    /// 插件已安装或版本已变更（安装/升级/降级统一使用此动作）
    Changed,
    /// 插件已卸载
    Removed,
}
```

**设计要点**：
- 只有 2 种动作（Changed / Removed），不区分安装/升级/降级，因为收到通知后会查询数据库获取准确状态
- 不携带 version、source 等业务数据，避免消息和数据库状态不一致
- 不需要 request_id，因为操作是幂等的
- 不需要 source_instance_id，因为重复处理无害

### 3.2 通知发布

在生命周期服务中，事务提交成功后发布通知：

```rust
// install.rs / upgrade.rs / downgrade.rs 事务提交后
txn_guard.commit().await?;

// 发布变更通知（非阻塞，失败不影响主流程）
if let Some(notifier) = &self.deps.plugin_notifier {
    let _ = notifier.notify_changed(&plugin_id).await;
}
```

```rust
// uninstall.rs 事务提交后
txn_guard.commit().await?;

// 发布移除通知
if let Some(notifier) = &self.deps.plugin_notifier {
    let _ = notifier.notify_removed(&plugin_id).await;
}
```

### 3.3 通知处理器

收到通知后，从数据库查询最新状态，与本地对比后执行操作：

```rust
/// 插件变更通知处理器
pub struct PluginChangeHandler {
    /// 数据库仓库
    repository: Arc<PluginRepository>,
    /// 版本历史仓库
    version_history_repository: Arc<VersionHistoryRepository>,
    /// 安装服务
    install_service: InstallService,
    /// 升级服务
    upgrade_service: UpgradeService,
    /// 卸载服务
    uninstall_service: UninstallService,
    /// 插件根目录
    plugin_root: PathBuf,
    /// 插件注册表
    registry: Arc<RwLock<PluginRegistry>>,
    /// 插件上下文
    contexts: Arc<RwLock<HashMap<String, PluginContext>>>,
}

impl PluginChangeHandler {
    /// 处理插件变更通知
    pub async fn handle_change(&self, plugin_id: &str, action: &PluginChangeAction) {
        match action {
            PluginChangeAction::Changed => {
                self.handle_plugin_changed(plugin_id).await;
            }
            PluginChangeAction::Removed => {
                self.handle_plugin_removed(plugin_id).await;
            }
        }
    }

    /// 处理插件变更（安装/升级/降级）
    async fn handle_plugin_changed(&self, plugin_id: &str) {
        // 1. 从数据库查询最新版本
        let db_plugin = match self.repository.find_plugin(plugin_id).await {
            Ok(Some(p)) => p,
            Ok(None) => {
                tracing::warn!("收到插件 {} 变更通知，但数据库中未找到", plugin_id);
                return;
            }
            Err(e) => {
                tracing::error!("查询插件 {} 失败: {}", plugin_id, e);
                return;
            }
        };

        // 2. 检查本地是否已是最新版本
        let local_path = self.plugin_root.join(plugin_id).join(&db_plugin.version);
        if local_path.exists() {
            tracing::debug!("插件 {} 版本 {} 本地已存在，跳过", plugin_id, db_plugin.version);
            return;
        }

        // 3. 根据 zip_source 构建 PluginSource
        let source = build_plugin_source(
            db_plugin.zip_source_url.as_deref(),
            db_plugin.zip_source_type.as_deref(),
        );

        // 4. 执行部署（自动判断安装/升级）
        let request = DeployRequest {
            plugin_id: plugin_id.to_string(),
            source,
            db_id: Some(db_plugin.db_id.clone()),
            force_reinstall: false,
            operator: Some("system_sync".to_string()),
            build_type: None,
        };

        match self.deploy_service.deploy(request).await {
            Ok(_) => tracing::info!("插件 {} 同步完成，版本 {}", plugin_id, db_plugin.version),
            Err(e) => tracing::error!("插件 {} 同步失败: {}", plugin_id, e),
        }
    }

    /// 处理插件移除
    async fn handle_plugin_removed(&self, plugin_id: &str) {
        // 从内存中移除
        {
            let mut registry = self.registry.write().await;
            registry.unregister(plugin_id);
        }
        {
            let mut contexts = self.contexts.write().await;
            contexts.remove(plugin_id);
        }

        // 清理本地文件
        let plugin_dir = self.plugin_root.join(plugin_id);
        if plugin_dir.exists() {
            if let Err(e) = tokio::fs::remove_dir_all(&plugin_dir).await {
                tracing::error!("清理插件 {} 本地文件失败: {}", plugin_id, e);
            }
        }

        tracing::info!("插件 {} 本地清理完成", plugin_id);
    }
}
```

### 3.4 PluginNotifier 组件

```rust
/// 插件变更通知器
/// 
/// 通过 Redis Pub/Sub 发布插件变更通知
pub struct PluginNotifier {
    /// Redis Pub/Sub
    pubsub: Arc<PubSubOps>,
    /// 通知频道
    channel: String,
}

impl PluginNotifier {
    const CHANNEL: &'static str = "cmx:plugin:changed";

    pub fn new(pubsub: Arc<PubSubOps>) -> Self {
        Self {
            pubsub,
            channel: Self::CHANNEL.to_string(),
        }
    }

    /// 发布插件变更通知
    pub async fn notify_changed(&self, plugin_id: &str) -> Result<(), String> {
        let notification = PluginChangeNotification {
            plugin_id: plugin_id.to_string(),
            action: PluginChangeAction::Changed,
            timestamp: Utc::now(),
        };
        let json = serde_json::to_string(&notification)
            .map_err(|e| format!("序列化通知失败: {}", e))?;
        self.pubsub.publish(&self.channel, &json).await
            .map_err(|e| format!("发布通知失败: {}", e))?;
        Ok(())
    }

    /// 发布插件移除通知
    pub async fn notify_removed(&self, plugin_id: &str) -> Result<(), String> {
        let notification = PluginChangeNotification {
            plugin_id: plugin_id.to_string(),
            action: PluginChangeAction::Removed,
            timestamp: Utc::now(),
        };
        let json = serde_json::to_string(&notification)
            .map_err(|e| format!("序列化通知失败: {}", e))?;
        self.pubsub.publish(&self.channel, &json).await
            .map_err(|e| format!("发布通知失败: {}", e))?;
        Ok(())
    }
}
```

### 3.5 启动同步逻辑变更

**核心变更**：不再查询 `cmx_plugin_deployments`，改为检查本地文件系统。

```rust
impl PluginInitializer {
    pub async fn sync_plugins(&self) -> PluginResult<PluginSyncResult> {
        // 步骤1: 查询 cmx_plugin 获取所有期望插件
        let expected_plugins = self.repository.list_plugins(&Default::default()).await?;

        // 步骤2: 收集本地已安装的插件（扫描文件系统）
        let local_plugins = self.scan_local_plugins().await?;

        // 步骤3: 对比生成操作计划
        for plugin in &expected_plugins {
            let local_version = local_plugins.get(&plugin.plugin_id);

            match local_version {
                Some(ver) if ver == &plugin.version => {
                    // 版本一致，跳过
                    result.skipped.push(plugin.plugin_id.clone());
                }
                Some(_ver) => {
                    // 版本不一致，需要升级/降级
                    let source = build_plugin_source(
                        plugin.zip_source_url.as_deref(),
                        plugin.zip_source_type.as_deref(),
                    );
                    upgrade_ops.push(PluginOperation::Upgrade { ... });
                }
                None => {
                    // 本地不存在，需要安装
                    let source = build_plugin_source(
                        plugin.zip_source_url.as_deref(),
                        plugin.zip_source_type.as_deref(),
                    );
                    install_ops.push(PluginOperation::Install { ... });
                }
            }
        }

        // 步骤4: 本地存在但数据库不存在的插件 → 清理本地文件
        for (plugin_id, _version) in &local_plugins {
            if !expected_map.contains_key(plugin_id) {
                uninstall_ops.push(PluginOperation::Uninstall { ... });
            }
        }

        // 步骤5: 执行计划
        // ...（与现有逻辑相同）

        // 步骤6: 加载 contexts
        self.load_contexts().await?;

        Ok(result)
    }

    /// 扫描本地文件系统，获取已安装的插件版本
    ///
    /// 目录结构: ${plugin_root}/${plugin_id}/${version}/
    /// 只要版本目录存在且包含 plugin.json，视为已安装
    async fn scan_local_plugins(&self) -> PluginResult<HashMap<String, String>> {
        let mut local_plugins = HashMap::new();

        if !self.plugin_root.exists() {
            return Ok(local_plugins);
        }

        let mut plugin_dirs = tokio::fs::read_dir(&self.plugin_root).await
            .map_err(|e| PluginError::Io(e.to_string()))?;

        while let Some(plugin_entry) = plugin_dirs.next_entry().await
            .map_err(|e| PluginError::Io(e.to_string()))?
        {
            if !plugin_entry.file_type().await.unwrap().is_dir() {
                continue;
            }

            let plugin_id = plugin_entry.file_name().to_string_lossy().to_string();
            let plugin_path = plugin_entry.path();

            // 查找最高版本目录
            let mut version_dirs = tokio::fs::read_dir(&plugin_path).await
                .map_err(|e| PluginError::Io(e.to_string()))?;

            let mut max_version = String::new();
            while let Some(version_entry) = version_dirs.next_entry().await
                .map_err(|e| PluginError::Io(e.to_string()))?
            {
                if !version_entry.file_type().await.unwrap().is_dir() {
                    continue;
                }

                let version = version_entry.file_name().to_string_lossy().to_string();
                // 检查是否包含 plugin.json（验证是有效安装）
                let manifest_path = version_entry.path().join("manifest.json");
                if manifest_path.exists() && version > max_version {
                    max_version = version;
                }
            }

            if !max_version.is_empty() {
                local_plugins.insert(plugin_id, max_version);
            }
        }

        Ok(local_plugins)
    }
}
```

### 3.6 生命周期服务变更

#### 3.6.1 通用变更

所有生命周期服务（Install / Upgrade / Downgrade / Uninstall）的变更方向一致：

1. **移除 `cmx_plugin_deployments` 写入**：不再 insert deployment 记录
2. **移除 deployment 存在性检查**：不再 `find_deployment()` 检查
3. **添加通知发布**：事务提交后发布 `PluginChangeNotification`
4. **`node_id` 可选化**：`node_id` 从必填变为可选，仅用于审计日志

#### 3.6.2 InstallService 变更要点

```rust
// 移除：步骤4 检查 deployment 记录
// 移除：步骤9.3 insert deployment

// 添加：事务提交后发布通知
if let Some(notifier) = &self.deps.plugin_notifier {
    let _ = notifier.notify_changed(&plugin_id).await;
}
```

#### 3.6.3 UpgradeService 变更要点

```rust
// 移除：步骤2 检查 deployment 存在性
// 移除：步骤12 insert deployment

// 添加：事务提交后发布通知
if let Some(notifier) = &self.deps.plugin_notifier {
    let _ = notifier.notify_changed(&plugin_id).await;
}
```

#### 3.6.4 UninstallService 变更要点

```rust
// 移除：步骤5 删除 deployment 记录
// 添加：事务提交后发布通知
if let Some(notifier) = &self.deps.plugin_notifier {
    let _ = notifier.notify_removed(&plugin_id).await;
}
```

### 3.7 配置变更

#### 3.7.1 PluginManagerSettings

```rust
pub struct PluginManagerSettings {
    pub plugin_root: PathBuf,
    pub backup_root: PathBuf,
    pub temp_root: PathBuf,
    pub default_database_id: String,
    pub cache: Option<CacheSettings>,
    pub cluster: Option<ClusterSettings>,

    // 变更：node_id 改为可选，仅用于审计日志
    /// 节点ID（可选，用于审计日志追踪，默认自动生成）
    pub node_id: Option<String>,
    pub node_name: Option<String>,
    pub node_type: Option<String>,
}
```

#### 3.7.2 web-server config.rs 变更

```rust
// 当前：
node_id: ConfigManager::global().get_string("node.node_id").unwrap_or("default".to_string()),

// 变更为：
node_id: ConfigManager::global().get_string("node.node_id").ok(),
```

### 3.8 降级逻辑处理

降级时本地可能不存在目标版本，统一处理为「清理旧版本 → 重新下载安装」：

```rust
// 在 sync_plugins 中，如果本地版本 > 期望版本（降级场景）
// 不需要特殊处理，直接走 DeployRequest
// DeployService 会自动判断并执行覆盖安装
```

由于降级后本地已有高版本目录，DeployService 的逻辑需要调整为：**如果本地版本高于目标版本，先删除本地版本目录，再执行安装**。

### 3.9 分布式锁的使用

分布式锁仅用于**防止同一插件的并发生命周期操作**（如两个管理员同时升级同一插件），不用于同步通知：

```rust
/// 生命周期操作加锁（仅对发起操作的节点）
pub async fn with_plugin_lock<F, T>(
    lock_manager: &LockManager,
    plugin_id: &str,
    operation: F,
) -> PluginResult<T>
where
    F: Future<Output = PluginResult<T>>,
{
    let lock_key = format!("cmx:plugin:lock:{}", plugin_id);
    let guard = lock_manager.lock(&lock_key).await
        .map_err(|e| PluginError::Lock(format!("获取锁失败: {}", e)))?;

    let result = operation.await;
    drop(guard);
    result
}
```

---

## 四、`cmx_plugin_deployments` 表退役策略

### 4.1 分阶段退役

| 阶段 | 操作 | 影响范围 |
|---|---|---|
| Phase 1 | 停止写入 deployment 记录 | Install/Upgrade/Downgrade 服务 |
| Phase 2 | 启动同步改为文件系统扫描 | PluginInitializer |
| Phase 3 | 清理孤儿记录 | 新增启动清理逻辑 |
| Phase 4 | 保留表结构但标记废弃 | 文档说明 |
| Phase 5（可选） | 删除表结构 | 数据库迁移 |

### 4.2 Phase 1+2 可同时进行

停止写入 deployment 和修改启动同步逻辑可以在同一个版本中完成，因为两者互不依赖。

### 4.3 孤儿记录清理

Phase 3 添加启动时的清理逻辑：

```rust
/// 清理过期的 deployment 记录
///
/// 清理条件：
/// 1. 对应的 plugin_id 在 cmx_plugin 表中不存在
/// 2. 或者对应的 plugin_id 在 cmx_plugin 表中的版本与 deployment 不匹配
pub async fn cleanup_orphan_deployments(&self) -> PluginResult<u64> {
    let all_deployments = self.deployment_repository.list_all_deployments().await?;
    let plugins = self.repository.list_plugins(&Default::default()).await?;
    let plugin_versions: HashMap<String, String> = plugins.iter()
        .map(|p| (p.plugin_id.clone(), p.version.clone()))
        .collect();

    let mut cleaned = 0u64;
    for deployment in all_deployments {
        match plugin_versions.get(&deployment.plugin_id) {
            None => {
                // plugin 不存在，删除 deployment
                self.deployment_repository.delete_deployments_by_plugin_id(
                    &deployment.plugin_id, None
                ).await?;
                cleaned += 1;
            }
            Some(version) if version != &deployment.version => {
                // 版本不匹配，删除旧 deployment
                self.deployment_repository.delete_deployment(
                    &deployment.plugin_id, &deployment.node_id, &deployment.version
                ).await?;
                cleaned += 1;
            }
            _ => {}
        }
    }

    if cleaned > 0 {
        tracing::info!("清理了 {} 条过期 deployment 记录", cleaned);
    }
    Ok(cleaned)
}
```

---

## 五、Redis Pub/Sub 订阅集成

### 5.1 订阅启动

在 `PluginManager::initialize()` 中启动 Pub/Sub 订阅：

```rust
// PluginManager::from_builder() 中
if let Some(pubsub) = builder.pubsub {
    let notifier = Arc::new(PluginNotifier::new(pubsub.clone()));
    let handler = PluginChangeHandler::new(/* deps */);

    // 订阅变更通知
    let handler_clone = handler.clone();
    tokio::spawn(async move {
        let mut rx = pubsub.subscribe(PluginNotifier::CHANNEL).await;
        while let Some(msg) = rx.next().await {
            if let Ok(notification) = serde_json::from_str::<PluginChangeNotification>(&msg) {
                handler_clone.handle_change(&notification.plugin_id, &notification.action).await;
            }
        }
    });

    // notifier 注入到各生命周期服务
}
```

### 5.2 优雅关闭

在 `PluginManager::shutdown()` 中取消订阅：

```rust
pub async fn shutdown(&self) -> PluginResult<()> {
    // 取消 Pub/Sub 订阅
    if let Some(subscription) = &self.subscription {
        subscription.unsubscribe().await;
    }

    // ... 现有关闭逻辑
    Ok(())
}
```

---

## 六、不采纳的设计

### 6.1 不采用：消息携带完整业务数据

原因：
- 数据可能已在消息传输过程中被再次修改（竞态条件）
- 增加消息大小和序列化开销
- 引入分布式一致性问题

### 6.2 不采用：请求去重机制

原因：
- 本方案中操作是幂等的，重复执行不会产生副作用
- 减少对 Redis 的额外依赖（不需要 request_id 存储）

### 6.3 不采用：instance_id 自动生成和持久化

原因：
- 本方案中不需要 instance_id 防循环（幂等操作）
- 文件持久化在容器环境中不可靠（Pod 重启丢失）
- 避免引入新的状态管理复杂性

### 6.4 不采用：`cmx_plugin_local_cache` 表

原因：
- 本地文件系统已是天然的缓存标识
- `plugin_root/plugin_id/version/plugin.json` 存在即表示已缓存
- 额外的数据库表增加了维护负担且无实际收益

### 6.5 不采用：复杂的 DeploymentStrategy

原因：
- 无状态模式下所有实例插件一致，无需 AllNodes / SpecificNodes / PrimaryOnly 策略
- 如需差异化部署，应通过业务层（API）控制，而非插件框架层

---

## 七、实施计划

### Phase 1：核心无状态化改造（最小可行版本）

**目标**：去除对 `cmx_plugin_deployments` 的运行时依赖

| 序号 | 任务 | 涉及文件 |
|---|---|---|
| 1.1 | 定义 `PluginChangeNotification` 和 `PluginChangeAction` | `cluster/sync.rs` |
| 1.2 | 实现 `PluginNotifier` | `cluster/sync.rs` |
| 1.3 | 实现 `PluginChangeHandler` | `service/sync.rs`（重构） |
| 1.4 | `PluginInitializer.sync_plugins()` 改为文件系统扫描 | `service/initializer.rs` |
| 1.5 | `InstallService` 移除 deployment 写入，添加通知发布 | `service/install.rs` |
| 1.6 | `UpgradeService` 移除 deployment 检查和写入，添加通知发布 | `service/upgrade.rs` |
| 1.7 | `UninstallService` 移除 deployment 删除，添加通知发布 | `service/uninstall.rs` |
| 1.8 | `DowngradeService` 移除 deployment 相关逻辑 | `service/downgrade.rs` |
| 1.9 | `PluginManagerSettings.node_id` 改为 Option | `config/settings.rs` |
| 1.10 | `PluginManager.from_builder()` 集成 Pub/Sub 订阅 | `core/manager.rs` |
| 1.11 | web-server `config.rs` 适配 node_id 变更 | `web-server/src/config.rs` |

### Phase 2：清理和优化

| 序号 | 任务 | 涉及文件 |
|---|---|---|
| 2.1 | 启动时清理孤儿 deployment 记录 | `service/initializer.rs` |
| 2.2 | 移除 `NodeSyncService`（已被 `PluginChangeHandler` 替代） | `service/sync.rs` |
| 2.3 | 简化 `DeployService`（移除 deployment 相关检查） | `service/deploy.rs` |
| 2.4 | 移除 `InstallServiceDeps` / `UpgradeServiceDeps` 中的 `deployment_repository` | 各服务文件 |
| 2.5 | 添加集成测试（多实例同步场景） | `tests/` |

### Phase 3：可选优化

| 序号 | 任务 | 说明 |
|---|---|---|
| 3.1 | 退役 `cmx_plugin_deployments` 表 | 确认所有功能正常后 |
| 3.2 | 清理 `cluster/node.rs` 和 `cluster/deployment.rs` | 如无其他使用方 |
| 3.3 | 添加 Prometheus 监控指标 | 插件同步延迟、失败率等 |

---

## 八、风险与应对

| 风险 | 影响 | 应对措施 |
|---|---|---|
| Pub/Sub 消息丢失 | 节点未收到通知，插件版本不一致 | 下次启动时 `sync_plugins()` 自动修复 |
| 数据库查询失败 | 节点无法确定最新版本 | 记录错误日志，保持本地版本不变 |
| 多实例同时安装同一插件 | 重复下载安装 | 幂等操作，安装目录已存在则跳过 |
| 降级时本地高版本目录残留 | 磁盘空间占用 | 启动同步时清理不匹配的版本目录 |

---

## 九、关键代码文件清单

| 文件 | 改动类型 | 说明 |
|---|---|---|
| `crates/libs/cmx-plugin/src/cluster/sync.rs` | 重构 | 替换为 `PluginNotifier` |
| `crates/libs/cmx-plugin/src/service/sync.rs` | 重构 | 替换为 `PluginChangeHandler` |
| `crates/libs/cmx-plugin/src/service/initializer.rs` | 修改 | 文件系统扫描替代 deployment 查询 |
| `crates/libs/cmx-plugin/src/service/install.rs` | 修改 | 移除 deployment 写入，添加通知 |
| `crates/libs/cmx-plugin/src/service/upgrade.rs` | 修改 | 移除 deployment 检查和写入，添加通知 |
| `crates/libs/cmx-plugin/src/service/uninstall.rs` | 修改 | 移除 deployment 删除，添加通知 |
| `crates/libs/cmx-plugin/src/service/downgrade.rs` | 修改 | 移除 deployment 相关逻辑 |
| `crates/libs/cmx-plugin/src/service/deploy.rs` | 修改 | 简化 deployment 检查 |
| `crates/libs/cmx-plugin/src/config/settings.rs` | 修改 | node_id 改为 Option |
| `crates/libs/cmx-plugin/src/core/manager.rs` | 修改 | 集成 Pub/Sub 订阅和通知器 |
| `crates/web/web-server/src/config.rs` | 修改 | 适配 node_id 变更 |
