# cmx-utils

> CMX 工具库，提供常用的工具函数和配置管理功能。

## 快速开始

### 安装

```toml
[dependencies]
cmx-utils = "0.1.0"
```

### 核心示例

```rust
use cmx_utils::config::{Config, ConfigManager};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    cmx_utils::ConfigManager::initialize(|| {
        cmx_utils::Config::builder()
            .add_toml_file("config/default.toml", 10)?
            .add_env()
            .build()
    })?;

    let host = cmx_utils::ConfigManager::global().get_string("database.host")?;
    Ok(())
}
```

## 核心功能与特性

| 功能 | 说明 |
|------|------|
| 配置管理 | 支持从 TOML/JSON/YAML 文件、环境变量、命令行参数等多种来源加载配置 |
| 加密解密 | 基于 AES-256-GCM 实现的对称加密功能 |
| ID 生成 | 提供 UUID 和雪花算法 ID 生成功能 |
| ZIP 压缩/解压 | 支持文件和目录的压缩与解压操作 |
| Base64 编码 | URL 安全的 Base64 编码与解码功能 |
| 时间处理 | 便捷的时间格式化和解析工具 |

## 模块结构

```
cmx-utils
├── src/
│   ├── lib.rs          # 库入口
│   ├── b64.rs          # Base64 编码/解码
│   ├── config/         # 配置管理模块
│   │   ├── mod.rs
│   │   ├── config_impl.rs
│   │   ├── error.rs
│   │   ├── source.rs
│   │   └── value.rs
│   ├── crypto/         # 加密解密模块
│   │   ├── mod.rs
│   │   ├── aes_gcm.rs
│   │   ├── cipher.rs
│   │   ├── error.rs
│   │   └── service.rs
│   ├── id/             # ID 生成模块
│   │   ├── mod.rs
│   │   ├── snowflake.rs
│   │   └── uuid_gen.rs
│   ├── time.rs         # 时间处理
│   └── zip/            # ZIP 压缩/解压模块
│       ├── mod.rs
│       ├── compressor.rs
│       ├── error.rs
│       └── extractor.rs
└── Cargo.toml
```

## 主要模块说明

### 配置管理模块 (`config`)

支持多种配置来源，优先级由低到高依次为：TOML 文件、.env 文件、环境变量、命令行参数。

### 加密解密模块 (`crypto`)

提供 AES-256-GCM 加密算法的实现，支持多种扩展算法，默认算法为 AES-256-GCM。

### ID 生成模块 (`id`)

支持 UUID 和雪花算法两种 ID 生成方式。

### ZIP 模块 (`zip`)

使用 `ZipCompressor` 压缩目录或文件，使用 `ZipExtractor` 解压 ZIP 文件至指定目录。

## 使用指南

### 一、配置管理模块 (`config`)

#### 1.1 基础配置加载

```rust
use cmx_utils::config::{Config, ConfigManager, CommandLineSource};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化全局配置管理器
    ConfigManager::initialize(|| {
        Config::builder()
            // 添加 TOML 配置文件（优先级最低）
            .add_toml_file("config/default.toml", 10)?
            // 添加环境变量（中等优先级）
            .add_env()
            // 添加命令行参数（最高优先级）
            .add_command_line(std::env::args().skip(1))
            .build()
    })?;

    // 读取配置值
    let host = ConfigManager::global().get_string("database.host")?;
    let port = ConfigManager::global().get_int("database.port")?;
    let timeout = ConfigManager::global().get_duration("database.timeout")?;

    Ok(())
}
```

#### 1.2 添加 JSON 配置文件

```rust
use cmx_utils::config::Config;

let config = Config::builder()
    .add_json_file("config/app.json", 10)?
    .add_env()
    .build()?;
```

#### 1.3 添加 YAML 配置文件

```rust
use cmx_utils::config::Config;

let config = Config::builder()
    .add_yaml_file("config/settings.yaml", 10)?
    .build()?;
```

#### 1.4 读取配置为强类型结构体

