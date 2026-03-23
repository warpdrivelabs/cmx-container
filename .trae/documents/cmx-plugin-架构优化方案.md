# cmx-plugin 模块架构优化与代码重构方案

## 一、现状分析

### 1.1 当前模块结构

当前 cmx-plugin 模块包含以下文件，全部位于单一目录下：

```
cmx-plugin/src/
├── lib.rs              # 入口文件
├── error.rs            # 错误类型定义
├── registry.rs         # 插件注册表
├── types.rs            # 类型定义
├── version.rs          # 版本管理
├── manager.rs          # 插件管理器
├── deployment.rs       # 部署协调器
├── activation.rs       # 激活管理器
├── audit.rs            # 审计日志
├── security.rs         # 安全验证器
├── repository.rs       # 插件仓库
├── transaction.rs      # 事务管理器
├── db.rs               # 数据库服务trait
├── db_impl.rs          # 数据库服务实现
├── config.rs           # 配置管理
├── cache.rs            # Redis缓存管理
├── service.rs          # 服务注册表
├── permission.rs       # 权限检查器
├── node.rs             # 节点管理器
├── message.rs          # 消息队列
├── memory_cache.rs     # 内存缓存
└── fetcher.rs          # 插件源获取器
```

### 1.2 存在的主要问题

#### 问题一：代码组织结构混乱
- 所有模块位于单一目录，缺乏合理的分包策略
- 模块间职责边界不清晰
- 难以快速定位相关功能代码

#### 问题二：功能模块间缺乏有效组合
- PluginManager 虽持有各组件引用，但协作机制不完善
- 缺乏统一的功能管理接口
- 组件初始化和依赖注入机制缺失

#### 问题三：插件初始化后功能管理系统缺失
- 缺少插件运行时状态管理
- 服务注册/发现机制不完善
- 插件间通信机制缺失

#### 问题四：数据库操作实现不完整
- db.rs 和 db_impl.rs 实现不完善
- 未完全按照架构文档使用 cmx-database 模式
- 缺少与 cmx-metadata 的集成

#### 问题五：缓存层整合不足
- memory_cache 和 cache（Redis）同时存在但缺乏统一策略
- 缺少多层缓存协调机制

#### 问题六：依赖模块利用不充分
- cmx-buffer（Redis缓存、分布式锁、消息订阅发布）利用不足
- cmx-database（数据库操作）集成不完整
- cmx-metadata（表元数据解析与SQL转换）未集成
- cmx-utils（zip解压、环境变量、配置文件解析）未充分利用

---

## 二、优化目标

### 2.1 架构目标
- **高内聚低耦合**：模块职责单一，依赖关系清晰
- **可维护性**：代码结构清晰，易于理解和修改
- **可扩展性**：支持功能扩展，不破坏现有结构
- **可测试性**：模块可独立测试

### 2.2 功能目标
- 完整实现架构文档定义的所有功能
- 建立清晰的模块间交互机制
- 实现插件初始化后的功能管理系统
- 充分利用已实现的依赖模块

---

## 三、新的模块结构设计

### 3.1 分包策略

