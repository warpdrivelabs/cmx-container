# cmx-database 数据库操作模块

cmx-database 是 CMX 框架的核心数据库操作模块，提供对 PostgreSQL、MySQL、SQLite 三种数据库的异步访问支持。该模块支持 WebAssembly 环境调用 host 实现数据库操作。

## 功能特性

- **多数据库支持**：PostgreSQL、MySQL、SQLite
- **连接池管理**：自动管理数据库连接池
- **事务管理**：完整的事务支持（提交、回滚、传播行为）
- **类型安全**：提供类型安全的 SQL 查询构建器
- **结果转换**：自动将 SQL 查询结果转换为 DataSet
- **参数绑定**：支持多种数据类型的参数绑定
- **监控功能**：数据库连接池监控
- **全局单例**：提供默认数据库管理器实例

## 支持的数据类型

### ParamValue 参数值类型

| 类型 | 说明 | 数据库映射 |
|------|------|------------|
| `Null` | 空值 | NULL |
| `Bool(bool)` | 布尔值 | BOOLEAN |
| `Int(i64)` | 64位整数 | INTEGER/BIGINT |
| `Float(f64)` | 浮点数 | FLOAT/DOUBLE |
| `String(String)` | 字符串 | VARCHAR/TEXT |
| `Decimal(Decimal)` | 高精度十进制 | DECIMAL/NUMERIC |
| `DateTime(NaiveDateTime)` | 日期时间 | TIMESTAMP/DATETIME |
| `Date(NaiveDate)` | 日期 | DATE |
| `Json(Value)` | JSON 数据 | JSON/JSONB |
| `Binary(Vec<u8>)` | 二进制数据 | BYTEA/BLOB |
| `Uuid(Uuid)` | UUID | UUID |

### DataValue 结果值类型

| 类型 | 说明 |
|------|------|
| `Null` | 空值 |
| `Bool(bool)` | 布尔值 |
| `Int(i64)` | 64位整数 |
| `Float(f64)` | 浮点数 |
| `String(String)` | 字符串 |
| `Decimal(Decimal)` | 高精度十进制 |
| `DateTime(DateTime<Utc>)` | UTC 日期时间 |
| `Date(NaiveDate)` | 日期 |
| `Binary(Vec<u8>)` | 二进制数据 |
| `Array(Vec<DataValue>)` | 数组 |
| `Json(String)` | JSON 字符串 |
| `Uuid(Uuid)` | UUID |
| `ShortStr(SmolStr)` | 短字符串 |
| `LongStr(SmolStr)` | 长字符串 |

## 代码结构

```
cmx-database/src/
├── lib.rs                 # 模块入口，导出所有公共接口
├── config/                # 配置模块
│   └── mod.rs            # 数据库配置（DbType, DbConfig, PoolConfig）
├── connection/            # 连接池管理模块
│   └── mod.rs           # 连接池创建和管理（DbPool, DatabasePoolImpl）
├── executor/             # 结果转换模块
│   └── mod.rs           # ParamValue, ResultConverter
├── transaction/          # 事务管理模块
│   ├── mod.rs           # 事务管理入口和宏
│   ├── core.rs          # 事务核心实现（Dbx, DbTransaction）
│   ├── api.rs           # 事务 API
│   ├── metadata.rs      # 事务元数据
│   ├── registry.rs      # 事务注册表
│   ├── txcontext.rs     # 事务上下文
│   └── conversion.rs    # 类型转换
├── manager/             # 数据库管理器
│   └── mod.rs          # DatabaseManager, TransactionOptions, get_default_db_manager
├── types/              # 类型安全模块
│   └── mod.rs          # QueryBuilder, CompareOp, OrderDirection
├── monitoring/         # 监控模块
│   └── mod.rs          # 连接池监控
└── error.rs            # 错误类型定义
```

## 快速开始

### 1. 获取默认数据库管理器

```rust
use cmx_database::get_default_db_manager;

// 获取默认数据库管理器（单例）
let db_manager = get_default_db_manager();
```

### 2. 注册数据源

```rust
use cmx_database::{DbConfig, DbType, PoolConfig};

// 创建数据库配置
let db_config = DbConfig {
    db_type: DbType::Postgres,
    host: "localhost".to_string(),
    port: 5432,
    username: "postgres".to_string(),
    password: "password".to_string(),
    database: "test_db".to_string(),
    pool: PoolConfig::default(),
};

// 注册数据源
db_manager.register_data_source(db_config).await?;
```

