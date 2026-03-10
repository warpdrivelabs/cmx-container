# CMX 配置管理模块

一个灵活、强大的 Rust 配置管理库，支持多种配置来源和格式，提供统一的配置访问接口。

## 设计思想

### 核心理念

本配置管理模块的设计基于以下核心理念：

1. **分层配置**：支持多层级配置来源，不同来源的配置可以相互覆盖，实现灵活的配置管理
2. **优先级机制**：配置来源具有明确的优先级，高优先级配置自动覆盖低优先级配置
3. **类型安全**：提供强类型配置值和类型转换功能，避免运行时类型错误
4. **扩展性强**：基于 trait 的设计，易于扩展新的配置来源和解析器

### 架构设计

```
┌─────────────────────────────────────────────────────────┐
│                    ConfigManager                         │
│  (配置管理器 - 负责协调配置来源、合并配置、提供访问接口)   │
└─────────────────────────────────────────────────────────┘
                            │
        ┌───────────────────┼───────────────────┐
        │                   │                   │
        ▼                   ▼                   ▼
┌──────────────┐  ┌──────────────┐  ┌──────────────┐
│ConfigSource  │  │ ConfigParser │  │ ConfigValue  │
│(配置来源抽象) │  │ (解析器抽象)  │  │ (配置值类型)  │
└──────────────┘  └──────────────┘  └──────────────┘
        │                   │                   │
        ├─ FileSource       ├─ TomlParser       ├─ String
        ├─ EnvSource        ├─ JsonParser       ├─ Integer
        ├─ CommandLineSource├─ EnvParser        ├─ Float
        └─ MemorySource     └─ ...              ├─ Boolean
                                                 ├─ Array
                                                 └─ Object
```

### 配置优先级

配置来源按优先级从高到低排列：

1. **命令行参数** (Priority: 100) - 最高优先级
   - 通过 `--key=value` 或 `--key value` 格式传递
   - 适用于临时覆盖配置

2. **系统环境变量** (Priority: 80)
   - 从操作系统环境变量读取
   - 支持前缀过滤
   - 适用于容器化部署和 CI/CD 环境

3. **环境变量文件** (.env) (Priority: 60)
   - 从 .env 文件读取
   - 适用于开发环境配置

4. **用户指定的TOML配置文件** (Priority: 用户指定)
   - 用户可以明确指定TOML配置文件的优先级
   - 支持多个TOML文件，每个文件可以有不同的优先级
   - 不再依赖文件名（如production.toml、default.toml）决定优先级

## 功能特性

### 1. 多格式支持

支持三种主流配置文件格式：

- **TOML**: 适合结构化配置，支持嵌套和复杂类型
- **JSON**: 通用格式，易于与其他系统集成
- **.env**: 简单键值对格式，适合环境变量配置

### 2. 用户指定的TOML配置文件优先级

**重要变更**：TOML配置文件的优先级现在由用户明确指定，而不是由文件名决定。

```rust
use cmx_utils::config::{Config, ConfigBuilder, FileSource, Priority};

// 创建配置构建器
let mut builder = Config::builder();

// 添加默认配置文件（优先级 10）
builder = builder.add_toml_file("config/default.toml", 10)?;

// 添加生产环境配置文件（优先级 20，会覆盖default.toml中的同名配置）
builder = builder.add_toml_file("config/production.toml", 20)?;

// 添加测试环境配置文件（优先级 30，会覆盖production.toml中的同名配置）
builder = builder.add_toml_file("config/test.toml", 30)?;

// 构建配置
let config = builder.build()?;
```

### 3. 从环境变量读取配置文件路径

支持从环境变量中读取配置文件路径，而不是在代码中硬编码路径。

#### 方式一：可选环境变量

如果环境变量存在则使用，不存在则跳过：

```rust
use cmx_utils::config::{Config, Priority};

// 如果环境变量 CONFIG_FILE 存在，则加载其指定的配置文件
let config = Config::builder()
    .add_toml_file_from_env("CONFIG_FILE", Priority::DEFAULT_TOML)
    .build()?;
```

