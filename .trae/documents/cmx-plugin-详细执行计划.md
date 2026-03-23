# cmx-plugin 模块架构优化 - 详细执行计划

## 执行原则

1. **每步独立验证**：每完成一个步骤，必须进行代码检查和功能验证
2. **保持编译通过**：每个步骤完成后，代码必须能编译通过
3. **渐进式重构**：先创建新结构，再迁移代码，最后删除旧代码
4. **文档同步更新**：每步完成后更新相关文档

---

## 第一阶段：基础重构

### 步骤 1.1：创建目录结构

**目标**：创建新的分包目录结构，但不移动任何代码

**操作**：
1. 在 `cmx-plugin/src/` 下创建以下目录：
   - `core/`
   - `domain/`
   - `service/`
   - `infrastructure/`
   - `infrastructure/database/`
   - `infrastructure/cache/`
   - `infrastructure/storage/`
   - `infrastructure/messaging/`
   - `cluster/`
   - `security/`
   - `runtime/`
   - `config/`
   - `audit/`
   - `fetcher/`

2. 在每个目录下创建空的 `mod.rs` 文件

**验证检查**：
- [ ] 所有目录已创建
- [ ] 所有 mod.rs 文件已创建
- [ ] `cargo check` 编译通过
- [ ] 现有功能不受影响

---

### 步骤 1.2：重构错误类型

**目标**：整合所有错误类型到统一的 error.rs

**操作**：
1. 分析现有错误类型：
   - `error.rs` 中的 `PluginError`
   - `db.rs` 中的 `PluginDbError`
   - `cache.rs` 中的 `PluginCacheError`
   - `node.rs` 中的 `NodeError`
   - `permission.rs` 中的 `PermissionError`
   - `message.rs` 中的 `MessageQueueError`

2. 设计统一的错误层次结构：
```rust
/// 插件错误类型
#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    // 安装相关
    #[error("安装错误: {0}")]
    Install(String),
    
    // 卸载相关
    #[error("卸载错误: {0}")]
    Uninstall(String),
    
    // 激活相关
    #[error("激活错误: {0}")]
    Activate(String),
    
    // 数据库相关
    #[error("数据库错误: {0}")]
    Database(#[from] DatabaseError),
    
    // 缓存相关
    #[error("缓存错误: {0}")]
    Cache(#[from] CacheError),
    
    // ... 其他错误类型
}

/// 数据库错误
#[derive(Debug, thiserror::Error)]
pub enum DatabaseError {
    #[error("连接失败: {0}")]
    Connection(String),
    #[error("查询失败: {0}")]
    Query(String),
    // ...
}

/// 缓存错误
#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("Redis错误: {0}")]
    Redis(String),
    #[error("序列化错误: {0}")]
    Serialization(String),
    // ...
}
```

3. 更新 `error.rs`，添加所有错误类型

4. 为其他模块的错误类型添加 `From` 实现

**验证检查**：
- [ ] error.rs 包含所有错误类型
- [ ] 所有错误类型有清晰的文档注释
- [ ] `From` 实现完整
- [ ] `cargo check` 编译通过
- [ ] 现有代码中的错误处理仍然工作

---

### 步骤 1.3：创建 domain 模块

**目标**：将 types.rs 拆分到 domain 模块

**操作**：
1. 创建 `domain/plugin.rs`，迁移插件定义相关类型：
   - `PluginInfo`
   - `PluginStatus`
   - `PluginSource`
   - `PluginFilter`
   - `PluginConfig`
   - `PluginDatabaseConfig`

2. 创建 `domain/status.rs`，迁移状态相关类型：
   - `PluginStatus` 枚举
   - 状态转换方法

3. 创建 `domain/version.rs`，从现有 `version.rs` 迁移：
   - `SemanticVersion`
   - `PreRelease`
   - `VersionConstraint`
   - `VersionRelation`

