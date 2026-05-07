# cmx-database

> 数据库操作模块，支持 WebAssembly 调用 host 实现数据库操作。

## 项目简介

cmx-database 是 cmx-container 项目的数据库操作层，提供数据库连接池管理、事务处理、CRUD 操作和 SQL 执行等功能。

## 快速开始

### 安装

```toml
[dependencies]
cmx-database = "0.1.0"
```

### 核心示例

```rust
use cmx_database::{DatabaseManager, DatabaseManagerConfig};

let config = DatabaseManagerConfig::default();
let manager = DatabaseManager::new(config).await?;
let pool = manager.get_pool();
```

## 核心功能与特性

| 功能 | 说明 |
|------|------|
| 连接池管理 | 基于 SQLx 的异步连接池 |
| 事务处理 | 支持事务开启、提交、回滚 |
| CRUD 操作 | 通用增删改查封装 |
| SQL 执行 | 灵活 SQL 执行接口 |
| 类型安全 | 参数绑定和结果转换 |

## 模块结构

```
cmx-database
├── src/
│   ├── lib.rs              # 库入口
│   ├── config.rs           # 数据库配置
│   ├── connection.rs       # 连接池封装
│   ├── crud.rs             # CRUD 操作
│   ├── error.rs            # 错误类型
│   ├── executor.rs         # SQL 执行器
│   ├── manager.rs          # 数据库管理器
│   ├── monitoring.rs       # 监控功能
│   ├── transaction.rs      # 事务处理
│   └── types.rs            # 类型定义
└── Cargo.toml
```

## 使用指南

### 一、数据库管理器初始化

#### 1.1 基础配置

```rust
use cmx_database::{DatabaseManager, DatabaseManagerConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = DatabaseManagerConfig::default();
    let manager = DatabaseManager::new(config).await?;

    // 获取连接池
    let pool = manager.get_pool();

    Ok(())
}
```

#### 1.2 自定义配置

```rust
use cmx_database::{DatabaseManager, DatabaseManagerConfig};

let config = DatabaseManagerConfig::builder()
    .with_url("postgresql://user:pass@localhost:5432/cmx")
    .with_max_connections(20)
    .with_min_connections(5)
    .with_connect_timeout(30)
    .with_idle_timeout(600)
    .with_max_lifetime(3600)
    .build();

let manager = DatabaseManager::new(config).await?;
```

### 二、执行查询

#### 2.1 查询单行

```rust
use cmx_database::{DatabaseManager, DatabaseManagerConfig, Row};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manager = DatabaseManager::new(DatabaseManagerConfig::default()).await?;
    let pool = manager.get_pool();

    // 查询单行
    let row: Option<Row> = sqlx::query_as!(
        Row,
        "SELECT id, name, email FROM users WHERE id = $1",
        1_i64
    )
    .fetch_optional(pool)
    .await?;

    if let Some(row) = row {
        let id: i64 = row.get("id");
        let name: String = row.get("name");
        let email: String = row.get("email");
        println!("User: {} - {} ({})", id, name, email);
    }

    Ok(())
}
```

#### 2.2 查询多行

```rust
use cmx_database::{DatabaseManager, DatabaseManagerConfig, Row};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manager = DatabaseManager::new(DatabaseManagerConfig::default()).await?;
    let pool = manager.get_pool();

    // 查询多行
    let rows: Vec<Row> = sqlx::query_as!(
        Row,
        "SELECT id, name, email FROM users WHERE status = $1",
        "active"
    )
    .fetch_all(pool)
    .await?;

    for row in rows {
        println!("User: {} - {}",
            row.get::<i64, _>("id"),
            row.get::<String, _>("name")
        );
    }

    Ok(())
}
```

#### 2.3 参数化查询

```rust
use sqlx::{types::Uuid, Type};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manager = DatabaseManager::new(DatabaseManagerConfig::default()).await?;
    let pool = manager.get_pool();

    // 使用 ? 占位符（PostgreSQL 风格）
    let name: String = sqlx::query_scalar!(
        "SELECT name FROM users WHERE id = $1 AND status = $2",
        1_i64,
        "active"
    )
    .fetch_one(pool)
    .await?;

    // 使用命名参数
    let user = sqlx::query_as!(
        User,
        r#"SELECT id, name, email FROM users
           WHERE name LIKE $1 AND created_at > $2"#,
        "%john%",
        chrono::Utc::now() - chrono::Duration::days(30)
    )
    .fetch_one(pool)
    .await?;

    Ok(())
}
```