#### 方式二：必需环境变量

如果环境变量不存在则返回错误：

```rust
use cmx_utils::config::{Config, Priority};

// 环境变量 CONFIG_FILE 必须存在，否则返回错误
let config = Config::builder()
    .add_toml_file_from_env_required("CONFIG_FILE", Priority::DEFAULT_TOML)?
    .build()?;
```

#### 方式三：带默认值

如果环境变量不存在则使用默认路径：

```rust
use cmx_utils::config::{Config, Priority};

// 如果环境变量 CONFIG_FILE 存在，使用其值；否则使用 "config/default.toml"
let config = Config::builder()
    .add_toml_file_from_env_or("CONFIG_FILE", "config/default.toml", Priority::DEFAULT_TOML)
    .build()?;
```

#### 完整示例

```rust
use cmx_utils::config::{Config, ConfigBuilder, EnvSource, CommandLineSource, Priority};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::builder()
        // 从环境变量读取默认配置文件路径（优先级 10）
        .add_toml_file_from_env_or("DEFAULT_CONFIG", "config/default.toml", Priority::DEFAULT_TOML)
        // 从环境变量读取生产配置文件路径（优先级 20）
        .add_toml_file_from_env("PRODUCTION_CONFIG", Priority(20))
        // 添加系统环境变量
        .add_source(EnvSource::new())
        // 添加命令行参数
        .add_source(CommandLineSource::from_args(std::env::args().skip(1)))
        .build()?;
    
    Ok(())
}
```

**环境变量设置示例**：

```bash
# Linux/macOS
export DEFAULT_CONFIG=/path/to/default.toml
export PRODUCTION_CONFIG=/path/to/production.toml

# Windows
set DEFAULT_CONFIG=C:\path\to\default.toml
set PRODUCTION_CONFIG=C:\path\to\production.toml
```

### 4. 配置来源扩展

通过实现 `ConfigSource` trait 可以轻松扩展新的配置来源：

```rust
pub trait ConfigSource: Send + Sync {
    fn load(&self) -> ConfigResult<ConfigStore>;
    fn name(&self) -> &str;
    fn priority(&self) -> Priority;
}
```

### 5. 类型转换

提供自动类型推断和显式类型转换功能：

```rust
// 自动类型推断
let value = config.get("port")?;
let port: i64 = value.try_into_type()?;

// 便捷方法
let host: String = config.get_string("host")?;
let port: i64 = config.get_int("port")?;
let debug: bool = config.get_bool("debug")?;
```

### 6. 嵌套配置访问

支持点分隔的键名访问嵌套配置：

```toml
[database]
host = "localhost"
[database.connection]
timeout = 30
```

```rust
let host = config.get_string("database.host")?;
let timeout = config.get_int("database.connection.timeout")?;
```

## 快速开始

### 添加依赖

在 `Cargo.toml` 中添加：

```toml
[dependencies]
cmx-utils = { path = "../cmx-utils" }
```

### 基本使用

#### 1. 创建配置管理器

```rust
use cmx_utils::config::{Config, ConfigBuilder, FileSource, EnvSource, CommandLineSource, Priority};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 创建配置构建器
    let mut builder = Config::builder();

    // 添加默认配置文件（优先级 10）
    builder = builder.add_toml_file("config/default.toml", 10)?;

    // 添加生产环境配置文件（优先级 20）
    builder = builder.add_toml_file("config/production.toml", 20)?;

    // 添加 .env 文件
    builder = builder.add_source(FileSource::env_file(".env"));

    // 添加系统环境变量
    builder = builder.add_source(EnvSource::new());

    // 添加命令行参数
    builder = builder.add_source(CommandLineSource::from_args(std::env::args().skip(1)));

    // 构建配置
    let config = builder.build()?;

    // 读取配置值
    let host: String = config.get_string("database.host")?;
    let port: i64 = config.get_int("database.port")?;
    
    Ok(())
}
```

#### 2. 读取配置值

