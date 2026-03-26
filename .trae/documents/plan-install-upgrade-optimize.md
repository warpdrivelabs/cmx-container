# 代码优化计划

#



## 二、安装 vs 升级 vs 降级 流程详细对比

### 2.1 完整流程对比表

| 阶段                          | 安装 (install)         | 升级 (upgrade)          | 降级 (downgrade)                 |
| --------------------------- | -------------------- | --------------------- |--------------------------------|
| **前置检查**                    | <br />               | <br />                | <br />                         |
| 插件是否已安装                     | ❌ 不关心                | ✅ 必须已安装               | ✅ 必须已安装                        |
| deployment 是否存在             | ❌ 不关心（只检查同版本）      | ❌ 不关心（只检查同版本）        | ❌ 不关心                         |
| 版本比较                        | v < db\_version → 拒绝 | new\_v <= old\_v → 拒绝 | target\_v 必须存在，要降级的版本号必须比基线版本小 |
| **文件操作**                    | <br />               | <br />                | <br />                         |
| 创建版本目录                      | ✅                    | ✅                     | ❌                              |
| 复制文件                        | ✅                    | ✅                     | ❌                              |
| 创建数据库表                      | ✅                    | ✅                     | ❌                              |
| **数据库操作**                   | <br />               | <br />                | <br />                         |
| 插入 cmx\_plugin              | ✅ (upsert)             | ❌                     | ❌                              |
| 更新 cmx\_plugin              | ✅ (upsert)             | ✅                     | ✅                              |
| 标记旧版本非当前                    | ❌                    | ✅                     | ✅                              |
| 插入 cmx\_plugin\_versions    | ✅                    | ✅                     | ❌                              |
| 更新 cmx\_plugin\_versions    | ❌                    | ❌                     | ✅                              |
| cmx\_plugin\_deployments | **插入新记录**（节点+版本不存在时）| **插入新记录**（节点+版本不存在时） | **更新版本号**（同节点切换当前版本）                    |
| **后置操作**                    | <br />               | <br />                | <br />                         |
| 更新注册表                       | ✅                    | ✅                     | ✅                              |
| 更新缓存                        | ✅                    | ✅                     | ✅                              |
| 审计日志                        | ✅                    | ✅                     | ✅                              |
| 发布事件                        | ✅                    | ✅                     | ✅                              |

### 2.1.1 cmx_plugin_deployments 表操作说明

**表设计目的**：记录每个节点（node_id）上已安装的插件版本列表，支持同一插件多版本共存。

**唯一约束**：`plugin_id + node_id + version`

**操作规则**：
- **安装**：检查 `plugin_id + node_id + version` 是否存在，不存在则插入
- **升级**：检查 `plugin_id + node_id + version` 是否存在，不存在则插入（同一个插件可以在一个节点上安装多个版本）
- **降级**：更新该节点上的 deployment 记录的 version 字段（只切换当前运行版本，不插入新记录）

**部署状态**：
- `deployed`：已部署
- `activating`：激活中
- `activated`：已激活
- `failed`：失败

### 2.2 关键差异分析

**核心差异点：**

1. **安装**：插件不存在时走插入流程，插件存在时走更新流程
2. **升级**：插件必须存在，新版本必须大于当前版本
3. **降级**：只切换版本目录，不复制文件，不创建表

### 2.3 如果用户想升级却调用安装流程的影响矩阵

| 场景                      | 调用 install 结果              | 调用 upgrade 结果 | 差异分析              |
| ----------------------- | -------------------------- | ------------- | ----------------- |
| v1.0 已安装，部署v1.0，想装 v2.0 | ❌ deployment 已存在 → 报错"已安装" | ✅ 正常升级到 v2.0  | install 报错信息不够明确  |
| v1.0 已安装，部署v1.0，想装 v1.0 | ❌ deployment 已存在 → 报错"已安装" | ❌ 报错"版本必须更大"  | upgrade 给出了更准确的错误 |
| v1.0 已安装，部署v1.0，想装 v0.9 | ❌ 报错"应使用降级方式"              | ❌ 报错"版本必须更大"  | install 给出了更明确的建议 |
| v0.9 已安装，部署v0.9，想装 v1.0 | ✅ 安装成功                     | ❌ 报错"请使用安装功能" | upgrade 提示不够友好    |
| 插件从未安装，想装 v1.0          | ✅ 安装成功                     | ❌ 报错"插件不存在"   | 行为正确              |

**结论：**

* install 和 upgrade 的职责有重叠区域（已有插件且版本更高时）