4. 创建 `domain/dependency.rs`，迁移依赖相关类型：
   - `DependencyCheckResult`
   - `DependencyResolution`
   - `DependencyGraph`
   - `MissingDependency`
   - `DependencyConflict`

5. 更新 `domain/mod.rs` 导出所有类型

6. 更新 `lib.rs`，添加 `pub mod domain;`

7. 在 `types.rs` 中添加类型重导出（保持向后兼容）：
```rust
// types.rs - 保持向后兼容
pub use crate::domain::plugin::*;
pub use crate::domain::status::*;
pub use crate::domain::dependency::*;
```

**验证检查**：
- [ ] domain 模块结构正确
- [ ] 所有类型已迁移
- [ ] 类型有文档注释
- [ ] `cargo check` 编译通过
- [ ] 现有代码无需修改即可编译

---

### 步骤 1.4：创建 core 模块

**目标**：创建核心模块的基础结构

**操作**：
1. 创建 `core/registry.rs`：
   - 从现有 `registry.rs` 迁移 `PluginRegistry`
   - 添加遍历所有插件的方法
   - 添加按状态筛选的方法

2. 创建 `core/context.rs`：
```rust
/// 插件上下文 - 管理插件运行时状态
pub struct PluginContext {
    /// 插件ID
    pub plugin_id: String,
    /// 插件版本
    pub version: String,
    /// 插件状态
    pub status: PluginStatus,
    /// 关联的数据库ID
    pub db_id: String,
    /// 安装路径
    pub install_path: PathBuf,
    /// WASM路径
    pub wasm_path: PathBuf,
    /// 配置
    pub config: Option<Value>,
    /// 激活时间
    pub activated_at: Option<DateTime<Utc>>,
    /// 服务句柄列表
    pub services: Vec<ServiceHandle>,
    /// 扩展元数据
    pub metadata: HashMap<String, Value>,
}

impl PluginContext {
    /// 从插件定义创建上下文
    pub fn from_definition(def: &PluginDefinition, install_path: &Path) -> Self;
    
    /// 转换为数据库记录
    pub fn to_db_record(&self) -> PluginDbRecord;
    
    /// 从数据库记录创建
    pub fn from_db_record(record: &PluginDbRecord) -> Self;
}
```

3. 创建 `core/lifecycle.rs`：
```rust
/// 插件生命周期状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleState {
    /// 未安装
    NotInstalled,
    /// 已安装
    Installed,
    /// 激活中
    Activating,
    /// 已激活
    Activated,
    /// 停用中
    Deactivating,
    /// 已停用
    Deactivated,
    /// 卸载中
    Uninstalling,
    /// 错误状态
    Error,
}

/// 生命周期状态机
pub struct LifecycleStateMachine;

impl LifecycleStateMachine {
    /// 检查状态转换是否有效
    pub fn can_transition(from: LifecycleState, to: LifecycleState) -> bool;
    
    /// 获取有效的目标状态
    pub fn valid_transitions(from: LifecycleState) -> Vec<LifecycleState>;
}
```

4. 更新 `core/mod.rs` 导出

5. 更新 `lib.rs`，添加 `pub mod core;`

**验证检查**：
- [ ] core 模块结构正确
- [ ] PluginContext 实现完整
- [ ] LifecycleStateMachine 实现完整
- [ ] 所有类型有文档注释
- [ ] `cargo check` 编译通过

---

### 步骤 1.5：创建 infrastructure/database 模块

**目标**：创建数据库层的基础结构

**操作**：
1. 创建 `infrastructure/database/schema.rs`：
```rust
/// 数据库表结构定义
pub struct SchemaManager;

impl SchemaManager {
    /// 获取创建插件系统表的SQL
    pub fn get_create_system_tables_sql() -> Vec<&'static str>;
    
    /// 获取创建插件功能表的SQL
    pub fn get_create_features_table_sql() -> &'static str;
    
    /// 获取创建插件事件表的SQL
    pub fn get_create_events_table_sql() -> &'static str;
}
```