```rust
// 获取字符串值
let host: String = config.get_string("database.host")?;

// 获取整数值
let port: i64 = config.get_int("database.port")?;

// 获取布尔值
let debug: bool = config.get_bool("app.debug")?;

// 获取可选配置（如果不存在返回 None）
let timeout: Option<i64> = config.get_optional("database.timeout")?;

// 获取配置值并设置默认值
let timeout: i64 = config.get_as_or("database.timeout", 30);
```

#### 3. 使用环境变量前缀过滤

```rust
use cmx_utils::config::EnvSource;

// 只加载 APP_ 开头的环境变量
let env_source = EnvSource::with_prefix("APP_");
builder = builder.add_source(env_source);

// 环境变量 APP_HOST 会映射为配置项 HOST
let host = config.get_string("HOST")?;
```

#### 4. 从命令行参数读取配置

```rust
use cmx_utils::config::CommandLineSource;

// 支持两种格式：
// 1. --key=value
// 2. --key value

let args = vec![
    "--host".to_string(),
    "localhost".to_string(),
    "--port=8080".to_string(),
];

let cmd_source = CommandLineSource::from_args(args.into_iter());
builder = builder.add_source(cmd_source);
```

### 配置文件示例

#### TOML 格式 (default.toml)

```toml
[app]
name = "my-app"
version = "1.0.0"
debug = false

[database]
host = "localhost"
port = 5432
name = "mydb"
connection_timeout = 30

[server]
host = "0.0.0.0"
port = 8080
workers = 4
```

#### JSON 格式 (config.json)

```json
{
  "app": {
    "name": "my-app",
    "version": "1.0.0",
    "debug": false
  },
  "database": {
    "host": "localhost",
    "port": 5432,
    "name": "mydb",
    "connection_timeout": 30
  }
}
```

#### .env 格式

```env
# 应用配置
APP_NAME=my-app
APP_DEBUG=true

# 数据库配置
DB_HOST=localhost
DB_PORT=5432
DB_NAME=mydb

# 服务器配置
SERVER_HOST=0.0.0.0
SERVER_PORT=8080
```

## 高级用法

### 1. 全局配置管理器 (ConfigManager)

使用 `ConfigManager` 可以实现配置初始化一次后全局访问，特别适合大型应用或多模块共享配置的场景。

#### 基本使用

```rust
use cmx_utils::config::{ConfigManager, ConfigBuilder, Config};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 应用启动时初始化配置（只调用一次）
    ConfigManager::initialize(|| {
        Config::builder()
            .add_toml_file("config/default.toml", 10)?
            .add_env()
            .build()
    })?;
    
    // 2. 任意位置获取配置
    let host = ConfigManager::global().get_string("database.host")?;
    let port = ConfigManager::global().get_int("database.port")?;
    
    println!("Database: {}:{}", host, port);
    Ok(())
}
```

#### 安全获取配置

```rust
use cmx_utils::config::ConfigManager;

// 方法1: 使用 try_global() 安全获取
if let Some(config) = ConfigManager::try_global() {
    let host = config.get_string("database.host")?;
}

// 方法2: 检查是否已初始化
if ConfigManager::is_initialized() {
    let config = ConfigManager::global();
    // 使用配置
}

// 方法3: 在获取前先初始化
let config = ConfigManager::try_global().unwrap_or_else(|| {
    // 如果未初始化，使用默认配置
    panic!("配置未初始化");
});
```

#### 使用 DefaultConfigLoader 初始化

```rust
use cmx_utils::config::{ConfigManager, DefaultConfigLoader};

// 使用默认配置加载器初始化
ConfigManager::initialize(|| {
    DefaultConfigLoader::new("config")
        .with_env_prefix("APP_")
        .load()
})?;

// 之后可以在任意位置访问
let db_host = ConfigManager::global().get_string("database.host")?;
```

#### 在多模块中使用