### 三、CRUD 操作

#### 3.1 插入数据

```rust
use cmx_database::DatabaseManager;

#[derive(Debug, serde::Serialize)]
struct NewUser {
    name: String,
    email: String,
    age: i32,
}

async fn create_user(
    manager: &DatabaseManager,
    user: NewUser,
) -> Result<i64, DbError> {
    let pool = manager.get_pool();

    // 插入并返回 ID
    let user_id: i64 = sqlx::query_scalar!(
        r#"INSERT INTO users (name, email, age, created_at)
           VALUES ($1, $2, $3, NOW())
           RETURNING id"#,
        user.name,
        user.email,
        user.age
    )
    .fetch_one(pool)
    .await?;

    Ok(user_id)
}

// 使用
let new_user = NewUser {
    name: "张三".to_string(),
    email: "zhangsan@example.com".to_string(),
    age: 30,
};
let user_id = create_user(manager, new_user).await?;
```

#### 3.2 更新数据

```rust
use cmx_database::DatabaseManager;

async fn update_user_email(
    manager: &DatabaseManager,
    user_id: i64,
    new_email: String,
) -> Result<u64, DbError> {
    let pool = manager.get_pool();

    // 更新并返回影响的行数
    let rows_affected = sqlx::query!(
        "UPDATE users SET email = $1, updated_at = NOW() WHERE id = $2",
        new_email,
        user_id
    )
    .execute(pool)
    .await?
    .rows_affected();

    Ok(rows_affected)
}

// 使用
let updated = update_user_email(manager, 1, "new_email@example.com").await?;
println!("Updated {} rows", updated);
```

#### 3.3 删除数据

```rust
use cmx_database::DatabaseManager;

async fn delete_user(
    manager: &DatabaseManager,
    user_id: i64,
) -> Result<u64, DbError> {
    let pool = manager.get_pool();

    let rows_affected = sqlx::query!(
        "DELETE FROM users WHERE id = $1",
        user_id
    )
    .execute(pool)
    .await?
    .rows_affected();

    Ok(rows_affected)
}
```

### 四、事务处理

#### 4.1 基础事务

```rust
use cmx_database::{DatabaseManager, Transaction};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manager = DatabaseManager::new(DatabaseManagerConfig::default()).await?;
    let pool = manager.get_pool();

    // 开始事务
    let mut tx = pool.begin().await?;

    // 执行多个操作
    sqlx::query!("INSERT INTO orders (user_id, total) VALUES ($1, $2)", 1_i64, 100.0)
        .execute(&mut *tx)
        .await?;

    sqlx::query!("UPDATE users SET order_count = order_count + 1 WHERE id = $1", 1_i64)
        .execute(&mut *tx)
        .await?;

    // 提交事务
    tx.commit().await?;

    Ok(())
}
```

#### 4.2 事务回滚

```rust
use cmx_database::DatabaseManager;

async fn transfer_funds(
    manager: &DatabaseManager,
    from_id: i64,
    to_id: i64,
    amount: f64,
) -> Result<(), DbError> {
    let pool = manager.get_pool();
    let mut tx = pool.begin().await?;

    // 扣除源账户
    let from_balance: f64 = sqlx::query_scalar!(
        "SELECT balance FROM accounts WHERE id = $1 FOR UPDATE",
        from_id
    )
    .fetch_one(&mut *tx)
    .await?;

    if from_balance < amount {
        // 余额不足，回滚
        tx.rollback().await?;
        return Err(DbError::InsufficientBalance);
    }

    // 更新源账户
    sqlx::query!(
        "UPDATE accounts SET balance = balance - $1 WHERE id = $2",
        amount,
        from_id
    )
    .execute(&mut *tx)
    .await?;

    // 更新目标账户
    sqlx::query!(
        "UPDATE accounts SET balance = balance + $1 WHERE id = $2",
        amount,
        to_id
    )
    .execute(&mut *tx)
    .await?;

    // 提交
    tx.commit().await?;

    Ok(())
}
```

#### 4.3 自动回滚的事务封装

```rust
use cmx_database::{DatabaseManager, DatabaseTransaction};

async fn atomic_operation(
    manager: &DatabaseManager,
) -> Result<(), DbError> {
    let mut tx = manager.begin_transaction().await?;

    // 执行操作
    sqlx::query!("INSERT INTO logs (message) VALUES ($1)", "operation start")
        .execute(&mut *tx)
        .await?;

    // 如果这里发生错误，事务会自动回滚
    // tx.drop() 或 tx.rollback() 会被调用

    tx.commit().await?;
    Ok(())
}
```

