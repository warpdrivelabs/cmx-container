# cmx-metadata

> 表定义元数据管理模块，负责 JSON 配置加载、DDL 生成/解析、增量 DDL diff、DDL 执行、i18n 伴生表生成。

## 项目简介

cmx-metadata 是 cmx-container 项目的元数据管理层，基础结构体（TableDefine、ColumnDefine 等）定义在 cmx-core 中，本模块提供元数据的加载、解析、执行等功能。

## 快速开始

### 安装

```toml
[dependencies]
cmx-metadata = "0.1.0"
```

### 核心示例

```rust
use cmx_metadata::{load_from_file, PgTableDefineExecutor};

let config = load_from_file("config.json")?;
let executor = PgTableDefineExecutor::new(db_manager);
executor.execute_ddl_by_ids(&config).await?;
```

## 核心功能与特性

| 功能 | 说明 |
|------|------|
| 配置加载 | 从文件或 JSON 加载表定义配置 |
| DDL 生成 | 将 TableDefine 转换为 Postgres DDL |
| DDL 解析 | 将 DDL 字符串转换为 TableDefine |
| 增量 DDL | 计算表结构变更差异 |
| DDL 执行 | 执行 DDL 语句到数据库 |
| i18n 支持 | 生成国际化伴生表 |

## 模块结构

```
cmx-metadata
├── src/
│   ├── lib.rs              # 库入口
│   ├── config.rs           # 配置管理
│   ├── ddl/               # DDL 处理模块
│   │   ├── mod.rs
│   │   ├── diff.rs        # 增量 DDL
│   │   └── postgres.rs    # Postgres DDL 生成
│   ├── error.rs           # 错误类型
│   ├── executor.rs        # DDL 执行器
│   ├── i18n.rs            # 国际化支持
│   ├── loader.rs          # 配置加载器
│   ├── parser/            # DDL 解析器
│   │   ├── mod.rs
│   │   └── postgres.rs
│   └── seed/              # 种子数据模块
│       ├── config.rs
│       ├── dml.rs
│       ├── executor.rs
│       ├── loader.rs
│       └── mod.rs
└── Cargo.toml
```

## 使用指南

### 一、配置加载

#### 1.1 从文件加载

```rust
use cmx_metadata::{load_from_file, MetadataConfig};

let config: MetadataConfig = load_from_file("metadata.json")?;
println!("Loaded {} tables", config.tables.len());
```

#### 1.2 从 JSON 字符串加载

```rust
use cmx_metadata::{load_from_str, MetadataConfig};

let json_str = r#"
{
    "tables": [
        {
            "name": "users",
            "columns": [
                {"name": "id", "type": "bigint", "pk": true},
                {"name": "name", "type": "varchar", "length": 255},
                {"name": "email", "type": "varchar", "length": 255}
            ]
        }
    ]
}
"#;

let config: MetadataConfig = load_from_str(json_str)?;
```

#### 1.3 从目录批量加载

```rust
use cmx_metadata::ConfigLoader;

let loader = ConfigLoader::new("metadata/");
let config = loader.load_all().await?;
println!("Loaded {} table definitions", config.tables.len());
```

### 二、DDL 生成

#### 2.1 生成建表 DDL

```rust
use cmx_metadata::{PgDdlGenerator, TableDefine};

let generator = PgDdlGenerator::new();

let table = TableDefine {
    name: "users".to_string(),
    schema: Some("public".to_string()),
    columns: vec![
        ColumnDefine {
            name: "id".to_string(),
            column_type: ColumnType::BigInt,
            nullable: false,
            default_value: None,
            is_primary_key: true,
            is_auto_increment: true,
        },
        ColumnDefine {
            name: "name".to_string(),
            column_type: ColumnType::Varchar(255),
            nullable: false,
            default_value: None,
            is_primary_key: false,
            is_auto_increment: false,
        },
    ],
    indexes: vec![],
    foreign_keys: vec![],
};

let ddl = generator.generate_create_table(&table)?;
println!("DDL: {}", ddl);
```

#### 2.2 生成索引 DDL

```rust
use cmx_metadata::{PgDdlGenerator, IndexDefine, IndexType};

let generator = PgDdlGenerator::new();

let index = IndexDefine {
    name: "idx_users_email".to_string(),
    table_name: "users".to_string(),
    columns: vec!["email".to_string()],
    index_type: IndexType::BTree,
    unique: true,
    concurrently: false,
};

let index_ddl = generator.generate_create_index(&index)?;
println!("Index DDL: {}", index_ddl);
```

#### 2.3 生成外键 DDL

```rust
use cmx_metadata::{PgDdlGenerator, ForeignKeyDefine, ReferentialAction};

let generator = PgDdlGenerator::new();

let fk = ForeignKeyDefine {
    name: "fk_orders_user".to_string(),
    table_name: "orders".to_string(),
    column: "user_id".to_string(),
    referenced_table: "users".to_string(),
    referenced_column: "id".to_string(),
    on_delete: ReferentialAction::Cascade,
    on_update: ReferentialAction::NoAction,
};

let fk_ddl = generator.generate_add_foreign_key(&fk)?;
println!("FK DDL: {}", fk_ddl);
```