```rust
// config.rs - 配置初始化模块
pub fn init_config() -> Result<(), Box<dyn std::error::Error>> {
    ConfigManager::initialize(|| {
        Config::builder()
            .add_toml_file("config/default.toml", 10)?
            .build()
    })?;
    Ok(())
}

// database.rs - 数据库模块
pub fn create_connection() -> Result<Connection, Box<dyn std::error::Error>> {
    let host = ConfigManager::global().get_string("database.host")?;
    let port = ConfigManager::global().get_int("database.port")?;
    // 创建连接...
    Ok(())
}

// main.rs - 应用入口
fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_config()?;
    let conn = create_connection()?;
    Ok(())
}
```

### 3. 自定义配置来源

```rust
use cmx_utils::config::{ConfigSource, Priority};
use cmx_utils::config::{ConfigStore, ConfigValue};
use cmx_utils::config::ConfigResult;

struct CustomSource {
    data: HashMap<String, String>,
}

impl ConfigSource for CustomSource {
    fn load(&self) -> ConfigResult<ConfigStore> {
        let mut store = ConfigStore::new();
        for (key, value) in &self.data {
            store.insert(key.clone(), ConfigValue::new_string(value.clone()));
        }
        Ok(store)
    }

    fn name(&self) -> &str {
        "custom"
    }

    fn priority(&self) -> Priority {
        Priority::COMMAND_LINE
    }
}
```

### 3. 配置热重载

```rust
// 重新加载配置
config.reload()?;

// 检查配置是否已加载
if config.is_loaded() {
    // 使用配置
}
```

### 4. 配置验证

```rust
// 检查配置项是否存在
if config.has("database.host") {
    // 配置项存在
}

// 获取所有配置键
let keys: Vec<&String> = config.keys().collect();

// 获取配置项数量
let count = config.len();
```

### 5. 类型转换

```rust
use cmx_utils::config::{ConfigValue, FromConfigValue};

// 从 ConfigValue 转换为具体类型
let value = ConfigValue::new_string("42");
let num: i64 = value.try_into_type()?;

// 自定义类型转换
impl FromConfigValue for MyType {
    fn from_config_value(value: &ConfigValue) -> ConfigResult<Self> {
        // 实现自定义转换逻辑
    }
}
```

## 错误处理

配置管理模块提供详细的错误信息：

```rust
use cmx_utils::config::{ConfigError, ConfigResult};

match config.get_string("database.host") {
    Ok(host) => println!("Database host: {}", host),
    Err(ConfigError::KeyNotFound { key }) => {
        eprintln!("配置项 '{}' 不存在", key);
    }
    Err(ConfigError::TypeConversionError { key, target_type }) => {
        eprintln!("配置项 '{}' 无法转换为类型 '{}'", key, target_type);
    }
    Err(ConfigError::FileNotFound { path }) => {
        eprintln!("配置文件不存在: {:?}", path);
    }
    Err(e) => {
        eprintln!("其他错误: {}", e);
    }
}
```

## 最佳实践

### 1. 配置文件组织

```
project/
├── config/
│   ├── default.toml      # 默认配置（优先级 10）
│   ├── development.toml  # 开发环境配置（优先级 20）
│   ├── production.toml   # 生产环境配置（优先级 30）
│   └── test.toml         # 测试环境配置（优先级 40）
├── .env                  # 本地环境变量
└── .env.example          # 环境变量示例
```

### 2. 环境变量命名规范

- 使用大写字母和下划线
- 使用有意义的前缀（如 `APP_`、`DB_`）
- 提供默认值或文档说明

```env
# 应用配置
APP_NAME=my-app
APP_ENV=production
APP_DEBUG=false

# 数据库配置
DB_HOST=localhost
DB_PORT=5432
DB_NAME=mydb
DB_USER=user
DB_PASSWORD=secret
```

### 3. 敏感信息处理

- 不要在配置文件中存储敏感信息
- 使用环境变量传递敏感配置
- 在 `.gitignore` 中添加 `.env` 文件

```gitignore
# 环境变量文件
.env
.env.local
.env.*.local

# 配置文件（如果包含敏感信息）
config/production.toml
```

### 4. 配置验证

在应用启动时验证必需的配置项：