### 五、长事务处理

#### 5.1 检测长事务

```rust
use cmx_database::{DatabaseManager, TransactionManager};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manager = DatabaseManager::new(DatabaseManagerConfig::default()).await?;
    let tx_manager = TransactionManager::new(manager.get_pool());

    // 获取所有进行中的事务
    let long_transactions = tx_manager.get_long_running_transactions(60).await?;

    for tx_info in long_transactions {
        println!("Long running transaction: {:?}", tx_info);
    }

    Ok(())
}
```

#### 5.2 强制终止长事务

```rust
use cmx_database::{DatabaseManager, TransactionManager};

async fn kill_long_transaction(
    manager: &DatabaseManager,
    tx_id: i64,
) -> Result<(), DbError> {
    let tx_manager = TransactionManager::new(manager.get_pool());

    tx_manager.kill_transaction(tx_id).await?;

    Ok(())
}
```

### 六、连接池监控

#### 6.1 获取连接池状态

```rust
use cmx_database::DatabaseManager;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manager = DatabaseManager::new(DatabaseManagerConfig::default()).await?;

    // 获取连接池统计
    let stats = manager.get_pool_stats().await?;

    println!("Total connections: {}", stats.total_connections);
    println!("Idle connections: {}", stats.idle_connections);
    println!("Waiting requests: {}", stats.waiting_requests);

    Ok(())
}
```

#### 6.2 监控活跃连接

```rust
use cmx_database::DatabaseManager;

async fn monitor_connections(manager: &DatabaseManager) {
    let pool = manager.get_pool();

    loop {
        let stats = pool.stat().await;
        println!("[{}] Total: {}, Idle: {}, Acquired: {}",
            chrono::Utc::now().format("%H:%M:%S"),
            stats.total_connections(),
            stats.idle_connections(),
            stats.acquired_connections()
        );
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    }
}
```

### 七、错误处理

#### 7.1 错误类型

```rust
use cmx_database::DbError;

match result {
    Ok(value) => println!("Success: {:?}", value),
    Err(e) => {
        match e {
            DbError::NotFound(msg) => {
                eprintln!("Record not found: {}", msg);
            }
            DbError::DuplicateKey(msg) => {
                eprintln!("Duplicate key: {}", msg);
            }
            DbError::ConstraintViolation(msg) => {
                eprintln!("Constraint violation: {}", msg);
            }
            DbError::ConnectionFailed(msg) => {
                eprintln!("Connection failed: {}", msg);
            }
            DbError::QueryFailed(msg) => {
                eprintln!("Query failed: {}", msg);
            }
            DbError::TransactionAborted => {
                eprintln!("Transaction was aborted");
            }
            DbError::InsufficientBalance => {
                eprintln!("Insufficient balance");
            }
        }
    }
}
```

#### 7.2 重试机制

```rust
use cmx_database::{DatabaseManager, RetryConfig};

async fn execute_with_retry(
    manager: &DatabaseManager,
    operation: impl Fn(&DatabaseManager) -> _,
) -> Result<(), DbError> {
    let config = DatabaseManagerConfig::builder()
        .with_retry_config(RetryConfig {
            max_attempts: 3,
            initial_delay_ms: 100,
            max_delay_ms: 5000,
            backoff_multiplier: 2.0,
        })
        .build();

    operation(manager).await
}
```

### 八、完整示例

