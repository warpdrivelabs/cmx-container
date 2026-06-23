# cmx-auth

> 企业级统一认证基础设施模块，提供 JWT 双令牌、Refresh Token Rotation、Argon2id 密码哈希、OAuth2 授权码 + PKCE、会话管理、API Key 两层缓存、密钥轮换等完整认证能力。

[![Version](https://img.shields.io/badge/version-0.1.9-blue.svg)]()
[![Edition](https://img.shields.io/badge/rust--edition-2024-orange.svg)]()
[![Authors](https://img.shields.io/badge/authors-skylake%40pansoft.com-lightgrey.svg)]()

`cmx-auth` 是 cmx 平台的认证基础设施层，实现 `cmx-traits::auth::AuthService` trait，
整合 JWT、Argon2、Redis 缓存、OAuth2 Provider、Prometheus 指标等能力，
为上层业务（HTTP 中间件、RPC 鉴权、SSO 网关）提供统一的认证 API。

> **架构说明**：`cmx-auth` 不直接依赖 `cmx-iam`，通过 `UserAuthQuery` trait 解耦用户数据查询，
> 由 `cmx-biz` 在运行期注入实现，保证认证模块可独立测试与复用。

---

## 快速开始

### 安装

在 `Cargo.toml` 中添加依赖（版本跟随 workspace）：

```toml
[dependencies]
# 内部依赖 - 认证基础设施
cmx-auth = { workspace = true }
```

### 核心示例

```rust
use std::sync::Arc;
use cmx_auth::{AuthServiceImpl, AuthConfig};
use cmx_buffer::CacheManager;
use cmx_traits::auth::{AuthService, Credentials, DeviceInfo};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 准备缓存管理器（Redis 连接池 + 本地缓存）
    let cache = CacheManager::new(/* redis 配置 */).await?;

    // 2. 加载认证配置（可从 TOML 反序列化）
    let config = AuthConfig::default();

    // 3. 注入用户数据查询实现（由 cmx-biz 提供）
    let user_query: Arc<dyn cmx_traits::auth::UserAuthQuery> = /* ... */;

    // 4. 构建 AuthService
    let auth_service = AuthServiceImpl::new(cache, config, user_query)?;

    // 5. 用户名密码登录
    let token_pair = auth_service
        .authenticate(
            Credentials::Password {
                username: "admin".to_string(),
                password: "P@ssw0rd!".to_string(),
            },
            Some(DeviceInfo {
                device_type: "web".to_string(),
                device_id: "browser-001".to_string(),
                ip: Some("127.0.0.1".to_string()),
                user_agent: Some("Mozilla/5.0".to_string()),
            }),
        )
        .await?;

    println!("Access Token: {}", token_pair.access_token);
    println!("Refresh Token: {}", token_pair.refresh_token);

    Ok(())
}
```

---

## 核心功能与特性

| 功能 | 说明 |
|------|------|
| JWT 双令牌 | Access Token（短 TTL，默认 30 分钟）+ Refresh Token（长 TTL，默认 7 天），分离权限与刷新职责 |
| Refresh Token Rotation | 使用 Redis Lua 脚本原子"检查旧 jti → 删除旧 token"，防止并发重放攻击 |
| Argon2id 密码哈希 | 内存/时间/并行度可配置（默认 64MB / 3 轮 / 4 并行），抗 GPU/ASIC 破解 |
| 密码策略校验 | 强制长度 ≥ 8、大小写 + 数字 + 特殊字符组合，硬编码默认策略 |
| 密码历史校验 | 防止密码复用，记录历史哈希并校验新密码是否已使用 |
| OAuth2 授权码 + PKCE | 实现 RFC 6749 授权码流程 + RFC 7636 PKCE 扩展，支持授权码一次性消费 |
| 第三方 OAuth2 Provider | 内置 Google / GitHub，支持 generic 通用 Provider，含账号关联/自动注册 |
| 会话管理 | SSO 互踢、最大会话数限制、空闲超时清理、心跳保活、在线用户统计 |
| API Key 认证 | M2M 无状态认证，两层缓存（ApiKeyEntity + AuthContext），TTL 60 秒 |
| 密钥轮换宽限期 | 支持 `kid` 标识 + `legacy_public_keys` 列表，验签时回退到旧密钥 |
| Token 黑名单 | Access Token 撤销加入 Redis 黑名单（TTL = 剩余有效期），Pub/Sub 广播失效 |
| 登录失败锁定 | 连续失败 N 次（默认 5）锁定账号 M 秒（默认 900 秒），含时序攻击防护 |
| Prometheus 指标 | 7 个指标：登录总数/失败数、Token 验证耗时、活跃会话、在线用户、撤销数、API Key 验证数 |
| Tracing 可观测性 | 关键路径（登录、刷新、OAuth2 回调）创建 span，结构化日志含 user_id / device |
| 审计日志 | 集成 `cmx-audit`，记录登录成功/失败、Token 签发/撤销、密码修改、OAuth2 绑定/解绑 |
| 静态 API Key 导入 | 从 TOML 配置加载 API Key，启动时自动 SHA256 哈希后持久化到数据库 |
| 超管账号初始化 | 启动时自动创建/同步超管账号，配置为密码唯一真源 |

### 可选 Features

当前 `cmx-auth` 未定义独立的 cargo features，所有功能默认启用。
JWT 算法（RS256 / HS256）、OAuth2 Provider、API Key 等均通过运行期配置切换。

---

## 模块结构

```text
cmx-auth
├── src
│   ├── lib.rs                  # 模块导出与公共 API re-export
│   ├── auth_service_impl.rs    # AuthService trait 实现（整合所有子模块）
│   ├── config.rs               # AuthConfig 及子配置（JWT/Token/Argon2/Session/Cache/OAuth2）
│   ├── error.rs                # AuthInfraError 错误类型（保留完整错误链）
│   ├── metrics.rs              # Prometheus 指标定义与注册
│   ├── api_key/                # API Key 认证模块
│   │   ├── mod.rs              #   模块导出
│   │   ├── entity.rs           #   ApiKeyEntity 实体定义
│   │   └── manager.rs          #   ApiKeyManager（SHA256 校验 + Redis 缓存）
│   ├── jwt/                    # JWT 编解码模块
│   │   ├── mod.rs              #   模块导出
│   │   ├── claims.rs           #   AccessClaims / RefreshClaims 定义
│   │   └── encoder.rs          #   JwtManager（RS256/HS256 + 密钥轮换宽限期）
│   ├── oauth2/                 # OAuth2 授权码 + PKCE + 第三方 Provider
│   │   ├── mod.rs              #   模块导出
│   │   ├── flows.rs            #   OAuth2FlowService（authorize/token 三步流程）
│   │   ├── pkce.rs             #   PkceVerifier（code_challenge/code_verifier）
│   │   ├── store.rs            #   OAuth2Store（授权码/State/回调码 Lua 原子消费）
│   │   └── provider/           #   第三方 Provider 抽象与实现
│   │       ├── mod.rs          #     OAuth2Provider trait + 通用结构体
│   │       ├── generic.rs      #     通用 Provider 实现
│   │       ├── google.rs       #     Google Provider 实现
│   │       ├── github.rs       #     GitHub Provider 实现
│   │       ├── registry.rs     #     OAuth2ProviderRegistry（全局注册表）
│   │       └── account_linker.rs #   AccountLinker（第三方账号关联/注册）
│   ├── password/               # 密码处理模块
│   │   ├── mod.rs              #   模块导出
│   │   ├── hasher.rs           #   Argon2Hasher（Argon2id 哈希与校验）
│   │   ├── history.rs          #   PasswordHistory（密码历史记录与查重）
│   │   └── policy.rs           #   PasswordPolicy（密码强度策略校验）
│   ├── policy/                 # 认证策略模式
│   │   ├── mod.rs              #   模块导出
│   │   ├── jwt_policy.rs       #   JwtBearerPolicy（JWT Bearer 策略）
│   │   └── oauth2_policy.rs    #   OAuth2Policy（OAuth2 授权码策略）
│   ├── session/                # 会话管理模块
│   │   ├── mod.rs              #   模块导出
│   │   ├── manager.rs          #   SessionManager（创建/查询/销毁/互踢 + moka 本地缓存）
│   │   └── online.rs           #   OnlineTracker（在线用户集合维护）
│   └── token/                  # Token 生命周期管理
│       ├── mod.rs              #   模块导出
│       ├── blacklist.rs        #   Blacklist（Access Token 黑名单）
│       ├── manager.rs          #   TokenManager（Refresh Token 存储/撤销 + 黑名单）
│       └── rotation.rs         #   RefreshRotation（Lua 原子轮换防重放）
```

### 主要模块说明

#### `auth_service_impl`

`AuthServiceImpl` 是 `AuthService` trait 的唯一实现，整合 `JwtManager`、`Argon2Hasher`、
`TokenManager`、`SessionManager`、`OAuth2Policy`、`PasswordPolicy`、`PasswordHistory`、
`OAuth2Store`、`AccountLinker` 等子模块，对外提供 `authenticate` / `validate_token` /
`refresh_token` / `revoke_token` / `change_password` / `validate_api_key` 等完整认证流程。

#### `jwt`

`JwtManager` 支持 RS256 / HS256 算法，解码时优先使用当前密钥，失败后回退到
`legacy_public_keys` 列表（密钥轮换宽限期）。`AccessClaims` 携带用户身份、角色、权限、
会话 ID、设备类型；`RefreshClaims` 仅含最小必要字段（sub/jti/sid/device）。

#### `token`

`TokenManager` 负责 Refresh Token 的存储（Redis SET + 索引集合）、撤销、黑名单管理。
`RefreshRotation` 通过 Lua 脚本原子执行"检查旧 jti → 删除旧 token → 移除索引"，
消除 exists → revoke → store 三步操作的竞态条件。

#### `oauth2`

实现 RFC 6749 授权码流程 + RFC 7636 PKCE 扩展。`OAuth2Store` 使用 Lua 脚本原子消费
授权码、Provider State、回调授权码，防止并发重放。`OAuth2Provider` trait 抽象第三方
Provider 接口，内置 Google / GitHub，支持 generic 通用 Provider。

#### `api_key`

`ApiKeyManager` 通过 `key_prefix`（前 8 位）查找 SHA256 哈希并比对明文 Key，
提供 M2M 场景下的无状态认证。Redis 缓存 `key_prefix → ApiKeyEntity`，TTL 60 秒，
缓存命中时跳过 DB 查询但仍校验 SHA256（防止缓存被篡改后绕过校验）。

---

## 使用指南

### 一、AuthService 初始化与配置

#### 1.1 从 TOML 加载配置并初始化

`AuthConfig` 实现 `serde::Deserialize`，可直接从 TOML 文件反序列化。

```rust
use std::sync::Arc;
use cmx_auth::{AuthServiceImpl, AuthConfig};
use cmx_buffer::CacheManager;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 从 TOML 文件读取配置
    let config_str = std::fs::read_to_string("config/auth.toml")?;
    let config: AuthConfig = toml::from_str(&config_str)?;

    // 2. 初始化缓存管理器（Redis 连接池）
    let cache = CacheManager::new(/* redis url */).await?;

    // 3. 注入用户数据查询实现（由 cmx-biz 提供）
    let user_query: Arc<dyn cmx_traits::auth::UserAuthQuery> = /* ... */;

    // 4. 构建 AuthService（内部自动创建 JwtManager / TokenManager / SessionManager 等）
    let auth_service = AuthServiceImpl::new(cache, config, user_query)?;

    // 5. 可选：注入审计日志记录器
    let audit_logger: Arc<dyn cmx_audit::AuditLogger> = /* ... */;
    let auth_service = auth_service.with_audit_logger(audit_logger);

    Ok(())
}
```

#### 1.2 配置文件示例（TOML）

```toml
# config/auth.toml
[auth.jwt]
algorithm = "HS256"
secret = "your-hmac-secret-key-here"
issuer = "cmx-auth"
audience = "cmx-platform"

[auth.token]
access_ttl_secs = 1800       # Access Token 30 分钟
refresh_ttl_secs = 604800     # Refresh Token 7 天

[auth.argon2]
memory_cost = 65536           # 64MB
time_cost = 3
parallelism = 4

[auth.session]
single_session_per_device_type = false
max_sessions = 5              # 0 = 不限制
idle_timeout_secs = 86400     # 24 小时
heartbeat_interval_secs = 300 # 5 分钟

[auth.cache]
enable_local_cache = true
local_ttl_secs = 30
local_cache_max_entries = 10000
max_login_attempts = 5
lock_duration_secs = 900

# 静态 API Key（启动时自动导入）
[[auth.static_api_keys]]
key = "cmx_sk_Ab3dEf9hJkLmN2pQrStUvWxYz123456"
service_name = "billing-service"
scopes = ["read:invoices", "write:invoices"]

# 超管账号（启动时自动创建/同步）
[auth.super_admin]
username = "admin"
password = "Admin@2024"
email = "admin@example.com"
roles = ["admin"]
```

#### 1.3 启动时初始化超管与静态 API Key

```rust
use cmx_traits::auth::AuthService;

// 启动时调用，幂等：超管存在则同步密码，API Key 存在则覆盖
auth_service.ensure_super_admin().await?;
auth_service.import_static_api_keys().await?;

// 启动后台清理任务（清理过期会话）
auth_service.start_cleanup_task().await;
```

---

### 二、用户名密码登录

#### 2.1 基本登录流程

```rust
use cmx_traits::auth::{AuthService, Credentials, DeviceInfo};

// 用户名密码登录
let token_pair = auth_service
    .authenticate(
        Credentials::Password {
            username: "admin".to_string(),
            password: "P@ssw0rd!".to_string(),
        },
        Some(DeviceInfo {
            device_type: "web".to_string(),
            device_id: "browser-001".to_string(),
            ip: Some("192.168.1.100".to_string()),
            user_agent: Some("Mozilla/5.0".to_string()),
        }),
    )
    .await?;

// token_pair 包含 access_token / refresh_token / 过期时间
println!("Access Token 过期时间: {}", token_pair.access_expires_at);
println!("Refresh Token 过期时间: {}", token_pair.refresh_expires_at);
println!("Token 类型: {}", token_pair.token_type); // "Bearer"
```

#### 2.2 登录失败锁定与时序攻击防护

`cmx-auth` 内置账号锁定机制：连续失败 N 次（默认 5）后锁定 M 秒（默认 900）。
用户不存在时也会执行 Argon2 dummy verify，消除响应时间差异。

```rust
use cmx_traits::auth::AuthError;

match auth_service.authenticate(credentials, device_info).await {
    Ok(token_pair) => println!("登录成功"),
    Err(AuthError::TooManyAttempts { secs, limit, window }) => {
        // 账号已锁定，提示用户等待
        eprintln!("账号已锁定 {} 秒（连续失败 {} 次，窗口 {} 秒）", secs, limit, window);
    }
    Err(AuthError::InvalidCredentials) => {
        eprintln!("用户名或密码错误");
    }
    Err(AuthError::UserDisabled) => {
        eprintln!("用户已被禁用");
    }
    Err(e) => eprintln!("登录失败: {}", e),
}
```

---

### 三、Token 校验与 AuthContext

#### 3.1 校验 Access Token

```rust
use cmx_traits::auth::AuthService;

// 中间件场景：从 Authorization: Bearer <token> 提取 token 后校验
let auth_context = auth_service
    .validate_token(&access_token)
    .await?;

// auth_context 包含完整身份信息
println!("用户 ID: {}", auth_context.user_id);
println!("用户名: {}", auth_context.username);
println!("角色: {:?}", auth_context.roles);
println!("权限: {:?}", auth_context.permissions);
println!("会话 ID: {:?}", auth_context.session_id);
println!("设备类型: {:?}", auth_context.device_type);
println!("认证方式: {:?}", auth_context.auth_method); // "jwt_bearer"
```

#### 3.2 处理各类 Token 错误

```rust
use cmx_traits::auth::AuthError;

match auth_service.validate_token(&token).await {
    Ok(ctx) => {
        // 校验通过，将 ctx 注入请求上下文
    }
    Err(AuthError::InvalidToken(msg)) => {
        // Token 解析失败或签名错误
    }
    Err(AuthError::TokenExpired) => {
        // Token 已过期，前端应使用 Refresh Token 刷新
    }
    Err(AuthError::TokenRevoked) => {
        // Token 已被加入黑名单（用户已登出/被踢下线）
    }
    Err(AuthError::SessionNotFound) => {
        // 会话不存在或不活跃（空闲超时/被清理）
    }
    Err(e) => eprintln!("其他错误: {}", e),
}
```

---

### 四、Refresh Token 刷新（Rotation）

#### 4.1 刷新 Access Token

使用 Refresh Token 换取新的 Token 对，旧 Refresh Token 立即失效（Rotation）。

```rust
use cmx_traits::auth::{AuthService, Credentials};

// 用 Refresh Token 刷新
let new_token_pair = auth_service
    .authenticate(
        Credentials::RefreshToken {
            refresh_token: old_refresh_token.clone(),
        },
        None, // Refresh 不需要 device_info，从 Token claims 中提取
    )
    .await?;

// 旧 Refresh Token 已被 Lua 脚本原子删除，无法再次使用
println!("新 Access Token: {}", new_token_pair.access_token);
println!("新 Refresh Token: {}", new_token_pair.refresh_token);
```

#### 4.2 处理重放攻击

```rust
use cmx_traits::auth::AuthError;

match auth_service.authenticate(
    Credentials::RefreshToken { refresh_token: token },
    None,
).await {
    Ok(pair) => println!("刷新成功"),
    Err(AuthError::ReplayDetected) => {
        // 旧 jti 不存在：可能是并发刷新或重放攻击
        // 应强制用户重新登录
        eprintln!("检测到 Refresh Token 重放，请重新登录");
    }
    Err(AuthError::UserDisabled) => {
        eprintln!("用户已被禁用");
    }
    Err(e) => eprintln!("刷新失败: {}", e),
}
```

---

### 五、API Key 验证（两层缓存）

#### 5.1 无状态 API Key 验证（推荐中间件场景）

`validate_api_key` 是无状态验证，不创建会话，适合高频 M2M 调用。
使用两层缓存：第一层 `key_prefix → ApiKeyEntity`，第二层 `key_prefix → AuthContext`。

```rust
use cmx_traits::auth::AuthService;

// 从 X-API-Key 请求头提取 key
let api_key = "cmx_sk_Ab3dEf9hJkLmN2pQrStUvWxYz123456";

let auth_context = auth_service
    .validate_api_key(api_key)
    .await?;

// 缓存命中时跳过全部 4 次 DB 查询，仅做 SHA256 校验
// 缓存未命中时走完整流程：查 DB → 校验 SHA256 → 查用户 → 查角色 → 查权限
println!("用户 ID: {}", auth_context.user_id); // 可能为空（纯服务间调用）
println!("认证方式: {:?}", auth_context.auth_method); // "api_key"
println!("设备类型: {:?}", auth_context.device_type); // "api_key"
```

#### 5.2 API Key 缓存失效

当 API Key 被撤销/禁用/修改时，通过 Pub/Sub 广播 `api_key:{key_prefix}` 触发缓存失效。

```rust
use cmx_traits::auth::AuthService;

// 订阅 Pub/Sub 缓存失效消息（多实例部署时各实例均需订阅）
// 收到消息后调用 invalidate_local_cache 清理本地缓存
auth_service.invalidate_local_cache("api_key:cmx_sk_A").await;
```

#### 5.3 API Key 错误处理

```rust
use cmx_traits::auth::AuthError;

match auth_service.validate_api_key(api_key).await {
    Ok(ctx) => println!("验证成功"),
    Err(AuthError::InvalidApiKey) => {
        // key_prefix 不存在 / SHA256 不匹配 / 状态为禁用
        eprintln!("API Key 无效");
    }
    Err(AuthError::UserDisabled) => {
        // API Key 关联的用户已被禁用
        eprintln!("关联用户已被禁用");
    }
    Err(e) => eprintln!("验证失败: {}", e),
}
```

---

### 六、OAuth2 授权码流程

#### 6.1 第三方 OAuth2 登录回调

`cmx-auth` 实现完整的第三方 OAuth2 回调流程：原子消费 state → 交换 Token →
获取用户信息 → 关联/注册用户 → 签发本平台 Token → 存储一次性回调授权码。

```rust
use cmx_traits::auth::{AuthService, DeviceInfo};

// 第三方 Provider 回调：provider=github, code=xxx, state=yyy
let callback_result = auth_service
    .handle_oauth2_callback(
        "github",
        "authorization_code_from_github",
        "state_stored_in_redis",
        Some(DeviceInfo {
            device_type: "web".to_string(),
            device_id: "browser-001".to_string(),
            ip: None,
            user_agent: None,
        }),
    )
    .await?;

// callback_result 包含一次性 callback_code，前端用它换取 TokenPair
println!("回调授权码: {}", callback_result.callback_code);
println!("是否新用户: {}", callback_result.is_new);
println!("Provider: {}", callback_result.provider);
```

#### 6.2 前端用回调授权码换取 TokenPair

```rust
use cmx_traits::auth::AuthService;

// 前端拿到 callback_code 后调用此接口换取 TokenPair（一次性消费）
let exchange_result = auth_service
    .exchange_oauth2_callback_code(&callback_code)
    .await?;

println!("Access Token: {}", exchange_result.access_token);
println!("Refresh Token: {}", exchange_result.refresh_token);
println!("是否新用户: {}", exchange_result.is_new);
println!("Provider: {}", exchange_result.provider);
```

#### 6.3 列出已配置的 OAuth2 Provider

```rust
use cmx_traits::auth::AuthService;

// 公开接口：GET /api/auth/oauth2/providers
let providers = auth_service.list_oauth2_providers().await?;
for p in &providers {
    println!("Provider: {} ({})", p.display_name, p.name);
    println!("  图标: {:?}", p.icon_url);
    println!("  品牌色: {:?}", p.brand_color);
}
```

#### 6.4 绑定/解绑第三方账号

```rust
use cmx_traits::auth::AuthService;

// 已登录用户绑定第三方账号（需用户主动授权）
auth_service
    .link_oauth2_account("user_123", "github", "github_auth_code")
    .await?;

// 解绑第三方账号
auth_service
    .unlink_oauth2_account("user_123", "github")
    .await?;
```

---

### 七、会话管理

#### 7.1 心跳保活

```rust
use cmx_traits::auth::AuthService;

// 前端定时（如每 5 分钟）调用心跳接口，更新 last_active_at
let is_active = auth_service
    .heartbeat("user_123", "web")
    .await?;

if is_active {
    println!("会话活跃");
} else {
    println!("会话已过期或不存在");
}
```

#### 7.2 撤销用户所有 Token 与会话

修改密码后或管理员强制下线时调用：撤销所有 Refresh Token + 将所有 Access Token
加入黑名单（TTL = 剩余有效期）+ 销毁所有会话 + Pub/Sub 广播本地缓存失效。

```rust
use cmx_traits::auth::AuthService;

// 修改密码（内部自动调用 revoke_all_tokens 强制下线所有旧会话）
auth_service
    .change_password("user_123", "OldP@ssw0rd", "NewP@ssw0rd!")
    .await?;

// 或管理员主动强制下线
auth_service.revoke_all_tokens("user_123").await?;
```

#### 7.3 撤销单个 Token

```rust
use cmx_traits::auth::AuthService;

// 登出时调用：自动识别 Access Token（加入黑名单）或 Refresh Token（删除记录）
auth_service.revoke_token(&access_token).await?;
auth_service.revoke_token(&refresh_token).await?;
```

---

### 八、错误处理

#### 8.1 完整错误处理示例

`cmx-auth` 内部使用 `AuthInfraError`（保留完整错误链：Redis/JWT/Database），
返回 `AuthService` trait 接口时通过 `.map_err()` 转换为 `cmx_traits::auth::AuthError`。

```rust
use cmx_traits::auth::{AuthService, Credentials, AuthError};

async fn login_and_handle(
    auth_service: &dyn AuthService,
    username: &str,
    password: &str,
) -> Result<(), String> {
    let credentials = Credentials::Password {
        username: username.to_string(),
        password: password.to_string(),
    };

    match auth_service.authenticate(credentials, None).await {
        Ok(token_pair) => {
            println!("登录成功，Access Token: {}", token_pair.access_token);
            Ok(())
        }
        Err(AuthError::TooManyAttempts { secs, limit, window }) => {
            Err(format!("账号已锁定 {} 秒（失败 {} 次/窗口 {} 秒）", secs, limit, window))
        }
        Err(AuthError::InvalidCredentials) => {
            Err("用户名或密码错误".to_string())
        }
        Err(AuthError::UserDisabled) => {
            Err("用户已被禁用，请联系管理员".to_string())
        }
        Err(AuthError::PasswordVerifyFailed) => {
            Err("密码校验失败".to_string())
        }
        Err(AuthError::PasswordPolicyViolated(msg)) => {
            Err(format!("密码不符合策略: {}", msg))
        }
        Err(AuthError::PasswordReused) => {
            Err("新密码与历史密码重复".to_string())
        }
        Err(AuthError::InvalidToken(msg)) => {
            Err(format!("Token 无效: {}", msg))
        }
        Err(AuthError::TokenRevoked) => {
            Err("Token 已被撤销".to_string())
        }
        Err(AuthError::ReplayDetected) => {
            Err("检测到 Refresh Token 重放攻击".to_string())
        }
        Err(AuthError::SessionNotFound) => {
            Err("会话不存在或已过期".to_string())
        }
        Err(AuthError::InvalidApiKey) => {
            Err("API Key 无效".to_string())
        }
        Err(AuthError::OAuth2(msg)) => {
            Err(format!("OAuth2 错误: {}", msg))
        }
        Err(AuthError::OAuth2CallbackCodeInvalid) => {
            Err("OAuth2 回调授权码无效或已过期".to_string())
        }
        Err(AuthError::Internal(msg)) => {
            // 内部错误（Redis/Database/序列化等），应记录详细日志
            tracing::error!(error = %msg, "认证内部错误");
            Err("系统内部错误，请稍后重试".to_string())
        }
        Err(e) => Err(format!("未知错误: {}", e)),
    }
}
```

#### 8.2 内部错误链（AuthInfraError）

`cmx-auth` 内部实现使用 `AuthInfraError`，保留 Redis / JWT / Database / SerdeJson /
Prometheus 完整错误链，便于调试。该类型实现 `From<AuthInfraError> for AuthError`，
在返回 `AuthService` trait 接口时自动转换。

```rust
use cmx_auth::error::AuthInfraError;

// cmx-auth 内部 Result 类型
pub type Result<T> = core::result::Result<T, AuthInfraError>;

// 错误变体：
// - Redis(#[from] cmx_buffer::error::Error)
// - Jwt(#[from] jsonwebtoken::errors::Error)
// - Database(#[from] cmx_database::error::Error)
// - SerdeJson(#[from] serde_json::Error)
// - Prometheus(#[from] prometheus::Error)
// - Auth(#[from] cmx_traits::auth::AuthError)
```

---

## 常见问题解答（FAQ）

### Q1: `cmx-auth` 与 `cmx-iam` 的关系是什么？

**A**: `cmx-auth` 是认证基础设施层，**不直接依赖** `cmx-iam`。两者通过 `UserAuthQuery`
trait 解耦：`cmx-auth` 定义 trait（查询用户、角色、权限、更新密码等），由 `cmx-biz` 在
运行期注入实现（可能委托给 `cmx-iam`）。这种设计使认证模块可独立测试与复用。

### Q2: 为什么 `validate_api_key` 不创建会话，而 `authenticate(ApiKey)` 会？

**A**: `validate_api_key` 是无状态验证，专为中间件高频 M2M 场景设计，仅返回 `AuthContext`
不创建会话，配合两层缓存（TTL 60 秒）避免打垮数据库。而 `authenticate(Credentials::ApiKey)`
会创建完整会话（含 session_id、Refresh Token），适合需要长期会话的场景，但不推荐用于
高频认证。中间件场景请优先使用 `validate_api_key`。

### Q3: Refresh Token Rotation 如何防止重放攻击？

**A**: 使用 Redis Lua 脚本原子执行"检查旧 jti 是否存在 → 删除旧 token → 从索引集合移除
旧 jti"。如果旧 jti 不存在（已被消费或从未签发），返回 `false`，触发 `ReplayDetected`
错误。这消除了 `exists → revoke → store` 三步操作的竞态条件，即使并发刷新也只能成功一次。

### Q4: 密钥轮换宽限期如何工作？

**A**: `JwtConfig` 提供 `current_kid`（当前签发使用的 kid）和 `legacy_public_keys`
（历史公钥列表，`Vec<(kid, pem)>`）。签发时在 Header 中写入 `current_kid`；验签时
优先用当前密钥，失败后从 Header 提取 `kid`，在 `legacy_public_keys` 中查找匹配密钥
重试。宽限期内旧 Token 仍可验证，过期后从列表移除即可完成轮换。

### Q5: 如何自定义 OAuth2 Provider？

**A**: 在 TOML 配置中添加 `[[auth.oauth2.providers]]`，设置 `provider_type = "generic"`，
并提供 `authorize_url` / `token_url` / `userinfo_url` / `scopes` / `field_mapping`。
`cmx-auth` 内置的 generic Provider 会按标准 OAuth2 流程对接，支持 `client_secret_post`
（默认）和 `client_secret_basic` 两种 Token 端点认证方式。

### Q6: 修改密码后为什么用户会被强制下线？

**A**: `change_password` 内部调用 `revoke_all_tokens`：撤销所有 Refresh Token + 将所有
Access Token 加入黑名单（TTL = 剩余有效期）+ 销毁所有会话 + Pub/Sub 广播本地缓存失效。
这是安全最佳实践，防止旧 Token 在密码泄露后仍被使用。用户需用新密码重新登录。

### Q7: 多实例部署时如何保证缓存一致性？

**A**: `cmx-auth` 通过 Redis Pub/Sub 频道 `auth:cache:invalidate` 广播缓存失效消息。
当某实例执行 `revoke_token` / `revoke_all_tokens` / API Key 失效时，会发布消息，
其他实例订阅后调用 `invalidate_local_cache` 清理本地 moka 缓存。部署时需确保所有
实例均订阅该频道。

### Q8: 时序攻击防护是如何实现的？

**A**: 当用户名不存在时，`authenticate_password` 仍会执行一次 Argon2 dummy verify
（使用固定的 dummy hash），消除"用户存在 vs 不存在"的响应时间差异，防止攻击者通过
时间侧信道枚举有效用户名。

### Q9: Prometheus 指标如何暴露？

**A**: 调用 `cmx_auth::metrics::init_metrics()` 将 7 个指标注册到 `AUTH_REGISTRY` 和
Prometheus 全局默认注册表。使用 `prometheus::TextEncoder` 从 `prometheus::default_registry()`
采集指标，通过 `/metrics` 端点暴露给 Prometheus 抓取。指标命名空间为 `cmx`，前缀 `auth_`。

### Q10: 静态 API Key 导入会泄露明文 Key 吗？

**A**: 启动时 `import_static_api_keys` 会将明文 Key SHA256 哈希后持久化到数据库
（`cmx_auth_api_key.key_hash`），数据库不存储明文。但**启动日志会打印明文 Key**
（`info!` 级别，便于管理员获取），请确保日志存储安全，或修改日志级别为 `warn` 抑制该输出。
