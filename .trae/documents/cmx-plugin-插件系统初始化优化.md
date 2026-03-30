# 插件系统启动初始化优化计划

## 需求概述

根据用户要求，需要完成以下两项任务：

1. **完善 cmx-plugin 表结构**：增加 `zip_source_url` 和 `zip_source_type` 字段
2. **重构 load\_installed\_plugins 流程**：实现正确的插件启动同步逻辑

***

## 任务一：完善 cmx\_plugin 表结构

### 1.1 修改 SQL 表结构

**文件**: `e:\rustspace\cmx\cmx-container\crates\libs\cmx-plugin\sql\plugin_lifecycle_schemanojson.sql`

在 `cmx_plugin` 表中增加两个字段：

```sql
-- 插件ZIP包来源地址
zip_source_url VARCHAR(500),
-- 插件来源类型: local, url, registry
zip_source_type VARCHAR(30),
```

**操作步骤**：

1. 修改 SQL 文件，在 `cmx_plugin` 表的 `signer_key_id` 字段后添加 `zip_source_url` 和 `zip_source_type` 字段
2. 添加相应的 COMMENT ON COLUMN 注释

### 1.2 修改 Repository 层

**文件**: `e:\rustspace\cmx\cmx-container\crates\libs\cmx-plugin\src\infrastructure\database\repository.rs`

**PluginDbRecord 结构体**增加字段：

```rust
pub zip_source_url: Option<String>,
pub zip_source_type: Option<String>,
```

**PluginUpdateFields 结构体**增加字段：

```rust
pub zip_source_url: Option<String>,
pub zip_source_type: Option<String>,
```

**修改方法**：

* `insert_plugin`: 添加新字段到插入列

* `update_plugin`: 添加新字段到更新列

* `upsert_plugin`: 添加新字段到 upsert 列

* `parse_plugin_record`: 添加新字段解析

### 1.3 修改 record\_builder

**文件**: `e:\rustspace\cmx\cmx-container\crates\libs\cmx-plugin\src\service\record_builder.rs`

修改 `build_plugin_db_record` 函数，增加 `zip_source_url` 和 `zip_source_type` 参数

### 1.4 修改 install.rs

**文件**: `e:\rustspace\cmx\cmx-container\crates\libs\cmx-plugin\src\service\install.rs`

在构建 `db_record` 时，传入来源信息：

* 从 `request.source` 解析出 URL 和类型

* 调用 `record_builder::build_plugin_db_record` 时传入

### 1.5 修改 upgrade.rs

**文件**: `e:\rustspace\cmx\cmx-container\crates\libs\cmx-plugin\src\service\upgrade.rs`

同样在构建 `db_record` 时传入来源信息

***

## 任务二：cmx\_plugin\_versions 表也存储 zip\_source 字段

### 2.1 设计背景

**问题**：如果只在 `cmx_plugin` 表存储 `zip_source_url` 和 `zip_source_type`，降级时会遇到问题：

* 假设插件 A 从 v1.0 升级到 v2.0（来源是 `http://example.com/v2.zip`）

* 后来需要降级回 v1.0，但该节点的 `cmx_plugin_deployments` 中可能没有 v1.0 的记录（因为之前被覆盖了）

* 此时无法获取 v1.0 的来源地址，无法完成降级

**解决方案**：在 `cmx_plugin_versions` 表中也存储 `zip_source_url` 和 `zip_source_type` 字段。

**理由**：

1. `cmx_plugin_versions` 记录每个插件的每个版本信息
2. 降级时可以从版本历史中获取目标版本的来源
3. 每个版本可以有不同的来源地址（比如 v1.0 来自本地，v2.0 来自远程）

### 2.2 修改 SQL 表结构

**文件**: `e:\rustspace\cmx\cmx-container\crates\libs\cmx-plugin\sql\plugin_lifecycle_schemanojson.sql`

在 `cmx_plugin_versions` 表中增加两个字段：

```sql
-- 插件ZIP包来源地址
zip_source_url VARCHAR(500),
-- 插件来源类型: local, url, registry
zip_source_type VARCHAR(30),
```

### 2.3 修改 VersionHistoryRepository

**文件**: `e:\rustspace\cmx\cmx-container\crates\libs\cmx-plugin\src\infrastructure\database\version_history.rs`

**VersionHistoryRecord 结构体**增加字段：

```rust
pub zip_source_url: Option<String>,
pub zip_source_type: Option<String>,
```

**修改方法**：

* `insert_version`: 添加新字段到插入列

* `upsert_version`: 添加新字段到 upsert 列和 ON CONFLICT 更新列

* `parse_version_record`: 添加新字段解析

### 2.4 修改 record\_builder

**文件**: `e:\rustspace\cmx\cmx-container\crates\libs\cmx-plugin\src\service\record_builder.rs`

修改 `build_version_record` 函数，增加 `zip_source_url` 和 `zip_source_type` 参数

### 2.5 修改 install.rs 和 upgrade.rs

在构建 `version_record` 时也传入来源信息，确保版本历史记录包含来源。

### 2.6 修改 initializer.rs

降级时从 `cmx_plugin_versions` 获取目标版本的 `zip_source_url` 和 `zip_source_type`，构建 `PluginSource`。

***

## 任务三：重构 load\_installed\_plugins 流程

### 3.1 设计思路

当前 `load_installed_plugins` 只是简单地从数据库加载已安装插件，没有实现启动时的同步逻辑。

**正确的流程应该是**：