```rust
use cmx_database::{DatabaseManager, DatabaseManagerConfig, DbError};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: i64,
    pub name: String,
    pub email: String,
    pub status: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateUserRequest {
    pub name: String,
    pub email: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateUserRequest {
    pub email: Option<String>,
    pub status: Option<String>,
}

pub struct UserRepository {
    manager: DatabaseManager,
}

impl UserRepository {
    pub fn new(manager: DatabaseManager) -> Self {
        Self { manager }
    }

    pub async fn create(&self, req: CreateUserRequest) -> Result<User, DbError> {
        let pool = self.manager.get_pool();

        let user_id: i64 = sqlx::query_scalar!(
            r#"INSERT INTO users (name, email, status, created_at)
               VALUES ($1, $2, 'active', NOW())
               RETURNING id"#,
            req.name,
            req.email
        )
        .fetch_one(pool)
        .await?;

        self.find_by_id(user_id).await
    }

    pub async fn find_by_id(&self, id: i64) -> Result<Option<User>, DbError> {
        let pool = self.manager.get_pool();

        let user = sqlx::query_as!(
            User,
            "SELECT id, name, email, status FROM users WHERE id = $1",
            id
        )
        .fetch_optional(pool)
        .await?;

        Ok(user)
    }

    pub async fn update(&self, id: i64, req: UpdateUserRequest) -> Result<Option<User>, DbError> {
        let pool = self.manager.get_pool();

        // 构建动态更新查询
        if let Some(email) = &req.email {
            sqlx::query!("UPDATE users SET email = $1, updated_at = NOW() WHERE id = $2", email, id)
                .execute(pool)
                .await?;
        }

        if let Some(status) = &req.status {
            sqlx::query!("UPDATE users SET status = $1, updated_at = NOW() WHERE id = $2", status, id)
                .execute(pool)
                .await?;
        }

        self.find_by_id(id).await
    }

    pub async fn delete(&self, id: i64) -> Result<bool, DbError> {
        let pool = self.manager.get_pool();

        let rows_affected = sqlx::query!("DELETE FROM users WHERE id = $1", id)
            .execute(pool)
            .await?
            .rows_affected();

        Ok(rows_affected > 0)
    }

    pub async fn list_active(&self) -> Result<Vec<User>, DbError> {
        let pool = self.manager.get_pool();

        let users = sqlx::query_as!(
            User,
            "SELECT id, name, email, status FROM users WHERE status = 'active'"
        )
        .fetch_all(pool)
        .await?;

        Ok(users)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = DatabaseManagerConfig::default();
    let manager = DatabaseManager::new(config).await?;

    let repo = UserRepository::new(manager);

    // 创建用户
    let user = repo.create(CreateUserRequest {
        name: "张三".to_string(),
        email: "zhangsan@example.com".to_string(),
    }).await?;

    println!("Created user: {:?}", user);

    // 查询用户
    let found = repo.find_by_id(user.id).await?;
    println!("Found user: {:?}", found);

    // 更新用户
    let updated = repo.update(user.id, UpdateUserRequest {
        email: Some("new_email@example.com".to_string()),
        status: None,
    }).await?;
    println!("Updated user: {:?}", updated);

    // 删除用户
    let deleted = repo.delete(user.id).await?;
    println!("Deleted: {}", deleted);

    Ok(())
}
```
`sqlx` **支持**通过连接字符串指定 PostgreSQL 的默认 Schema，但**不能直接使用 `currentSchema` 这个参数名**。

`sqlx` 底层使用的是 PostgreSQL 官方的 `libpq` 驱动协议。在 `libpq` 的标准中，指定搜索路径（Schema）的参数是 `options`，而不是 `currentSchema`（`currentSchema` 通常是 JDBC 或其他特定驱动使用的参数）。

在 `sqlx` 中，你需要使用以下格式来指定 Schema：

```text
postgres://dbuser_dba:hkO4Mjkgih6dYVVhmuFYRLm5@192.168.1.14:5432/cmx?options=-c%20search_path%3Dmyschema
```

### 💡 参数拆解说明
由于 URL 中不能直接包含空格和等号，需要进行 URL 编码：
* `options=`：`libpq` 用于传递 PostgreSQL 后端启动参数的标准选项。
* `-c`：表示设置一个配置参数。
* `search_path=myschema`：PostgreSQL 中用于指定 Schema 搜索路径的真实配置。
* **URL 编码转换**：`-c search_path=myschema` 经过编码后，空格变成了 `%20`，等号变成了 `%3D`，最终拼接为 `-c%20search_path%3Dmyschema`。

### 🔧 另一种推荐做法
如果你不想把连接字符串写得这么复杂，也可以在建立 `sqlx` 连接池后，通过执行一条 SQL 语句来动态设置当前会话的 Schema：

```rust
use sqlx::PgPool;

async fn set_schema(pool: &PgPool) -> Result<(), sqlx::Error> {
    // 在获取连接后，先执行设置 search_path 的命令
    sqlx::query("SET search_path TO myschema")
        .execute(pool)
        .await?;
    Ok(())
}
```

**总结：** 如果你必须写在 `.env` 或连接字符串里，请使用 `?options=-c%20search_path%3D你的模式名`；如果在代码里灵活控制，使用 `SET search_path` 语句会更加直观。
