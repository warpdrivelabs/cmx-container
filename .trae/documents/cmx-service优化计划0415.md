# cmx-service 模块代码审查与优化计划

## 一、代码审查发现的问题

---

### 1.1 降级逻辑Bug（严重）

**问题位置**: `cmx-plugin/src/service/downgrade.rs` 第 199-216 行

**问题描述**:
降级时只更新了现有服务的版本号，但没有删除降级后不再存在的服务。

**场景举例**:
- 插件 v1.0 有 4 个服务: `A, B, C, D`
- 插件 v2.0 新增了 1 个服务: `E`，共 5 个服务
- 数据库中保存了 5 条服务定义记录
- 降级到 v1.0 时，代码逻辑:
  ```rust
  // 只更新了现有服务的版本号为旧版本
  for service in services { // 5个服务
      updated_service.version = request.target_version.clone();
      save_service(&updated_service); // 保存了5个服务
  }
  ```
- **结果**: 降级后仍然有 5 个服务（A, B, C, D, E），E 是 v2.0 新增的，不应该存在

**根本原因**:
降级时需要获取目标版本（旧版本）实际包含的服务列表，而不是当前数据库中的服务列表。旧版本插件的文件系统中才保存着正确的服务定义。

---

### 1.2 启动时全量加载性能问题

**问题位置**: `web-server/src/config.rs` 第 215-248 行

**问题描述**:
```rust
let services: Vec<ServiceDefinition> = repository.list_services().await?;
// ...
for service in &services {                          // N 次循环
    let versions = repository.get_service_versions(&service.service_key).await?; // 每次 1 次 DB 查询
    if let Some((version, _)) = versions.first() {
        let config_opt = repository.get_service_config(&service.service_key, &version_str).await?; // 又 1 次 DB 查询
    }
}
```

**性能问题**:
- `list_services()`: 1 次查询获取所有服务
- `get_service_versions()`: N 次查询（每个服务 1 次）
- `get_service_config()`: 最多 N 次查询（每个服务 1 次）
- **总计**: 1 + 2N 次数据库查询
- 如果有 1000 个服务，启动时需要 2001 次数据库查询

---

### 1.3 缓存与数据库同步问题

#### 1.3.1 生命周期监听器核心问题

**问题位置**: `cmx-service/src/lifecycle_listener.rs`

**关键调用链分析**:

```
lifecycle_listener.rs:
  handle_installed() -> query.get_services_by_plugin() -> ServiceQueryImpl.get_services_by_plugin()

ServiceQueryImpl.get_services_by_plugin():
  1. 先查 registry.get_by_plugin() 缓存
  2. 缓存非空 -> 直接返回缓存
  3. 缓存为空 -> 查 repository.get_services_by_plugin() 数据库
```

**问题 1.3.1.1: handle_installed 依赖数据库，如果数据库不对则缓存会同步错误数据**

```rust
async fn handle_installed(
    query: Arc<dyn ServiceQuery>,
    registry: Arc<ServiceRegistry>,
    event: PluginLifecyclePayload,
) {
    // 问题：这里 query.get_services_by_plugin 会先查缓存，缓存为空才查数据库
    // 如果数据库中的数据本身就是错的（例如降级bug导致多了服务），缓存刷新后仍然错误
    match query.get_services_by_plugin(&event.plugin_id).await {
        Ok(services) => {
            registry.sync_plugin_services(&event.plugin_id, services, orchestrations).await;
        }
    }
}
```

**场景**:
- 降级场景下，数据库中有不应该存在的服务（新版新增的）
- 降级发布事件，handle_downgraded -> handle_installed 被调用
- query.get_services_by_plugin 从数据库获取到错误数据
- 缓存被同步成错误数据

#### 1.3.2 缓存未回写问题

**问题位置**: `cmx-service/src/service_query_impl.rs` 第 68-78 行

```rust
async fn get_services_by_plugin(&self, plugin_id: &str) -> Result<Vec<ServiceInfo>, TraitError> {
    let services = self.registry.get_by_plugin(plugin_id).await;
    if !services.is_empty() {
        return Ok(services);  // 缓存命中，直接返回
    }

    // 缓存未命中时，从数据库查询
    let service_defs = self.repository.get_services_by_plugin(plugin_id).await
        .map_err(|e| TraitError::Internal(e.to_string()))?;

    // 问题：数据库查询后没有回写到缓存！
    // 调用方需要手动调用 registry.sync_plugin_services 来同步
    // 如果调用方没有正确同步，缓存就不会被更新
    Ok(service_defs.into_iter().map(ServiceInfo::from).collect())
}
```