```rust
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct DatabaseConfig {
    host: String,
    port: u16,
    username: String,
    password: String,
}

#[derive(Debug, Deserialize)]
struct AppConfig {
    database: DatabaseConfig,
    debug: bool,
}

let app_config = ConfigManager::global().get::<AppConfig>("app")?;
println!("Database host: {}", app_config.database.host);
```

#### 1.5 配置来源优先级

```
1. TOML/JSON/YAML 配置文件（最低优先级）
2. .env 文件（通过 dotenvy 加载到环境变量）
3. 系统环境变量
4. 命令行参数（最高优先级）
```

### 二、加密解密模块 (`crypto`)

#### 2.1 基础加密解密

```rust
use cmx_utils::crypto::{CryptoService, Aes256Gcm};

// 方式一：使用默认算法（AES-256-GCM）初始化
CryptoService::init("my-secret-key-32-bytes-long!!");

// 方式二：手动指定算法
CryptoService::init_with(Aes256Gcm::new("my-key"));

// 加密
let encrypted = CryptoService::global()?.encrypt("hello world")?;
// 输出: ENC(AESGCM(NONCE.CIPHERTEXT))

// 解密
let decrypted = CryptoService::global()?.decrypt(&encrypted)?;
assert_eq!(decrypted, "hello world");
```

#### 2.2 批量加密

```rust
let messages = vec!["message1", "message2", "message3"];
let encrypted_messages: Result<Vec<String>, _> = messages
    .iter()
    .map(|msg| CryptoService::global()?.encrypt(msg))
    .collect();

for em in encrypted_messages? {
    println!("Encrypted: {}", em);
}
```

#### 2.3 扩展新加密算法

```rust
use cmx_utils::crypto::{Cipher, CipherMeta, CryptoService};

pub struct ChaCha20PolyCipher {
    key: Vec<u8>,
}

impl Cipher for ChaCha20PolyCipher {
    fn meta(&self) -> CipherMeta {
        CipherMeta {
            name: "ChaCha20-Poly1305".to_string(),
            prefix: "CHACHA(".to_string(),
        }
    }

    fn encrypt(&self, plaintext: &str) -> Result<String, Error> {
        // 实现加密逻辑
    }

    fn decrypt(&self, ciphertext: &str) -> Result<String, Error> {
        // 实现解密逻辑
    }
}

// 注册并使用
CryptoService::init_with(ChaCha20PolyCipher { key: vec![] });
```

### 三、ID 生成模块 (`id`)

#### 3.1 雪花 ID 生成（全局单例）

```rust
use cmx_utils::id::snowflake_id;

// 生成 i64 类型的雪花 ID
let id = snowflake_id();
println!("Snowflake ID: {}", id);

// 生成字符串类型的雪花 ID
let id_str = cmx_utils::id::snowflake_id_str();
println!("Snowflake ID String: {}", id_str);
```

#### 3.2 雪花 ID 生成器（自定义节点 ID）

```rust
use cmx_utils::id::SnowflakeGenerator;

// 创建指定节点 ID 的生成器
let node_id: i64 = 1024;
let generator = SnowflakeGenerator::new(node_id);

// 生成 ID
let id = generator.next_id();
let id_str = generator.next_id_str();
println!("Generated ID: {}", id_str);
```

#### 3.3 UUID 生成器

```rust
use cmx_utils::id::UuidGenerator;

let generator = UuidGenerator::new();

// 生成 v4 UUID
let uuid_v4 = generator.generate_v4();
println!("UUID v4: {}", uuid_v4);

// 生成带前缀的 UUID
let prefixed_uuid = generator.generate_with_prefix("user_");
println!("Prefixed UUID: {}", prefixed_uuid);
```

### 四、ZIP 压缩/解压模块 (`zip`)

#### 4.1 压缩目录

```rust
use cmx_utils::zip::{ZipCompressor, ZipExtractor};
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 压缩目录（递归）
    ZipCompressor::compress_dir(
        Path::new("data"),
        Path::new("output.zip"),
        6,  // 压缩级别 0-9
    )?;

    // 压缩单个文件
    ZipCompressor::compress_file(
        Path::new("document.txt"),
        Path::new("document.zip"),
        6,
    )?;

    Ok(())
}
```