2. 创建 `infrastructure/database/repository.rs`：
```rust
use cmx_database::DatabaseManager;

/// 插件数据仓库
pub struct PluginRepository {
    db_manager: Arc<DatabaseManager>,
    default_db_id: String,
}

impl PluginRepository {
    /// 创建新的数据仓库
    pub fn new(db_manager: Arc<DatabaseManager>, default_db_id: String) -> Self;
    
    /// 初始化系统表
    pub async fn init_system_tables(&self) -> Result<(), PluginError>;
    
    /// 插入插件记录
    pub async fn insert_plugin(&self, record: &PluginDbRecord) -> Result<(), PluginError>;
    
    /// 更新插件记录
    pub async fn update_plugin(&self, plugin_id: &str, fields: &PluginUpdateFields) -> Result<(), PluginError>;
    
    /// 删除插件记录
    pub async fn delete_plugin(&self, plugin_id: &str) -> Result<(), PluginError>;
    
    /// 查询插件记录
    pub async fn find_plugin(&self, plugin_id: &str) -> Result<Option<PluginDbRecord>, PluginError>;
    
    /// 列出所有插件
    pub async fn list_plugins(&self, filter: &PluginFilter) -> Result<Vec<PluginDbRecord>, PluginError>;
}
```

3. 创建 `infrastructure/database/migration.rs`：
```rust
/// 表结构迁移管理器
pub struct MigrationManager {
    db_manager: Arc<DatabaseManager>,
}

impl MigrationManager {
    /// 执行迁移
    pub async fn migrate(&self, db_id: &str) -> Result<(), PluginError>;
    
    /// 检查迁移状态
    pub async fn check_migration_status(&self, db_id: &str) -> Result<MigrationStatus, PluginError>;
}
```

4. 更新 `infrastructure/database/mod.rs` 导出

5. 更新 `infrastructure/mod.rs`，添加 `pub mod database;`

**验证检查**：
- [ ] database 模块结构正确
- [ ] PluginRepository 实现完整
- [ ] 集成 cmx-database
- [ ] 所有方法有文档注释
- [ ] `cargo check` 编译通过

---

### 步骤 1.6：创建 infrastructure/cache 模块

**目标**：创建缓存层的基础结构

**操作**：
1. 创建 `infrastructure/cache/memory.rs`：
   - 从现有 `memory_cache.rs` 迁移内存缓存实现
   - 添加文档注释

2. 创建 `infrastructure/cache/redis.rs`：
   - 从现有 `cache.rs` 迁移 Redis 缓存实现
   - 添加文档注释

3. 创建 `infrastructure/cache/layered.rs`：
```rust
use std::sync::Arc;
use std::time::Duration;

/// 缓存值类型
#[derive(Debug, Clone)]
pub enum CacheValue {
    String(String),
    Json(serde_json::Value),
    PluginInfo(PluginInfo),
    PluginList(Vec<PluginInfo>),
}

/// 缓存策略
#[derive(Debug, Clone)]
pub struct CacheStrategy {
    /// L1 缓存 TTL（秒）
    pub l1_ttl_seconds: u64,
    /// L2 缓存 TTL（秒）
    pub l2_ttl_seconds: u64,
    /// 是否启用 L1 缓存
    pub enable_l1: bool,
    /// 是否启用 L2 缓存
    pub enable_l2: bool,
}

impl Default for CacheStrategy {
    fn default() -> Self {
        Self {
            l1_ttl_seconds: 300,
            l2_ttl_seconds: 3600,
            enable_l1: true,
            enable_l2: true,
        }
    }
}

/// 多层缓存协调器
pub struct LayeredCacheManager {
    /// 内存缓存（L1）
    memory_cache: Arc<MemoryCache<CacheValue>>,
    /// Redis缓存（L2）
    redis_cache: Option<Arc<cmx_buffer::CacheManager>>,
    /// 缓存策略
    strategy: CacheStrategy,
}

impl LayeredCacheManager {
    /// 创建新的缓存管理器
    pub fn new(strategy: CacheStrategy) -> Self;
    
    /// 设置 Redis 缓存
    pub fn with_redis(mut self, redis_cache: Arc<cmx_buffer::CacheManager>) -> Self;
    
    /// 获取缓存
    pub async fn get(&self, key: &str) -> Option<CacheValue>;
    
    /// 设置缓存
    pub async fn set(&self, key: &str, value: CacheValue, ttl: Option<Duration>);
    
    /// 删除缓存
    pub async fn delete(&self, key: &str);
    
    /// 刷新缓存（从L2同步到L1）
    pub async fn refresh(&self, key: &str);
    
    /// 清空所有缓存
    pub async fn clear(&self);
}
```

