# CMX 配置管理模块 (v2)

基于 Rust [`config`](https://crates.io/crates/config) crate (v0.15) 实现的配置管理库，支持多种配置来源和格式的分层合并，提供统一的配置访问接口和 serde 反序列化支持。

## 设计思想

### 核心理念

1. **分层配置**：支持多层级配置来源，不同来源的配置可以相互覆盖
2. **添加顺序即优先级**：后添加的配置源优先级更高，自动覆盖先添加的同名配置
3. **类型安全**：基于 serde `Deserialize`，支持反序列化为任意 Rust 结构体
4. **全局单例**：`ConfigManager` 提供初始化一次后全局访问的能力

### 架构设计

```
┌───────────────────────────────────────────────────────────────┐
│                       ConfigManager                           │
│        (全局单例 — 基于 OnceLock，初始化一次后全局访问)         │
└───────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌───────────────────────────────────────────────────────────────┐
│                         Config                                │
│        (对 config::Config 的薄封装，提供统一访问接口)           │
│   get_string / get_int / get_bool / get_as / deserialize     │
└───────────────────────────────────────────────────────────────┘
                              ▲
                              │
┌───────────────────────────────────────────────────────────────┐
│                       ConfigBuilder                           │
│        (对 config::ConfigBuilder 的封装，链式 API)             │
│                                                              │
│  ┌──────────────┐ ┌──────────────┐ ┌───────────────────────┐ │
│  │ TOML/JSON/   │ │ Environment  │ │ CommandLineSource     │ │
│  │ YAML 文件    │ │ 环境变量     │ │ 命令行参数（自研）     │ │
│  │ (config-rs)  │ │ (config-rs)  │ │ (实现 config::Source) │ │
│  └──────────────┘ └──────────────┘ └───────────────────────┘ │
└───────────────────────────────────────────────────────────────┘
```

### 配置优先级

配置源按添加顺序决定优先级（**后添加的覆盖先添加的**）：

| 优先级 | 配置来源 | 说明 |
|--------|---------|------|
| 最低 | TOML 配置文件 | `add_toml_file()` / `add_toml_file_from_env()` |
| ↓ | .env 文件 | `add_env_file()` — 通过 `dotenvy` 加载到系统环境变量 |
| ↓ | 系统环境变量 | `add_env()` / `add_env_with_prefix()` |
| 最高 | 命令行参数 | `add_command_line()` / `add_source(CommandLineSource)` |

---

## 快速开始

### 1. 基本使用

```rust
use cmx_utils::Config;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 方式一：从 TOML 文件创建
    let config = Config::from_file("config/default.toml")?;

    // 方式二：使用构建器链式组合多个配置源
    let config = Config::builder()
        .add_toml_file("config/default.toml", 10)?        // 第 1 层：默认配置
        .add_toml_file("config/production.toml", 20)?     // 第 2 层：生产覆盖（覆盖 default）
        .add_env()                                        // 第 3 层：环境变量覆盖
        .add_command_line(std::env::args().skip(1))       // 第 4 层：命令行覆盖（最高优先级）
        .build()?;

    // 读取配置值
    let host: String = config.get_string("database.host")?;
    let port: i64 = config.get_int("database.port")?;

    println!("Database: {}:{}", host, port);
    Ok(())
}
```

### 2. 全局配置管理器

```rust
use cmx_utils::{ConfigManager, Config};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 应用启动时初始化（只调用一次）
    ConfigManager::initialize(|| {
        Config::builder()
            .add_toml_file("config/default.toml", 10)?
            .add_env()
            .build()
    })?;

    // 任意位置获取配置
    let host = ConfigManager::global().get_string("database.host")?;
    let port = ConfigManager::global().get_int("database.port")?;
    println!("Database: {}:{}", host, port);
    Ok(())
}
```

### 3. 反序列化为结构体（推荐）

```rust
use serde::Deserialize;
use cmx_utils::Config;

#[derive(Deserialize, Debug)]
struct DatabaseConfig {
    host: String,
    port: u16,
    pool_size: u32,
}

#[derive(Deserialize, Debug)]
struct AppConfig {
    database: DatabaseConfig,
    debug: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::from_file("config/default.toml")?;

    // 方式一：反序列化整个配置
    let app: AppConfig = config.deserialize()?;
    println!("DB: {}:{}", app.database.host, app.database.port);

    // 方式二：只反序列化某个子节点
    let db: DatabaseConfig = config.get_as("database")?;
    println!("Pool size: {}", db.pool_size);

    Ok(())
}
```

---

## API 参考

### ConfigBuilder — 配置构建器

| 方法 | 说明 | 示例 |
|------|------|------|
| `Config::builder()` | 创建空的配置构建器 | `Config::builder()` |
| `add_toml_file(path, priority)` | 添加 TOML 文件（文件不存在不报错） | `.add_toml_file("config/app.toml", 10)?` |
| `add_toml_file_from_env(env_var, priority)` | 从环境变量读取路径，可选 | `.add_toml_file_from_env("CONFIG_FILE", 10)` |
| `add_toml_file_from_env_required(env_var, priority)` | 从环境变量读取路径，必需 | `.add_toml_file_from_env_required("CONFIG_FILE", 10)?` |
| `add_toml_file_from_env_or(env_var, default, priority)` | 从环境变量读取路径，带默认值 | `.add_toml_file_from_env_or("CONFIG_FILE", "default.toml", 10)` |
| `add_env_file(path)` | 加载 .env 文件到环境变量 | `.add_env_file(".env")` |
| `add_env()` | 添加所有系统环境变量 | `.add_env()` |
| `add_env_with_prefix(prefix)` | 添加带前缀的环境变量 | `.add_env_with_prefix("APP_")` |
| `add_command_line(args)` | 添加命令行参数 | `.add_command_line(std::env::args().skip(1))` |
| `add_source(source)` | 添加自定义配置源 | `.add_source(my_source)` |
| `build()` | 构建配置实例 | `.build()?` |

> **注意**：`priority` 参数保留以兼容旧 API，但 config-rs 实际通过**添加顺序**决定优先级，后添加的 source 会覆盖先添加的同名配置。

### Config — 配置实例

#### 基本读取

| 方法 | 返回类型 | 说明 |
|------|---------|------|
| `get_string(key)` | `ConfigResult<String>` | 获取字符串值 |
| `get_int(key)` | `ConfigResult<i64>` | 获取整数值 |
| `get_float(key)` | `ConfigResult<f64>` | 获取浮点数值 |
| `get_bool(key)` | `ConfigResult<bool>` | 获取布尔值 |
| `get(key)` | `Option<config::Value>` | 获取原始值（可选） |
| `get_as<T>(key)` | `ConfigResult<T>` | 反序列化为任意 `Deserialize` 类型 |
| `get_optional<T>(key)` | `Option<T>` | 获取可选值（不存在返回 `None`） |
| `get_as_or<T>(key, default)` | `T` | 获取值或返回默认值 |

#### 高级功能

| 方法 | 说明 |
|------|------|
| `deserialize<T>()` | 将整个配置反序列化为结构体 |
| `sub_config(prefix)` | 获取指定前缀下的子配置视图 |
| `keys()` | 获取所有顶层配置键 |
| `contains(key)` | 检查配置键是否存在 |
| `len()` | 获取配置项数量 |
| `is_empty()` | 检查配置是否为空 |
| `inner()` | 获取底层 `config::Config` 引用 |

### ConfigManager — 全局配置管理器

| 方法 | 说明 |
|------|------|
| `ConfigManager::initialize(init_fn)` | 初始化全局配置（只能调用一次） |
| `ConfigManager::global()` | 获取全局配置引用（未初始化会 panic） |
| `ConfigManager::try_global()` | 安全获取全局配置（返回 `Option`） |
| `ConfigManager::is_initialized()` | 检查是否已初始化 |

### CommandLineSource — 命令行参数来源

config-rs 不原生支持命令行参数，本模块保留了自研的 `CommandLineSource`，实现了 `config::Source` trait。

```rust
use cmx_utils::CommandLineSource;

// 支持 --key=value 和 --key value 两种格式
let source = CommandLineSource::from_args(std::env::args().skip(1));

// 也可以直接通过 ConfigBuilder 使用
Config::builder()
    .add_command_line(std::env::args().skip(1))
    .build()?
```

### DefaultConfigLoader — 标准加载器

提供开箱即用的标准配置加载流程：

```rust
use cmx_utils::{DefaultConfigLoader, ConfigManager};

ConfigManager::initialize(|| {
    DefaultConfigLoader::new("config")
        .with_env_prefix("APP_")
        .load()
})?;
```

加载顺序（从低到高优先级）：
1. `{config_dir}/default.toml`
2. 环境变量 `CONFIG_FILE` 指定的 TOML 文件
3. `{config_dir}/.env`（通过 dotenvy）
4. 系统环境变量
5. 命令行参数

---

## 环境变量配置

### 环境变量命名规范

使用 `__`（双下划线）作为嵌套分隔符。config-rs 推荐使用双下划线，避免与点号在其他场景下的语义冲突。

| 场景 | 示例 |
|------|------|
| 嵌套配置 | `APP_database__host` |
| 二级嵌套 | `APP_database__connection__timeout` |
| 前缀分隔 | `APP_` 前缀会被自动去除，剩余部分用 `__` 分隔 |

### 使用示例

```rust
use cmx_utils::Config;

// 添加带前缀的环境变量（APP_ 前缀会被自动去除）
let config = Config::builder()
    .add_env_with_prefix("APP_")
    .build()?;

// 环境变量 APP_database__host=localhost 会映射为配置键 database.host
let host = config.get_string("database.host")?;
```

```bash
# Linux/macOS
export APP_database__host=prod-db.example.com
export APP_database__port=5433
export APP_server__port=443

# Windows PowerShell
$env:APP_database__host = "prod-db.example.com"
$env:APP_database__port = 5433
$env:APP_server__port = 443
```

### 从环境变量读取配置文件路径

```rust
// 可选：环境变量存在则加载，不存在则跳过
Config::builder()
    .add_toml_file_from_env("CONFIG_FILE", 10)
    .build()?

// 必需：环境变量不存在则报错
Config::builder()
    .add_toml_file_from_env_required("CONFIG_FILE", 10)?
    .build()?

// 带默认值：环境变量不存在则使用默认路径
Config::builder()
    .add_toml_file_from_env_or("CONFIG_FILE", "config/default.toml", 10)
    .build()?
```

---

## 配置文件格式

### TOML 格式（推荐）

```toml
[app]
name = "my-app"
version = "1.0.0"
debug = false

[database]
host = "localhost"
port = 5432
name = "mydb"

[database.pool_config]
max_connections = 10
min_connections = 2
connect_timeout = 30

[server]
host = "0.0.0.0"
port = 8080
workers = 4

[[databases]]
db_type = "Postgres"
db_url = "postgres://localhost/mydb"
db_id = "main"
default = true

[[databases]]
db_type = "Postgres"
db_url = "postgres://localhost/mydb2"
db_id = "secondary"
```

### JSON / YAML 格式

config-rs 内置支持 JSON 和 YAML 格式，通过文件扩展名自动识别：

```rust
// 自动根据扩展名识别格式
config::File::with_name("config/settings")     // .toml / .json / .yaml
config::File::new("config/settings", config::FileFormat::Json)  // 显式指定
```

---

## 错误处理

```rust
use cmx_utils::{ConfigError, ConfigResult};

match config.get_string("database.host") {
    Ok(host) => println!("Database host: {}", host),
    Err(ConfigError::KeyNotFound { key }) => {
        eprintln!("配置键 '{}' 不存在", key);
    }
    Err(ConfigError::TypeConversionError { key, target_type }) => {
        eprintln!("配置键 '{}' 无法转换为类型 '{}'", key, target_type);
    }
    Err(ConfigError::EnvVarError { var_name }) => {
        eprintln!("环境变量 '{}' 读取失败", var_name);
    }
    Err(ConfigError::FileNotFound { path }) => {
        eprintln!("配置文件不存在: {:?}", path);
    }
    Err(e) => {
        eprintln!("配置错误: {}", e);
    }
}
```

---

## 迁移指南（从 v1 迁移到 v2）

### 主要变更

| 变更项 | v1（自研） | v2（config-rs） |
|--------|-----------|----------------|
| 底层引擎 | 自研扁平化存储 | `config::Config` 树状结构 |
| 反序列化 | `FromConfigValue` trait | serde `Deserialize` |
| 配置文件解析 | 自研 `TomlParser`/`JsonParser` | config-rs 内置 |
| 环境变量来源 | 自研 `EnvSource` | `config::Environment` |
| 环境变量分隔符 | `.`（点号） | `__`（双下划线）— config-rs 推荐风格 |
| **键名大小写** | **保留原始大小写** | **强制转为小写** |
| 优先级机制 | 显式 `Priority(u8)` | 隐式添加顺序 |
| 已删除类型 | — | `FileSource`、`EnvSource`、`MemorySource`、`ConfigParser`、`Priority` |
| 已删除方法 | — | `Config::has()` → 改用 `Config::contains()` |

> **重要：键名自动小写**
>
> config-rs 的 `Environment` 源会将所有环境变量键名**强制转为小写**（这是 config-rs 的硬编码行为，不可配置）。
> 例如：环境变量 `WEB_FOLDER=static` 加载后，配置键变为 `web_folder`，访问时必须使用小写键名 `get_string("web_folder")`。
> `CommandLineSource` 也保持一致的行为，命令行参数 `--WEB_FOLDER=static` 同样会转为小写。
> 建议所有配置结构体字段使用 snake_case 命名，与 config-rs 的行为对齐。

### 代码迁移示例

**初始化配置：**
```rust
// v1（旧）
let mut builder = Config::builder();
builder = builder.add_toml_file("config/default.toml", 10)?;
builder = builder.add_source(EnvSource::new());
builder = builder.add_source(CommandLineSource::from_args(std::env::args().skip(1)));
let config = builder.build()?;

// v2（新）
let config = Config::builder()
    .add_toml_file("config/default.toml", 10)?
    .add_env()
    .add_command_line(std::env::args().skip(1))
    .build()?;
```

**反序列化配置结构体：**
```rust
// v1（旧）— 手动实现 FromConfigValue
impl FromConfigValue for DbConfig {
    fn from_config_value(value: &ConfigValue) -> ConfigResult<Self> {
        match value {
            ConfigValue::Object(map) => {
                let db_type = get_string_field(map, "db_type")?;
                let db_url = get_string_field(map, "db_url")?;
                // ... 手动解析每个字段
                Ok(DbConfig { db_type, db_url, ... })
            }
            _ => Err(ConfigError::TypeConversionError { ... }),
        }
    }
}

// v2（新）— 使用 serde Deserialize
#[derive(Deserialize)]
struct DbConfig {
    db_type: DbType,
    db_url: String,
    #[serde(default)]
    default: bool,
}

let db_config: DbConfig = config.get_as("database")?;
// 或直接获取数组
let configs: Vec<DbConfig> = config.get_as("databases")?;
```

**环境变量前缀：**
```rust
// v1（旧）
builder = builder.add_source(EnvSource::with_prefix("APP_"));
// 环境变量: APP_database.host=localhost

// v2（新）
builder = builder.add_env_with_prefix("APP_");
// 环境变量: APP_database__host=localhost  (分隔符从 . 改为 __)
```

---

## 最佳实践

### 1. 推荐使用 serde 反序列化

优先使用 `get_as::<T>()` 或 `deserialize::<T>()` 将配置反序列化为强类型结构体，而不是逐个字段读取：

```rust
// 推荐
let db: DatabaseConfig = config.get_as("database")?;

// 不推荐（繁琐且容易出错）
let host = config.get_string("database.host")?;
let port = config.get_int("database.port")?;
let pool_size = config.get_int("database.pool_size")?;
```

### 2. 为配置结构体设置默认值

```rust
#[derive(Deserialize)]
struct ServerConfig {
    host: String,
    #[serde(default = "default_port")]
    port: u16,
    #[serde(default)]
    debug: bool,
    #[serde(default = "default_workers")]
    workers: usize,
}

fn default_port() -> u16 { 8080 }
fn default_workers() -> usize { 4 }
```

### 3. 配置文件组织

```
project/
├── config/
│   ├── default.toml          # 默认配置（最先加载）
│   ├── production.toml       # 生产环境覆盖（后加载，优先级更高）
│   └── test.toml             # 测试环境覆盖
├── .env                      # 本地开发环境变量
└── .env.example              # 环境变量示例（提交到 git）
```

### 4. 敏感信息处理

- 不要在配置文件中存储密码、密钥等敏感信息
- 使用环境变量传递敏感配置
- 在 `.gitignore` 中排除 `.env` 文件

```gitignore
.env
.env.local
.env.*.local
config/production.toml
```

### 5. 启动时验证必需配置

```rust
fn validate_config(config: &Config) -> ConfigResult<()> {
    config.get_string("database.host")?;
    config.get_int("database.port")?;

    let port: u16 = config.get_as("server.port")?;
    if port < 1024 {
        return Err(ConfigError::BuildError {
            message: "端口号必须 >= 1024".to_string(),
        });
    }

    Ok(())
}
```

---

## 测试

```bash
# 运行 cmx-utils 的所有配置相关测试
cargo test -p cmx-utils config

# 运行集成测试
cargo test -p cmx-utils --test integration_test

# 运行特定测试
cargo test -p cmx-utils test_config_from_file
```

---

## 更新日志

### v2.0.0 (2026-03-31)

- 底层引擎从自研切换到 `config` crate (v0.15)
- 删除自研的 `ConfigSource`/`ConfigParser` trait 和 `TomlParser`/`JsonParser`/`EnvParser`
- 删除 `FileSource`、`EnvSource`、`MemorySource`（由 config-rs 内置功能替代）
- 删除 `Priority` 优先级类型（改为添加顺序决定优先级）
- `ConfigValue` 改为 `config::Value` 的类型别名
- `FromConfigValue` trait 保留为向后兼容层，推荐迁移到 serde `Deserialize`
- 环境变量嵌套分隔符改为 `__`（双下划线），config-rs 推荐风格
- 保留 `CommandLineSource`（实现 `config::Source` trait）
- 保留 `ConfigManager` 全局单例和 `DefaultConfigLoader`

### v0.2.0 (2026-03-10)

- 新增 `ConfigManager` 全局配置管理器
- 支持配置初始化一次后全局访问

### v0.1.0 (2026-03-09)

- 初始版本发布
- 自研配置管理框架
