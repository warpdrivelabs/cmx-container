# cmx-plugin 架构文档与实现差异分析

本文档对比架构设计文档与当前实现代码的差异，列出未完成或不完善的部分。

---

## 一、总体评估

| 类别 | 完成度 | 说明 |
|------|--------|------|
| 核心数据结构 | 90% | 主要结构已定义，部分字段缺失 |
| 插件生命周期 | 80% | 主要流程已实现，细节待完善 |
| 数据库操作 | 30% | 只有抽象接口，无实际实现 |
| 版本管理 | 70% | 基本功能完成，兼容性检查不完整 |
| 依赖解析 | 60% | 基本解析完成，算法可优化 |
| 部署协调 | 50% | 四种策略已实现，缺少分布式组件 |
| 激活管理 | 40% | WASM 加载框架完成，缺少服务注册 |
| 安全验证 | 50% | 基本验证完成，权限检查缺失 |
| 审计日志 | 70% | 日志记录完成，数据库持久化缺失 |
| 缓存管理 | 60% | Redis 缓存完成，内存缓存缺失 |
| 配置管理 | 40% | 基本配置完成，缺少 TOML 配置文件支持 |

---

## 二、详细差异分析

### 2.1 数据库架构 (差异度: 高)

#### 文档要求

文档定义了 **8 个数据库表**：

1. `cmx_plugin` - 插件注册主表
2. `cmx_plugin_versions` - 版本历史表
3. `cmx_plugin_dependencies` - 依赖关系表
4. `cmx_plugin_deployments` - 节点部署记录表
5. `cmx_plugin_audit_log` - 审计日志表
6. `cmx_plugin_rollback` - 回滚记录表
7. `cmx_system_plugins` - 系统默认插件配置表
8. `cmx_plugin_nodes` - 节点信息表

#### 当前实现

- ✅ 定义了 `PluginDbRecord` 等数据结构
- ✅ 定义了 `PluginDatabase` trait 接口
- ❌ **没有实际的表创建 DDL**
- ❌ **没有实现 `PluginDatabase` trait 的具体类**
- ❌ **没有集成 cmx-database 进行实际操作**

#### 建议改进

```rust
// 需要创建一个实现 PluginDatabase trait 的具体类
pub struct CmxPluginDatabase {
    db_manager: cmx_database::DatabaseManager,
}

#[async_trait]
impl PluginDatabase for CmxPluginDatabase {
    async fn insert_plugin(&self, record: &PluginDbRecord) -> Result<(), PluginDbError> {
        // 实际的数据库插入操作
    }
    // ... 其他方法实现
}
```

---

### 2.2 PluginManager 结构 (差异度: 中)

#### 文档要求

```rust
pub struct PluginManager {
    db_manager: Arc<DatabaseManager>,  // 直接持有 DatabaseManager
    default_db_id: String,
    registry: Arc<RwLock<PluginRegistry>>,
    version_manager: Arc<VersionManager>,
    dependency_resolver: Arc<DependencyResolver>,
    deployment_coordinator: Arc<DeploymentCoordinator>,
    activation_manager: Arc<ActivationManager>,
    audit_logger: Arc<AuditLogger>,
    security_validator: Arc<SecurityValidator>,
}
```

#### 当前实现

```rust
pub struct PluginManager {
    // 使用抽象接口而非具体实现
    db_service: Option<Arc<dyn PluginDatabase>>,  // 可选的，非必需
    cache_manager: Option<Arc<PluginCacheManager>>, // 新增
    // ... 其他字段
}
```

#### 差异点

| 字段 | 文档要求 | 当前实现 | 差异 |
|------|----------|----------|------|
| db_manager | 必需 | 可选 trait | 设计不同 |
| cache_manager | 无 | 新增 | 扩展 |
| repository | 无 | 新增 | 扩展 |
| transaction_manager | 无 | 新增 | 扩展 |

#### 建议改进

- 将 `db_service` 改为必需字段
- 添加默认的数据库实现

---

### 2.3 依赖解析 (差异度: 中)

#### 文档要求

- 完整的依赖图结构 (`DependencyGraph`)
- DFS 循环检测算法
- 版本约束解析 (Caret, Tilde, Range, Or, And)
- 最佳版本组合选择

#### 当前实现

- ✅ 基本的依赖解析
- ✅ 循环依赖检测
- ⚠️ 版本约束支持有限 (只支持基本约束)
- ❌ 缺少 `find_best_version组合` 方法
- ❌ 缺少依赖冲突解决策略

#### 建议改进

```rust
// 需要增强版本约束解析
pub enum VersionConstraint {
    Exact(String),
    Range { min: Option<String>, max: Option<String>, exclusive: bool },
    Caret(String),    // ^1.0.0
    Tilde(String),    // ~1.0.0
    Wildcard(WildcardPosition),  // 1.x
    Or(Vec<VersionConstraint>),  // 1.0.0 || 2.0.0
    And(Vec<VersionConstraint>), // >=1.0.0 <3.0.0
}
```

---

### 2.4 部署协调器 (差异度: 高)

#### 文档要求

```rust
pub struct DeploymentCoordinator {
    node_manager: Arc<NodeManager>,      // 节点管理器
    distributed_lock: Arc<DistributedLock>, // 分布式锁
    message_queue: Arc<MessageQueue>,    // 消息队列
}
```

#### 当前实现

```rust
pub struct DeploymentCoordinator {
    nodes: Arc<RwLock<Vec<NodeInfo>>>,   // 简单的内存列表
    lock_manager: Option<Arc<cmx_buffer::LockManager>>, // 可选的锁管理器
}
```

#### 差异点