4. 更新 `infrastructure/cache/mod.rs` 导出

**验证检查**：
- [ ] cache 模块结构正确
- [ ] LayeredCacheManager 实现完整
- [ ] 集成 cmx-buffer
- [ ] 所有方法有文档注释
- [ ] `cargo check` 编译通过

---

### 步骤 1.7：创建 infrastructure/storage 模块

**目标**：创建存储层的基础结构

**操作**：
1. 创建 `infrastructure/storage/file.rs`：
```rust
/// 文件存储管理器
pub struct FileStorage {
    base_path: PathBuf,
}

impl FileStorage {
    /// 创建新的文件存储
    pub fn new(base_path: &Path) -> Self;
    
    /// 复制目录
    pub async fn copy_directory(&self, src: &Path, dst: &Path) -> Result<(), PluginError>;
    
    /// 删除目录
    pub async fn remove_directory(&self, path: &Path) -> Result<(), PluginError>;
    
    /// 检查路径是否存在
    pub async fn exists(&self, path: &Path) -> bool;
    
    /// 列出目录内容
    pub async fn list_directory(&self, path: &Path) -> Result<Vec<PathBuf>, PluginError>;
}
```

2. 创建 `infrastructure/storage/backup.rs`：
```rust
/// 备份管理器
pub struct BackupManager {
    backup_root: PathBuf,
}

impl BackupManager {
    /// 创建新的备份管理器
    pub fn new(backup_root: PathBuf) -> Self;
    
    /// 创建备份
    pub async fn create_backup(&self, plugin_id: &str, version: &str, source_path: &Path) -> Result<PathBuf, PluginError>;
    
    /// 恢复备份
    pub async fn restore_backup(&self, backup_path: &Path, target_path: &Path) -> Result<(), PluginError>;
    
    /// 列出所有备份
    pub async fn list_backups(&self, plugin_id: &str) -> Result<Vec<BackupInfo>, PluginError>;
    
    /// 删除备份
    pub async fn delete_backup(&self, backup_path: &Path) -> Result<(), PluginError>;
    
    /// 清理过期备份
    pub async fn cleanup_old_backups(&self, plugin_id: &str, keep_count: usize) -> Result<usize, PluginError>;
}

/// 备份信息
#[derive(Debug, Clone)]
pub struct BackupInfo {
    pub path: PathBuf,
    pub version: String,
    pub created_at: DateTime<Utc>,
    pub size: u64,
}
```

3. 更新 `infrastructure/storage/mod.rs` 导出

**验证检查**：
- [ ] storage 模块结构正确
- [ ] FileStorage 实现完整
- [ ] BackupManager 实现完整
- [ ] 所有方法有文档注释
- [ ] `cargo check` 编译通过

---

### 步骤 1.8：创建 infrastructure/messaging 模块

**目标**：创建消息层的基础结构

**操作**：
1. 创建 `infrastructure/messaging/queue.rs`：
   - 从现有 `message.rs` 迁移消息队列实现
   - 添加文档注释