**问题 1.3.2.1: get_services_by_plugin 没有缓存回写**

- 缓存未命中时从数据库查询，但查询结果没有回写到缓存
- 只有调用方手动调用 `registry.sync_plugin_services` 才能更新缓存
- 如果调用方忘记同步，缓存就不会被更新

**问题 1.3.2.2: get_service 没有缓存回写**

```rust
async fn get_service(&self, service_key: &str) -> Result<Option<ServiceInfo>, TraitError> {
    if let Some(service) = self.registry.get(service_key).await {
        return Ok(Some(service));  // 缓存命中
    }

    // 缓存未命中，查数据库
    let service_def = self.repository.get_service(service_key).await
        .map_err(|e| TraitError::Internal(e.to_string()))?;

    // 问题：没有回写到缓存！
    Ok(service_def.map(|def| ServiceInfo::from(def)))
}
```

#### 1.3.3 handle_uninstalled 依赖缓存，可能失效

**问题位置**: `cmx-service/src/lifecycle_listener.rs` 第 140-152 行

```rust
async fn handle_uninstalled(registry: Arc<ServiceRegistry>, event: PluginLifecyclePayload) {
    // 问题：get_by_plugin 只查缓存，如果缓存为空则返回空列表
    let services = registry.get_by_plugin(&event.plugin_id).await;

    // 如果缓存为空，services 是空列表，for 循环不执行
    // 没有任何日志或错误提示
    for service in services {
        registry.unregister(&service.service_key, &event.plugin_id).await;
    }

    info!("插件 {} 服务定义已从缓存清理", event.plugin_id);
}
```

**问题**:
- 如果服务从未加载到缓存，`get_by_plugin` 返回空列表
- 循环不执行，缓存清理"静默失败"
- 无法感知缓存与数据库不一致

---

### 1.4 list_active_services 绕过了缓存

**问题位置**: `cmx-service/src/service_query_impl.rs` 第 84-95 行

```rust
async fn list_active_services(&self) -> Result<Vec<ServiceInfo>, TraitError> {
    // 问题：直接查数据库，完全绕过缓存
    let all_services = self.repository.list_services().await
        .map_err(|e| TraitError::Internal(e.to_string()))?;

    let active: Vec<ServiceInfo> = all_services
        .into_iter()
        .filter(|s| s.status == 1)
        .map(ServiceInfo::from)
        .collect();

    Ok(active)
}
```

**问题**:
- 如果缓存中有数据，不优先使用缓存
- 每次调用都要查数据库，性能差

---

## 二、问题汇总表

| 序号 | 问题 | 严重程度 | 所在文件 |
|------|------|----------|----------|
| 1 | 降级时没有删除新版新增的服务 | 严重 | cmx-plugin/src/service/downgrade.rs |
| 2 | 启动时全量加载服务编排，1+2N 次 DB 查询 | 严重 | web-server/src/config.rs |
| 3 | handle_installed 依赖数据库，数据库错误时同步错误数据 | 严重 | cmx-service/src/lifecycle_listener.rs |
| 4 | get_services_by_plugin 没有缓存回写 | 中等 | cmx-service/src/service_query_impl.rs |
| 5 | get_service 没有缓存回写 | 中等 | cmx-service/src/service_query_impl.rs |
| 6 | handle_uninstalled 依赖缓存，缓存为空时静默失败 | 中等 | cmx-service/src/lifecycle_listener.rs |
| 7 | list_active_services 绕过缓存 | 低 | cmx-service/src/service_query_impl.rs |

---

## 三、解决方案

### 3.1 修复降级逻辑Bug

**核心思路**: 降级时需要从旧版本插件文件系统中解析服务定义，而不是从数据库查询。

**修改文件**:
- `cmx-plugin/src/service/downgrade.rs`
- `cmx-plugin/src/service/service_parser.rs`（新增函数）

**具体步骤**:

1. 在 `service_parser.rs` 中新增 `parse_services_from_plugin_dir` 函数，从插件目录解析服务定义但不保存到数据库