```
cmx-plugin/src/
├── lib.rs                      # 入口文件，统一导出
├── error.rs                    # 统一错误类型定义
│
├── core/                       # 核心模块
│   ├── mod.rs
│   ├── manager.rs              # 插件管理器（核心协调器）
│   ├── registry.rs             # 插件注册表
│   ├── context.rs              # 插件上下文（运行时状态）
│   └── lifecycle.rs            # 生命周期管理
│
├── domain/                     # 领域模型
│   ├── mod.rs
│   ├── plugin.rs               # 插件定义与信息
│   ├── version.rs              # 版本管理
│   ├── dependency.rs           # 依赖关系
│   └── status.rs               # 状态定义
│
├── service/                    # 服务层
│   ├── mod.rs
│   ├── install.rs              # 安装服务
│   ├── uninstall.rs            # 卸载服务
│   ├── activate.rs             # 激活服务
│   ├── upgrade.rs              # 升级服务
│   ├── downgrade.rs            # 降级服务
│   └── rollback.rs             # 回滚服务
│
├── infrastructure/             # 基础设施层
│   ├── mod.rs
│   ├── database/               # 数据库操作
│   │   ├── mod.rs
│   │   ├── repository.rs       # 数据仓库
│   │   ├── schema.rs           # 表结构定义
│   │   └── migration.rs        # 表结构迁移
│   │
│   ├── cache/                  # 缓存层
│   │   ├── mod.rs
│   │   ├── memory.rs           # 内存缓存
│   │   ├── redis.rs            # Redis缓存
│   │   └── layered.rs          # 多层缓存协调
│   │
│   ├── storage/                # 存储层
│   │   ├── mod.rs
│   │   ├── file.rs             # 文件存储
│   │   └── backup.rs           # 备份管理
│   │
│   └── messaging/              # 消息层
│       ├── mod.rs
│       ├── queue.rs            # 消息队列
│       └── event.rs            # 事件发布
│
├── cluster/                    # 集群模块
│   ├── mod.rs
│   ├── node.rs                 # 节点管理
│   ├── deployment.rs           # 部署协调
│   └── sync.rs                 # 状态同步
│
├── security/                   # 安全模块
│   ├── mod.rs
│   ├── validator.rs            # 安全验证器
│   ├── signature.rs            # 签名验证
│   └── permission.rs           # 权限管理
│
├── runtime/                    # 运行时模块
│   ├── mod.rs
│   ├── activation.rs           # 激活管理
│   ├── service_registry.rs     # 服务注册表
│   └── feature.rs              # 功能管理
│
├── config/                     # 配置模块
│   ├── mod.rs
│   ├── settings.rs             # 配置设置
│   └── loader.rs               # 配置加载
│
├── audit/                      # 审计模块
│   ├── mod.rs
│   ├── logger.rs               # 审计日志
│   └── record.rs               # 审计记录
│
└── fetcher/                    # 获取器模块
    ├── mod.rs
    ├── source.rs               # 来源定义
    ├── local.rs                # 本地获取
    ├── remote.rs               # 远程获取
    └── registry.rs             # 注册表获取
```

### 3.2 模块职责说明

#### core 模块
- **manager.rs**: 插件管理器，作为核心协调器，协调各子模块完成生命周期操作
- **registry.rs**: 插件注册表，管理已加载插件的元数据
- **context.rs**: 插件上下文，管理插件运行时状态
- **lifecycle.rs**: 生命周期状态机，定义和转换插件状态

#### domain 模块
- **plugin.rs**: 插件定义、插件信息等核心数据结构
- **version.rs**: 语义版本、版本约束、版本比较
- **dependency.rs**: 依赖关系、依赖图、依赖解析
- **status.rs**: 插件状态定义、状态转换规则

#### service 模块
- **install.rs**: 安装服务，处理插件安装流程
- **uninstall.rs**: 卸载服务，处理插件卸载流程
- **activate.rs**: 激活服务，处理插件激活/停用
- **upgrade.rs**: 升级服务，处理插件升级
- **downgrade.rs**: 降级服务，处理插件降级
- **rollback.rs**: 回滚服务，处理操作回滚

#### infrastructure 模块
- **database**: 数据库操作，集成 cmx-database
- **cache**: 缓存层，整合内存缓存和 Redis
- **storage**: 文件存储和备份管理
- **messaging**: 消息队列和事件发布，集成 cmx-buffer

#### cluster 模块
- **node.rs**: 节点管理，管理集群节点
- **deployment.rs**: 部署协调，协调多节点部署
- **sync.rs**: 状态同步，同步插件状态到各节点

#### security 模块
- **validator.rs**: 安全验证器，验证插件安全性
- **signature.rs**: 签名验证，验证插件签名
- **permission.rs**: 权限管理，管理插件权限

#### runtime 模块
- **activation.rs**: 激活管理，管理 WASM 运行时
- **service_registry.rs**: 服务注册表，管理插件提供的服务
- **feature.rs**: 功能管理，管理插件初始化后的功能

#### config 模块
- **settings.rs**: 配置设置，定义配置结构
- **loader.rs**: 配置加载，加载和解析配置文件

#### audit 模块
- **logger.rs**: 审计日志，记录操作日志
- **record.rs**: 审计记录，定义审计记录结构

#### fetcher 模块
- **source.rs**: 来源定义，定义插件来源类型
- **local.rs**: 本地获取，从本地文件获取插件
- **remote.rs**: 远程获取，从 URL 获取插件
- **registry.rs**: 注册表获取，从插件注册表获取

---

## 四、核心设计

### 4.1 插件上下文 (PluginContext)

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
```

### 4.2 功能管理器 (FeatureManager)

```rust
/// 功能管理器 - 管理插件初始化后的功能
pub struct FeatureManager {
    /// 功能注册表
    features: Arc<RwLock<HashMap<String, Feature>>>,
    /// 服务注册表
    service_registry: Arc<ServiceRegistry>,
    /// 事件总线
    event_bus: Arc<EventBus>,
}