2. 创建 `infrastructure/messaging/event.rs`：
```rust
use std::sync::Arc;
use cmx_buffer::PubSubOps;

/// 事件类型
#[derive(Debug, Clone)]
pub enum EventType {
    PluginInstalled,
    PluginUninstalled,
    PluginActivated,
    PluginDeactivated,
    PluginUpgraded,
    PluginDowngrade,
    PluginError,
}

/// 事件
#[derive(Debug, Clone)]
pub struct Event {
    pub event_type: EventType,
    pub plugin_id: String,
    pub data: serde_json::Value,
    pub timestamp: DateTime<Utc>,
}

/// 事件总线
pub struct EventBus {
    pubsub: Option<Arc<PubSubOps>>,
    handlers: Arc<RwLock<HashMap<String, Vec<EventHandler>>>>,
}

/// 事件处理器
pub type EventHandler = Box<dyn Fn(Event) + Send + Sync>;

impl EventBus {
    /// 创建新的事件总线
    pub fn new() -> Self;
    
    /// 设置 Redis PubSub
    pub fn with_pubsub(mut self, pubsub: Arc<PubSubOps>) -> Self;
    
    /// 发布事件
    pub async fn publish(&self, event: Event) -> Result<(), PluginError>;
    
    /// 订阅事件
    pub async fn subscribe(&self, event_type: &str, handler: EventHandler);
    
    /// 取消订阅
    pub async fn unsubscribe(&self, event_type: &str);
}
```

3. 更新 `infrastructure/messaging/mod.rs` 导出

**验证检查**：
- [ ] messaging 模块结构正确
- [ ] EventBus 实现完整
- [ ] 集成 cmx-buffer PubSub
- [ ] 所有方法有文档注释
- [ ] `cargo check` 编译通过

---

### 步骤 1.9：创建其他模块骨架

**目标**：创建 service、cluster、security、runtime、config、audit、fetcher 模块的骨架

**操作**：
1. 创建 `service/` 模块骨架：
   - `service/mod.rs`
   - `service/install.rs` - 空的 InstallService 结构体
   - `service/uninstall.rs` - 空的 UninstallService 结构体
   - `service/activate.rs` - 空的 ActivateService 结构体
   - `service/upgrade.rs` - 空的 UpgradeService 结构体
   - `service/downgrade.rs` - 空的 DowngradeService 结构体
   - `service/rollback.rs` - 空的 RollbackService 结构体

2. 创建 `cluster/` 模块骨架：
   - `cluster/mod.rs`
   - `cluster/node.rs` - 从现有 `node.rs` 迁移
   - `cluster/deployment.rs` - 从现有 `deployment.rs` 迁移
   - `cluster/sync.rs` - 新的状态同步实现

3. 创建 `security/` 模块骨架：
   - `security/mod.rs`
   - `security/validator.rs` - 从现有 `security.rs` 迁移
   - `security/signature.rs` - 签名验证实现
   - `security/permission.rs` - 从现有 `permission.rs` 迁移

4. 创建 `runtime/` 模块骨架：
   - `runtime/mod.rs`
   - `runtime/activation.rs` - 从现有 `activation.rs` 迁移
   - `runtime/service_registry.rs` - 从现有 `service.rs` 迁移
   - `runtime/feature.rs` - 新的功能管理实现

5. 创建 `config/` 模块骨架：
   - `config/mod.rs`
   - `config/settings.rs` - 从现有 `config.rs` 迁移
   - `config/loader.rs` - 配置加载实现

6. 创建 `audit/` 模块骨架：
   - `audit/mod.rs`
   - `audit/logger.rs` - 从现有 `audit.rs` 迁移
   - `audit/record.rs` - 审计记录定义

7. 创建 `fetcher/` 模块骨架：
   - `fetcher/mod.rs`
   - `fetcher/source.rs` - 来源定义
   - `fetcher/local.rs` - 本地获取
   - `fetcher/remote.rs` - 远程获取
   - `fetcher/registry.rs` - 注册表获取