2. 修改 `downgrade.rs` 步骤 6.2 的服务处理逻辑:
   ```rust
   // 步骤 6.2: 处理降级时的服务定义
   // 1. 解析旧版本插件目录获取实际的服务定义
   let old_version_services = crate::service::service_parser::parse_services_from_plugin_dir(
       &PathBuf::from(&target_version_record.install_path),
       &plugin_id,
       &request.target_version,
   )?;

   let old_service_keys: HashSet<String> = old_version_services.iter()
       .map(|s| s.service_key.clone())
       .collect();

   // 2. 查询数据库中该插件的所有服务
   let db_services = self.deps.service_query
       .get_services_by_plugin(&plugin_id)
       .await?;

   // 3. 删除在新版本中存在但旧版本中不存在的服务
   for service in db_services {
       if !old_service_keys.contains(&service.service_key) {
           // 服务在旧版本中不存在，应该删除
           self.deps.service_storage
               .delete_service(&service.service_key, Some(txn_guard.txn_id()), None)
               .await?;
       } else {
           // 更新保留服务的版本号
           let mut updated_service: ServiceDefinition = service.into();
           updated_service.version = request.target_version.clone();
           self.deps.service_storage
               .save_service(&updated_service, Some(txn_guard.txn_id()))
               .await?;
       }
   }
   ```

---

### 3.2 延迟加载（Lazy Loading）优化启动性能

**核心思路**: 启动时不加载所有服务，在第一次访问服务时才加载。

**修改文件**: `web-server/src/config.rs`

**具体步骤**:
1. 修改 `init_services` 函数，移除全量加载逻辑
2. 只初始化 ServiceRepository、ServiceRegistry、ServiceQueryImpl 等组件
3. 让服务数据在首次访问时自动加载

---

### 3.3 实现缓存回写机制

**核心思路**: 在 ServiceQueryImpl 中实现缓存回写，避免重复查数据库。

**修改文件**: `cmx-service/src/service_query_impl.rs`

**具体步骤**:

1. 修改 `get_service` 方法，添加缓存回写:
   ```rust
   async fn get_service(&self, service_key: &str) -> Result<Option<ServiceInfo>, TraitError> {
       // 1. 先查缓存
       if let Some(service) = self.registry.get(service_key).await {
           return Ok(Some(service));
       }

       // 2. 缓存未命中，查数据库
       let service_def = self.repository.get_service(service_key).await
           .map_err(|e| TraitError::Internal(e.to_string()))?;

       // 3. 如果数据库中存在，回写到缓存
       if let Some(def) = &service_def {
           let service_info = ServiceInfo::from(def.clone());
           self.registry.register(service_info.clone(), None).await;
           return Ok(Some(service_info));
       }

       Ok(None)
   }
   ```

2. 修改 `get_services_by_plugin` 方法，添加缓存回写:
   ```rust
   async fn get_services_by_plugin(&self, plugin_id: &str) -> Result<Vec<ServiceInfo>, TraitError> {
       // 1. 先查缓存
       let cached_services = self.registry.get_by_plugin(plugin_id).await;
       if !cached_services.is_empty() {
           return Ok(cached_services);
       }

       // 2. 缓存未命中，查数据库
       let service_defs = self.repository.get_services_by_plugin(plugin_id).await
           .map_err(|e| TraitError::Internal(e.to_string()))?;

       // 3. 批量回写到缓存
       for def in &service_defs {
           let service_info = ServiceInfo::from(def.clone());
           self.registry.register(service_info, None).await;
       }

       Ok(service_defs.into_iter().map(ServiceInfo::from).collect())
   }
   ```

---

### 3.4 增强生命周期监听器

**核心思路**:
- handle_installed 应该从插件目录解析服务，而不是依赖数据库
- handle_uninstalled 应该确保即使缓存为空也要记录日志

**修改文件**: `cmx-service/src/lifecycle_listener.rs`

**具体步骤**:

1. 修改 `handle_installed`，从插件目录解析服务定义（需要传递 install_path）:
   ```rust
   async fn handle_installed(
       query: Arc<dyn ServiceQuery>,
       registry: Arc<ServiceRegistry>,
       event: PluginLifecyclePayload,
   ) {
       info!("处理插件安装事件: {} v{}", event.plugin_id, event.version);

       // 从数据库加载服务定义
       match query.get_services_by_plugin(&event.plugin_id).await {
           Ok(services) => {
               // 解析编排配置（如果缓存需要）
               let mut orchestrations = std::collections::HashMap::new();
               for service in &services {
                   if !service.config.is_empty() {
                       if let Ok(orch) = serde_json::from_str::<serde_json::Value>(&service.config) {
                           orchestrations.insert(service.service_key.clone(), orch);
                       }
                   }
               }

               registry.sync_plugin_services(&event.plugin_id, services, orchestrations).await;
               info!("插件 {} 服务定义已加载到缓存", event.plugin_id);
           }
           Err(e) => {
               error!("加载插件 {} 服务定义失败: {}", event.plugin_id, e);
           }
       }
   }
   ```