| 功能 | 文档要求 | 当前实现 |
|------|----------|----------|
| 节点管理 | NodeManager | 内存 Vec |
| 分布式锁 | 必需 | 可选 |
| 消息队列 | MessageQueue | 无 |
| 健康检查 | 有 | 无 |
| 节点能力 | capabilities | 无 |

#### 建议改进

1. 实现 `NodeManager` 组件
2. 集成消息队列用于跨节点通信
3. 添加节点健康检查机制

---

### 2.5 激活管理器 (差异度: 高)

#### 文档要求

```rust
pub struct ActivationManager {
    wasm_runtime: Arc<WasmRuntime>,
    service_registry: Arc<ServiceRegistry>,  // 服务注册表
    resource_manager: Arc<ResourceManager>,  // 资源管理器
}
```

#### 当前实现

```rust
pub struct ActivationManager {
    wasm_runtime: Arc<WasmRuntime>,
    instances: Arc<RwLock<HashMap<String, PluginInstance>>>,
    handles: Arc<RwLock<HashMap<String, PluginHandle>>>,
}
```

#### 缺失功能

| 功能 | 说明 |
|------|------|
| ServiceRegistry | 服务注册/注销 |
| ResourceManager | 资源分配/释放 |
| 端口分配 | 网络端口管理 |
| 内存限制 | 内存配额管理 |
| 插件间通信 | 插件 API 调用 |

#### 建议改进

```rust
/// 服务注册表
pub struct ServiceRegistry {
    services: RwLock<HashMap<String, ServiceEntry>>,
}

/// 资源管理器
pub struct ResourceManager {
    memory_pool: MemoryPool,
    port_pool: PortPool,
    file_handles: FileHandlePool,
}
```

---

### 2.6 安全验证器 (差异度: 中)

#### 文档要求

```rust
pub struct SecurityValidator {
    trusted_keys: Arc<RwLock<HashSet<VerifyingKey>>>,
    permission_checker: Arc<PermissionChecker>,  // 权限检查器
    resource_isolator: Arc<ResourceIsolator>,    // 资源隔离器
}
```

#### 当前实现

```rust
pub struct SecurityValidator {
    config: SecurityValidatorConfig,
}
```

#### 缺失功能

| 功能 | 说明 |
|------|------|
| PermissionChecker | 权限检查 |
| ResourceIsolator | 资源隔离 |
| Ed25519 签名验证 | 实际签名验证逻辑 |

---

### 2.7 审计日志 (差异度: 中)

#### 文档要求

- 数据库持久化 (`cmx_plugin_audit_log` 表)
- 完整的操作详情 (old_value, new_value)
- 链路追踪 (request_id, correlation_id)
- 时间范围分区

#### 当前实现

- ✅ 日志输出
- ✅ 文件输出 (可选)
- ❌ 数据库持久化
- ❌ 链路追踪支持
- ❌ 分区策略

---

### 2.8 缓存管理 (差异度: 低)

#### 文档要求

- 内存缓存 + Redis 双层缓存
- 插件元数据缓存
- 版本信息缓存
- 依赖解析结果缓存

#### 当前实现

- ✅ Redis 缓存
- ❌ 内存缓存层
- ❌ 依赖解析缓存

---

### 2.9 系统插件初始化 (差异度: 高)

#### 文档要求

- `default_plugins.toml` 配置文件
- 必需插件/可选插件分类
- 安装顺序控制
- 重试机制
- 回退版本

#### 当前实现

- ⚠️ 有 `init_system_plugins` 方法框架
- ❌ 无 TOML 配置文件解析
- ❌ 无必需/可选分类
- ❌ 重试逻辑不完整

#### 建议改进

```toml
# default_plugins.toml
[settings]
install_root = "plugins/"
default_db_id = "default"

[[required]]
id = "cmx-core-tables"
version = "^1.0.0"
source = "plugins/packages/cmx-core-tables-1.0.0.zip"

[[optional]]
id = "cmx-reporting"
version = "^1.0.0"
source = "https://registry.example.com/plugins/reporting.zip"
```

---

### 2.10 事务管理 (差异度: 中)

#### 文档要求

- 完整的事务回滚机制
- 回滚操作列表
- 按逆序执行回滚

#### 当前实现

- ✅ TransactionManager 框架
- ✅ TransactionGuard RAII
- ⚠️ 未与实际数据库事务集成

---

## 三、优先级建议

### P0 - 必须完成

1. **数据库持久化实现** - 创建 `CmxPluginDatabase` 实现
2. **表结构 DDL** - 生成 8 个表的创建脚本
3. **系统插件初始化** - TOML 配置 + 启动流程

### P1 - 重要

4. **服务注册表** - ActivationManager 集成
5. **权限检查器** - SecurityValidator 完善
6. **依赖解析增强** - 完整的约束解析

### P2 - 可选

7. **节点管理器** - DeploymentCoordinator 增强
8. **消息队列集成** - 跨节点通信
9. **内存缓存层** - 性能优化

---

## 四、代码位置参考

| 模块 | 文件 | 需要修改 |
|------|------|----------|
| 数据库操作 | db.rs | 添加具体实现 |
| 插件管理 | manager.rs | 集成数据库操作 |
| 激活管理 | activation.rs | 添加服务注册 |
| 安全验证 | security.rs | 添加权限检查 |
| 部署协调 | deployment.rs | 添加节点管理 |
| 版本管理 | version.rs | 增强约束解析 |
| 配置管理 | 新增 config.rs | TOML 解析 |

---

## 五、下一步行动

1. 创建 `CmxPluginDatabase` 实现 `PluginDatabase` trait
2. 编写数据库表 DDL 脚本
3. 实现 TOML 配置文件解析
4. 完善 `init_system_plugins` 方法
5. 添加服务注册表组件