* upgrade 对"插件未安装"的错误提示不够友好

* install 对"已安装想升级"的场景提示不够明确

* **建议：考虑合并或增加智能路由**

***

## 三、优化方案详细设计

### 3.1 仓库层优化



#### 3.1.2 添加 `upsert_version` 方法

```rust
/// 插入或更新版本历史记录
///
/// 使用 plugin_id + version 作为唯一约束
pub async fn upsert_version(
    &self,
    record: &VersionHistoryRecord,
    txn_id: Option<&str>,
) -> PluginResult<bool> {
    // ON CONFLICT (plugin_id, version) DO UPDATE ...
}
```



***




***

### 3.3 版本检查统一

建议新增一个版本比较服务或工具函数：

```rust
/// 版本操作决策
#[derive(Debug)]
pub enum VersionAction {
    Install,      // 新安装
    Upgrade,      // 升级到更新版本
    Downgrade,    // 降级到更旧版本
    Reinstall,    // 同一版本重新安装
    AlreadyLatest, // 已是最新版本
}

/// 比较版本并决定操作类型
pub fn decide_version_action(
    requested_version: &str,
    current_version: Option<&str>,
) -> VersionAction {
    match current_version {
        None => VersionAction::Install,
        Some(cv) => {
            if requested_version == cv {
                VersionAction::Reinstall
            } else if requested_version > cv {
                VersionAction::Upgrade
            } else {
                VersionAction::Downgrade
            }
        }
    }
}
```

***



### 3.5 Repository 层重构：使用 cmx-database crud 模块（新增）

#### 3.5.1 当前问题

当前 `repository.rs`、`version_history.rs`、`deployment.rs` 都是手动实现的 CRUD 操作，存在以下问题：

1. **代码重复**：每个 Repository 都有类似的 `insert`、`update`、`find`、`delete` 方法
2. **风格不统一**：有的使用 `sea_query` 构建 SQL，有的使用原生 SQL
3. **维护成本高**：修改字段需要同时修改多处代码

#### 3.5.2 建议方案：使用 GenericCrudService

参考 `cmx-database/src/crud` 模块，使用 `GenericCrudService` 和 `DbBmc` trait 封装：

**步骤一：定义 DbBmc trait 实现**

```rust
// repository.rs
use cmx_database::crud::{DbBmc, GenericCrudService};

/// 插件表 BMC (Business Model Controller)
pub struct PluginBmc;

impl DbBmc for PluginBmc {
    const TABLE: &'static str = "cmx_plugin";
    const PK_COLUMN: &'static str = "id";
    
    fn has_timestamps() -> bool { true }
    fn has_owner_id() -> bool { false }
}
```

**步骤二：使用 GenericCrudService**

```rust
// 插入
GenericCrudService::<PluginBmc>::create(mm, db_id, txn_id, &record).await?;

// 查询
let record: Option<PluginDbRecord> = GenericCrudService::<PluginBmc>::get_by_pk(mm, db_id, txn_id, &id).await?;

// 更新
GenericCrudService::<PluginBmc>::update(mm, db_id, txn_id, &id, &fields).await?;

// 删除
GenericCrudService::<PluginBmc>::delete(mm, db_id, txn_id, &id).await?;
```

**步骤三：定义其他表的 BMC**

```rust
// version_history.rs
pub struct PluginVersionBmc;

impl DbBmc for PluginVersionBmc {
    const TABLE: &'static str = "cmx_plugin_versions";
    const PK_COLUMN: &'static str = "id";
    fn has_timestamps() -> bool { true }
    fn has_owner_id() -> bool { false }
}

// deployment.rs
pub struct PluginDeploymentBmc;

impl DbBmc for PluginDeploymentBmc {
    const TABLE: &'static str = "cmx_plugin_deployments";
    const PK_COLUMN: &'static str = "id";
    fn has_timestamps() -> bool { true }
    fn has_owner_id() -> bool { false }
}
```

#### 3.5.3 重构后的优势

| 方面 | 重构前 | 重构后 |
|------|--------|--------|
| 代码量 | 每个 Repository ~300行 | 每个 BMC ~20行 |
| CRUD 方法 | 手动实现 | 自动继承 |
| SQL 构建 | 手动 sea_query | 框架统一处理 |
| 字段映射 | 手动解析 | 自动映射 |
| 错误处理 | 各自定义 | 统一格式 |

#### 3.5.4 需要保留的自定义方法

某些业务特定的查询方法仍需保留：