```rust
fn validate_config(config: &Config) -> ConfigResult<()> {
    // 验证必需配置项
    config.get_string("database.host")?;
    config.get_int("database.port")?;
    config.get_string("database.name")?;
    
    // 验证配置值范围
    let port = config.get_int("server.port")?;
    if port < 1024 || port > 65535 {
        return Err(ConfigError::BuildError {
            message: "端口号必须在 1024-65535 之间".to_string(),
        });
    }
    
    Ok(())
}
```

## 测试

### 运行测试

```bash
# 运行所有测试
cargo test --package cmx-utils

# 运行特定模块测试
cargo test --package cmx-utils config

# 运行特定测试用例
cargo test --package cmx-utils test_toml_parser
```

### 测试覆盖率

```bash
# 生成测试覆盖率报告
cargo tarpaulin --out Html
```

## 性能考虑

1. **配置加载时机**：在应用启动时一次性加载所有配置，避免运行时重复加载
2. **内存占用**：配置数据存储在内存中，访问速度快
3. **线程安全**：`ConfigManager` 实现了 `Send + Sync`，可在多线程环境中安全使用

## 常见问题

### Q: 如何使用全局配置管理器？

A: 使用 `ConfigManager` 可以实现配置初始化一次后全局访问。

```rust
use cmx_utils::config::{ConfigManager, Config};

// 应用启动时初始化（只调用一次）
ConfigManager::initialize(|| {
    Config::builder()
        .add_toml_file("config/default.toml", 10)?
        .add_env()
        .build()
})?;

// 任意位置获取配置
let host = ConfigManager::global().get_string("database.host")?;
```

### Q: 如何处理配置项不存在的情况？

A: 使用 `get_optional` 方法返回 `Option<T>`，或使用 `get_as_or` 提供默认值。

```rust
// 方法1: 使用 Option
let timeout: Option<i64> = config.get_optional("database.timeout")?;

// 方法2: 使用默认值
let timeout: i64 = config.get_as_or("database.timeout", 30);
```

### Q: 如何支持配置热重载？

A: 目前需要手动调用 `reload()` 方法。未来版本计划支持文件监听自动重载。

```rust
// 重新加载配置
config.reload()?;
```

### Q: 如何处理复杂的嵌套配置？

A: 使用点分隔的键名访问嵌套配置，或直接获取对象类型。

```rust
// 方法1: 点分隔键名
let host = config.get_string("database.connection.host")?;

// 方法2: 获取整个对象
let db_config = config.get("database")?;
if let Some(obj) = db_config.as_object() {
    let host = obj.get("host")?;
}
```

### Q: TOML配置文件的优先级如何指定？

A: 使用 `add_toml_file` 方法时，第二个参数指定优先级（0-100），数值越大优先级越高。

```rust
// 优先级 10（最低）
builder = builder.add_toml_file("config/default.toml", 10)?;

// 优先级 20（中等）
builder = builder.add_toml_file("config/production.toml", 20)?;

// 优先级 30（最高）
builder = builder.add_toml_file("config/custom.toml", 30)?;
```

## 许可证

MIT License

## 贡献指南

欢迎提交 Issue 和 Pull Request！

### 开发环境设置

```bash
# 克隆仓库
git clone https://github.com/your-org/cmx-container.git

# 进入项目目录
cd cmx-container/crates/libs/cmx-utils

# 运行测试
cargo test

# 运行格式化
cargo fmt

# 运行 lint
cargo clippy
```

## 更新日志

### v0.2.0 (2026-03-10)

- 新增 `ConfigManager` 全局配置管理器
- 支持配置初始化一次后全局访问
- 提供 `initialize()`、`global()`、`try_global()`、`is_initialized()` 方法
- 适用于大型应用和多模块共享配置场景

### v0.1.0 (2026-03-09)

- 初始版本发布
- 支持 TOML、JSON、.env 格式
- 支持多种配置来源
- 实现优先级机制
- **重要变更**：TOML配置文件优先级由用户指定，不再依赖文件名
- 提供类型安全的配置访问接口