### 3. 执行查询（不带事务）

```rust
use cmx_database::DataSet;

// 通过 db_id 执行查询
let dataset: DataSet = db_manager
    .query_sql("my_db", None, "SELECT * FROM users", "users")
    .await?;
```

### 4. 执行带参数查询

```rust
// 使用 serde_json::Value 数组作为位置参数
let params = serde_json::json!([
    1,
    "test"
]);

let dataset: DataSet = db_manager
    .query_sql_with_params("my_db", None, "SELECT * FROM users WHERE id = ? AND name = ?", params, "users")
    .await?;
```

### 5. 执行更新

```rust
// 使用位置参数
let params = serde_json::json!([
    "new_name",
    1
]);

let affected = db_manager
    .execute_sql_with_params("my_db", None, "UPDATE users SET name = ? WHERE id = ?", params)
    .await?;
```

### 6. 事务管理

```rust
use cmx_database::TransactionOptions;

// 开始事务
let txn_id = db_manager
    .begin_transaction("my_db", TransactionOptions::default())
    .await?;

// 在事务中执行查询
let dataset: DataSet = db_manager
    .query_sql("my_db", Some(&txn_id), "SELECT * FROM orders", "orders")
    .await?;

// 提交事务
db_manager.commit_transaction(&txn_id).await?;

// 或者回滚事务
db_manager.rollback_transaction(&txn_id).await?;
```

## 核心 API

### DatabaseManager

```rust
// 获取默认数据库管理器（单例）
pub fn get_default_db_manager() -> &'static Arc<DatabaseManager>

// 创建新的数据库管理器
pub fn new(config: DatabaseManagerConfig) -> Self

// 注册数据源
pub async fn register_data_source(&self, db_config: DbConfig) -> Result<()>

// 注销数据源
pub async fn unregister_data_source(&self, db_id: &str) -> Result<()>

// 获取数据库访问对象
pub fn get_dbx(&self, db_id: &str) -> Result<Dbx>

// 开始事务
pub async fn begin_transaction(&self, db_id: &str, options: TransactionOptions) -> Result<String>

// 执行 SQL 查询
pub async fn query_sql(&self, db_id: &str, txn_id: Option<&str>, sql: &str, dataset_id: &str) -> Result<DataSet>

// 执行带参数的 SQL 查询
pub async fn query_sql_with_params(&self, db_id: &str, txn_id: Option<&str>, sql: &str, params: serde_json::Value, dataset_id: &str) -> Result<DataSet>

// 执行 SQL 更新
pub async fn execute_sql(&self, db_id: &str, txn_id: Option<&str>, sql: &str) -> Result<u64>

// 执行带参数的 SQL 更新
pub async fn execute_sql_with_params(&self, db_id: &str, txn_id: Option<&str>, sql: &str, params: serde_json::Value) -> Result<u64>

// 提交事务
pub async fn commit_transaction(&self, txn_id: &str) -> Result<()>

// 回滚事务
pub async fn rollback_transaction(&self, txn_id: &str) -> Result<()>

// 健康检查
pub async fn health_check(&self, db_id: &str) -> Result<bool>

// 优雅关闭
pub async fn shutdown(&self) -> Result<()>
```

### TransactionOptions

```rust
#[derive(Debug, Clone)]
pub struct TransactionOptions {
    pub propagation: Propagation,  // 事务传播行为
}

impl Default for TransactionOptions {
    fn default() -> Self {
        Self {
            propagation: Propagation::Required,
        }
    }
}
```

### Dbx (数据库访问对象)

```rust
// 创建数据库访问对象
pub fn new(db_pool: DbPool, with_txn: bool) -> Result<Self>

// 开始事务
pub async fn begin_txn(&self, db_id: &str, propagation: Propagation) -> Result<String>

// 提交事务
pub async fn commit_txn(&mut self) -> Result<()>

// 回滚事务
pub async fn rollback_txn(&mut self) -> Result<()>
```

### ParamValue

```rust
// 从 serde_json::Value 自动转换
pub fn from_json(value: serde_json::Value) -> Self

// 支持的类型：
// - Null, Bool, Int, Float, String
// - Decimal, DateTime, Date
// - Json, Binary, Uuid
```

