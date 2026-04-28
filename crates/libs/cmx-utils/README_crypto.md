# CMX 加解密模块

一个基于 AES-256-GCM 的企业级字段加解密库，提供透明的字段级加解密能力，适用于数据库敏感信息保护。

## 设计思想

### 核心理念

1. **企业级安全**：采用 AES-256-GCM 认证加密，密文自带认证标签，防篡改
2. **透明加解密**：对业务代码无感，通过声明式配置自动完成加解密
3. **向后兼容**：非加密格式的值原样返回，兼容已有的明文数据
4. **零侵入扩展**：通过 trait 扩展机制，无需修改现有 CRUD 代码

### 加密格式

加密后的密文格式为 `ENC(BASE64_NONCE.BASE64_CIPHERTEXT)`，其中：
- `NONCE`：12 字节随机数，每次加密不同
- `CIPHERTEXT`：包含密文和 16 字节认证标签
- `ENC(...)` 前缀用于标识密文，非加密值原样返回

## 功能特性

### 1. AES-256-GCM 认证加密

```rust
use cmx_utils::crypto::CryptoService;

// 初始化（通常在应用启动时调用一次）
CryptoService::init("your-32-byte-secret-key-here")?;

// 加密
let ciphertext = crypto.encrypt("sensitive data")?;
// 输出: ENC(BASE64_NONCE.BASE64_CIPHERTEXT)

// 解密
let plaintext = crypto.decrypt(&ciphertext)?;
// 输出: sensitive data
```

### 2. 全局单例访问

```rust
use cmx_utils::crypto::CryptoService;

// 获取全局实例（需先调用 init）
let crypto = CryptoService::global()?;

// 使用全局实例加解密
let encrypted = crypto.encrypt("password")?;
let decrypted = crypto.decrypt(&encrypted)?;
```

### 3. 环境变量初始化

```rust
use cmx_utils::crypto::CryptoService;

// 从环境变量 CMX_ENCRYPT_KEY 读取密钥并初始化
CryptoService::init_from_env()?;
```

### 4. 向后兼容

```rust
// 解密非 ENC(...) 格式的值时，原样返回（兼容明文数据）
let crypto = CryptoService::global()?;
let result = crypto.decrypt("plaintext-value")?;
// 输出: plaintext-value（未加密的值直接返回）
```

### 5. 密钥长度处理

```rust
// 密钥不足 32 字节时，尾部填充 0x00
// 密钥超过 32 字节时，截断到 32 字节
CryptoService::init("short-key")?;  // 填充为 32 字节
CryptoService::init("very-long-secret-key-over-32-bytes")?;  // 截断为 32 字节
```

## 在数据库 CRUD 中使用

### 声明加密字段

在任何 BMC（Base Model Controller）中覆写 `encrypted_fields()` 方法即可启用字段加密：

```rust
use cmx_database::crud::DbBmc;

pub struct SysDatasourceBmc;

impl DbBmc for SysDatasourceBmc {
    const TABLE: &'static str = "cmx_sys_datasource";
    const PK_COLUMN: &'static str = "id";
    fn has_timestamps() -> bool { true }
    fn has_owner_id() -> bool { false }

    /// 声明 db_url 字段需要加密存储
    fn encrypted_fields() -> &'static [&'static str] {
        &["db_url"]
    }
}
```

### 工作原理

一旦 BMC 声明了 `encrypted_fields()`，`GenericCrudService` 会自动在以下时机处理加解密：

```
写入操作（create / update）：
  用户输入明文 → 自动加密 → 存入数据库密文

读取操作（get / list / page）：
  数据库密文 → 自动解密 → 返回用户明文
```

### 支持的 BMC

以下 BMC 已启用 db_url 加密：

| BMC | 加密字段 | 说明 |
|-----|---------|------|
| `SysDatasourceBmc` | `db_url` | 数据源连接 URL |

### 为其他表启用加密

```rust
// 示例：为用户表启用密码字段加密
pub struct UserBmc;

impl DbBmc for UserBmc {
    const TABLE: &'static str = "sys_user";
    const PK_COLUMN: &'static str = "id";

    fn encrypted_fields() -> &'static [&'static str] {
        &["password", "secret_question"]
    }
}
```

## API 参考

### CryptoService

```rust
pub struct CryptoService;

impl CryptoService {
    /// 使用指定密钥初始化全局实例
    pub fn init(key: &str) -> Result<(), Error>

    /// 从环境变量 CMX_ENCRYPT_KEY 初始化
    pub fn init_from_env() -> Result<(), Error>

    /// 获取全局实例（需先调用 init）
    pub fn global() -> Result<&'static CryptoService, Error>

    /// 加密明文，返回 ENC(NONCE.CIPHERTEXT) 格式字符串
    pub fn encrypt(&self, plaintext: &str) -> Result<String, Error>

    /// 解密密文，非 ENC(...) 格式值原样返回
    pub fn decrypt(&self, ciphertext: &str) -> Result<String, Error>
}
```

### DbBmc trait 扩展

```rust
pub trait DbBmc {
    // ... 现有方法不变 ...

    /// 声明需要加密存储的字段名列表
    /// 默认返回空数组，表示无加密字段（向后兼容）
    fn encrypted_fields() -> &'static [&'static str] {
        &[]
    }
}
```

## 快速开始

### 1. 设置加密密钥

通过环境变量设置密钥（推荐）：

