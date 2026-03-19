# cmx-database 模块重构设计方案

## 文档信息

- **版本**: v2.1.0
- **日期**: 2026-03-10
- **状态**: 设计方案（评审优化后）
- **目标**: 解决现有架构缺陷，提供可测试、可维护、高性能的数据库访问层

---

## 目录

1. [需求分析](#1-需求分析)
3. [设计目标与原则](#3-设计目标与原则)
4. [整体架构设计](#4-整体架构设计)
5. [核心模块详细设计](#5-核心模块详细设计)
6. [接口设计](#6-接口设计)
7. [错误处理设计](#7-错误处理设计)
8. [监控与可观测性](#8-监控与可观测性)
9. [测试策略](#9-测试策略)
10. [迁移指南](#10-迁移指南)
11. [分阶段实施路径](#11-分阶段实施路径)

---

## 1. 需求分析

### 1.1 功能需求

#### 1.1.1 多数据源支持
- 支持 PostgreSQL、MySQL、SQLite 三种数据库
- 支持动态注册、更新、注销数据源
- 每个数据源独立配置连接池参数
- 支持数据源健康检查和自动故障恢复

#### 1.1.2 事务管理
- 支持声明式事务和编程式事务
- 支持事务超时检测和自动回滚
- 支持只读事务优化

#### 1.1.3 WebAssembly 兼容
- WASM 端不持有数据库连接资源
- 通过 ID（db_id、txn_id）经由 wasmtime/wasmer host function 进行远程操作
- 支持异步操作
- 序列化参数和结果传输
- 保持现有 `*_by_ids` 函数接口模式

#### 1.1.4 CRUD 操作
- 通用的 CRUD 接口，与数据库类型无关
- 支持条件查询、分页查询
- 支持通过数据库 id 参数自动获取数据库连接池来使用
- 支持事务 id 参数
- 支持批量操作
- 查询结果统一转换为 DataSet 格式

### 1.2 非功能需求

#### 1.2.1 性能需求
- 连接池获取连接时间 < 10ms（P99）
- 事务开启时间 < 5ms
- 支持连接池预热

#### 1.2.2 可靠性需求
- 连接池耗尽时优雅降级
- 数据库故障时自动重连
- 事务异常时自动回滚
- 资源泄漏防护

#### 1.2.3 可维护性需求
- 代码覆盖率 > 80%
- 支持单元测试和集成测试
- 清晰的模块边界
- 完善的文档和示例

---



## 3. 设计目标与原则

### 3.1 设计目标

1. **最小化全局状态**: 优先使用依赖注入管理状态，支持多实例。WASM 场景需要事务注册表作为必要的实例级状态
2. **类型安全**: 本地使用场景通过类型系统保证事务状态正确性；WASM 远程调用路径通过运行时检查保证
3. **资源安全**: RAII 模式管理资源，防止泄漏
4. **合理抽象开销**: 在合理范围内最小化抽象开销。数据库 IO 是毫秒级瓶颈，vtable 查找等纳秒级开销可忽略
5. **可测试**: 支持 Mock 和 Stub，便于单元测试

### 3.2 设计原则

#### 3.2.1 单一职责原则 (SRP)
- 连接池管理：只负责连接的创建、复用、回收
- 事务管理：只负责事务的生命周期和传播
- 查询执行：只负责 SQL 的执行和结果转换

#### 3.2.2 依赖倒置原则 (DIP)
- 高层模块依赖抽象接口
- 具体实现通过依赖注入提供
- 便于切换实现和测试

#### 3.2.3 接口隔离原则 (ISP)
- 细粒度的 trait 定义
- 客户端只依赖需要的接口
- 避免胖接口

---

## 4. 整体架构设计

### 4.1 模块划分

#### 4.1.1 当前实际结构

```
cmx-database
├── Cargo.toml
├── src/
│   ├── lib.rs                    # 模块导出和 pub use
│   ├── error.rs                  # 扁平 Error 枚举
│   │
│   ├── config/                   # 配置模块
│   │   └── mod.rs                # DbType, PoolConfig, DbConfig
│   │
│   ├── connection/               # 连接池管理模块
│   │   └── mod.rs                # DbPool, DatabasePoolImpl, DbRegistry, 全局注册函数
│   │
│   ├── transaction/              # 事务管理模块
│   │   ├── mod.rs                # 模块导出 + transaction! 宏
│   │   ├── core.rs               # Dbx, DbTransaction, TxnHolder, Propagation, IsolationLevel
│   │   ├── api.rs                # WASM 兼容 API (*_by_ids 函数)
│   │   ├── context.rs            # TransactionFrame, TransactionContextStack (未使用)
│   │   ├── conversion.rs         # TransactionConverter trait (与 executor 重复)
│   │   ├── metadata.rs           # TransactionMetadata, TransactionStatus, 全局注册表
│   │   └── registry.rs           # 全局 TxnHolder 注册表
│   │
│   ├── executor/                 # 结果转换模块
│   │   └── mod.rs                # ParamValue, ResultConverter
│   │
│   ├── manager/                  # 数据库管理器模块
│   │   └── mod.rs                # DatabaseManager, PoolManager, TransactionContext
│   │
│   ├── monitoring/               # 监控模块
│   │   └── mod.rs                # 健康检查 + 事务超时监控
│   │
│   └── types/                    # 类型安全查询构建器
│       └── mod.rs                # QueryBuilder, TypedRow, TypedResult (部分实现)
│
└── tests/
    └── integration_test.rs       # 集成测试 (仅 PostgreSQL)
```

#### 4.1.2 目标结构（分阶段演进）

```
cmx-database
├── src/
│   ├── lib.rs                    # 模块导出
│   ├── error.rs                  # 层次化错误类型 (Phase 2)
│   │
│   ├── pool/                     # 连接池管理模块 (Phase 2: 从 connection/ + config/ 重组)
│   │   ├── mod.rs
│   │   ├── manager.rs            # 连接池管理器
│   │   ├── config.rs             # 连接池配置
│   │   ├── health.rs             # 健康检查
│   │   └── inner.rs              # 内部连接池封装
│   │
│   ├── transaction/              # 事务管理模块 (Phase 3: 重构)
│   │   ├── mod.rs
│   │   ├── manager.rs            # 事务管理器
│   │   ├── context.rs            # 事务上下文
│   │   ├── propagation.rs        # 传播行为实现
│   │   ├── options.rs            # 事务选项
│   │   └── handle.rs             # 事务句柄
│   │
│   ├── executor/                 # 查询执行模块 (Phase 4)
│   │   ├── mod.rs
│   │   ├── trait.rs              # QueryExecutor trait
│   │   ├── sqlx_impl.rs          # SQLx 实现
│   │   ├── converter.rs          # 统一结果转换
│   │   └── params.rs             # 统一参数绑定
│   │
│   ├── repository/               # 仓库模块 (Phase 4)
│   │   ├── mod.rs                # CrudRepository trait
│   │   └── generic.rs            # 通用实现（内部按 db_type 分发）
│   │
│   ├── wasm_api/                 # WebAssembly Host Function API (Phase 2)
│   │   └── mod.rs                # 保持现有 *_by_ids 接口模式
│   │
│   └── monitoring/               # 监控模块
│       └── mod.rs                # 健康检查 + tracing 日志
│
└── tests/                        # 测试目录
    ├── unit/                     # 单元测试
    └── integration/              # 集成测试
```

### 4.2 核心架构图

```
┌─────────────────────────────────────────────────────────────────┐
│                        Application Layer                         │
│  ┌─────────────┐  ┌─────────────────┐  ┌─────────────────────┐  │
│  │  Repository │  │  WASM Host Fn   │  │ Transactional Svc   │  │
│  └──────┬──────┘  └───────┬─────────┘  └──────────┬──────────┘  │
└─────────┼─────────────────┼────────────────────────┼────────────┘
          │                 │                        │
          ▼                 ▼                        ▼
┌─────────────────────────────────────────────────────────────────┐
│                      cmx-database Layer                          │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │                 DatabaseManager (入口)                     │  │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────────┐   │  │
│  │  │PoolManager  │  │TxnManager   │  │QueryExecutor    │   │  │
│  │  │             │  │             │  │                 │   │  │
│  │  │- create_pool│  │- begin_txn  │  │- execute_sql    │   │  │
│  │  │- get_pool   │  │- commit     │  │- query          │   │  │
│  │  │- health     │  │- rollback   │  │- batch_execute  │   │  │
│  │  └─────────────┘  └──────┬──────┘  └─────────────────┘   │  │
│  │                          │                                │  │
│  │                   ┌──────┴──────┐                        │  │
│  │                   │Transaction  │                        │  │
│  │                   │Context      │                        │  │
│  │                   │(事务栈)      │                        │  │
│  │                   └─────────────┘                        │  │
│  └───────────────────────────────────────────────────────────┘  │
│                                                                 │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │  WASM API（保持现有 *_by_ids 接口）                        │  │
│  │  通过 txn_id 字符串操作事务注册表                           │  │
│  └───────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
          │                │                     │
          ▼                ▼                     ▼
┌─────────────────────────────────────────────────────────────────┐
│                      sqlx Layer                                  │
│         ┌──────────┐  ┌──────────┐  ┌──────────┐               │
│         │ Postgres │  │  MySQL   │  │  SQLite  │               │
│         │  Pool    │  │  Pool    │  │  Pool    │               │
│         └──────────┘  └──────────┘  └──────────┘               │
└─────────────────────────────────────────────────────────────────┘
```

### 4.3 关键设计模式

#### 4.3.1 类型状态模式 (Type State Pattern)

使用类型系统区分不同状态，编译期保证正确性：

```rust
// 非事务状态
pub struct Dbx<State = NoTransaction> {
    pool: PoolRef,
    state: PhantomData<State>,
}

// 事务状态
pub struct InTransaction {
    txn_handle: TransactionHandle,
}

// 状态转换
impl Dbx<NoTransaction> {
    pub async fn begin_txn(self) -> Result<Dbx<InTransaction>> {
        // ...
    }
}

impl Dbx<InTransaction> {
    pub async fn commit(self) -> Result<Dbx<NoTransaction>> {
        // ...
    }
}
```

> **适用范围说明**: 类型状态模式仅适用于本地直接使用 Dbx 的场景，提供编译期事务状态安全。对于 WASM 远程调用路径（通过 `txn_id` 字符串操作事务），仍然使用运行时检查方案（`TransactionHandle` + 事务注册表）。两条路径并存，互不影响。

#### 4.3.2 资源池模式 (Pool Pattern)

```rust
pub struct PoolManager {
    pools: RwLock<HashMap<String, ManagedPool>>,
}

struct ManagedPool {
    pool: DbPool,
    config: PoolConfig,
    health_checker: HealthChecker,
    shutdown_tx: oneshot::Sender<()>,
}
```

#### 4.3.3 工作单元模式 (Unit of Work)

```rust
pub struct TransactionContext {
    stack: Vec<TransactionFrame>,
    options: TransactionOptions,
}

struct TransactionFrame {
    handle: TransactionHandle,
    propagation: Propagation,
    // 未来增强 (P3): savepoint: Option<String>,
}
```

---

## 5. 核心模块详细设计

### 5.1 连接池管理模块 (pool)

#### 5.1.1 职责
- 管理多个数据库连接池的生命周期
- 提供健康检查和故障恢复
- 支持动态添加、更新、删除数据源
- 优雅关闭和资源回收

#### 5.1.2 核心结构

```rust
/// 连接池管理器
pub struct PoolManager {
    /// 连接池集合
    pools: RwLock<HashMap<String, PoolEntry>>,
    /// 全局配置
    global_config: GlobalPoolConfig,
    /// 事件发送器
    event_tx: mpsc::Sender<PoolEvent>,
}

/// 连接池条目
struct PoolEntry {
    /// 连接池实例
    pool: DbPool,
    /// 配置信息
    config: PoolConfig,
    /// 创建时间
    create_time: Instant,
    /// 关闭信号发送器
    shutdown_tx: Option<oneshot::Sender<()>>,
    /// 健康状态
    health: AtomicU8, // 0=健康, 1=降级, 2=故障
}

/// 数据库连接池枚举
pub enum DbPool {
    Postgres(Pool<Postgres>),
    MySql(Pool<MySql>),
    Sqlite(Pool<Sqlite>),
}

/// 连接池配置
#[derive(Clone, Debug)]
pub struct PoolConfig {
    pub db_type: DbType,
    pub connection_string: String,
    pub max_connections: u32,
    pub min_connections: u32,
    pub acquire_timeout: Duration,
    pub idle_timeout: Duration,
    pub max_lifetime: Duration,
    pub health_check_interval: Duration,
    pub health_check_timeout: Duration,
    pub warmup_on_start: bool,
}
```

#### 5.1.3 核心接口

```rust
impl PoolManager {
    /// 创建新的连接池管理器
    pub fn new(global_config: GlobalPoolConfig) -> Self;

    /// 注册新的数据源
    pub async fn register(&self, db_id: &str, config: PoolConfig) -> Result<()>;

    /// 更新数据源配置（优雅替换）
    pub async fn update(&self, db_id: &str, config: PoolConfig) -> Result<()>;

    /// 注销数据源（优雅关闭）
    pub async fn unregister(&self, db_id: &str, timeout: Duration) -> Result<()>;

    /// 获取连接池
    pub fn get_pool(&self, db_id: &str) -> Result<DbPool>;

    /// 检查数据源健康状态
    pub async fn health_check(&self, db_id: &str) -> Result<HealthStatus>;

    /// 获取所有数据源状态
    pub fn list_pools(&self) -> Vec<PoolInfo>;

    /// 优雅关闭所有连接池
    pub async fn shutdown_all(&self, timeout: Duration) -> Result<()>;
}
```

#### 5.1.4 优雅关闭机制

```rust
/// 优雅关闭流程
async fn graceful_shutdown(pool_entry: PoolEntry, timeout: Duration) -> Result<()> {
    // 1. 发送关闭信号，阻止新连接获取
    if let Some(tx) = pool_entry.shutdown_tx {
        let _ = tx.send(());
    }

    // 2. 等待活跃连接释放（带超时）
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        let active = pool_entry.pool.size() - pool_entry.pool.idle();
        if active == 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    // 3. 强制关闭连接池
    pool_entry.pool.close().await;
    Ok(())
}
```

#### 5.1.5 健康检查机制

```rust
/// 健康检查器
pub struct HealthChecker {
    interval: Duration,
    timeout: Duration,
    failure_threshold: u32,
}

impl HealthChecker {
    pub async fn check(&self, pool: &DbPool) -> HealthStatus {
        let start = Instant::now();

        let result = match pool {
            DbPool::Postgres(p) => self.check_postgres(p).await,
            DbPool::MySql(p) => self.check_mysql(p).await,
            DbPool::Sqlite(p) => self.check_sqlite(p).await,
        };

        HealthStatus {
            healthy: result.is_ok(),
            latency: start.elapsed(),
            last_error: result.err(),
            checked_at: Instant::now(),
        }
    }
}
```

### 5.2 事务管理模块 (transaction)

#### 5.2.1 职责
- 管理事务的生命周期
- 事务超时检测和自动回滚

#### 5.2.2 核心结构

```rust
/// 事务管理器
pub struct TransactionManager {
    pool_manager: Arc<PoolManager>,
    config: TransactionConfig,
}

/// 事务上下文
///
/// 注意：通过 `tokio::task_local!` 或参数显式传递，
/// 不使用 thread_local!（async 任务可能跨线程调度）。
pub struct TransactionContext {
    /// 事务栈（仅支持单数据源嵌套）
    stack: Vec<TransactionFrame>,
    /// 默认事务选项
    default_options: TransactionOptions,
    /// 创建时间
    create_time: Instant,
}

/// 事务帧
struct TransactionFrame {
    /// 事务句柄
    handle: TransactionHandle,
    /// 传播行为
    propagation: Propagation,
    /// 隔离级别
    isolation: IsolationLevel,
    /// 是否只读
    read_only: bool,
    // 未来增强 (P3): savepoint: Option<String>,
}

/// 事务句柄
pub struct TransactionHandle {
    /// 事务ID
    id: Uuid,
    /// 数据库ID
    db_id: String,
    /// 底层事务
    inner: DbTransaction,
    /// 状态
    status: AtomicU8,
    /// 创建时间
    create_time: Instant,
    /// 超时时间
    timeout: Option<Duration>,
}

/// 事务选项
#[derive(Clone, Debug)]
pub struct TransactionOptions {
    pub propagation: Propagation,
    pub isolation: IsolationLevel,
    pub read_only: bool,
    pub timeout: Option<Duration>,
}

/// 传播行为
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Propagation {
    /// 如果存在事务则加入，否则创建新事务（默认）
    Required,
    /// 创建新事务，挂起当前事务
    RequiresNew,
    /// 如果存在事务则加入，否则非事务执行
    Supports,
    /// 非事务执行，挂起当前事务
    NotSupported,
    /// 必须在事务中执行，否则报错
    Mandatory,
    /// 必须非事务执行，否则报错
    Never,
}

/// 隔离级别
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IsolationLevel {
    ReadUncommitted,
    ReadCommitted,
    RepeatableRead,
    Serializable,
}
```

#### 5.2.3 事务栈实现

> **设计约束**: 事务栈仅支持单数据源嵌套。不支持跨数据源分布式事务（XA/Saga/TCC 等需要独立的协调机制）。

```rust
impl TransactionContext {
    /// 开始新事务或加入现有事务
    pub async fn begin(&mut self, db_id: &str, options: TransactionOptions) -> Result<TransactionRef> {
        match options.propagation {
            Propagation::Required => self.handle_required(db_id, options).await,
            Propagation::RequiresNew => self.handle_requires_new(db_id, options).await,
            Propagation::Supports => self.handle_supports(db_id, options).await,
            Propagation::NotSupported => self.handle_not_supported(db_id, options).await,
            Propagation::Mandatory => self.handle_mandatory(db_id, options).await,
            Propagation::Never => self.handle_never(db_id, options).await,
        }
    }

    /// Required: 存在则加入，否则创建
    async fn handle_required(&mut self, db_id: &str, options: TransactionOptions) -> Result<TransactionRef> {
        if let Some(top) = self.stack.last() {
            if top.handle.db_id == db_id {
                // 加入现有事务，增加引用计数
                return Ok(TransactionRef::join(&top.handle));
            }
            // 不同 db_id: 不支持跨数据源事务
            return Err(Error::CrossDatabaseTransaction {
                current: top.handle.db_id.clone(),
                requested: db_id.to_string(),
            });
        }
        // 创建新事务
        self.create_transaction(db_id, options).await
    }

    /// RequiresNew: 创建新事务，挂起当前
    async fn handle_requires_new(&mut self, db_id: &str, options: TransactionOptions) -> Result<TransactionRef> {
        // 保存当前事务栈状态
        let suspended = self.suspend_current();

        // 创建新事务
        let txn = self.create_transaction(db_id, options).await?;

        // 记录挂起状态
        txn.mark_suspended_parent(suspended);

        Ok(txn)
    }

    /// 提交事务
    pub async fn commit(&mut self, txn_ref: TransactionRef) -> Result<()> {
        let frame = self.find_frame(&txn_ref)?;

        if frame.propagation == Propagation::RequiresNew {
            // 恢复挂起的事务
            if let Some(suspended) = txn_ref.take_suspended_parent() {
                self.resume(suspended);
            }
        }

        // 执行提交
        frame.handle.commit().await?;
        self.remove_frame(&txn_ref);

        Ok(())
    }

    /// 回滚事务
    pub async fn rollback(&mut self, txn_ref: TransactionRef) -> Result<()> {
        let frame = self.find_frame(&txn_ref)?;

        match frame.propagation {
            Propagation::RequiresNew => {
                frame.handle.rollback().await?;
            }
            _ => {
                // 标记为回滚-only
                frame.handle.mark_rollback_only();
            }
        }

        Ok(())
    }
}
```

#### 5.2.4 声明式事务宏

```rust
/// 声明式事务宏
#[macro_export]
macro_rules! transactional {
    ($manager:expr, $db_id:expr, $options:expr, $body:block) => {{
        let ctx = $manager.get_context();
        let txn = ctx.begin($db_id, $options).await?;

        let result = async { $body }.await;

        match result {
            Ok(val) => {
                ctx.commit(txn).await?;
                Ok(val)
            }
            Err(e) => {
                ctx.rollback(txn).await.ok();
                Err(e)
            }
        }
    }};

    // 简化版本，使用默认选项
    ($manager:expr, $db_id:expr, $body:block) => {
        transactional!($manager, $db_id, TransactionOptions::default(), $body)
    };
}
```

#### 5.2.5 事务超时监控

```rust
/// 事务监控器
pub struct TransactionMonitor {
    /// 活跃事务集合（通过 txn_id 索引，而非 db_id）
    active_txns: Arc<RwLock<HashMap<Uuid, Weak<TransactionHandle>>>>,
    /// 检查间隔
    check_interval: Duration,
}

impl TransactionMonitor {
    pub async fn start_monitoring(&self) {
        loop {
            tokio::time::sleep(self.check_interval).await;

            let expired = self.check_timeouts().await;
            for txn_id in expired {
                // 直接通过事务句柄回滚，而非通过 db_id 获取新 Dbx
                if let Some(txn) = self.get_transaction(&txn_id).await {
                    tracing::warn!(%txn_id, "事务超时，自动回滚");
                    let _ = txn.rollback().await;
                }
            }
        }
    }

    async fn check_timeouts(&self) -> Vec<Uuid> {
        let mut expired = Vec::new();
        let txns = self.active_txns.read().await;

        for (id, weak) in txns.iter() {
            if let Some(txn) = weak.upgrade() {
                if let Some(timeout) = txn.timeout {
                    if txn.create_time.elapsed() > timeout {
                        expired.push(*id);
                    }
                }
            }
        }

        expired
    }
}
```

### 5.3 查询执行模块 (executor)

#### 5.3.1 职责
- 执行 SQL 查询和更新
- 参数绑定和类型转换
- 结果集转换为 DataSet
- 批量操作优化

#### 5.3.2 核心结构

```rust
/// 查询执行器 trait
///
/// 使用原生 async fn in trait（Rust 2024 edition），无需 async-trait 依赖。
/// 关于 Send 约束：见 §5.5 并发模型说明。
pub trait QueryExecutor: Send + Sync {
    /// 执行更新语句
    fn execute(&self, sql: &str, params: &[ParamValue]) -> impl Future<Output = Result<u64>> + Send;

    /// 执行查询语句
    fn query(&self, sql: &str, params: &[ParamValue], dataset_id: &str) -> impl Future<Output = Result<DataSet>> + Send;

    /// 批量执行
    fn batch_execute(&self, sql: &str, params_batch: &[Vec<ParamValue>]) -> impl Future<Output = Result<Vec<u64>>> + Send;
}

/// SQLx 执行器实现
pub struct SqlxExecutor {
    pool: DbPool,
    converter: ResultConverter,
}

/// 参数值
#[derive(Clone, Debug)]
pub enum ParamValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Bytes(Vec<u8>),
    Decimal(Decimal),
    DateTime(chrono::NaiveDateTime),
    Date(chrono::NaiveDate),
    Json(serde_json::Value),
}

/// 结果转换器
pub struct ResultConverter {
    type_mappings: HashMap<String, FieldType>,
}
```

#### 5.3.3 参数绑定

```rust
/// 参数绑定 trait
pub trait BindParams<'q> {
    fn bind_params(self, params: &[ParamValue]) -> Self;
}

impl<'q> BindParams<'q> for Query<'q, Postgres, PgArguments> {
    fn bind_params(mut self, params: &[ParamValue]) -> Self {
        for param in params {
            self = match param {
                ParamValue::Null => self.bind(None::<String>),
                ParamValue::Bool(v) => self.bind(*v),
                ParamValue::Int(v) => self.bind(*v),
                ParamValue::Float(v) => self.bind(*v),
                ParamValue::String(v) => self.bind(v),
                ParamValue::Decimal(v) => self.bind(*v),
                ParamValue::DateTime(v) => self.bind(*v),
                _ => self.bind(param.to_string()),
            };
        }
        self
    }
}
```

#### 5.3.4 结果转换

```rust
impl ResultConverter {
    /// 将 SQLx 行转换为 DataSet
    pub fn convert_rows<R: Row>(&self, rows: Vec<R>, dataset_id: &str) -> Result<DataSet> {
        if rows.is_empty() {
            return Ok(DataSet::empty(dataset_id));
        }

        // 构建 Schema
        let first_row = &rows[0];
        let columns = first_row.columns();
        let mut fields = Vec::with_capacity(columns.len());

        for col in columns {
            fields.push(Field {
                name: col.name().to_string(),
                field_type: self.map_type(col.type_info()),
                label: String::new(),
            });
        }

        let schema = Arc::new(Schema::new(dataset_id.to_string(), fields));
        let mut dataset = DataSet::with_capacity(dataset_id, schema, rows.len());

        // 转换数据行
        for row in rows {
            let mut values = Vec::with_capacity(columns.len());
            for i in 0..columns.len() {
                values.push(self.extract_value(&row, i)?);
            }
            dataset.add_row(Row::new(values));
        }

        Ok(dataset)
    }
}
```

### 5.4 WebAssembly API 模块 (wasm_api)

WASM 通信通过 wasmtime/wasmer 的 host function 机制实现，保持现有 `*_by_ids` 接口模式：

```rust
/// Host 端暴露给 WASM 的函数接口
///
/// WASM 端不持有数据库资源，所有操作通过 db_id/txn_id 字符串委托给 Host
pub async fn execute_sql_by_ids(db_id: &str, txn_id: Option<&str>, sql: &str) -> Result<u64>;
pub async fn execute_sql_with_params_by_ids(db_id: &str, txn_id: Option<&str>, sql: &str, params: serde_json::Value) -> Result<u64>;
pub async fn query_sql_by_ids(db_id: &str, txn_id: Option<&str>, sql: &str, dataset_id: &str) -> Result<DataSet>;
pub async fn query_sql_with_params_by_ids(db_id: &str, txn_id: Option<&str>, sql: &str, params: serde_json::Value, dataset_id: &str) -> Result<DataSet>;
pub async fn commit_txn_by_id(txn_id: &str) -> Result<()>;
pub async fn rollback_txn_by_id(txn_id: &str) -> Result<()>;
pub fn get_dbx_by_db_id(db_id: &str) -> Option<Dbx>;
pub async fn with_transaction_by_id<T, F, Fut>(txn_id: &str, f: F) -> Result<T>
    where F: FnOnce(&mut DbTransaction) -> Fut + Send,
          Fut: Future<Output = Result<T>> + Send;
```

> **事务注册表**: WASM 路径需要通过 `txn_id` 字符串查找事务句柄，因此必须维护一个实例级事务注册表（`HashMap<String, Arc<Mutex<Option<TxnHolder>>>>`）。这是 WASM 架构的固有约束，与"最小化全局状态"目标不矛盾——注册表是 `DatabaseManager` 的实例字段，而非进程级 static。

### 5.5 并发模型与 Send 约束

#### 5.5.1 问题背景

当前代码中，`with_transaction_by_id` 函数持有 `MutexGuard` 跨 await 调用闭包：

```rust
pub async fn with_transaction_by_id<T, F>(txn_id: &str, f: F) -> Result<T> {
    let holder = get_txn_holder_by_id(txn_id)?;
    let mut guard = holder.lock().unwrap();  // MutexGuard 在此获取
    let txn = guard.as_mut().unwrap();
    f(txn).await  // 跨 await 持有 guard
}
```

`std::sync::MutexGuard` 不是 `Send`，因此整个 future 非 Send，无法在 tokio 多线程 runtime 中跨线程调度。当前代码使用 `LocalBoxFuture` 解决此问题，但限制了使用场景。

#### 5.5.2 解决方案分析

| 方案 | 优点 | 缺点 |
|------|------|------|
| (a) 避免跨 await 持锁 | future 可 Send，兼容多线程 runtime | 需要重构事务操作流程，可能引入更多复杂性 |
| (b) 原生 AFIT + `!Send`（`impl Future + '_ ` 不加 Send） | 最小改动，与当前 `LocalBoxFuture` 方案一致 | 所有使用事务的代码都必须在 `LocalSet` 中运行 |
| (c) `tokio::sync::Mutex` | async-aware 锁，更符合异步生态 | `tokio::MutexGuard` 同样非 Send（跨 await 时） |

#### 5.5.3 推荐方案

**推荐方案 (a): 避免跨 await 持锁**

核心思路：将事务句柄从 `Mutex<Option<TxnHolder>>` 改为 "取出-使用-放回" 模式：

```rust
pub async fn with_transaction_by_id<T, F>(txn_id: &str, f: F) -> Result<T> {
    let holder = get_txn_holder_by_id(txn_id)?;

    // 取出事务（短暂持锁）
    let mut txn = {
        let mut guard = holder.lock().unwrap();
        guard.take().ok_or(Error::NoTxn)?
    }; // guard 在此释放

    // 执行闭包（无锁）
    let result = f(&mut txn.txn).await;

    // 放回事务（短暂持锁）
    {
        let mut guard = holder.lock().unwrap();
        *guard = Some(txn);
    }

    result
}
```

这样 future 是 Send 的，可以在多线程 tokio runtime 中使用。trade-off 是取出期间其他线程访问同一事务会得到 `NoTxn` 错误——但事务本身就应该在单个执行流中顺序操作，这个约束是合理的。

#### 5.5.4 对 trait 定义的影响

采用方案 (a) 后，`QueryExecutor` 和 `CrudRepository` 的 trait 方法可以使用原生 `async fn in trait`（Rust 2024 edition 支持）配合 `+ Send` 约束，无需 `async-trait` 或 `LocalBoxFuture`。

---

## 6. 接口设计

### 6.1 统一入口: DatabaseManager

```rust
/// 数据库管理器 - 统一入口
pub struct DatabaseManager {
    pool_manager: Arc<PoolManager>,
    txn_manager: Arc<TransactionManager>,
    executor_factory: ExecutorFactory,
    /// WASM 事务注册表（实例级，非 static）
    txn_registry: Arc<RwLock<HashMap<String, Arc<Mutex<Option<TxnHolder>>>>>>,
}

impl DatabaseManager {
    /// 创建新的数据库管理器
    pub fn new(config: DatabaseConfig) -> Result<Self>;

    /// 注册数据源
    pub async fn register_data_source(&self, db_id: &str, config: PoolConfig) -> Result<()>;

    /// 注销数据源
    pub async fn unregister_data_source(&self, db_id: &str) -> Result<()>;

    /// 获取数据库访问对象（非事务）
    pub fn get_dbx(&self, db_id: &str) -> Result<Dbx<NoTransaction>>;

    /// 开始事务
    pub async fn begin_transaction(&self, db_id: &str, options: TransactionOptions) -> Result<Transaction>;

    /// 获取事务上下文（用于声明式事务）
    pub fn get_transaction_context(&self) -> TransactionContext;

    /// 关闭管理器
    pub async fn shutdown(&self) -> Result<()>;
}
```

### 6.2 Dbx API

```rust
/// 数据库访问对象（类型状态模式 — 仅本地使用场景）
pub struct Dbx<State = NoTransaction> {
    pool: DbPool,
    executor: Box<dyn QueryExecutor>,
    state: PhantomData<State>,
}

/// 非事务状态
pub struct NoTransaction;

/// 事务状态
pub struct InTransaction {
    handle: TransactionHandle,
}

impl Dbx<NoTransaction> {
    /// 执行 SQL（非事务）
    pub async fn execute(&self, sql: &str, params: &[ParamValue]) -> Result<u64>;

    /// 查询 SQL（非事务）
    pub async fn query(&self, sql: &str, params: &[ParamValue], dataset_id: &str) -> Result<DataSet>;

    /// 开始事务
    pub async fn begin_transaction(self, options: TransactionOptions) -> Result<Dbx<InTransaction>>;

    /// 简化的开始事务
    pub async fn begin_txn(self) -> Result<Dbx<InTransaction>> {
        self.begin_transaction(TransactionOptions::default()).await
    }
}

impl Dbx<InTransaction> {
    /// 执行 SQL（在事务中）
    pub async fn execute(&self, sql: &str, params: &[ParamValue]) -> Result<u64>;

    /// 查询 SQL（在事务中）
    pub async fn query(&self, sql: &str, params: &[ParamValue], dataset_id: &str) -> Result<DataSet>;

    /// 提交事务
    pub async fn commit(self) -> Result<Dbx<NoTransaction>>;

    /// 回滚事务
    pub async fn rollback(self) -> Result<Dbx<NoTransaction>>;

    /// 获取事务ID
    pub fn transaction_id(&self) -> &Uuid;
}
```

### 6.3 Repository API

保持 DataSet 为核心返回类型，与现有架构保持一致：

```rust
/// 通用 CRUD 仓库 trait
///
/// 查询结果统一返回 DataSet，保持与 cmx-core 数据模型的一致性。
/// 使用原生 async fn in trait（Rust 2024 edition），无需 async-trait 依赖。
pub trait CrudRepository: Send + Sync {
    /// 根据 ID 查询
    async fn find_by_id(&self, db_id: &str, txn_id: Option<&str>, table: &str,
                         id_column: &str, id_value: &DataValue, dataset_id: &str) -> Result<DataSet>;

    /// 查询所有
    async fn find_all(&self, db_id: &str, txn_id: Option<&str>, table: &str,
                      dataset_id: &str) -> Result<DataSet>;

    /// 条件查询
    async fn find_list(&self, db_id: &str, txn_id: Option<&str>, table: &str,
                       conditions: &[Condition], dataset_id: &str) -> Result<DataSet>;

    /// 分页查询
    async fn find_page(&self, db_id: &str, txn_id: Option<&str>, table: &str,
                       conditions: &[Condition], page: u64, page_size: u64,
                       dataset_id: &str) -> Result<PageResult>;

    /// 计数
    async fn count(&self, db_id: &str, txn_id: Option<&str>, table: &str,
                   conditions: &[Condition]) -> Result<u64>;

    /// 插入
    async fn insert(&self, db_id: &str, txn_id: Option<&str>, table: &str,
                    columns: &[&str], values: &[DataValue]) -> Result<u64>;

    /// 根据 ID 更新
    async fn update_by_id(&self, db_id: &str, txn_id: Option<&str>, table: &str,
                          id_column: &str, id_value: &DataValue,
                          columns: &[&str], values: &[DataValue]) -> Result<u64>;

    /// 条件更新
    async fn update_by_condition(&self, db_id: &str, txn_id: Option<&str>, table: &str,
                                 conditions: &[Condition],
                                 columns: &[&str], values: &[DataValue]) -> Result<u64>;

    /// 根据 ID 删除
    async fn delete_by_id(&self, db_id: &str, txn_id: Option<&str>, table: &str,
                          id_column: &str, id_value: &DataValue) -> Result<u64>;

    /// 条件删除
    async fn delete_by_condition(&self, db_id: &str, txn_id: Option<&str>, table: &str,
                                 conditions: &[Condition]) -> Result<u64>;
}

/// 条件操作符
pub enum ConditionOp {
    Eq, Ne, Gt, Ge, Lt, Le, Like, In, IsNull, IsNotNull,
}

/// 查询条件
pub struct Condition {
    pub column: String,
    pub op: ConditionOp,
    pub value: Option<DataValue>,
    pub in_values: Vec<DataValue>,
}

/// 分页结果
pub struct PageResult {
    pub data: DataSet,
    pub total: u64,
    pub page: u64,
    pub page_size: u64,
}

/// 通用 CRUD 仓库实现
///
/// 内部根据 db_type 分发 SQL 方言差异，替代当前 3 个独立实现
pub struct GenericCrudRepository;

impl GenericCrudRepository {
    pub fn new() -> Self;
}
```

> **未来可选增强**: 可在 DataSet 之上提供 Entity 映射层（`trait Entity: From<DataSet> + Into<DataSet>`），实现强类型的 ORM 风格 API。但不在本次重构范围内。

---

## 7. 错误处理设计

### 7.1 错误类型层次

```rust
/// 顶级错误类型
#[derive(Debug, Error)]
pub enum DatabaseError {
    #[error("连接错误: {0}")]
    Connection(#[from] ConnectionError),

    #[error("事务错误: {0}")]
    Transaction(#[from] TransactionError),

    #[error("查询错误: {0}")]
    Query(#[from] QueryError),

    #[error("配置错误: {0}")]
    Config(#[from] ConfigError),

    #[error("资源未找到: {0}")]
    NotFound(String),

    #[error("超时")]
    Timeout,

    #[error("已关闭")]
    Closed,
}

/// 连接错误
#[derive(Debug, Error)]
pub enum ConnectionError {
    #[error("连接池耗尽 (db_id: {db_id}, 等待时间: {wait_time:?})")]
    PoolExhausted { db_id: String, wait_time: Duration },

    #[error("连接失败: {message}")]
    ConnectionFailed { message: String, source: Option<Box<dyn Error>> },

    #[error("健康检查失败: {reason}")]
    HealthCheckFailed { reason: String },

    #[error("数据源未注册: {0}")]
    DataSourceNotFound(String),
}

/// 事务错误
#[derive(Debug, Error)]
pub enum TransactionError {
    #[error("事务已存在")]
    TransactionAlreadyExists,

    #[error("事务不存在: {0}")]
    TransactionNotFound(String),

    #[error("事务已超时 (txn_id: {txn_id}, 运行时间: {elapsed:?})")]
    TransactionTimeout { txn_id: String, elapsed: Duration },

    #[error("事务回滚-only")]
    RollbackOnly,

    #[error("传播行为错误: {0}")]
    PropagationError(String),

    #[error("不支持跨数据源事务 (当前: {current}, 请求: {requested})")]
    CrossDatabaseTransaction { current: String, requested: String },
}

/// 查询错误
#[derive(Debug, Error)]
pub enum QueryError {
    #[error("SQL 语法错误: {sql}")]
    SyntaxError { sql: String, message: String },

    #[error("参数绑定错误: {message}")]
    ParameterBinding { message: String },

    #[error("类型转换错误: from={from}, to={to}")]
    TypeConversion { from: String, to: String },

    #[error("约束违反: {constraint}")]
    ConstraintViolation { constraint: String },
}
```

### 7.2 错误上下文

```rust
/// 带上下文的错误
pub struct ContextualError<E> {
    error: E,
    context: ErrorContext,
}

pub struct ErrorContext {
    pub db_id: Option<String>,
    pub txn_id: Option<String>,
    pub sql: Option<String>,
    pub operation: &'static str,
    pub timestamp: Instant,
}

/// 添加上下文
pub trait WithContext<T> {
    fn with_db_id(self, db_id: &str) -> Result<T>;
    fn with_txn_id(self, txn_id: &str) -> Result<T>;
    fn with_sql(self, sql: &str) -> Result<T>;
    fn with_operation(self, op: &'static str) -> Result<T>;
}
```

> **关于重试策略**: 自动重试数据库操作需要极其谨慎——非幂等操作不应重试，事务中的操作不应在数据库层重试。重试策略应由应用层根据业务语义决定，不在本模块中实现。

---

## 8. 监控与可观测性

### 8.1 日志与追踪

使用 `tracing` 作为主要可观测性手段：

```rust
/// 操作跨度
#[derive(Debug)]
pub struct OperationSpan {
    operation: &'static str,
    db_id: Option<String>,
    txn_id: Option<String>,
    sql: Option<String>,
    start: Instant,
}

impl OperationSpan {
    pub fn new(operation: &'static str) -> Self {
        let span = Self {
            operation,
            db_id: None,
            txn_id: None,
            sql: None,
            start: Instant::now(),
        };

        tracing::info!(operation = %operation, "开始数据库操作");
        span
    }

    pub fn with_db_id(mut self, db_id: &str) -> Self {
        self.db_id = Some(db_id.to_string());
        self
    }

    pub fn finish(self, result: &Result<()>) {
        let duration = self.start.elapsed();

        match result {
            Ok(_) => {
                tracing::info!(
                    operation = %self.operation,
                    db_id = ?self.db_id,
                    txn_id = ?self.txn_id,
                    duration = ?duration,
                    "数据库操作成功"
                );
            }
            Err(e) => {
                tracing::error!(
                    operation = %self.operation,
                    db_id = ?self.db_id,
                    txn_id = ?self.txn_id,
                    error = %e,
                    duration = ?duration,
                    "数据库操作失败"
                );
            }
        }
    }
}
```

### 8.2 健康检查端点

```rust
/// 健康检查响应
#[derive(Serialize)]
pub struct HealthCheckResponse {
    pub status: HealthStatus,
    pub data_sources: Vec<DataSourceHealth>,
}

#[derive(Serialize)]
pub struct DataSourceHealth {
    pub db_id: String,
    pub status: HealthStatus,
    pub latency: Duration,
    pub active_connections: u32,
    pub idle_connections: u32,
}

/// 健康检查端点
pub async fn health_check_endpoint(manager: &DatabaseManager) -> HealthCheckResponse {
    let data_sources = manager.list_data_sources().await;
    let mut results = Vec::new();
    let mut all_healthy = true;

    for db_id in data_sources {
        let start = Instant::now();
        let status = manager.health_check(&db_id).await;
        let latency = start.elapsed();

        let pool_stats = manager.get_pool_stats(&db_id).await;

        results.push(DataSourceHealth {
            db_id: db_id.clone(),
            status: status.clone(),
            latency,
            active_connections: pool_stats.active,
            idle_connections: pool_stats.idle,
        });

        if !status.is_healthy() {
            all_healthy = false;
        }
    }

    HealthCheckResponse {
        status: if all_healthy { HealthStatus::Healthy } else { HealthStatus::Degraded },
        data_sources: results,
    }
}
```

### 8.3 未来增强：可扩展指标接口预留

```rust
/// 指标收集器 trait（接口预留，当前不实现）
///
/// 未来可接入 Prometheus、OpenTelemetry 等监控系统
pub trait MetricsCollector: Send + Sync {
    fn record_query_duration(&self, db_id: &str, duration: Duration);
    fn record_transaction_duration(&self, db_id: &str, duration: Duration);
    fn record_pool_stats(&self, db_id: &str, active: u32, idle: u32);
}

/// 空实现（默认，零开销）
pub struct NoopMetrics;
impl MetricsCollector for NoopMetrics {
    fn record_query_duration(&self, _: &str, _: Duration) {}
    fn record_transaction_duration(&self, _: &str, _: Duration) {}
    fn record_pool_stats(&self, _: &str, _: u32, _: u32) {}
}
```

---

## 9. 测试策略

### 9.1 单元测试

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use mockall::predicate::*;

    /// Mock 连接池管理器
    mock! {
        PoolManager {}

        #[async_trait]  // mockall 需要，仅测试代码使用
        impl PoolManager for PoolManager {
            async fn register(&self, db_id: &str, config: PoolConfig) -> Result<()>;
            async fn get_pool(&self, db_id: &str) -> Result<DbPool>;
        }
    }

    #[tokio::test]
    async fn test_transaction_commit() {
        let mut mock_pool = MockPoolManager::new();
        mock_pool.expect_get_pool()
            .with(eq("test_db"))
            .returning(|_| Ok(create_mock_pool()));

        let txn_manager = TransactionManager::new(Arc::new(mock_pool));

        let txn = txn_manager.begin_transaction("test_db", TransactionOptions::default()).await.unwrap();
        let result = txn.commit().await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_transaction_rollback_on_drop() {
        let rollback_called = Arc::new(AtomicBool::new(false));
        let rollback_clone = rollback_called.clone();

        let txn = create_test_transaction(move || {
            rollback_clone.store(true, Ordering::SeqCst);
        });

        drop(txn);

        assert!(rollback_called.load(Ordering::SeqCst));
    }
}
```

### 9.2 集成测试

优先使用 SQLite 内存数据库进行集成测试（当前模式），无需依赖 Docker：

```rust
#[cfg(test)]
mod integration_tests {
    use super::*;

    #[tokio::test]
    async fn test_sqlite_crud() {
        let manager = DatabaseManager::new(DatabaseConfig::default()).unwrap();

        manager.register_data_source("test", PoolConfig {
            db_type: DbType::Sqlite,
            connection_string: "sqlite::memory:".to_string(),
            ..Default::default()
        }).await.unwrap();

        let dbx = manager.get_dbx("test").unwrap();

        // 创建表
        dbx.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)", &[]).await.unwrap();

        // 插入
        let rows = dbx.execute(
            "INSERT INTO users (name) VALUES (?1)",
            &[ParamValue::String("Alice".into())]
        ).await.unwrap();
        assert_eq!(rows, 1);

        // 查询
        let dataset = dbx.query("SELECT * FROM users", &[], "users").await.unwrap();
        assert_eq!(dataset.rows.len(), 1);
    }

    #[tokio::test]
    async fn test_transaction_propagation() {
        let manager = create_test_manager().await;

        // Required: 嵌套事务共享
        let txn1 = manager.begin_transaction("test", TransactionOptions::default()).await.unwrap();
        // 验证嵌套行为...
    }
}
```

### 9.3 性能测试

```rust
#[cfg(test)]
mod benchmarks {
    use criterion::{black_box, criterion_group, criterion_main, Criterion};

    fn benchmark_connection_acquire(c: &mut Criterion) {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let manager = runtime.block_on(async {
            create_test_manager().await
        });

        c.bench_function("connection_acquire", |b| {
            b.to_async(&runtime).iter(|| async {
                let pool = manager.get_pool("test").unwrap();
                let conn = pool.acquire().await.unwrap();
                black_box(conn);
            });
        });
    }

    criterion_group!(benches, benchmark_connection_acquire);
    criterion_main!(benches);
}
```

---

## 10. 迁移指南

### 10.1 从旧版本迁移

#### 10.1.1 API 变化对照表

| 旧 API | 新 API | 说明 |
|--------|--------|------|
| `register_db_pool(id, config).await` | `manager.register_data_source(id, config).await` | 方法重命名，需要管理器实例 |
| `get_db_access(id)` | `manager.get_dbx(id)` | 返回类型从 `Option` 改为 `Result` |
| `dbx.with_transaction()` | `dbx.begin_txn().await` | 异步操作，返回类型状态 |
| `dbx.begin_txn_default(id)` | `manager.begin_transaction(id, options).await` | 移到管理器，支持更多选项 |
| `execute_sql_by_ids(db, txn, sql)` | `dbx.execute(sql, params).await` | 统一接口，支持参数绑定 |
| `transaction!` 宏 | `transactional!` 宏 | 参数顺序变化 |

#### 10.1.2 代码迁移示例

**旧代码：**
```rust
// 全局注册
register_db_pool("db1".to_string(), config).await?;

// 获取访问
let dbx = get_db_access("db1").unwrap();
let dbx_with_txn = dbx.with_transaction().unwrap();

// 开始事务
let txn_id = dbx_with_txn.begin_txn_default("db1").await?;

// 执行 SQL
let result = execute_sql_by_ids("db1", Some(&txn_id), "INSERT INTO users (name) VALUES ('test')").await?;

// 提交
dbx_with_txn.commit_txn().await?;
```

**新代码：**
```rust
// 创建管理器
let manager = DatabaseManager::new(DatabaseConfig::default())?;

// 注册数据源
manager.register_data_source("db1", config).await?;

// 获取访问对象
let dbx = manager.get_dbx("db1")?;

// 开始事务（类型状态模式）
let dbx_txn = dbx.begin_txn().await?;

// 执行 SQL（在事务中）
let result = dbx_txn.execute(
    "INSERT INTO users (name) VALUES ($1)",
    &[ParamValue::String("test".into())]
).await?;

// 提交（消费事务，返回非事务状态）
let dbx = dbx_txn.commit().await?;
```

#### 10.1.3 配置迁移

**旧配置：**
```rust
DbConfig {
    db_type: DbType::Postgres,
    db_url: "postgresql://localhost/test".to_string(),
    pool_config: PoolConfig::default(),
    health_check_interval: 60,
    health_check_timeout: 5,
}
```

**新配置：**
```rust
PoolConfig {
    db_type: DbType::Postgres,
    connection_string: "postgresql://localhost/test".to_string(),
    max_connections: 10,
    min_connections: 2,
    acquire_timeout: Duration::from_secs(30),
    idle_timeout: Duration::from_secs(600),
    max_lifetime: Duration::from_secs(1800),
    health_check_interval: Duration::from_secs(60),
    health_check_timeout: Duration::from_secs(5),
    warmup_on_start: true,
}
```

#### 10.1.4 破坏性变更清单

| 变更项 | 旧签名/行为 | 新签名/行为 | 迁移方式 |
|--------|------------|------------|---------|
| 全局函数移除 | `register_db_pool(key, config)` 等 6 个全局函数 | `manager.register_data_source(id, config)` | 需获取 `DatabaseManager` 实例 |
| Dbx 类型变更 | `Dbx { with_txn: bool }` | `Dbx<NoTransaction>` / `Dbx<InTransaction>` | 移除 `with_transaction()` 调用 |
| `with_transaction_by_id` | `F: FnOnce(&mut DbTransaction) -> Fut` | 签名不变，但内部实现改为"取出-放回"模式 | 无需修改调用代码 |
| `transaction!` 宏 | `transaction!(db_id, dbx, body)` | `transactional!(manager, db_id, body)` | 替换宏名和参数 |
| Error 枚举 | `Error::TxnCantCommitNoOpenTxn` 等 | `DatabaseError::Transaction(TransactionError::...)` | 更新 match 分支 |
| `rollback_txn_by_id` | 通过 `db_id` 间接操作 | 通过 `txn_id` 直接操作 | 调整参数 |
| `sea-query` 依赖 | 用于 SQL 构建 | 由 `ParamValue` + 内联 SQL 替代 | 迁移 SQL 构建代码 |

> **对下游模块的影响**: 任何 `use cmx_database::*` 的模块都需要适配新的导出名称。`cmx-core` 模块的 `DataSet`/`DataValue` 类型不受影响。

### 10.2 部署注意事项

1. **连接池预热**: 新版本支持启动时预热连接池，建议在生产环境启用
2. **健康检查**: 新的健康检查机制更全面，需要调整监控告警规则
3. **事务超时**: 默认事务超时时间可能需要根据业务调整
4. **优雅关闭**: 新版本支持优雅关闭，确保在应用停止时正确关闭连接

---

## 11. 分阶段实施路径

### Phase 1: 修复现有关键 Bug (P0)

**目标**: 修复影响正确性的现有问题，不改变整体架构。

| 任务 | 文件 | 说明 | 状态 |
|------|------|------|:----:|
| ~~修复监控回滚 bug~~ | `monitoring/mod.rs` | ~~超时事务应通过 `txn_id` 从注册表获取句柄直接回滚~~ | **已修复** |
| 修复参数化执行忽略参数 | `transaction/api.rs` | `execute_sql_with_params_by_ids` / `query_sql_with_params_by_ids` 事务路径需传入 params | 待修复 |
| 修复 `with_transaction_by_id` 跨 await 持锁 | `transaction/api.rs` | 改为"取出-使用-放回"模式，移除 `futures` 依赖 | 待修复 |
| 修复 `resume_suspended_txn` 死锁 | `transaction/core.rs` | 避免同一 Mutex 重入锁定 | 待修复 |
| 修复 `remove_db_pool` block_on panic | `connection/mod.rs` | 改为 async 函数 | 待修复 |
| 补充 `DbTransaction` 参数化方法 | `transaction/core.rs` | 添加 `execute_with_params` / `query_with_params` | 待修复 |

**验证标准**: 所有现有测试通过 + 新增针对修复点的测试用例。

**依赖**: 无。

### Phase 2: DatabaseManager 封装 (P1)

**目标**: 将全局状态封装为实例级状态，保持 API 向后兼容。

| 任务 | 说明 |
|------|------|
| 创建 `DatabaseManager` 结构体 | 包含 pool_manager + txn_registry |
| 封装 3 个全局 static | 迁移到 `DatabaseManager` 实例字段 |
| 保留全局辅助函数 | 作为 `DatabaseManager` 默认实例的 delegate |
| 改进错误类型 | 引入 `DatabaseError` 层次 + 上下文信息 |
| 连接池优雅关闭 | `update_db_pool` 时等待活跃连接排空 |

**验证标准**: 现有代码无需修改即可编译运行（向后兼容）+ 新代码可使用 `DatabaseManager` 实例。

**依赖**: Phase 1 完成。

### Phase 3: 事务管理改进 (P1)

**目标**: 完善事务传播行为。

| 任务 | 说明 |
|------|------|
| RequiresNew 挂起/恢复 | 创建新事务前保存当前事务状态，提交/回滚后恢复 |
| NotSupported 挂起 | 暂时挂起当前事务，非事务执行后恢复 |
| 事务超时监控改进 | 使用 `txn_id` 索引而非 `db_id` |
| 事务栈实现 | 引入 `TransactionContext` + `TransactionFrame` |

**验证标准**: 所有 6 种传播行为的集成测试通过。

**依赖**: Phase 2 完成。

### Phase 4: 类型安全增强 (P2)

**目标**: 本地场景引入编译期事务安全。

| 任务 | 说明 |
|------|------|
| `Dbx<State>` 类型状态 | 本地使用路径的编译期安全 |
| 解决 Send 约束 | 实现"取出-使用-放回"模式 |
| `ParamValue` 参数绑定 | 替代当前 SQL 内联字面量方式 |
| `QueryExecutor` trait | 统一查询执行接口 |
| `GenericCrudRepository` | 统一 Repository 实现 |

**验证标准**: 全部测试通过 + 新 API 的使用示例可编译。

**依赖**: Phase 3 完成。

### Phase 5: 未来增强 (P3)

以下为长期增强项，视需求择机实施：

- 监控指标集成（Prometheus/OpenTelemetry）
- Savepoint 支持
- Entity 映射层（DataSet → 强类型）
- 批量操作优化
- 连接池预热

---

## 附录

### A. 术语表

| 术语 | 说明 |
|------|------|
| Dbx | 数据库访问对象 (Database Access Object) |
| Pool | 连接池 |
| Transaction | 事务 |
| Propagation | 事务传播行为 |
| Isolation | 事务隔离级别 |
| WASM | WebAssembly |
| Host Function | wasmtime/wasmer 中宿主环境暴露给 WASM 模块的函数 |

### B. 参考资料

1. [sqlx 文档](https://docs.rs/sqlx/)
2. [Rust 异步编程](https://rust-lang.github.io/async-book/)
3. [事务传播行为](https://docs.spring.io/spring-framework/docs/current/javadoc-api/org/springframework/transaction/annotation/Propagation.html)
4. [数据库连接池最佳实践](https://github.com/brettwooldridge/HikariCP/wiki/About-Pool-Sizing)

### C. 版本历史

| 版本 | 日期 | 说明 |
|------|------|------|
| v1.0.0 | 2026-03-10 | 初始版本 |
| v1.1.0 | 2026-03-10 | 补充第三方依赖清单 |
| v2.0.0 | 2026-03-10 | 评审优化：修正技术矛盾、删除过度设计、补充实施路径 |
| v2.1.0 | 2026-03-10 | 代码审查后更新：修正§2.2/§2.3问题描述、更新§4.1模块结构、更新§11 Phase 1任务状态、更新附录D依赖清单 |

---

### D. 第三方依赖清单

#### D.1 当前实际依赖

| 依赖 | 版本 | 用途 | 目标状态 |
|------|------|------|---------|
| cmx-core | workspace | 核心数据模型（DataSet, DataValue） | 保留 |
| sqlx | workspace + features | 数据库访问层 | 保留 |
| sea-query | workspace | SQL 查询构建器 | **待移除**（Phase 4 替代） |
| sea-query-binder | workspace | SQL 参数绑定 | **待移除**（Phase 4 替代） |
| tokio | workspace | 异步运行时 | 保留 |
| serde | workspace | 序列化框架 | 保留 |
| serde_json | workspace | JSON 序列化 | 保留 |
| derive_more | workspace | derive 宏（From） | **待替换**为 thiserror（Phase 2） |
| serde_with | workspace | 序列化辅助 | 保留 |
| tracing | workspace | 结构化日志 | 保留 |
| log | 0.4.29 | 日志 | **待移除**（统一使用 tracing） |
| uuid | 1.8.0 | UUID 生成 | 保留 |
| rand | 0.8.5 | 随机数 | 检查是否仍需要 |
| futures | 0.3.30 | BoxFuture | **待移除**（原生 async fn 替代） |
| rust_decimal | 1.40.0 | 精确小数 | 保留 |
| chrono | 0.4.44 | 日期时间 | 保留 |

#### D.2 目标依赖（重构完成后）

| 依赖 | 用途 | 说明 |
|------|------|------|
| sqlx | 数据库访问层 | features: runtime-tokio, postgres, mysql, sqlite, chrono, uuid, json, decimal |
| tokio | 异步运行时 | workspace 统一管理 |
| tracing | 结构化日志 | 主要可观测性手段 |
| thiserror | 错误类型定义 | 库层错误定义（替代 derive_more） |
| serde / serde_json | 序列化 | 参数和结果传输 |
| rust_decimal | 精确小数计算 | |
| chrono | 日期时间处理 | |
| uuid | UUID 生成（事务ID） | |

#### D.3 测试相关（dev-dependencies）

| 依赖 | 用途 | 说明 |
|------|------|------|
| tokio-test | Tokio 测试工具 | 可选 |
| mockall | Mock 框架 | 接口稳定后引入 |
| criterion | 性能测试 | 功能稳定后引入 |

---

### E. Cargo.toml 配置示例

```toml
[package]
name = "cmx-database"
version = "0.1.0"
edition = "2024"

[dependencies]
# 核心依赖（使用 workspace 管理）
cmx-core = { workspace = true }
sqlx = { workspace = true, features = ["postgres", "mysql", "sqlite", "runtime-tokio", "chrono", "uuid", "json"] }
tokio = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }

# 错误处理
thiserror = { workspace = true }

# 日志
tracing = { workspace = true }

# 数据类型
chrono = { workspace = true }
rust_decimal = "1.40.0"
uuid = { version = "1.8.0", features = ["v4"] }


[dev-dependencies]
tokio-test = "0.4"
mockall = "0.13"

[[bench]]
name = "database_benchmarks"
harness = false
```

---

*文档结束*