**验证检查**：
- [ ] 所有模块骨架已创建
- [ ] 所有 mod.rs 正确导出
- [ ] `cargo check` 编译通过
- [ ] 现有功能不受影响

---

### 第一阶段检查点

完成以上所有步骤后，进行以下检查：

**代码检查**：
- [ ] `cargo check` 无错误
- [ ] `cargo clippy` 无警告
- [ ] `cargo test` 所有测试通过

**结构检查**：
- [ ] 目录结构符合规划
- [ ] 模块职责清晰
- [ ] 导出路径正确

**功能检查**：
- [ ] 现有 API 保持兼容
- [ ] 现有功能正常工作

---

## 第二阶段：基础设施层实现

### 步骤 2.1：实现 PluginRepository

**目标**：完整实现数据库仓库

**操作**：
1. 实现 `PluginRepository::init_system_tables`
2. 实现 `PluginRepository::insert_plugin`
3. 实现 `PluginRepository::update_plugin`
4. 实现 `PluginRepository::delete_plugin`
5. 实现 `PluginRepository::find_plugin`
6. 实现 `PluginRepository::list_plugins`
7. 实现 `PluginRepository::create_plugin_tables`（集成 cmx-metadata）

**验证检查**：
- [ ] 所有方法实现完整
- [ ] 单元测试通过
- [ ] 集成 cmx-database 正确
- [ ] `cargo check` 编译通过

---

### 步骤 2.2：实现 LayeredCacheManager

**目标**：完整实现多层缓存协调器

**操作**：
1. 实现 `LayeredCacheManager::get`
2. 实现 `LayeredCacheManager::set`
3. 实现 `LayeredCacheManager::delete`
4. 实现 `LayeredCacheManager::refresh`
5. 实现 `LayeredCacheManager::clear`

**验证检查**：
- [ ] 所有方法实现完整
- [ ] 单元测试通过
- [ ] 集成 cmx-buffer 正确
- [ ] `cargo check` 编译通过

---

### 步骤 2.3：实现 BackupManager

**目标**：完整实现备份管理器

**操作**：
1. 实现 `BackupManager::create_backup`
2. 实现 `BackupManager::restore_backup`
3. 实现 `BackupManager::list_backups`
4. 实现 `BackupManager::delete_backup`
5. 实现 `BackupManager::cleanup_old_backups`

**验证检查**：
- [ ] 所有方法实现完整
- [ ] 单元测试通过
- [ ] `cargo check` 编译通过

---

### 步骤 2.4：实现 EventBus

**目标**：完整实现事件总线

**操作**：
1. 实现 `EventBus::publish`
2. 实现 `EventBus::subscribe`
3. 实现 `EventBus::unsubscribe`

**验证检查**：
- [ ] 所有方法实现完整
- [ ] 单元测试通过
- [ ] 集成 cmx-buffer PubSub 正确
- [ ] `cargo check` 编译通过

---

### 第二阶段检查点

完成以上所有步骤后，进行以下检查：

**代码检查**：
- [ ] `cargo check` 无错误
- [ ] `cargo clippy` 无警告
- [ ] `cargo test` 所有测试通过

**功能检查**：
- [ ] 数据库操作正常
- [ ] 缓存操作正常
- [ ] 备份操作正常
- [ ] 事件发布订阅正常

---

## 第三阶段：服务层实现

### 步骤 3.1：实现 InstallService

**目标**：完整实现安装服务

**操作**：
1. 创建 `InstallService` 结构体：
```rust
pub struct InstallService {
    repository: Arc<PluginRepository>,
    cache: Arc<LayeredCacheManager>,
    storage: Arc<FileStorage>,
    validator: Arc<SecurityValidator>,
    event_bus: Arc<EventBus>,
    audit_logger: Arc<AuditLogger>,
}
```