#### 4.2 解压 ZIP 文件

```rust
use cmx_utils::zip::ZipExtractor;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 解压到指定目录
    ZipExtractor::extract(
        Path::new("output.zip"),
        Path::new("extracted"),
    )?;

    // 解压并获取文件列表
    let files = ZipExtractor::extract_with_list(
        Path::new("output.zip"),
        Path::new("extracted"),
    )?;

    for file in files {
        println!("Extracted: {}", file);
    }

    Ok(())
}
```

### 五、Base64 编码模块 (`b64`)

#### 5.1 URL 安全 Base64 编码解码

```rust
use cmx_utils::b64::{b64u_encode, b64u_decode, b64u_decode_to_string};

// 编码
let original = "Hello, World! 你好世界！";
let encoded = b64u_encode(original);
println!("Encoded: {}", encoded);
// 输出: SGVsbG8sIFdvcmxkISDiIU1lZ2F6adB1

// 解码为字节数组
let decoded_bytes = b64u_decode(&encoded)?;
println!("Decoded bytes: {:?}", decoded_bytes);

// 解码为 UTF-8 字符串
let decoded_string = b64u_decode_to_string(&encoded)?;
println!("Decoded string: {}", decoded_string);
```

#### 5.2 处理二进制数据

```rust
use cmx_utils::b64::b64u_encode;

// 编码字节数组
let binary_data: &[u8] = &[0x00, 0xFF, 0x12, 0x34];
let encoded = b64u_encode(binary_data);
println!("Binary encoded: {}", encoded);
```

### 六、时间处理模块 (`time`)

#### 6.1 获取当前时间

```rust
use cmx_utils::time::{now_utc, format_time, Rfc3339};

// 获取当前 UTC 时间
let now = now_utc();
println!("Current UTC: {:?}", now);

// 格式化为 RFC3339 字符串
let formatted = format_time(now);
println!("Formatted: {}", formatted);
// 输出: "2024-01-15T10:30:00Z"
```

#### 6.2 时间偏移计算

```rust
use cmx_utils::time::{now_utc_plus_sec_str, now_utc};

// 计算 1 小时后的时间
let future = now_utc_plus_sec_str(3600.0);
println!("1 hour later: {}", future);

// 计算 30 天前的时间
let past = now_utc_plus_sec_str(-30 * 24 * 3600.0);
println!("30 days ago: {}", past);
```

#### 6.3 解析时间字符串

```rust
use cmx_utils::time::{parse_utc, Rfc3339};

let time_str = "2024-01-15T10:30:00Z";
let parsed = parse_utc(time_str)?;

println!("Parsed: {:?}", parsed);
println!("Year: {}", parsed.year());
println!("Month: {}", parsed.month());
println!("Day: {}", parsed.day());
```

### 七、组合使用示例

```rust
use cmx_utils::{
    ConfigManager, CryptoService,
    id::{snowflake_id_str, UuidGenerator},
    zip::{ZipCompressor, ZipExtractor},
    b64::b64u_encode,
    time::{now_utc, format_time},
};
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 初始化配置
    ConfigManager::initialize(|| {
        Config::builder()
            .add_toml_file("config/default.toml", 10)?
            .add_env()
            .build()
    })?;

    // 2. 初始化加密服务
    let encryption_key = ConfigManager::global().get_string("app.encryption_key")?;
    CryptoService::init(&encryption_key);

    // 3. 生成唯一 ID
    let record_id = snowflake_id_str();
    let trace_id = UuidGenerator::new().generate_v4().to_string();

    // 4. 加密敏感数据并编码
    let sensitive_data = "user_password_123";
    let encrypted = CryptoService::global()?.encrypt(sensitive_data)?;
    let encoded = b64u_encode(&encrypted);

    // 5. 压缩并记录
    let timestamp = format_time(now_utc());
    println!("[{}] Record {} created, encrypted trace: {}", timestamp, record_id, encoded);

    // 6. 备份配置文件
    ZipCompressor::compress_file(
        Path::new("config/production.toml"),
        Path::new("backup/config_backup.zip"),
        9,
    )?;

    Ok(())
}
```