2. 修改 `handle_uninstalled`，添加日志和错误处理:
   ```rust
   async fn handle_uninstalled(registry: Arc<ServiceRegistry>, event: PluginLifecyclePayload) {
       info!("处理插件卸载事件: {} v{}", event.plugin_id, event.version);

       let services = registry.get_by_plugin(&event.plugin_id).await;

       if services.is_empty() {
           // 缓存可能为空，但数据库清理已在 uninstall.rs 中完成
           info!("插件 {} 的服务缓存为空，跳过缓存清理", event.plugin_id);
       } else {
           for service in services {
               registry.unregister(&service.service_key, &event.plugin_id).await;
           }
           info!("插件 {} 服务定义已从缓存清理", event.plugin_id);
       }
   }
   ```

3. **注意**: `handle_downgraded` 的问题需要通过修复降级逻辑（3.1）来解决，因为问题的根源在降级流程没有正确处理新增的服务

---

### 3.5 优化 list_active_services

**修改文件**: `cmx-service/src/service_query_impl.rs`

**具体步骤**:

```rust
async fn list_active_services(&self) -> Result<Vec<ServiceInfo>, TraitError> {
    // 优先从缓存获取所有服务
    let all_keys = self.registry.get_all_keys().await;

    let mut active = Vec::new();
    for key in all_keys {
        if let Some(service) = self.registry.get(&key).await {
            if service.status == 1 {
                active.push(service);
            }
        } else {
            // 缓存未命中，从数据库加载单个服务
            if let Ok(Some(def)) = self.repository.get_service(&key).await {
                if def.status == 1 {
                    let info = ServiceInfo::from(def);
                    self.registry.register(info.clone(), None).await;
                    active.push(info);
                }
            }
        }
    }

    Ok(active)
}
```

---

## 四、任务分解

### 任务 1: 修复降级逻辑Bug
- **文件**: `cmx-plugin/src/service/downgrade.rs`
- **文件**: `cmx-plugin/src/service/service_parser.rs`
- **步骤**:
  1. 在 `service_parser.rs` 中新增 `parse_services_from_plugin_dir` 函数
  2. 修改 `downgrade.rs` 步骤 6.2 的服务处理逻辑

### 任务 2: 实现延迟加载（Lazy Loading）
- **文件**: `web-server/src/config.rs`
- **步骤**:
  1. 移除 `init_services` 中的全量加载循环
  2. 确保延迟加载生效

### 任务 3: 实现缓存回写
- **文件**: `cmx-service/src/service_query_impl.rs`
- **步骤**:
  1. 修改 `get_service` 方法，添加缓存回写
  2. 修改 `get_services_by_plugin` 方法，添加缓存回写

### 任务 4: 增强生命周期监听器
- **文件**: `cmx-service/src/lifecycle_listener.rs`
- **步骤**:
  1. 优化卸载事件处理
  2. 添加更详细的日志

### 任务 5: 优化 list_active_services
- **文件**: `cmx-service/src/service_query_impl.rs`
- **步骤**:
  1. 优先使用缓存
  2. 缓存未命中时回写

---

## 五、验证方案

1. **降级测试**:
   - 准备一个有 4 个服务的插件 v1.0
   - 升级到有 5 个服务的 v2.0
   - 降级回 v1.0
   - 验证服务数量为 4，新增的服务 E 已被删除

2. **启动性能测试**:
   - 对比优化前后的启动时间
   - 使用 100/500/1000 个服务进行测试

3. **缓存命中测试**:
   - 验证首次查询后缓存已填充
   - 验证后续查询直接从缓存返回

4. **全量回归测试**:
   - 插件安装/升级/降级/卸载后，缓存与数据库状态一致

---

## 六、依赖关系

```
任务1 (修复降级Bug)
    │
    ├── 任务3 (缓存回写) 的前置条件：降级Bug修复后，缓存刷新才能获取正确数据
    │
    └── 任务4 (增强监听器) 的前置条件：降级Bug修复后，handle_downgraded 才能正确处理

任务2 (延迟加载)
    │
    └── 可独立进行，不依赖其他任务

任务5 (优化 list_active_services)
    │
    └── 依赖任务3 (缓存回写)
```