### 三、DDL 解析

#### 3.1 解析建表 DDL

```rust
use cmx_metadata::{PgParser, ParserConfig};

let parser = PgParser::new(ParserConfig::default());

let ddl = r#"
CREATE TABLE users (
    id BIGSERIAL PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    email VARCHAR(255) UNIQUE NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
)
"#;

let table: TableDefine = parser.parse_create_table(ddl)?;
println!("Table: {}", table.name);
println!("Columns: {}", table.columns.len());
```

#### 3.2 解析 ALTER TABLE

```rust
use cmx_metadata::PgParser;

let parser = PgParser::new(ParserConfig::default());

let alter_ddl = r#"
ALTER TABLE users
    ADD COLUMN phone VARCHAR(20),
    ALTER COLUMN name TYPE VARCHAR(500),
    DROP COLUMN old_column
"#;

let alter_ops = parser.parse_alter_table(alter_ddl)?;
for op in alter_ops {
    println!("Operation: {:?}", op);
}
```

### 四、增量 DDL

#### 4.1 计算表结构差异

```rust
use cmx_metadata::{DiffCalculator, TableDefine, ColumnType};

let calculator = DiffCalculator::new();

// 原始表结构
let old_table = TableDefine {
    name: "users".to_string(),
    columns: vec![
        ColumnDefine {
            name: "id".to_string(),
            column_type: ColumnType::BigInt,
            nullable: false,
            default_value: None,
            is_primary_key: true,
            is_auto_increment: true,
        },
        ColumnDefine {
            name: "name".to_string(),
            column_type: ColumnType::Varchar(255),
            nullable: false,
            default_value: None,
            is_primary_key: false,
            is_auto_increment: false,
        },
    ],
    ..Default::default()
};

// 新表结构
let new_table = TableDefine {
    name: "users".to_string(),
    columns: vec![
        ColumnDefine {
            name: "id".to_string(),
            column_type: ColumnType::BigInt,
            nullable: false,
            default_value: None,
            is_primary_key: true,
            is_auto_increment: true,
        },
        ColumnDefine {
            name: "name".to_string(),
            column_type: ColumnType::Varchar(500),  // 长度变更
            nullable: false,
            default_value: None,
            is_primary_key: false,
            is_auto_increment: false,
        },
        ColumnDefine {
            name: "email".to_string(),  // 新增列
            column_type: ColumnType::Varchar(255),
            nullable: false,
            default_value: None,
            is_primary_key: false,
            is_auto_increment: false,
        },
    ],
    ..Default::default()
};

let diff = calculator.calculate_diff(&old_table, &new_table)?;

println!("Added columns: {:?}", diff.added_columns);
println!("Modified columns: {:?}", diff.modified_columns);
println!("Dropped columns: {:?}", diff.dropped_columns);
```

#### 4.2 生成增量 DDL

```rust
use cmx_metadata::{DiffToDdl, DiffResult};

let converter = DiffToDdl::new();

let alter_ddl = converter.convert(&diff)?;
println!("Generated DDL: {}", alter_ddl);

// 输出示例:
// ALTER TABLE users ALTER COLUMN name TYPE VARCHAR(500);
// ALTER TABLE users ADD COLUMN email VARCHAR(255) NOT NULL;
```

### 五、DDL 执行

#### 5.1 执行单个 DDL

```rust
use cmx_metadata::{PgTableDefineExecutor, DatabaseManager};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db_manager = DatabaseManager::new(DatabaseManagerConfig::default()).await?;
    let executor = PgTableDefineExecutor::new(db_manager);

    executor.execute("CREATE TABLE users (id BIGSERIAL PRIMARY KEY)").await?;

    Ok(())
}
```

#### 5.2 执行表定义

```rust
use cmx_metadata::{PgTableDefineExecutor, MetadataConfig, load_from_file};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db_manager = DatabaseManager::new(DatabaseManagerConfig::default()).await?;
    let executor = PgTableDefineExecutor::new(db_manager);

    let config: MetadataConfig = load_from_file("tables.json")?;

    // 执行所有表的 DDL
    for table in &config.tables {
        executor.execute_ddl_for_table(table).await?;
    }

    Ok(())
}
```

#### 5.3 按 ID 执行 DDL

```rust
use cmx_metadata::{PgTableDefineExecutor, MetadataConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db_manager = DatabaseManager::new(DatabaseManagerConfig::default()).await?;
    let executor = PgTableDefineExecutor::new(db_manager);

    let config: MetadataConfig = load_from_file("tables.json")?;

    // 只执行指定的表（通过 table_id）
    let target_ids = vec!["users", "orders"];
    executor.execute_ddl_by_ids(&config, &target_ids).await?;

    Ok(())
}
```

### 六、国际化(i18n)支持