impl FeatureManager {
    /// 注册功能
    pub async fn register_feature(&self, feature: Feature) -> Result<(), PluginError>;
    
    /// 注销功能
    pub async fn unregister_feature(&self, feature_id: &str) -> Result<(), PluginError>;
    
    /// 获取功能
    pub async fn get_feature(&self, feature_id: &str) -> Option<Feature>;
    
    /// 调用功能
    pub async fn invoke_feature(&self, feature_id: &str, request: FeatureRequest) -> Result<FeatureResponse, PluginError>;
    
    /// 订阅事件
    pub async fn subscribe_event(&self, event_type: &str, handler: EventHandler) -> Result<(), PluginError>;
    
    /// 发布事件
    pub async fn publish_event(&self, event: Event) -> Result<(), PluginError>;
}
```

### 4.3 多层缓存协调器 (LayeredCacheManager)

```rust
/// 多层缓存协调器 - 整合内存缓存和 Redis
pub struct LayeredCacheManager {
    /// 内存缓存（L1）
    memory_cache: Arc<MemoryCache<CacheValue>>,
    /// Redis缓存（L2）
    redis_cache: Arc<CacheManager>,
    /// 缓存策略
    strategy: CacheStrategy,
}

impl LayeredCacheManager {
    /// 获取缓存（先查L1，再查L2）
    pub async fn get(&self, key: &str) -> Option<CacheValue>;
    
    /// 设置缓存（同时写入L1和L2）
    pub async fn set(&self, key: &str, value: CacheValue, ttl: Duration);
    
    /// 删除缓存（同时删除L1和L2）
    pub async fn delete(&self, key: &str);
    
    /// 刷新缓存（从L2同步到L1）
    pub async fn refresh(&self, key: &str);
}
```

### 4.4 数据库仓库 (PluginRepository)

```rust
/// 插件数据仓库 - 集成 cmx-database
pub struct PluginRepository {
    /// 数据库管理器
    db_manager: Arc<DatabaseManager>,
    /// 默认数据库ID
    default_db_id: String,
}

impl PluginRepository {
    /// 插入插件记录
    pub async fn insert_plugin(&self, record: &PluginDbRecord) -> Result<(), PluginError>;
    
    /// 更新插件记录
    pub async fn update_plugin(&self, plugin_id: &str, fields: &PluginUpdateFields) -> Result<(), PluginError>;
    
    /// 删除插件记录
    pub async fn delete_plugin(&self, plugin_id: &str) -> Result<(), PluginError>;
    
    /// 查询插件记录
    pub async fn find_plugin(&self, plugin_id: &str) -> Option<PluginDbRecord>;
    
    /// 列出所有插件
    pub async fn list_plugins(&self, filter: &PluginFilter) -> Vec<PluginDbRecord>;
    