```rust
impl PluginRepository {
    /// 查询基线版本（业务特定）
    pub async fn get_baseline_version(&self, plugin_id: &str) -> PluginResult<Option<String>> { ... }
    
    /// 通过 plugin_id 查询（业务特定）
    pub async fn find_plugin(&self, plugin_id: &str) -> PluginResult<Option<PluginDbRecord>> { ... }
}

impl VersionHistoryRepository {
    /// 标记所有版本为非当前（业务特定）
    pub async fn mark_all_not_current(&self, plugin_id: &str, txn_id: Option<&str>) -> PluginResult<()> { ... }
    
    /// 获取当前版本（业务特定）
    pub async fn get_current_baseline(&self, plugin_id: &str) -> PluginResult<Option<VersionHistoryRecord>> { ... }
}
```

***

### 3.6 并发安全优化（新增）



#### 3.6.2 使用数据库锁或乐观锁

**方案一：SELECT FOR UPDATE**

```rust
// 在事务开始时锁定插件记录
let sql = "SELECT * FROM cmx_plugin WHERE plugin_id = $1 FOR UPDATE";
```

**方案二：乐观锁（推荐）**

在 `cmx_plugin` 表添加 `version` 字段（乐观锁版本号）：

```rust
UPDATE cmx_plugin SET ..., version = version + 1 
WHERE plugin_id = $1 AND version = $old_version;

// 如果 affected_rows == 0，说明有并发修改，抛出错误
```

***



***

## 四、是否合并 install/upgrade 的决策分析

### 4.1 合并方案的优点

1. **用户体验更好**：用户不需要关心该调用哪个方法
2. **代码复用性更高**：减少重复代码
3. **统一版本策略**：避免分散的版本检查逻辑

### 4.2 合并方案的缺点

1. **职责边界模糊**：`install_or_upgrade` 方法承担了太多职责
2. **日志和审计不清晰**：无法区分是安装还是升级操作
3. **前端适配问题**：可能需要修改 API 设计

### 4.3 建议方案

**不合并，但优化错误提示：**

在 install 失败时增加智能提示：

```rust
// 当前代码（install.rs 183-191行）
if existing_deployment.is_some() {
    return Err(PluginError::plugin_already_exists(&plugin_id));
}

// 优化为：
if existing_deployment.is_some() {
    let plugin = self.deps.repository.find_plugin(&plugin_id).await?;
    if let Some(ref p) = plugin {
        if version > p.version {
            return Err(PluginError::Install(format!(
                "插件 {} 已安装版本 {}，要升级到 {} 请使用升级功能",
                plugin_id, p.version, version
            )));
        } else if version < p.version {
            return Err(PluginError::Install(format!(
                "插件 {} 已安装版本 {}，要降级到 {} 请使用降级功能",
                plugin_id, p.version, version
            )));
        } else {
            return Err(PluginError::plugin_already_exists(&plugin_id));
        }
    }
}
```

***

## 五、具体修改步骤

### 步骤 1: 修复 repository.rs 的 bug

- [x] 修复 `update_plugin` 方法中 `name` 字段的判断条件

### 步骤 2: 添加 upsert 方法

- [x] 在 `repository.rs` 添加 `upsert_plugin` 方法

- [x] 在 `version_history.rs` 添加 `upsert_version` 方法 ✅ 已实现

### 步骤 3: 提取记录构建函数

- [x] 新建 `service/record_builder.rs`

- [x] 实现 `build_plugin_db_record` 函数

- [x] 实现 `build_version_record` 函数

- [x] 实现 `build_deployment_record` 函数

### 步骤 4: 简化 install.rs 步骤9

- [x] 使用 `build_*` 函数替代直接构建

- [x] 使用 `upsert_plugin` 替代 if-else 逻辑

- [x] 添加智能错误提示

### 步骤 5: 简化 upgrade.rs

- [x] 使用 `build_*` 函数替代直接构建

- [x] 使用 `upsert_plugin` 优化逻辑

- [x] 优化错误提示

### 步骤 6: 验证和测试

- [ ] 编写单元测试验证 upsert 行为

- [ ] 手动测试 install/upgrade/downgrade 流程

***

## 六、预期优化效果

| 指标                      | 优化前       | 优化后       | 改善幅度  |
| ----------------------- | --------- | --------- | ----- |
| `db_record` 构建重复代码      | \~35行 x 2 | \~35行 x 1 | -50%  |
| `version_record` 构建重复代码 | \~22行 x 2 | \~22行 x 1 | -50%  |
| `update_fields` 构建重复代码  | \~29行 x 2 | \~29行 x 1 | -50%  |
| install.rs 步骤9代码行数      | \~135行    | \~50行     | -63%  |
| upgrade.rs 数据库操作代码行数    | \~100行    | \~40行     | -60%  |
| if-else 分支深度            | 3层        | 1层        | -67%  |
| Bug 数量                  | 1个已知      | 0         | -100% |
| 错误提示质量                  | 模糊        | 明确        | ✅     |