2. 实现 `InstallService::install`：
   - 获取插件包
   - 验证插件安全性
   - 解析插件定义
   - 检查已安装状态
   - 解析依赖
   - 创建安装目录
   - 复制文件
   - 创建数据库表
   - 注册插件
   - 保存数据库记录
   - 更新缓存
   - 记录审计日志

**验证检查**：
- [ ] 安装流程完整
- [ ] 单元测试通过
- [ ] 集成测试通过
- [ ] `cargo check` 编译通过

---

### 步骤 3.2：实现 UninstallService

**目标**：完整实现卸载服务

**操作**：
1. 创建 `UninstallService` 结构体
2. 实现 `UninstallService::uninstall`：
   - 检查插件存在
   - 检查依赖
   - 停用插件
   - 删除文件
   - 清理数据库记录
   - 清除缓存
   - 记录审计日志

**验证检查**：
- [ ] 卸载流程完整
- [ ] 单元测试通过
- [ ] `cargo check` 编译通过

---

### 步骤 3.3：实现 ActivateService

**目标**：完整实现激活服务

**操作**：
1. 创建 `ActivateService` 结构体
2. 实现 `ActivateService::activate`
3. 实现 `ActivateService::deactivate`

**验证检查**：
- [ ] 激活/停用流程完整
- [ ] 单元测试通过
- [ ] `cargo check` 编译通过

---

### 步骤 3.4：实现 UpgradeService 和 DowngradeService

**目标**：完整实现升级和降级服务

**操作**：
1. 创建 `UpgradeService` 结构体
2. 实现 `UpgradeService::upgrade`
3. 创建 `DowngradeService` 结构体
4. 实现 `DowngradeService::downgrade`

**验证检查**：
- [ ] 升级流程完整
- [ ] 降级流程完整
- [ ] 单元测试通过
- [ ] `cargo check` 编译通过

---

### 步骤 3.5：实现 RollbackService

**目标**：完整实现回滚服务

**操作**：
1. 创建 `RollbackService` 结构体
2. 实现 `RollbackService::rollback`

**验证检查**：
- [ ] 回滚流程完整
- [ ] 单元测试通过
- [ ] `cargo check` 编译通过

---

### 第三阶段检查点

完成以上所有步骤后，进行以下检查：

**代码检查**：
- [ ] `cargo check` 无错误
- [ ] `cargo clippy` 无警告
- [ ] `cargo test` 所有测试通过

**功能检查**：
- [ ] 安装功能正常
- [ ] 卸载功能正常
- [ ] 激活/停用功能正常
- [ ] 升级/降级功能正常
- [ ] 回滚功能正常

---

## 第四阶段：集群模块实现

### 步骤 4.1：完善 NodeManager

**目标**：完善节点管理器

**操作**：
1. 从现有 `node.rs` 迁移代码
2. 添加分布式锁支持
3. 添加健康检查实现

**验证检查**：
- [ ] 节点管理完整
- [ ] 分布式锁集成
- [ ] `cargo check` 编译通过

---

### 步骤 4.2：完善 DeploymentCoordinator

**目标**：完善部署协调器

**操作**：
1. 从现有 `deployment.rs` 迁移代码
2. 添加多节点部署策略
3. 添加故障恢复机制

**验证检查**：
- [ ] 部署协调完整
- [ ] `cargo check` 编译通过

---

### 步骤 4.3：实现状态同步

**目标**：实现节点间状态同步

**操作**：
1. 创建 `cluster/sync.rs`
2. 实现 `SyncManager`

**验证检查**：
- [ ] 状态同步完整
- [ ] `cargo check` 编译通过

---

### 第四阶段检查点

完成以上所有步骤后，进行以下检查：

**代码检查**：
- [ ] `cargo check` 无错误
- [ ] `cargo clippy` 无警告
- [ ] `cargo test` 所有测试通过

**功能检查**：
- [ ] 节点管理正常
- [ ] 部署协调正常
- [ ] 状态同步正常