### ResultConverter

```rust
// 将 PostgreSQL 行转换为 DataSet
pub fn convert_postgres_rows(rows: Vec<PgRow>, dataset_id: &str) -> DataSet

// 将 MySQL 行转换为 DataSet
pub fn convert_mysql_rows(rows: Vec<MySqlRow>, dataset_id: &str) -> DataSet

// 将 SQLite 行转换为 DataSet
pub fn convert_sqlite_rows(rows: Vec<SqliteRow>, dataset_id: &str) -> DataSet
```



## 参数绑定方式

cmx-database 使用**位置参数**绑定方式，参数必须为 JSON 数组：

```rust
let params = serde_json::json!([
    1,
    "test"
]);

db_manager
    .query_sql_with_params("my_db", None, "SELECT * FROM users WHERE id = ? AND name = ?", params, "users")
    .await?;
```

### 参数类型自动转换

`ParamValue::from_json` 会自动将 JSON 数组元素转换为合适的数据库类型：

| JSON 元素 | 转换结果 |
|-----------|----------|
| `1` (数字) | `ParamValue::Int` |
| `1.5` (浮点数) | `ParamValue::Float` |
| `"abc"` (字符串) | `ParamValue::String` |
| `true/false` | `ParamValue::Bool` |
| `"uuid-string"` | `ParamValue::Uuid` |
| `"123.45"` (数字字符串) | `ParamValue::Decimal` |
| `"2024-01-01"` | `ParamValue::Date` |
| `"2024-01-01T12:00:00Z"` | `ParamValue::DateTime` |
| `"base64-encoded"` | `ParamValue::Binary` |
| `"{\"key\": \"value\"}"` | `ParamValue::Json` |
| `[1,2,3]` (数字数组) | `ParamValue::Binary` |

## 类型转换

### ParamValue::from_json 智能转换

自动将 JSON 值转换为合适的 ParamValue 类型：

| JSON 输入 | 转换结果 |
|-----------|----------|
| `null` | `ParamValue::Null` |
| `true/false` | `ParamValue::Bool` |
| `123` | `ParamValue::Int` |
| `1.5` | `ParamValue::Float` |
| `"uuid-string"` | `ParamValue::Uuid` |
| `"123.45"` | `ParamValue::Decimal` |
| `"2024-01-01T12:00:00Z"` | `ParamValue::DateTime` |
| `"2024-01-01"` | `ParamValue::Date` |
| `"base64-encoded"` | `ParamValue::Binary` |
| `"{\"key\": \"value\"}"` | `ParamValue::Json` |

### ResultConverter 数据库类型映射

| 数据库类型 | DataValue 类型 |
|-----------|---------------|
| INT/BIGINT/SMALLINT | `DataValue::Int` |
| FLOAT/DOUBLE/REAL | `DataValue::Float` |
| BOOL | `DataValue::Bool` |
| DECIMAL/NUMERIC | `DataValue::Decimal` |
| UUID | `DataValue::Uuid` |
| BYTEA/BLOB | `DataValue::Binary` |
| JSON/JSONB | `DataValue::Json` |
| DATE | `DataValue::Date` |
| TIMESTAMP/DATETIME | `DataValue::DateTime` |
| VARCHAR/TEXT | `DataValue::String` |

## 错误处理

```rust
use cmx_database::{Error, Result};

match db_manager.query_sql("my_db", None, "SELECT * FROM users", "users").await {
    Ok(dataset) => { /* 处理结果 */ }
    Err(Error::Sqlx(e)) => { /* SQLx 错误 */ }
    Err(Error::InvalidSql(e)) => { /* SQL 语法错误 */ }
    Err(Error::NoDb) => { /* 数据库不存在 */ }
    Err(e) => { /* 其他错误 */ }
}
```

## 配置选项

### PoolConfig

```rust
PoolConfig {
    max_connections: 10,      // 最大连接数
    min_connections: 2,       // 最小空闲连接数
    connect_timeout: 30,      // 连接超时（秒）
    idle_timeout: 600,        // 空闲超时（秒）
    max_lifetime: 1800,       // 最大生命周期（秒）
}
```

### DatabaseManagerConfig