    /// 在指定数据库创建插件表
    pub async fn create_plugin_tables(&self, plugin_def: &PluginDefinition, db_id: &str, base_path: &Path) -> Result<(), PluginError>;
}
```

---

## 五、实施步骤

### 第一阶段：基础重构（预计 3-4 天）

#### 步骤 1.1：创建新的目录结构
- 创建分包目录
- 移动现有文件到对应目录
- 更新 mod.rs 导出

#### 步骤 1.2：重构错误类型
- 整合所有错误类型到 error.rs
- 定义清晰的错误层次结构
- 添加错误转换实现

#### 步骤 1.3：重构领域模型
- 将 types.rs 拆分到 domain 模块
- 完善版本管理模块
- 完善依赖解析模块

#### 步骤 1.4：重构核心模块
- 重构 PluginRegistry
- 实现 PluginContext
- 实现 Lifecycle 状态机

### 第二阶段：基础设施层重构（预计 4-5 天）

#### 步骤 2.1：重构数据库层
- 完善 PluginRepository 实现
- 集成 cmx-database 模块
- 实现表结构创建和迁移
- 集成 cmx-metadata 创建插件表

#### 步骤 2.2：重构缓存层
- 整合内存缓存和 Redis 缓存
- 实现 LayeredCacheManager
- 定义缓存策略

#### 步骤 2.3：重构存储层
- 实现文件存储管理
- 实现备份管理
- 集成 cmx-utils 的 zip 功能

#### 步骤 2.4：重构消息层
- 完善消息队列实现
- 实现事件发布订阅
- 集成 cmx-buffer 的 PubSub

### 第三阶段：服务层重构（预计 4-5 天）

#### 步骤 3.1：重构安装服务
- 完善安装流程
- 集成依赖解析
- 集成安全验证
- 实现数据库表创建

#### 步骤 3.2：重构卸载服务
- 完善卸载流程
- 实现依赖检查
- 实现数据清理

#### 步骤 3.3：重构激活服务
- 完善激活/停用流程
- 集成 WASM 运行时
- 实现服务注册

#### 步骤 3.4：重构升级/降级服务
- 完善升级流程
- 完善降级流程
- 实现数据迁移

#### 步骤 3.5：重构回滚服务
- 完善回滚流程
- 实现备份恢复

### 第四阶段：集群模块重构（预计 2-3 天）

#### 步骤 4.1：重构节点管理
- 完善节点注册和发现
- 实现健康检查
- 集成 cmx-buffer 分布式锁

#### 步骤 4.2：重构部署协调
- 完善多节点部署
- 实现部署策略
- 实现故障恢复

#### 步骤 4.3：实现状态同步
- 实现节点间状态同步
- 集成消息队列

### 第五阶段：运行时模块重构（预计 2-3 天）

#### 步骤 5.1：重构激活管理
- 完善 WASM 运行时管理
- 实现资源隔离

#### 步骤 5.2：重构服务注册表
- 完善服务注册和发现
- 实现服务调用

#### 步骤 5.3：实现功能管理器
- 实现功能注册
- 实现事件总线
- 实现插件间通信

### 第六阶段：整合测试与文档（预计 2-3 天）

#### 步骤 6.1：单元测试
- 为各模块编写单元测试
- 确保测试覆盖率

#### 步骤 6.2：集成测试
- 编写集成测试
- 验证各模块协作

#### 步骤 6.3：文档更新
- 更新模块文档
- 添加使用示例
- 更新架构文档

---

## 六、数据库表结构优化

### 6.1 表结构修改建议

基于现有 SQL 文件，建议进行以下优化：

#### 6.1.1 cmx_plugin 表
- 添加 `context_data` 字段（JSONB），存储插件上下文数据
- 添加 `features` 字段（JSONB），存储插件功能定义
- 添加 `last_error` 字段（TEXT），存储最近错误信息

#### 6.1.2 新增 cmx_plugin_features 表
```sql
CREATE TABLE cmx_plugin_features (
    id                  VARCHAR(64) NOT NULL,
    plugin_id           VARCHAR(64) NOT NULL,
    feature_id          VARCHAR(255) NOT NULL,
    feature_name        VARCHAR(500) NOT NULL,
    feature_type        VARCHAR(50) NOT NULL,
    description         TEXT,
    config              JSONB,
    status              VARCHAR(30) NOT NULL DEFAULT 'active',
    create_time         TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    update_time         TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

COMMENT ON COLUMN cmx_plugin_features.id IS '主键ID';
COMMENT ON COLUMN cmx_plugin_features.plugin_id IS '关联插件ID';
COMMENT ON COLUMN cmx_plugin_features.feature_id IS '功能唯一标识';
COMMENT ON COLUMN cmx_plugin_features.feature_name IS '功能名称';
COMMENT ON COLUMN cmx_plugin_features.feature_type IS '功能类型: service, event_handler, scheduler, api';
COMMENT ON COLUMN cmx_plugin_features.description IS '功能描述';
COMMENT ON COLUMN cmx_plugin_features.config IS '功能配置';
COMMENT ON COLUMN cmx_plugin_features.status IS '状态: active, inactive, error';
```

#### 6.1.3 新增 cmx_plugin_events 表
```sql
CREATE TABLE cmx_plugin_events (
    id                  VARCHAR(64) NOT NULL,
    plugin_id           VARCHAR(64) NOT NULL,
    event_type          VARCHAR(100) NOT NULL,
    event_data          JSONB,
    processed           BOOLEAN NOT NULL DEFAULT FALSE,
    processed_at        TIMESTAMP WITH TIME ZONE,
    create_time         TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

COMMENT ON COLUMN cmx_plugin_events.id IS '主键ID';
COMMENT ON COLUMN cmx_plugin_events.plugin_id IS '关联插件ID';
COMMENT ON COLUMN cmx_plugin_events.event_type IS '事件类型';
COMMENT ON COLUMN cmx_plugin_events.event_data IS '事件数据';
COMMENT ON COLUMN cmx_plugin_events.processed IS '是否已处理';
COMMENT ON COLUMN cmx_plugin_events.processed_at IS '处理时间';
```

---

## 七、依赖模块集成

### 7.1 cmx-buffer 集成

```rust
use cmx_buffer::{
    CacheManager, CacheOps,        // Redis 缓存
    LockManager, LockGuard,         // 分布式锁
    PubSubOps, SharedSubscriber,    // 消息订阅发布
};

// 在 PluginManager 中使用
pub struct PluginManager {
    // ... 其他字段
    cache_manager: Option<Arc<CacheManager>>,
    lock_manager: Option<Arc<LockManager>>,
    pubsub: Option<Arc<PubSubOps>>,
}
```

### 7.2 cmx-database 集成

```rust
use cmx_database::{
    DatabaseManager,                 // 数据库管理器
    TransactionOptions,              // 事务选项
    Dbx,                            // 数据库连接
};

// 在 PluginRepository 中使用
pub struct PluginRepository {
    db_manager: Arc<DatabaseManager>,
    default_db_id: String,
}

impl PluginRepository {
    pub async fn execute_in_transaction<F, T>(&self, db_id: &str, f: F) -> Result<T, PluginError>
    where
        F: FnOnce(&Dbx) -> futures::future::BoxFuture<'_, Result<T, PluginError>>,
    {
        let txn_id = self.db_manager.begin_transaction(
            db_id,
            TransactionOptions::default(),
        ).await?;
        
        // 执行操作...
    }
}
```

### 7.3 cmx-metadata 集成

```rust
use cmx_metadata::{
    config::TableDefinesConfigManager,
    executor::MetadataExecutor,
};

// 在创建插件表时使用
pub async fn create_plugin_tables(
    plugin_def: &PluginDefinition,
    db_id: &str,
    base_path: &Path,
) -> Result<(), PluginError> {
    let config_paths: Vec<PathBuf> = plugin_def
        .table_config_files
        .iter()
        .map(|f| base_path.join(f))
        .collect();
    
    let config_manager = TableDefinesConfigManager::from_config_paths(&config_paths)?;
    
    // 按依赖顺序获取表配置
    let sorted_configs = config_manager.sorted_configs();
    
    for table_config in sorted_configs {
        // 创建表...
    }
}
```

### 7.4 cmx-utils 集成

```rust
use cmx_utils::{
    ZipExtractor,                    // ZIP 解压
    Config, ConfigBuilder,           // 配置管理
};

// 在插件安装时使用
pub async fn extract_plugin_zip(zip_path: &Path, dest_dir: &Path) -> Result<(), PluginError> {
    let extractor = ZipExtractor::new()?;
    extractor.extract(zip_path, dest_dir, true).await?;
    Ok(())
}
```

---

## 八、验收标准

### 8.1 功能验收
- [ ] 插件安装流程完整可用
- [ ] 插件卸载流程完整可用
- [ ] 插件激活/停用流程完整可用
- [ ] 插件升级/降级流程完整可用
- [ ] 插件回滚流程完整可用
- [ ] 多节点部署功能可用
- [ ] 缓存功能正常工作
- [ ] 审计日志记录完整

### 8.2 架构验收
- [ ] 模块分包结构清晰
- [ ] 模块间依赖关系明确
- [ ] 依赖注入机制可用
- [ ] 错误处理机制完整
- [ ] 测试覆盖率达标

### 8.3 文档验收
- [ ] 模块文档完整
- [ ] API 文档完整
- [ ] 使用示例完整
- [ ] 架构文档更新

---

## 九、风险与应对

### 9.1 风险识别
1. **重构过程中可能影响现有功能**
   - 应对：分阶段实施，每阶段完成后进行测试

2. **模块间依赖关系复杂**
   - 应对：先定义清晰的接口，再逐步实现

3. **与依赖模块的集成可能遇到问题**
   - 应对：先阅读依赖模块源码，理解其使用方式

### 9.2 回滚策略
- 每个阶段完成后创建 Git 标签
- 保留原有代码备份
- 问题严重时可回滚到上一阶段

---

## 十、总结

本优化方案从代码组织结构、模块间交互机制、功能管理系统、数据库操作、缓存层整合等方面对 cmx-plugin 模块进行全面优化。通过分阶段实施，确保每个阶段的成果可验证、可回滚，最终实现高内聚低耦合、可维护、可扩展、可测试的插件管理模块。
