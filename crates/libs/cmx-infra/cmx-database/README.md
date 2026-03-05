# cmx-database 模块

## 模块简介

cmx-database 是一个功能强大的数据库连接管理模块，提供了多数据源管理、事务管理、连接池监控和负载均衡等功能。该模块设计用于支持企业级应用的数据库操作需求，具有高可靠性和可扩展性。

## 代码结构

模块采用模块化设计，按功能划分为以下目录：

```
src/
├── config/        # 配置相关代码
├── connection/    # 连接池管理代码
├── transaction/   # 事务管理代码
├── monitoring/    # 监控和健康检查代码
├── load_balancing/ # 负载均衡代码
├── metrics/       # 性能指标代码
└── lib.rs         # 模块入口和功能导出
```

### 各模块职责

- **config/**：定义数据库类型、连接池配置和数据库配置结构
- **connection/**：管理数据库连接池的创建、注册、更新和获取
- **transaction/**：处理事务的创建、提交、回滚和状态管理
- **monitoring/**：监控连接池健康状态和事务超时
- **load_balancing/**：实现轮询和随机负载均衡策略
- **metrics/**：采集和管理连接池性能指标

## 主要功能

### 1. 多数据源管理

- 支持多种数据库类型（PostgreSQL、MySQL、SQLite）
- 动态注册和更新数据库连接池
- 基于键值的连接池查找

### 2. 事务管理

- 支持事务的创建、提交和回滚
- 事务生命周期管理
- 事务状态跟踪
- 声明式事务管理宏

### 3. 连接池监控

- 连接池健康检查
- 事务超时监控和自动回滚
- 性能指标采集

### 4. 负载均衡

- 轮询负载均衡策略
- 随机负载均衡策略

### 5. 性能优化

- 连接池大小自动调整
- 连接超时控制
- 等待队列管理

## 如何使用

### 1. 注册数据库连接池

```rust
use cmx_database::{register_db_pool, DbConfig, DbType, PoolConfig};

async fn setup_database() -> cmx_database::Result<()> {
    let pool_config = PoolConfig {
        max_connections: 10,
        min_connections: 2,
        connect_timeout: 30,
        idle_timeout: 600,
        max_lifetime: 1800,
    };

    let db_config = DbConfig {
        db_type: DbType::Postgres,
        db_url: "postgresql://user:password@localhost:5432/mydb".to_string(),
        pool_config,
        health_check_interval: 60,
        health_check_timeout: 5,
    };

    register_db_pool("primary".to_string(), db_config).await?;
    Ok(())
}
```

### 2. 使用事务

```rust
use cmx_database::{get_db_access, transaction};

async fn perform_transaction() -> cmx_database::Result<()> {
    let dbx = get_db_access("primary").unwrap();

    let result = transaction!("primary", dbx, async {
        // 执行数据库操作
        // 例如：sqlx::query!("INSERT INTO users (name) VALUES (?)", "John").execute(dbx.db()).await?;
        Ok(())
    }).await;

    result
}
```

### 3. 启动监控

```rust
use cmx_database::start_monitoring;

fn main() {
    // 启动监控任务
    tokio::spawn(async {
        start_monitoring().await;
    });

    // 其他应用代码
}
```

### 4. 通过事务ID提交或回滚事务

```rust
use cmx_database::{commit_txn_by_id, rollback_txn_by_id};

async fn handle_transaction_by_id(txn_id: &str, should_commit: bool) -> cmx_database::Result<()> {
    if should_commit {
        // 通过事务ID提交事务
        commit_txn_by_id(txn_id).await?;
    } else {
        // 通过事务ID回滚事务
        rollback_txn_by_id(txn_id).await?;
    }
    Ok(())
}
```

## 依赖项

- `sqlx`：数据库操作库
- `tokio`：异步运行时
- `tracing`：日志记录
- `uuid`：生成唯一标识符
- `rand`：随机数生成

## 注意事项

1. 确保在使用前注册数据库连接池
2. 事务操作应在异步上下文中执行
3. 监控任务应在应用启动时启动
4. 对于生产环境，应根据实际负载调整连接池配置

## 性能建议

- 根据应用并发量调整 `max_connections`
- 合理设置 `idle_timeout` 和 `max_lifetime` 以避免连接泄漏
- 使用负载均衡策略分散数据库负载
- 定期检查性能指标以优化配置

## 故障排查

- 检查数据库连接 URL 是否正确
- 确保数据库服务正在运行
- 查看应用日志以获取详细错误信息
- 使用性能指标分析连接池状态

## 版本历史

- v0.1.0：初始版本，实现基本功能

---

通过以上功能和结构，cmx-database 模块为应用提供了可靠、高效的数据库连接管理能力，适合各种规模的企业应用使用。