```rust
DatabaseManagerConfig {
    default_pool_config: PoolConfig::default(),
    health_check_interval: Duration::from_secs(60),  // 健康检查间隔
    health_check_timeout: Duration::from_secs(5),    // 健康检查超时
}
```

### DbConfig

```rust
DbConfig {
    db_type: DbType::Postgres,  // 数据库类型
    host: "localhost",            // 主机地址
    port: 5432,                  // 端口
    username: "postgres",        // 用户名
    password: "password",         // 密码
    database: "test_db",         // 数据库名
    pool: PoolConfig::default(), // 连接池配置
}
```

## 依赖关系

```toml
[dependencies]
cmx-core = "0.1.0"
serde = "1.0"
serde_json = "1.0"
sqlx = { version = "0.8", features = ["postgres", "mysql", "sqlite", "runtime-tokio"] }
sea-query = "0.32"
rust_decimal = "1.40"
chrono = "0.4"
uuid = { version = "1.8", features = ["v4"] }
base64 = "0.22"
tokio = "1.0"
```

## 完整示例

### 基本 CRUD 操作

```rust
use cmx_database::{
    get_default_db_manager,
    DbConfig, DbType, PoolConfig,
    DatabaseManagerConfig,
    TransactionOptions,
    DataSet
};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 获取默认数据库管理器
    let db_manager = get_default_db_manager();

    // 2. 注册数据源
    let config = DbConfig {
        db_type: DbType::Postgres,
        host: "localhost".to_string(),
        port: 5432,
        username: "postgres".to_string(),
        password: "password".to_string(),
        database: "erp_db".to_string(),
        pool: PoolConfig::default(),
    };
    db_manager.register_data_source(config).await?;

    // 3. 查询数据
    let users: DataSet = db_manager
        .query_sql("erp_db", None, "SELECT id, name, email FROM users WHERE status = 'active'", "users")
        .await?;
    println!("查询到 {} 条记录", users.row_count());

    // 4. 带参数查询（使用位置参数）
    let params = json!([
        "vip",
        100
    ]);
    let orders: DataSet = db_manager
        .query_sql_with_params(
            "erp_db",
            None,
            "SELECT * FROM orders WHERE customer_type = ? AND amount > ?",
            params,
            "orders"
        )
        .await?;

    // 5. 插入数据
    let insert_params = json!([
        "张三",
        "zhangsan@example.com",
        1
    ]);
    let affected = db_manager
        .execute_sql_with_params(
            "erp_db",
            None,
            "INSERT INTO users (name, email, status) VALUES (?, ?, ?)",
            insert_params
        )
        .await?;
    println!("插入了 {} 条记录", affected);

    // 6. 事务操作
    let txn_id = db_manager
        .begin_transaction("erp_db", TransactionOptions::default())
        .await?;

    // 在事务中执行多项操作
    db_manager.execute_sql("erp_db", Some(&txn_id), "INSERT INTO orders (customer_id, amount) VALUES (1, 1000)").await?;
    db_manager.execute_sql("erp_db", Some(&txn_id), "UPDATE customers SET balance = balance - 1000 WHERE id = 1").await?;

    // 提交事务
    db_manager.commit_transaction(&txn_id).await?;
    println!("事务 {} 已提交", txn_id);

    // 7. 关闭管理器
    db_manager.shutdown().await?;

    Ok(())
}
```

### 使用 Dbx 直接操作

```rust
use cmx_database::{
    get_default_db_manager,
    DbConfig, DbType, PoolConfig,
    Dbx
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db_manager = get_default_db_manager();

    // 注册数据源
    let config = DbConfig {
        db_type: DbType::Postgres,
        host: "localhost".to_string(),
        port: 5432,
        username: "postgres".to_string(),
        password: "password".to_string(),
        database: "erp_db".to_string(),
        pool: PoolConfig::default(),
    };
    db_manager.register_data_source(config).await?;

    // 获取 Dbx
    let dbx: Dbx = db_manager.get_dbx("erp_db")?;

    // 开始事务
    let txn_id = dbx.begin_txn("erp_db", crate::transaction::Propagation::Required).await?;

    // 执行查询
    let dataset = dbx.query("SELECT * FROM orders", "orders").await?;

    // 提交事务
    dbx.commit_txn().await?;

    Ok(())
}
```

## 许可证

MIT License
