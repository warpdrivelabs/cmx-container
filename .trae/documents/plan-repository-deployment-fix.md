# 修复 cmx-plugin 的 repository.rs 和 deployment.rs 报错计划

## 问题分析

### 1. repository.rs 问题

**`update_plugin`** **方法（228行）：**

* 当前签名：`pub async fn update_plugin(&self, record: &PluginDbRecord, txn_id: Option<&str>)`

* 问题1：参数是 `&PluginDbRecord` 但应该是 `&PluginUpdateFields`

* 问题2：调用方传入的是 `plugin_id` 和 `&PluginUpdateFields` 两个参数

* 问题3：方法体为空，直接返回 `Ok(())`

**调用方示例（upgrade.rs:239）：**

```rust
self.deps.repository.update_plugin(&plugin_id, &fields).await?;
```

**`update_plugin_status`** **方法（430行）：**

* 调用方式错误：`self.update_plugin(plugin_id, &fields).await`

* 应该是：`self.update_plugin(plugin_id, &fields, None).await`

### 2. deployment.rs 问题

**`DeploymentRecord`** **结构体只有 16 个字段（17-46行）：**

```rust
id, plugin_id, node_id, node_type, version, status, progress,
error_message, error_details, archived, create_by, create_name,
update_by, update_name, create_time, update_time
```

**但** **`insert_deployment`** **方法（91-133行）使用了不存在的字段：**

* `node_name` - 不存在

* `deployment_type` - 不存在

* `sync_token` - 不存在

* `last_sync_at` - 不存在

* `deployed_at` - 不存在

* `validated_at` - 不存在

**`DeploymentUpdateFields`** **结构体只有 12 个字段（50-68行）：**

```rust
plugin_id, node_id, version, status, progress, error_message,
error_details, archived, create_by, create_name, update_by, update_name
```

**但** **`update_deployment`** **方法（136-189行）引用了不存在的字段：**

* `deployment_type` - 行144-146

* `sync_token` - 行164-166

* `last_sync_at` - 行168-170

* `validated_at` - 行172-174

**`parse_deployment_record`** **方法（267-303行）也使用了不存在的字段：**

* `node_name` - 行280

* `deployment_type` - 行283

* `sync_token` - 行288

* `last_sync_at` - 行289

* `deployed_at` - 行290

* `validated_at` - 行291

## 修改计划

### 1. repository.rs 修改

**1.1 修改** **`update_plugin`** **方法（228行）：**

```rust
// 新签名：
pub async fn update_plugin(
    &self,
    plugin_id: &str,
    fields: &PluginUpdateFields,
    txn_id: Option<&str>,
) -> PluginResult<()>

// 使用 plugin_id 和 version 作为 where 条件
// 只更新 fields 中非默认值的字段
```

**1.2 修改** **`update_plugin_status`** **方法（430行）：**

```rust
// 改为：
self.update_plugin(plugin_id, &fields, None).await
```

### 2. deployment.rs 修改

**2.1 修改** **`insert_deployment`** **方法（91-133行）：**

* 移除不存在的字段，只使用 `DeploymentRecord` 中实际存在的字段

**2.2 修改** **`update_deployment`** **方法（136-189行）：**

* 移除对 `deployment_type`, `sync_token`, `last_sync_at`, `validated_at` 的引用

* 改为使用 `DeploymentUpdateFields` 中实际存在的字段

**2.3 修改** **`parse_deployment_record`** **方法（267-303行）：**

* 移除对 `node_name`, `deployment_type`, `sync_token`, `last_sync_at`, `deployed_at`, `validated_at` 的解析

* 只解析 `DeploymentRecord` 中实际存在的字段

**2.4 修改** **`list_plugin_deployments`** **和** **`list_node_deployments`** **方法（205-228行）：**

* SQL 中的 `ORDER BY deployed_at` 应该改为 `ORDER BY create_time`（因为 `deployed_at` 不存在）

### 3. 调用方修改

**3.1 upgrade.rs 修改（243-250行）：**

```rust
// 当前：
let update_fields = crate::infrastructure::database::deployment::DeploymentUpdateFields {
    version: Some(new_version.clone()),
    deployment_type: Some("upgrade".to_string()),
    last_sync_at: Some(Utc::now()),
    ..Default::default()
};

// 改为（移除不存在的字段）：
let update_fields = crate::infrastructure::database::deployment::DeploymentUpdateFields {
    version: Some(new_version.clone()),
    ..Default::default()
};
```

**3.2 downgrade.rs 修改（148-154行）：**

```rust
// 当前：
let update_fields = crate::infrastructure::database::deployment::DeploymentUpdateFields {
    version: Some(request.target_version.clone()),
    deployment_type: Some("downgrade".to_string()),
    status: Some("deployed".to_string()),
    last_sync_at: Some(Utc::now()),
    ..Default::default()
};

// 改为（移除不存在的字段）：
let update_fields = crate::infrastructure::database::deployment::DeploymentUpdateFields {
    version: Some(request.target_version.clone()),
    status: Some("deployed".to_string()),
    ..Default::default()
};
```

## 实施步骤

1. 修改 `repository.rs` 中的 `update_plugin` 方法实现
2. 修改 `repository.rs` 中的 `update_plugin_status` 方法调用
3. 修改 `deployment.rs` 中的 `insert_deployment` 方法
4. 修改 `deployment.rs` 中的 `update_deployment` 方法
5. 修改 `deployment.rs` 中的 `parse_deployment_record` 方法
6. 修改 `deployment.rs` 中的 `list_plugin_deployments` 和 `list_node_deployments` SQL
7. 修改 `upgrade.rs` 中创建 `DeploymentUpdateFields` 的代码
8. 修改 `downgrade.rs` 中创建 `DeploymentUpdateFields` 的代码
9. 运行 `cargo check -p cmx-plugin` 验证修复