***

## 八、风险评估

| 风险                  | 影响 | 缓解措施                                  |
| ------------------- | -- | ------------------------------------- |
| upsert SQL 错误       | 高  | 先在测试环境验证 SQL                          |
| 事务边界问题              | 高  | 统一所有数据库操作在同一事务中                       |
| 并发竞态条件              | 高  | 添加数据库唯一约束 + 乐观锁                       |
| is_current 标记不一致    | 中  | 提取统一的版本历史管理方法                         |
| Repository 重构兼容性   | 中  | 保留自定义方法，仅重构通用 CRUD                    |
| 向后兼容性               | 低  | 仅内部重构，不改 API                          |
| 性能影响                | 低  | upsert 比 select+insert/update 更高效     |

***

## 九、最终建议总结

### 9.1 已完成优化项 ✅

以下优化已按计划完成：

| 优化项 | 完成状态 | 备注 |
|-------|---------|------|
| repository.rs 第262行 Bug | ✅ 已修复 | `name` 字段判断条件已更正 |
| upsert_plugin 方法 | ✅ 已实现 | 支持 RETURNING 子句 |
| record_builder.rs | ✅ 已创建 | build_plugin_db_record, build_version_record, build_deployment_record |
| install.rs 重构 | ✅ 已完成 | 使用辅助函数和 upsert_plugin |
| upgrade.rs 重构 | ✅ 已完成 | 使用辅助函数和 upsert_plugin |
| 智能错误提示 | ✅ 已完成 | install 时版本比较提示 |

### 9.2 必须修复的问题（高优先级）

1. **version_history.update_version WHERE 条件错误** ✅ 已修复
   - 位置：`version_history.rs` 第137行
   - 修复方案：使用传入的 `id` 参数作为 WHERE 条件

2. **downgrade.rs 调用逻辑问题** ✅ 已修复
   - 位置：`downgrade.rs` 第130-140行
   - 修复方案：使用 `set_current_version` 原子方法替代

3. ~~**repository.rs 第262行 Bug**~~ ✅ 已修复（之前已标记）

### 9.3 建议优化的问题（中优先级）

1. **代码重复**：提取 `build_*` 辅助函数 ✅ 已完成
2. **upsert 方法**：减少数据库往返次数
   - [x] upsert_plugin ✅ 已完成
   - [x] upsert_version ✅ 已完成
3. **版本历史标记**：统一 `is_current` 标记逻辑 ✅ 已完成
   - `set_current_version` 原子方法已实现，downgrade 现已使用

### 9.4 可选优化的问题（低优先级）

1. **错误提示优化** ✅ 智能提示用户使用正确的操作 - 已完成
2. **Repository 层重构**：使用 `cmx-database` 的 `GenericCrudService` 和 `DbBmc` 封装
3. **并发安全优化**：添加 SELECT FOR UPDATE 或乐观锁

### 9.5 关于 install/upgrade 合并的最终建议

**不建议合并**，原因如下：

1. **职责清晰**：install 和 upgrade 有明确的业务语义差异
2. **审计友好**：分开的操作便于审计日志分析
3. **API 稳定**：避免破坏现有 API

**但需要改进：**

1. 在 install 失败时提供智能提示 ✅ 已完成
2. 统一两者的版本历史管理逻辑 ✅ 已完成（downgrade 现使用 set_current_version）
3. 提取公共代码减少重复 ✅ 已完成

---

## 十、当前状态总结

| 分类 | 完成 | 待完成 | 完成率 |
|-----|------|-------|--------|
| 步骤 1-6 | 10 | 2 | 83% |
| 高优先级 Bug | 3 | 0 | 100% |
| 中优先级优化 | 4 | 2 | 67% |
| 低优先级优化 | 1 | 2 | 33% |

**已完成的关键修复：**
1. ✅ version_history.update_version WHERE 条件 bug
2. ✅ downgrade.rs 使用 set_current_version 原子方法
3. ✅ upsert_version 方法

**剩余待优化项：**
- Repository 层重构（GenericCrudService）
- 并发安全优化（SELECT FOR UPDATE 或乐观锁）

**检查日期：2026-03-26**