1. 查询 `cmx_plugin` 表获取期望的插件列表（应该安装哪些插件及版本）
2. 查询 `cmx_plugin_deployments` 获取当前节点已部署的插件版本
3. 对比得出需要执行的操作（安装/升级/降级/卸载）
4. 根据 `zip_source_url` 和 `zip_source_type` 构建 `PluginSource`

   * **安装/升级**：从 `cmx_plugin` 获取来源

   * **降级**：从 `cmx_plugin_versions` 获取目标版本的来源
5. 调用对应的服务完成操作
6. 最后初始化内存中的 contexts

### 3.2 创建独立的初始化模块

**新文件**: `e:\rustspace\cmx\cmx-container\crates\libs\cmx-plugin\src\service\initializer.rs`

该模块负责：

* `sync_plugins`: 主同步逻辑

* `compare_and_plan`: 对比数据库状态，生成操作计划

* `execute_plan`: 执行计划中的操作

* `build_plugin_source`: 根据 zip\_source 构建 PluginSource

### 3.3 定义操作计划结构

```rust
/// 插件操作计划
enum PluginOperation {
    Install { plugin_id, version, source },
    Upgrade { plugin_id, from_version, to_version, source },
    Downgrade { plugin_id, from_version, to_version, source },  // source 从 versions 表获取
    Uninstall { plugin_id, version },
    None,
}
```

### 3.4 修改 manager.rs

**文件**: `e:\rustspace\cmx\cmx-container\crates\libs\cmx-plugin\src\core\manager.rs`

1. 引入新的 initializer 模块
2. 修改 `initialize` 方法，调用 initializer 的同步逻辑
3. 保留 `load_installed_plugins` 作为纯内存加载（但重命名或重构）

### 3.5 实现初始化逻辑

**sync\_plugins 函数流程**：

```
1. 获取当前节点的 node_id

2. 查询 cmx_plugin 表获取所有插件（期望状态）
   - WHERE archived = 0

3. 查询 cmx_plugin_deployments 获取当前节点的部署记录
   - WHERE node_id = {node_id} AND archived = 0

4. 构建已部署插件的 Map<plugin_id, version>
   - 当前节点安装了哪些插件的哪些版本

5. 对每个 cmx_plugin 中的插件：
   a. 如果 plugin_id 不在已部署列表中 → 需要安装
   b. 如果版本不同：
      - 如果 cmx_plugin.version > 已部署.version → 需要升级
      - 如果 cmx_plugin.version < 已部署.version → 需要降级
         - 降级时从 cmx_plugin_versions 表获取目标版本的来源信息

6. 对每个已部署但不在 cmx_plugin 中的插件 → 需要卸载

7. 执行计划：
   a. 先处理安装和升级（按依赖顺序）
   b. 再处理降级
   c. 最后处理卸载

8. 加载 contexts 到内存
```

### 3.6 PluginSource 构建逻辑

根据数据库中的 `zip_source_type` 和 `zip_source_url` 构建：

```rust
fn build_plugin_source(source_type: Option<&str>, source_url: Option<&str>) -> PluginSource {
    match source_type {
        Some("local") => PluginSource::Local { path: PathBuf::from(source_url.unwrap_or_default()) },
        Some("url") | Some("remote") => PluginSource::Remote { url: source_url.unwrap_or_default().to_string(), checksum: None },
        Some("registry") => PluginSource::Registry { registry_url: None, package_name: source_url.unwrap_or_default().to_string() },
        _ => PluginSource::Local { path: PathBuf::from(source_url.unwrap_or_default()) },
    }
}
```

***

## 任务四：确保代码编译通过

修改完成后，执行编译检查：

```bash
cargo check -p cmx-plugin
```

如有问题，根据错误信息进行修复。

***

## 文件清单

### 需要修改的文件：

1. `e:\rustspace\cmx\cmx-container\crates\libs\cmx-plugin\sql\plugin_lifecycle_schemanojson.sql`
2. `e:\rustspace\cmx\cmx-container\crates\libs\cmx-plugin\src\infrastructure\database\repository.rs`
3. `e:\rustspace\cmx\cmx-container\crates\libs\cmx-plugin\src\infrastructure\database\version_history.rs`
4. `e:\rustspace\cmx\cmx-container\crates\libs\cmx-plugin\src\service\record_builder.rs`
5. `e:\rustspace\cmx\cmx-container\crates\libs\cmx-plugin\src\service\install.rs`
6. `e:\rustspace\cmx\cmx-container\crates\libs\cmx-plugin\src\service\upgrade.rs`
7. `e:\rustspace\cmx\cmx-container\crates\libs\cmx-plugin\src\core\manager.rs`
8. `e:\rustspace\cmx\cmx-container\crates\libs\cmx-plugin\src\service\initializer.rs`

### 需要新建的文件：

无（initializer.rs 已创建）

### 需要确认的文件（可能需要修改）：

1. `e:\rustspace\cmx\cmx-container\crates\libs\cmx-plugin\src\service\mod.rs` - 导出新模块

***

## 注意事项

1. **事务处理**：安装/升级/卸载操作需要在事务中执行
2. **依赖顺序**：安装时需要按依赖顺序处理
3. **错误处理**：启动过程中的非关键插件失败不应阻止系统启动
4. **并发安全**：多个节点同时启动时需要考虑并发
5. **回滚机制**：操作失败时需要能够回滚
6. **版本来源**：降级时必须能从 `cmx_plugin_versions` 获取目标版本的来源信息