---

## 第五阶段：运行时模块实现

### 步骤 5.1：完善 ActivationManager

**目标**：完善 WASM 运行时管理

**操作**：
1. 从现有 `activation.rs` 迁移代码
2. 添加资源隔离
3. 添加错误处理

**验证检查**：
- [ ] 激活管理完整
- [ ] `cargo check` 编译通过

---

### 步骤 5.2：完善 ServiceRegistry

**目标**：完善服务注册表

**操作**：
1. 从现有 `service.rs` 迁移代码
2. 添加服务发现
3. 添加服务调用

**验证检查**：
- [ ] 服务注册表完整
- [ ] `cargo check` 编译通过

---

### 步骤 5.3：实现 FeatureManager

**目标**：实现功能管理器

**操作**：
1. 创建 `runtime/feature.rs`
2. 实现 `FeatureManager`：
   - 功能注册
   - 功能注销
   - 功能调用
   - 事件订阅
   - 事件发布

**验证检查**：
- [ ] 功能管理完整
- [ ] `cargo check` 编译通过

---

### 第五阶段检查点

完成以上所有步骤后，进行以下检查：

**代码检查**：
- [ ] `cargo check` 无错误
- [ ] `cargo clippy` 无警告
- [ ] `cargo test` 所有测试通过

**功能检查**：
- [ ] WASM 运行时正常
- [ ] 服务注册发现正常
- [ ] 功能管理正常

---

## 第六阶段：整合与清理

### 步骤 6.1：重构 PluginManager

**目标**：重构 PluginManager 使用新模块

**操作**：
1. 更新 `core/manager.rs`：
   - 使用新的服务层
   - 使用新的基础设施层
   - 保持 API 兼容

**验证检查**：
- [ ] PluginManager 重构完成
- [ ] API 保持兼容
- [ ] `cargo check` 编译通过

---

### 步骤 6.2：删除旧代码

**目标**：删除已迁移的旧代码

**操作**：
1. 删除旧的 `types.rs`（保留重导出）
2. 删除旧的 `version.rs`（保留重导出）
3. 删除旧的 `cache.rs`（保留重导出）
4. 删除旧的 `memory_cache.rs`（保留重导出）
5. 删除旧的 `db.rs` 和 `db_impl.rs`（保留重导出）
6. 删除其他已迁移的文件

**验证检查**：
- [ ] 旧代码已删除
- [ ] 重导出保持兼容
- [ ] `cargo check` 编译通过

---

### 步骤 6.3：更新文档

**目标**：更新所有文档

**操作**：
1. 更新模块文档
2. 更新 API 文档
3. 更新架构文档
4. 添加使用示例

**验证检查**：
- [ ] 文档完整
- [ ] 示例可运行

---

### 步骤 6.4：最终测试

**目标**：执行最终测试

**操作**：
1. 运行所有单元测试
2. 运行所有集成测试
3. 手动测试关键功能

**验证检查**：
- [ ] 所有测试通过
- [ ] 功能正常

---

## 最终验收

### 代码质量
- [ ] `cargo check` 无错误
- [ ] `cargo clippy` 无警告
- [ ] `cargo test` 所有测试通过
- [ ] `cargo doc` 文档生成成功

### 功能验收
- [ ] 插件安装流程完整可用
- [ ] 插件卸载流程完整可用
- [ ] 插件激活/停用流程完整可用
- [ ] 插件升级/降级流程完整可用
- [ ] 插件回滚流程完整可用
- [ ] 多节点部署功能可用
- [ ] 缓存功能正常工作
- [ ] 审计日志记录完整

### 架构验收
- [ ] 模块分包结构清晰
- [ ] 模块间依赖关系明确
- [ ] 依赖注入机制可用
- [ ] 错误处理机制完整

### 文档验收
- [ ] 模块文档完整
- [ ] API 文档完整
- [ ] 使用示例完整
- [ ] 架构文档更新