```bash
# Linux/macOS
export CMX_ENCRYPT_KEY="your-32-byte-secret-key-here!"

# Windows (PowerShell)
$env:CMX_ENCRYPT_KEY="your-32-byte-secret-key-here!"
```

或在应用代码中初始化：

```rust
use cmx_utils::crypto::CryptoService;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 在应用启动时初始化加密服务
    CryptoService::init("your-32-byte-secret-key-here!")?;
    // ... 其余代码 ...
    Ok(())
}
```

### 2. 在 BMC 中声明加密字段

```rust
impl DbBmc for YourBmc {
    const TABLE: &'static str = "your_table";
    const PK_COLUMN: &'static str = "id";

    fn encrypted_fields() -> &'static [&'static str] {
        &["password", "api_key", "db_url"]
    }
}
```

### 3. 使用 GenericCrudService

加密字段会自动处理，无需额外代码：

```rust
// 创建时自动加密
service.create(data).await?;

// 更新时自动加密
service.update(id, data).await?;

// 查询时自动解密
let record = service.get(id).await?;
// record.password 已是解密后的明文

// 列表查询时自动解密
let list = service.list(filter).await?;
// list 中所有记录的加密字段都已解密
```

## 安全性说明

### AES-256-GCM

- **算法**：AES-256-GCM 是 AES-256 的伽罗瓦计数器模式，提供加密和认证
- **密钥长度**：256 位（32 字节）
- **Nonce**：96 位（12 字节）随机数，每条消息必须唯一
- **认证标签**：128 位（16 字节），防篡改

### 密钥管理建议

1. **生产环境**：使用环境变量或密钥管理服务（KMS）注入密钥，不要硬编码
2. **密钥轮换**：定期更换密钥，更换前将旧数据重新加密
3. **密钥强度**：使用随机生成的 32 字节密钥，不要使用弱密钥
4. **日志保护**：确保日志中不打印明文敏感信息

## 最佳实践

### 1. 密钥初始化时机

在应用启动的最早阶段初始化加密服务：

```rust
// main.rs
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 最先初始化加密服务
    CryptoService::init_from_env()
        .map_err(|e| eprintln!("加密服务初始化失败: {}", e));

    // 然后初始化其他组件
    init_datasources().await?;
    init_api_server().await?;

    Ok(())
}
```

### 2. 加密字段选择

只对真正需要加密的字段启用加密：

```rust
// 推荐：只加密真正敏感的信息
fn encrypted_fields() -> &'static [&'static str] {
    &["password", "secret_key", "api_token", "db_url"]
}

// 不推荐：过度加密影响性能
fn encrypted_fields() -> &'static [&'static str] {
    &["name", "email", "phone"]  // 这些通常不需要加密
}
```

### 3. 向后兼容

如果数据库中已有明文数据，首次部署时会正常工作（解密时原样返回），但建议：

```bash
# 1. 部署新代码（解密兼容明文）
# 2. 迁移现有明文数据为密文
# 3. 确保所有数据都是密文格式
```

## 错误处理

```rust
use cmx_utils::crypto::{CryptoService, Error};

fn main() {
    match CryptoService::init("key") {
        Ok(()) => println!("加密服务初始化成功"),
        Err(Error::CryptoFailed(msg)) => eprintln!("加密失败: {}", msg),
        Err(Error::KeyEmpty) => eprintln!("密钥不能为空"),
    }
}
```

## 测试

```bash
# 运行加解密模块测试
cargo test --package cmx-utils crypto

# 运行所有测试
cargo test --package cmx-utils

# 运行特定测试用例
cargo test --package cmx-utils test_encrypt_decrypt
```

## 常见问题

### Q: 如何处理密钥未配置的情况？

A: `init_from_env()` 会在密钥为空时返回错误。建议在应用启动时检查：

```rust
if let Err(e) = CryptoService::init_from_env() {
    tracing::warn!("加密服务未配置: {}", e);
}
```

### Q: 加密后的数据占用多少空间？

A: 加密后的数据比原始数据多约 40 字节：`ENC(` 前缀（4字节）+ BASE64(NONCE) ~16字节 + BASE64(CIPHERTEXT) ~原始数据长度 + `)` 结尾（1字节）。

### Q: 可以加密哪些类型的数据？

A: 当前版本只支持字符串类型。数值、布尔等其他类型需要先转换为字符串再加密。

### Q: 如何为已有表添加加密字段？

A:
1. 在 BMC 中声明 `encrypted_fields()`
2. 部署代码（读取时明文数据会自动原样返回）
3. 后台迁移：将现有明文字段值读取后重新写入（自动加密存储）
4. 确认所有数据都是密文格式

## 更新日志

### v0.1.0 (2026-04-27)

- 新增 `CryptoService` AES-256-GCM 加解密服务
- 支持 `CryptoService::init()` 和 `CryptoService::init_from_env()` 初始化
- 支持全局单例访问 `CryptoService::global()`
- 加密输出格式：`ENC(BASE64_NONCE.BASE64_CIPHERTEXT)`
- 解密时非 `ENC(...)` 格式值原样返回（向后兼容明文数据）
- `DbBmc` trait 新增 `encrypted_fields()` 方法声明加密字段
- `GenericCrudService` 在 create/update 时自动加密指定字段
- `GenericCrudService` 在 get/list/page 时自动解密指定字段