#### 6.1 生成 i18n 伴生表

```rust
use cmx_metadata::{I18nGenerator, TableDefine, Language};

let generator = I18nGenerator::new();

// 为指定表生成 i18n 伴生表
let source_table = TableDefine {
    name: "products".to_string(),
    columns: vec![
        ColumnDefine {
            name: "id".to_string(),
            column_type: ColumnType::BigInt,
            nullable: false,
            default_value: None,
            is_primary_key: true,
            is_auto_increment: true,
        },
        ColumnDefine {
            name: "name".to_string(),
            column_type: ColumnType::Varchar(255),
            nullable: false,
            default_value: None,
            is_primary_key: false,
            is_auto_increment: false,
        },
        ColumnDefine {
            name: "description".to_string(),
            column_type: ColumnType::Text,
            nullable: true,
            default_value: None,
            is_primary_key: false,
            is_auto_increment: false,
        },
    ],
    ..Default::default()
};

let i18n_tables = generator.generate_i18n_tables(&source_table, &[
    Language::ZhCn,
    Language::EnUs,
])?;

for table in &i18n_tables {
    println!("Generated table: {}", table.name);
    // 创建 i18n 表...
}
```

#### 6.2 配置语言支持

```rust
use cmx_metadata::{I18nConfig, Language};

let i18n_config = I18nConfig {
    default_language: Language::EnUs,
    supported_languages: vec![
        Language::ZhCn,
        Language::EnUs,
        Language::JaJp,
    ],
    fallback_language: Language::EnUs,
    i18n_table_suffix: "_i18n".to_string(),
};
```

### 七、种子数据管理

#### 7.1 加载种子数据配置

```rust
use cmx_metadata::seed::{SeedLoader, SeedConfig};

let loader = SeedLoader::new("seeds/");
let seed_config: SeedConfig = loader.load("users.yaml").await?;
```

#### 7.2 执行种子数据

```rust
use cmx_metadata::seed::{SeedExecutor, SeedConfig};

let executor = SeedExecutor::new(db_manager);

// 加载种子数据
let seed_config: SeedConfig = SeedConfig {
    table_name: "users".to_string(),
    data: vec![
        vec!["id", "name", "email"],
        vec!["1", "张三", "zhangsan@example.com"],
        vec!["2", "李四", "lisi@example.com"],
    ],
    // 或使用 YAML 格式
    // data_yaml: "...",

    // 冲突处理策略
    on_conflict: SeedConflictStrategy::Upsert,
    unique_columns: vec!["email".to_string()],
};

// 执行种子数据
executor.execute(&seed_config).await?;
```

### 八、完整示例

```rust
use cmx_metadata::{
    load_from_file, MetadataConfig,
    PgDdlGenerator, PgParser, DiffCalculator, DiffToDdl,
    PgTableDefineExecutor, DatabaseManager, DatabaseManagerConfig,
};
use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 加载表定义配置
    let config: MetadataConfig = load_from_file("tables.json")?;

    // 2. 初始化数据库执行器
    let db_manager = DatabaseManager::new(DatabaseManagerConfig::default()).await?;
    let executor = PgTableDefineExecutor::new(db_manager);

    // 3. 创建 DDL 生成器
    let generator = PgDdlGenerator::new();

    // 4. 为每个表生成并执行 DDL
    for table in &config.tables {
        println!("Processing table: {}", table.name);

        // 生成建表 DDL
        let create_ddl = generator.generate_create_table(table)?;
        println!("DDL: {}", create_ddl);

        // 执行 DDL
        executor.execute_ddl_for_table(table).await?;
        println!("Created table: {}", table.name);
    }

    // 5. 生成索引
    for index in &config.indexes {
        let index_ddl = generator.generate_create_index(index)?;
        executor.execute(&index_ddl).await?;
        println!("Created index: {}", index.name);
    }

    // 6. 生成外键
    for fk in &config.foreign_keys {
        let fk_ddl = generator.generate_add_foreign_key(fk)?;
        executor.execute(&fk_ddl).await?;
        println!("Created foreign key: {}", fk.name);
    }

    println!("All metadata operations completed!");
    Ok(())
}
```

### 九、错误处理

```rust
use cmx_metadata::MetadataError;

match result {
    Ok(_) => println!("Success"),
    Err(e) => {
        match e {
            MetadataError::TableNotFound(name) => {
                eprintln!("Table not found: {}", name);
            }
            MetadataError::ColumnNotFound(table, column) => {
                eprintln!("Column not found: {}.{}", table, column);
            }
            MetadataError::DdlGenerationFailed(msg) => {
                eprintln!("DDL generation failed: {}", msg);
            }
            MetadataError::DdlExecutionFailed(msg) => {
                eprintln!("DDL execution failed: {}", msg);
            }
            MetadataError::ParseError(msg) => {
                eprintln!("Parse error: {}", msg);
            }
            MetadataError::ValidationFailed(msg) => {
                eprintln!("Validation failed: {}", msg);
            }
        }
    }
}
```
