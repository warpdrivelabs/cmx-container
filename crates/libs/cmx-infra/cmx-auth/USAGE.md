# cmx-auth 使用与配置指南

> 版本：v3 | 适用：`crates/libs/cmx-infra/cmx-auth` 模块
>
> 关联文档：
> - 配置项字典：[config/CONFIG_MANUAL.md#认证配置](../../../config/CONFIG_MANUAL.md)
> - 架构设计：[.trae/documents/20260615_cmx-auth_企业级统一认证模块架构方案.md](../../documents/20260615_cmx-auth_企业级统一认证模块架构方案.md)
> - 第三方 OAuth2 对接：[.trae/documents/20260615_cmx-auth_第三方OAuth2Provider对接方案.md](../../documents/20260615_cmx-auth_第三方OAuth2Provider对接方案.md)

***

## 目录

- [一、模块概述](#一模块概述)
- [二、核心概念](#二核心概念)
- [三、快速开始](#三快速开始)
- [四、认证模式](#四认证模式)
- [五、配置详解](#五配置详解)
- [六、路由白名单](#六路由白名单)
- [七、第三方 OAuth2 Provider 对接](#七第三方-oauth2-provider-对接)
- [八、API 端点参考](#八api-端点参考)
- [九、数据库表](#九数据库表)
- [十、Redis Key 设计](#十redis-key-设计)
- [十一、安全机制](#十一安全机制)
- [十二、可观测性](#十二可观测性)
- [十三、运维与排障](#十三运维与排障)

***

## 一、模块概述

`cmx-auth` 是 CMX Container 平台的**统一认证基础设施 crate**，位于 `crates/libs/cmx-infra/cmx-auth/`。

### 1.1 能力清单

| 能力 | 描述 |
|------|------|
| **JWT 双令牌** | Access Token（自包含、短有效期）+ Refresh Token（Redis 持久化、长有效期） |
| **Refresh Token Rotation** | 每次刷新轮换新 jti，Lua 原子操作；旧 jti 二次使用视为重放攻击，撤销该用户所有 Token |
| **Argon2id 密码哈希** | OWASP 推荐参数（64MB / 3 / 4），哈希慢计算防暴力破解 |
| **密码策略与历史** | 强度校验 + 最近 5 次密码不可重复 |
| **OAuth2 授权码 + PKCE** | 自建 Authorization Server，授权码一次性 + PKCE 必填 |
| **第三方 OAuth2 Provider** | 平台作为 OAuth2 Client，对接 Google / GitHub / GitLab 等 |
| **会话管理** | 多端会话、心跳、闲置过期清理、SSO 互踢（可选） |
| **API Key** | 服务间调用（M2M），支持配置文件静态导入 + API 动态管理 |
| **JWT 密钥轮换** | `kid` 机制 + 旧公钥列表，宽限期内旧 Token 仍可验签 |
| **路由白名单** | 内置 + TOML 自定义，支持 `*` / `**` / `?` 通配符，编译为正则 |
| **本地缓存** | moka 本地缓存黑名单与会话存活查询，Redis Pub/Sub 主动失效 |
| **可观测性** | 7 个 Prometheus 指标 + Tracing span + 审计日志 |
| **账号锁定** | 连续登录失败自动锁定（默认 5 次 / 15 分钟） |

### 1.2 设计原则

- **协议无关**：`AuthService` trait 不依赖 axum/volo，HTTP 中间件和 gRPC interceptor 共同调用
- **Trait 解耦**：通过 `UserAuthQuery` trait 从 cmx-iam 获取用户数据，cmx-auth **不直接依赖** cmx-iam
- **零成本抽象**：优先泛型静态分发；仅跨 crate 边界使用 `dyn Trait`
- **配置驱动**：所有可调参数集中在 `config.toml` 的 `[auth.*]` 段，零代码修改即可调整

### 1.3 Token 传递方式

**Access Token 通过 `Authorization: Bearer <token>` Header 传递**，不使用 Cookie。优势：

- 天然免疫 CSRF（不受浏览器自动发送机制影响）
- SPA / 移动端方便携带
- API Key 通过 `X-API-Key` Header 传递（中间件自动识别）

### 1.4 不在本模块范围

- 用户 CRUD（`cmx_user` 表）由 `cmx-iam` 负责
- 角色 / 权限（`cmx_role` / `cmx_user_role` / `cmx_role_permission`）由 `cmx-iam` 负责
- cmx-auth 仅在签发 Token 时读取用户角色 / 权限写入 claims

***

## 二、核心概念

### 2.1 双令牌机制

| 令牌 | 用途 | 默认有效期 | 存储 | 可撤销 |
|------|------|-----------|------|--------|
| Access Token | API 访问凭证，自包含 | 30 分钟 | 客户端内存 | 黑名单 |
| Refresh Token | 换取新 Access Token | 7 天 | Redis | Redis 删除 |

**Refresh Token Rotation**：每次用 Refresh Token 换新对时，旧的 Refresh Token 立即失效。若旧的 Refresh Token 二次出现，认定为**重放攻击**，自动撤销该用户所有 Refresh Token。

### 2.2 策略模式入口：`Credentials` 枚举

`authenticate()` 接收一个 `Credentials` 枚举作为策略入口，根据变体走不同的认证路径：

- `Password { username, password }` — 用户名密码
- `RefreshToken { refresh_token }` — 刷新 Token
- `ApiKey { key }` — 服务间调用
- `AuthorizationCode { code, code_verifier, client_id }` — 自建 OAuth2 授权码
- `ThirdPartyOAuth2 { provider, provider_user_id, user_id }` — 第三方登录

### 2.3 Token 生命周期

1. **签发**：登录成功 → 创建会话 → 签发 Access + Refresh → Refresh 存 Redis
2. **校验**：中间件解码 Access → 检查黑名单 → 检查会话活跃 → 注入 `AuthContext`
3. **刷新**：用 Refresh Token 换新对 → Rotation 原子操作 → 旧 Refresh 失效
4. **撤销**：Access 加入 Redis 黑名单（TTL = 剩余有效期）+ 删除 Refresh Token
5. **过期**：Access 自然过期（无需清理），Refresh 靠 Redis TTL

### 2.4 会话 vs Token 区别

- **Token（JWT）**：自包含的字符串，签名验证即可信
- **会话（Session）**：存于 Redis Hash，记录用户登录的设备、IP、UA、最后活跃时间
- **关系**：校验 Token 时除了验签还需检查会话是否活跃（防止"签发后被踢"仍可用）

### 2.5 AuthContext 注入流程

1. `mw_context_resolver` 中间件：构建 `CmxSvrContext`（含 request_id、headers）
2. `mw_auth` 中间件：从 `Authorization: Bearer <token>` 提取 Token
3. `AuthService::validate_token` 验签 + 检查黑名单 + 检查会话
4. 成功后将 `AuthContext { user_id, username, roles, permissions, org_id, session_id, device_type, auth_method }` 注入 `CmxSvrContext.auth_context`
5. 业务 Handler 可从 `svr_ctx.auth_context` 读取当前用户信息

### 2.6 AuthError 强类型映射

`AuthError` 定义在 `cmx-traits`，消费方按变体映射 HTTP 状态码：

| AuthError 变体 | HTTP 状态码 | 含义 |
|---------------|------------|------|
| `InvalidCredentials` | 401 | 用户名密码错误 |
| `InvalidToken` / `TokenExpired` / `TokenRevoked` | 401 | Token 相关错误 |
| `ReplayDetected` | 401 | 检测到重放攻击 |
| `SessionNotFound` | 401 | 会话失效 |
| `TooManyAttempts` | 429 | 登录次数超限（账号被锁） |
| `Forbidden` / `UserDisabled` | 403 | 权限不足 / 用户被禁用 |
| `OAuth2(...)` | 400 | OAuth2 流程错误 |
| `ClientNotFound` / `PkceVerificationFailed` | 400 | OAuth2 客户端配置错误 |
| `InvalidApiKey` / `ApiKeyExpired` | 401 | API Key 错误 |
| `Internal` | 500 | 内部错误（含 Redis / DB 底层错误链） |

***

## 三、快速开始

### 3.1 接入步骤

#### 步骤 1：引入最小配置

在 `config/config.toml` 添加认证配置段（最小可用版本）：

```
[auth.jwt]
algorithm = "HS256"
secret = "your-random-256-bit-secret-key"   # 生产环境务必随机生成

[auth.token]
access_ttl_secs = 1800      # 30 分钟
refresh_ttl_secs = 604800   # 7 天

[auth.session]
single_session_per_device_type = false
idle_timeout_secs = 86400    # 24 小时
heartbeat_interval_secs = 300 # 5 分钟

[auth.cache]
enable_local_cache = true
local_ttl_secs = 30
max_login_attempts = 5
lock_duration_secs = 900

[auth.super_admin]
username = "admin"
password = "change-me-immediately"
email = "admin@example.com"
```

> **注意**：以上是片段，完整配置见 [五、配置详解](#五配置详解)。生产环境 JWT 密钥必须替换为随机长字符串。

#### 步骤 2：启动服务

启动时序（无需用户操作，由 `web-server` main.rs 编排）：

1. `DatabaseManager::initialize()` — 连接数据库
2. `CacheManager::initialize()` — 连接 Redis
3. `GlobalAuthService::initialize()` — 加载 `AuthConfig`、导入静态 API Key、创建超管
4. `GlobalAuthService::initialize_whitelist()` — 合并内置白名单 + TOML `[auth].whitelist`，编译为正则
5. `GlobalIamService::initialize()` — 初始化用户 / 角色
6. 监听 HTTP 端口

启动后日志中应看到 `超管账号创建成功` 或 `超管账号已存在`、`静态 API Key 已导入`（如配置了）、`认证白名单初始化完成`。

#### 步骤 3：客户端登录获取 Token

HTTP 调用 `POST /api/auth/login`，请求体：

```
{
  "username": "admin",
  "password": "change-me-immediately",
  "device_type": "web",
  "device_id": "browser-fingerprint-001"
}
```

成功响应（HTTP 200）：

```
{
  "code": 0,
  "data": {
    "access_token": "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9...",
    "refresh_token": "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9...",
    "token_type": "Bearer",
    "access_expires_at": 1718544123,
    "refresh_expires_at": 1719148923
  }
}
```

#### 步骤 4：调用受保护 API

在所有受保护接口的请求头中携带：

```
Authorization: Bearer eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9...
```

服务端中间件自动验证 Token 并注入用户信息。无需手动处理。

#### 步骤 5：刷新 Token

Access Token 过期前（建议剩余 5 分钟）调用 `POST /api/auth/refresh`：

```
{
  "refresh_token": "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9..."
}
```

成功响应与登录相同（新的 Access + Refresh）。**注意**：每次刷新后旧 Refresh Token 立即失效，必须使用新返回的 Refresh Token。

### 3.2 验证接入

- `GET /api/auth/health` — 检查 Redis 连通性 + JWT 密钥可用性
- `POST /api/auth/validate` — 验证 Token 有效性
- 任何受保护接口都应返回 200

***

## 四、认证模式

cmx-auth 支持 4 种认证模式，按使用场景选择。

### 4.1 用户名密码登录

**适用场景**：Web / 移动端用户登录。

**流程**：

1. 客户端 `POST /api/auth/login {username, password, device_type, device_id}`
2. 检查账号是否被锁定（Redis `auth:{username}:locked` 存在则锁定）
3. 通过 `UserAuthQuery::get_user_by_username` 查询用户
4. Argon2 校验密码
5. 查询用户角色和权限
6. 签发 Token + 创建会话
7. 返回 Access + Refresh

**错误码**：

- `401` 用户名密码错误（同时执行 Argon2 dummy verify 防时序攻击）
- `429` 连续 5 次失败后账号锁定 15 分钟
- `403` 用户被禁用

**最佳实践**：

- 客户端不要在本地存储密码
- HTTPS 传输
- 登录成功后立即丢弃明文密码

### 4.2 API Key（服务间调用）

**适用场景**：服务间调用、M2M（Machine to Machine）、CI/CD 脚本、CLI 工具。

**特点**：

- 通过 `X-API-Key: <key>` Header 传递
- 不创建会话（无状态校验）
- 不计入登录指标（使用专用指标 `cmx_auth_api_key_validations_total`）
- 可关联用户（用于审计）或仅关联服务名

**两种来源**：

1. **配置文件静态导入**（推荐用于固定服务调用）：`[[auth.static_api_keys]]` 段，启动时 UPSERT 到数据库
2. **API 动态管理**（推荐用于运行时生成）：通过 `POST /api/auth/api-keys/create` 等管理接口（见 8.4 节）

**两层缓存优化**（避免高频 M2M 调用打垮数据库）：

| 缓存层 | Redis Key | TTL | 缓存内容 | 命中后跳过的查询 |
|--------|-----------|-----|----------|-----------------|
| 第一层 | `auth:api_key:{key_prefix}` | 60s | `ApiKeyEntity`（key_hash / status / scopes 等元数据） | DB 查询 `cmx_auth_api_key` |
| 第二层 | `auth:api_key_ctx:{key_prefix}` | 60s | `AuthContext`（含 user / roles / permissions 完整上下文） | DB 查询 `cmx_user` + 角色权限（共 3 次） |

- **缓存命中**：仅做 SHA256 校验（防缓存篡改绕过），跳过全部 4 次 DB 查询
- **缓存失效**：API Key 删除 / 禁用 / 修改时，通过 Pub/Sub 频道 `auth:cache:invalidate` 广播 `api_key:{key_prefix}` 消息，各实例收到后清理两层缓存，秒级生效
- **安全校验**：即使缓存命中，仍需校验明文 key 的 SHA256 与缓存中的 `key_hash` 一致，防止缓存被篡改后绕过校验

**安全建议**：

- API Key 明文配置在 `config.toml` 时建议 `chmod 600`
- 生产环境建议结合密钥管理服务（Hashicorp Vault / 阿里云 KMS）
- 定期轮换（删除旧的、生成新的）
- scope 限定（每个 Key 仅授予必需的权限范围）

### 4.3 OAuth2 Authorization Code（自建授权服务）

**适用场景**：CMX Container 作为 OAuth2 Provider，对外提供"用 cmx 账号登录第三方应用"能力。

**流程（标准授权码模式 + PKCE）**：

1. **授权请求**：第三方应用 `GET /api/auth/oauth2/authorize?response_type=code&client_id=X&code_challenge=Y&method=S256&redirect_uri=Z&state=STATE`
2. **用户登录**：客户端跳转登录页 `POST /api/auth/oauth2/login {state, username, password}`（首次需用户授权）
3. **授权码签发**：服务端签发授权码（Redis 存储 10 分钟，含 client_id / redirect_uri / code_challenge / user_id）
4. **重定向**：302 跳转到 `redirect_uri?code=AUTH_CODE&state=STATE`
5. **Token 交换**：`POST /api/auth/oauth2/token {grant_type=authorization_code, code, code_verifier, client_id, redirect_uri}`
6. **校验**：校验 client_id / redirect_uri 一致性 + PKCE（SHA256(code_verifier) == code_challenge）
7. **签发本平台 Token 对** + 删除授权码（一次性，防重放）

**配置**：

```
[auth.oauth2]
auth_code_ttl_secs = 600    # 授权码有效期 10 分钟
pkce_required = true        # 强制 PKCE（生产必须）
```

**客户端管理**：

`cmx_auth_client` 表存储注册的第三方应用：

- `client_id` / `client_secret`（密钥哈希存储）
- `client_type`：public（无密钥，如 SPA）/ confidential（有密钥）
- `redirect_uris`：允许的回调地址列表
- `grant_types`：允许的授权类型
- `allowed_scopes`：允许请求的 scope
- `pkce_required`：是否强制 PKCE
- `status`：0-禁用 / 1-启用

**scope 策略**：OAuth2 scope 直接映射为 RBAC permission（`resource:action` 格式），不另建映射表。

### 4.4 第三方 OAuth2 Provider（Social Login）

**适用场景**：用户使用 Google / GitHub / GitLab 等第三方账号登录 cmx 平台。

详见 [七、第三方 OAuth2 Provider 对接](#七第三方-oauth2-provider-对接)。

### 4.5 模式选择决策树

| 场景 | 推荐模式 |
|------|---------|
| Web / 移动端用户登录 | 用户名密码 |
| SPA 内部调用本平台其他服务 | 用户名密码 → JWT |
| 服务间调用（固定服务） | API Key（静态配置） |
| 服务间调用（动态生成） | API Key（API 管理） |
| 第三方应用用 cmx 账号登录 | OAuth2 Authorization Code |
| 用户用 Google / GitHub 登录 | 第三方 OAuth2 Provider |
| 移动端到移动端 | 用户名密码 + Refresh Token |
| 自动化脚本 / 定时任务 | API Key |

***

## 五、配置详解

所有认证配置集中在 `config/config.toml` 的 `[auth]` 段。下面按配置子段逐项说明。

### 5.1 全局结构

`AuthConfig` 由 12 个子段组成：

```
AuthConfig
├── jwt              JWT 签名配置
├── token            Token 过期时间
├── argon2           密码哈希参数
├── session          会话管理
├── cache            本地缓存 + 登录限流
├── oauth2           OAuth2（含自建和第三方）
├── super_admin      超管账号
├── static_api_keys[]  静态 API Key 列表
└── whitelist[]      自定义白名单（支持 * / ** / ? 通配符）
```

### 5.2 `[auth.jwt]` — JWT 签名

#### `algorithm`

- **类型**：String
- **可选值**：`HS256`（默认）/ `RS256`
- **说明**：
  - `HS256`：HMAC 对称加密，性能好，适合单中心
  - `RS256`：RSA 非对称加密，公钥可公开分发验签，适合多服务 / 微服务
- **建议**：生产环境多服务使用 `RS256`；单服务使用 `HS256`

#### `secret`

- **类型**：String
- **必填**：`HS256` 模式必需
- **默认**：`"a7k9m2p4x8q1w5e3r6t0y7u2i9o4p1"`（**生产必须修改**）
- **建议**：256 位随机字符串（32 字节十六进制）

#### `private_key` / `public_key`

- **类型**：String
- **必填**：`RS256` 模式必需
- **支持**：文件路径（如 `/etc/cmx/keys/jwt-private.pem`）或 PEM 内容（以 `-----BEGIN` 开头）
- **建议**：使用文件路径 + `chmod 600` 权限控制
- **生成命令**：`openssl genpkey -algorithm RSA -out private.pem -pkeyopt rsa_keygen_bits:2048`

#### `issuer` / `audience`

- **类型**：String
- **默认**：`"cmx-auth"` / `"cmx-platform"`
- **说明**：写入 Token 的 `iss` / `aud` 声明，验签时校验。跨服务调用时需统一

#### `current_kid`

- **类型**：String（可选）
- **说明**：当前签发使用的密钥 ID，写入 JWT Header 的 `kid` 字段
- **场景**：密钥轮换时，新 Token 用新 kid 签发；旧 kid 签发的 Token 在宽限期内仍可验签
- **建议**：使用日期或版本号，如 `"key-2026-06"`

#### `legacy_public_keys[]`

- **类型**：数组，每个条目含 `kid` 和 `pem`
- **说明**：密钥轮换宽限期的旧公钥列表
- **限制**：最多 5 个
- **示例**：

```
[[auth.jwt.legacy_public_keys]]
kid = "key-2026-01"
pem = "/etc/cmx/keys/old-public-2026-01.pem"
```

**环境变量覆盖格式**：`AUTH__JWT__LEGACY_PUBLIC_KEYS_0_KID` / `AUTH__JWT__LEGACY_PUBLIC_KEYS_0_PEM`（索引 0-4）

### 5.3 `[auth.token]` — Token 过期

#### `access_ttl_secs`

- **类型**：Integer（秒）
- **默认**：`1800`（30 分钟）
- **建议**：`900` ~ `3600`（15 分钟 ~ 1 小时），越小越安全但刷新越频繁

#### `refresh_ttl_secs`

- **类型**：Integer（秒）
- **默认**：`604800`（7 天）
- **建议**：远大于 `access_ttl_secs`，用户在此期间可无感刷新

### 5.4 `[auth.argon2]` — 密码哈希

| 字段 | 默认值 | 含义 | 调优建议 |
|------|--------|------|---------|
| `memory_cost` | `65536`（64MB） | 内存开销（KB） | OWASP 推荐 64MB；生产可根据机器内存调整 |
| `time_cost` | `3` | 迭代次数 | 越大越慢越安全 |
| `parallelism` | `4` | 并行度 | 匹配 CPU 核心数 |

**性能基准**：默认参数下 hash ~100ms。调优方法：用 `cargo bench` 测量，目标 50-200ms。

### 5.5 `[auth.session]` — 会话管理

#### `single_session_per_device_type`

- **类型**：Boolean
- **默认**：`false`
- **说明**：同设备类型是否只允许一个活跃会话
- **场景**：`true` → 同设备类型新登录会踢掉旧会话（SSO 互踢）
- **建议**：内部系统 `true`；面向 C 端用户 `false`

#### `max_sessions`

- **类型**：Integer
- **默认**：`0`（不限制）
- **说明**：单用户最大并发会话数
- **行为**：超限时踢掉最早会话

#### `idle_timeout_secs`

- **类型**：Integer（秒）
- **默认**：`86400`（24 小时）
- **说明**：会话空闲超时。超过此时间无心跳的会话将被定时清理任务标记为过期

#### `heartbeat_interval_secs`

- **类型**：Integer（秒）
- **默认**：`300`（5 分钟）
- **说明**：客户端心跳间隔
- **客户端实现**：定时调用 `POST /api/auth/heartbeat` 刷新 `last_active_at`

### 5.6 `[auth.cache]` — 本地缓存与限流

#### `enable_local_cache`

- **类型**：Boolean
- **默认**：`true`
- **说明**：是否启用 moka 本地缓存（黑名单 + 会话存活查询）
- **影响**：启用后 Token 校验 QPS > 10000（命中本地缓存），禁用则每次都查 Redis

#### `local_ttl_secs`

- **类型**：Integer（秒）
- **默认**：`30`
- **说明**：本地缓存 TTL
- **影响**：撤销 Token 后，最长延迟 = 此值 + Pub/Sub 消息传播时间
- **建议**：安全敏感场景（金融 / 政务）设为 5 秒以下

#### `local_cache_max_entries`

- **类型**：Integer
- **默认**：`10000`
- **说明**：本地缓存最大容量（条目数）

#### `max_login_attempts`

- **类型**：Integer
- **默认**：`5`
- **说明**：登录失败锁定阈值。连续失败达到此次数后账号被锁

#### `lock_duration_secs`

- **类型**：Integer（秒）
- **默认**：`900`（15 分钟）
- **说明**：账号锁定时长

**注意**：以上两个限流配置是**用户名级**（不是 IP 级），存在用户名枚举风险。生产环境建议结合 WAF / API Gateway 的 IP 限流。

### 5.7 `[auth.oauth2]` — OAuth2 配置

OAuth2 段包含两部分：**自建 Authorization Server** 和 **第三方 Provider 对接**。

#### 5.7.1 自建 Authorization Server

| 字段 | 默认值 | 说明 |
|------|--------|------|
| `auth_code_ttl_secs` | `600` | 授权码有效期（秒） |
| `pkce_required` | `true` | 是否强制 PKCE（生产必须） |

#### 5.7.2 第三方 Provider 对接

| 字段 | 默认值 | 说明 |
|------|--------|------|
| `state_ttl_secs` | `600` | Provider state 有效期（防 CSRF） |
| `callback_code_ttl_secs` | `30` | 回调授权码有效期（30 秒，强烈建议保持） |
| `frontend_callback_url` | `""` | 登录成功后重定向前端的 URL（必需配置） |
| `providers[]` | `[]` | Provider 列表（数组表） |
| `account_link` | 默认 | 账号关联策略（见下） |

**`[auth.oauth2.account_link]`** 账号关联策略：

| 字段 | 默认值 | 说明 |
|------|--------|------|
| `auto_link_by_email` | `true` | 邮箱已验证时自动关联本地用户 |
| `auto_register` | `false` | 无匹配用户时是否自动注册 |
| `default_role` | `None` | 自动注册时的默认角色（role_code） |
| `username_strategy` | `"provider_prefix"` | 用户名生成策略（`provider_prefix` / `email_prefix` / `display_name`） |

#### 5.7.3 `[[auth.oauth2.providers]]` — Provider 配置

| 字段 | 必填 | 默认值 | 说明 |
|------|------|--------|------|
| `name` | 是 | — | Provider 唯一标识（用于 API 路由和关联记录） |
| `display_name` | 否 | 同 `name` | 前端展示名 |
| `provider_type` | 否 | `"generic"` | 实现类型：`google` / `github` / `generic` |
| `client_id` | 是 | — | Provider 后台获取 |
| `client_secret` | 是 | — | Provider 后台获取（仅服务端使用） |
| `redirect_uri` | 是 | — | 回调地址，前端不可覆盖 |
| `authorize_url` | `generic` 必填 | 内置类型自动 | 授权端点 |
| `token_url` | `generic` 必填 | 内置类型自动 | Token 端点 |
| `userinfo_url` | `generic` 必填 | 内置类型自动 | 用户信息端点 |
| `scopes` | 否 | 内置类型默认 | 请求的 scope（TOML 中逗号分隔） |
| `token_endpoint_auth_method` | 否 | `"client_secret_post"` | `client_secret_post` / `client_secret_basic` |
| `field_mapping` | 否 | 空 | 字段映射（仅 `generic`），支持 number→string |
| `icon_url` | 否 | 内置类型默认 | Provider 图标 URL |
| `brand_color` | 否 | 内置类型默认 | 品牌色（前端按钮样式） |
| `enabled` | 否 | `true` | 是否启用 |

### 5.8 `[auth.super_admin]` — 超管账号

启动时自动创建（已存在则跳过）：

| 字段 | 必填 | 默认值 | 说明 |
|------|------|--------|------|
| `username` | 是 | — | 超管用户名 |
| `password` | 是 | — | 超管初始密码（启动时 Argon2 哈希） |
| `email` | 否 | — | 超管邮箱 |
| `roles` | 否 | `["admin"]` | 角色编码列表（TOML 逗号分隔） |

**安全建议**：

- 生产环境**必须通过环境变量注入密码**，不要写在配置文件中
- 部署后立即通过改密接口修改
- `config.toml` 设置 `chmod 600`

### 5.9 `[[auth.static_api_keys]]` — 静态 API Key

启动时自动 UPSERT 到 `cmx_auth_api_key` 表（已存在则更新）。

| 字段 | 必填 | 说明 |
|------|------|------|
| `key` | 是 | 明文 Key（启动时自动 SHA256 哈希） |
| `key_prefix` | 否 | 唯一标识；未填时**自动从 `key` 前 8 位提取**（key 不足 8 位时取全部） |
| `user_id` | 否 | 关联用户 ID |
| `service_name` | 否 | 关联服务名 |
| `scopes` | 否 | 允许的 scope（TOML 数组） |
| `description` | 否 | 描述 |

#### 5.9.1 简化用法（推荐）

只填 `key` 字段，`key_prefix` 自动从 `key` 前 8 位提取，无需关心前缀命名：

```
[[auth.static_api_keys]]
key = "cmx_sk_Ab3dEf9hJkLmN2pQrStUvWxYz123456"
service_name = "billing-service"
description = "计费服务调用"
```

上例中 `key_prefix` 自动解析为 `cmx_sk_A`。

#### 5.9.2 高级用法（显式指定前缀）

用于迁移场景（保持与历史前缀一致）或自定义前缀命名：

```
[[auth.static_api_keys]]
key_prefix = "cmx-runtime"
key = "cmx-runtime-xxxx-yyyy-zzzz"
service_name = "cmx-runtime"
scopes = ["service:invoke", "plugin:read"]
description = "运行时服务间调用"
```

#### 5.9.3 解析规则

由 `StaticApiKeyConfig::resolve_key_prefix()` 实现：

1. **优先使用显式 `key_prefix`**：若配置且非空字符串，直接采用
2. **否则从 `key` 前 8 位提取**：`key` 长度 ≥ 8 时取前 8 个字符，否则取全部
3. **向后兼容**：已有配置中的 `key_prefix` 保持原行为，不做强制修改

**注意**：`key_prefix` 仅作为人类可读标识和 `cmx_auth_api_key` 表的查询条件，**安全校验仍基于 `key_hash`（SHA256）**。前缀冲突不影响鉴权。

### 5.10 `[auth] whitelist` — 自定义认证白名单

无需认证的路径列表。启动时与内置白名单 `BUILTIN_WHITELIST`（在 `cmx_auth::config` 中定义）**合并去重**，统一编译为正则表达式后注入全局。

详见 [六、路由白名单](#六路由白名单)。

#### 5.10.1 字段说明

| 字段 | 类型 | 必填 | 默认值 | 说明 |
|------|------|------|--------|------|
| `whitelist` | 数组 | 否 | `[]` | 自定义白名单条目，每条支持普通路径或通配符模式 |

#### 5.10.2 配置示例

```
[auth]
whitelist = [
    "/api/public/**",      # 公开资源目录（含子路径）
    "/api/v1/webhook",     # 支付回调（精确前缀匹配）
    "/api/biz/*",          # 业务公开接口（单层路径）
    "/api/v?/docs",        # 版本号占位（v1 / v2 都命中）
]
```

#### 5.10.3 与内置白名单的关系

- **内置白名单**（`BUILTIN_WHITELIST`）始终生效，覆盖登录、刷新、OAuth2、文档、健康检查等基础路径
- **用户白名单**通过 TOML `[auth] whitelist` 追加，与内置白名单合并
- 重复条目**自动去重**，无需手工排除
- 编译失败的规则**仅警告不中断启动**，便于临时配置错误修复

#### 5.10.4 通配符语法（详见六章）

| 符号 | 含义 | 示例 | 匹配 |
|------|------|------|------|
| 普通字符串 | 前缀匹配 | `/api/auth/login` | `/api/auth/login`、`/api/auth/login/extra` |
| `*` | 单层路径段（不含 `/`） | `/api/biz/*` | `/api/biz/users`；不匹配 `/api/biz/users/123` |
| `**` | 多层路径（含 `/`） | `/api/auth/**` | `/api/auth/oauth2/token`、`/api/auth/a/b/c` |
| `?` | 单个非 `/` 字符 | `/api/v?/users` | `/api/v1/users`；不匹配 `/api/v12/users` |

### 5.11 完整配置示例

完整的 `config.toml` 认证段示例（生产环境）：

```
[auth.jwt]
algorithm = "RS256"
private_key = "/etc/cmx/keys/jwt-private.pem"
public_key = "/etc/cmx/keys/jwt-public.pem"
issuer = "cmx-auth"
audience = "cmx-platform"
current_kid = "key-2026-06"

[[auth.jwt.legacy_public_keys]]
kid = "key-2026-01"
pem = "/etc/cmx/keys/old-public-2026-01.pem"

[auth.argon2]
memory_cost = 65536
time_cost = 3
parallelism = 4

[auth.token]
access_ttl_secs = 1800
refresh_ttl_secs = 604800

[auth.session]
single_session_per_device_type = false
max_sessions = 0
idle_timeout_secs = 86400
heartbeat_interval_secs = 300

[auth.cache]
enable_local_cache = true
local_ttl_secs = 30
local_cache_max_entries = 10000
max_login_attempts = 5
lock_duration_secs = 900

[auth.oauth2]
auth_code_ttl_secs = 600
pkce_required = true
state_ttl_secs = 600
callback_code_ttl_secs = 30
frontend_callback_url = "https://app.example.com/auth/callback"

[auth.oauth2.account_link]
auto_link_by_email = true
auto_register = false
default_role = "user"
username_strategy = "provider_prefix"

[[auth.oauth2.providers]]
name = "google"
display_name = "Google"
provider_type = "google"
client_id = "your-client.apps.googleusercontent.com"
client_secret = "your-secret"
redirect_uri = "https://your-domain.com/api/auth/oauth2/provider/google/callback"
scopes = ["openid", "email", "profile"]
enabled = true

[[auth.oauth2.providers]]
name = "github"
display_name = "GitHub"
provider_type = "github"
client_id = "your-github-client-id"
client_secret = "your-github-secret"
redirect_uri = "https://your-domain.com/api/auth/oauth2/provider/github/callback"
scopes = ["user:email", "read:user"]
enabled = true

[[auth.oauth2.providers]]
name = "gitlab"
display_name = "GitLab"
provider_type = "generic"
client_id = "your-gitlab-app-id"
client_secret = "your-gitlab-secret"
redirect_uri = "https://your-domain.com/api/auth/oauth2/provider/gitlab/callback"
authorize_url = "https://gitlab.com/oauth/authorize"
token_url = "https://gitlab.com/oauth/token"
userinfo_url = "https://gitlab.com/api/v4/user"
scopes = ["read_user", "email"]
token_endpoint_auth_method = "client_secret_post"
field_mapping = { provider_user_id = "id", email = "email", username = "username", display_name = "name", avatar_url = "avatar_url" }
enabled = true

[auth.super_admin]
username = "admin"
password = "change-me-immediately"
email = "admin@example.com"
roles = "admin"

# 自定义白名单（与内置白名单合并；支持 *、**、? 通配符）
auth.whitelist = ["/api/public/**", "/api/v1/webhook", "/api/biz/*"]

[[auth.static_api_keys]]
key = "cmx_sk_Ab3dEf9hJkLmN2pQrStUvWxYz123456"
service_name = "cmx-runtime"
scopes = ["service:invoke", "plugin:read"]
description = "运行时服务间调用（key_prefix 自动提取为 cmx_sk_A）"

[[auth.static_api_keys]]
key_prefix = "cmx-agent"
key = "cmx-agent-aaaa-bbbb-cccc"
user_id = "1"
scopes = ["service:invoke", "service:execute"]
description = "AI Agent 调用（显式 key_prefix 高级用法）"
```

### 5.12 配置优先级

从高到低：

1. **环境变量**（最高优先级，不可覆盖）
2. **远程配置中心**（Nacos / Consul / etcd）
3. **本地 TOML 文件**
4. **代码默认值**

环境变量覆盖格式：`AUTH__JWT__ALGORITHM=RS256`（双下划线分隔段名）。

### 5.13 配置项分类速查

| 分类 | 配置路径 | 关键项 | 默认值 |
|------|----------|--------|--------|
| JWT 签名 | `auth.jwt` | `algorithm` / `secret` / `private_key` / `current_kid` | `HS256` |
| Token 有效期 | `auth.token` | `access_ttl_secs` / `refresh_ttl_secs` | `30min` / `7天` |
| 密码哈希 | `auth.argon2` | `memory_cost` / `time_cost` / `parallelism` | `64MB` / `3` / `4` |
| 会话管理 | `auth.session` | `single_session_per_device_type` / `idle_timeout` / `heartbeat` | `false` / `24h` / `5min` |
| 缓存安全 | `auth.cache` | `enable_local_cache` / `local_ttl` / `max_attempts` | `true` / `30s` / `5` |
| OAuth2 自建 | `auth.oauth2` | `auth_code_ttl_secs` / `pkce_required` | `600` / `true` |
| OAuth2 第三方 | `auth.oauth2` | `state_ttl_secs` / `callback_code_ttl_secs` | `600` / `30` |
| 账号关联 | `auth.oauth2.account_link` | `auto_link_by_email` / `auto_register` | `true` / `false` |
| Provider 列表 | `auth.oauth2.providers[]` | `name` / `provider_type` / `client_id` | — |
| 超管 | `auth.super_admin` | `username` / `password` / `roles` | `admin` / `change-me` / `super_admin` |
| 静态 API Key | `auth.static_api_keys[]` | `key` / `key_prefix`（可选） / `scopes` | — |
| 路由白名单 | `auth.whitelist[]` | 通配符规则（`*` / `**` / `?`） | `[]`（仅内置白名单） |

***

## 六、路由白名单

`mw_auth` 中间件在每个请求进入时检查路径是否命中白名单，命中则跳过 Token 校验。

### 6.1 白名单来源

白名单由两部分**合并去重**构成：

1. **内置白名单**：`cmx_auth::config::BUILTIN_WHITELIST` 常量，硬编码在 `cmx-auth` crate 中
2. **用户自定义白名单**：TOML `[auth].whitelist` 数组，启动时通过 `GlobalAuthService::initialize_whitelist` 注入

详见配置项 [5.10 `[auth] whitelist`](#510-auth-whitelist--自定义认证白名单)。

### 6.2 内置白名单（`BUILTIN_WHITELIST`）

来源：`crates/libs/cmx-infra/cmx-auth/src/config.rs` 的 `BUILTIN_WHITELIST` 常量。

| 路径前缀 | 用途 |
|----------|------|
| `/api/auth/login` | 用户名密码登录 |
| `/api/auth/refresh` | 刷新 Token |
| `/api/auth/validate` | 校验 Token |
| `/api/auth/logout` | 登出（撤销当前 Token） |
| `/api/auth/health` | 认证服务健康检查 |
| `/api/auth/oauth2/authorize` | OAuth2 授权请求 |
| `/api/auth/oauth2/login` | OAuth2 用户登录授权 |
| `/api/auth/oauth2/token` | OAuth2 换 Token |
| `/api/auth/oauth2/providers` | 列出已启用的第三方 Provider |
| `/api/auth/oauth2/provider` | 第三方 Provider 子路径（authorize / callback / exchange / link / unlink） |
| `/swagger` | Swagger UI |
| `/api-docs` | OpenAPI 文档 |
| `/health` | 通用健康检查 |

> **重要**：以上列表**以源码 `cmx_auth::config::BUILTIN_WHITELIST` 为准**，版本升级可能增减。

### 6.3 匹配规则

白名单规则分两类，启动时**统一编译为正则**后存入 `GLOBAL_AUTH_WHITELIST`（位于 `mw_auth.rs`）：

| 规则类型 | 匹配行为 | 示例 |
|----------|----------|------|
| **普通规则**（不含通配符） | 前缀匹配（隐式 `**` 后缀，等价正则 `^prefix.*$`） | `/api/auth/login` → 匹配 `/api/auth/login`、`/api/auth/login/extra` |
| **含 `*` 通配符** | 匹配单层路径段（不含 `/`），等价正则 `[^/]*` | `/api/biz/*` → 匹配 `/api/biz/users`；不匹配 `/api/biz/users/123` |
| **含 `**` 通配符** | 匹配多层路径（含 `/`），等价正则 `.*` | `/api/auth/**` → 匹配 `/api/auth/oauth2/token`、`/api/auth/a/b/c` |
| **含 `?` 通配符** | 匹配单个非 `/` 字符 | `/api/v?/users` → 匹配 `/api/v1/users`；不匹配 `/api/v12/users` |

**实现位置**：`mw_auth.rs` 的 `compile_wildcard_to_regex()` 和 `compile_rule()`。

**转义处理**：路径中的正则元字符（`. + ? ^ $ ( ) [ ] { } | \`）会被自动转义为字面字符。例如 `/api/data.json` 不会被 `/api/dataXjson` 误匹配。

**未初始化回退**：若 `initialize_whitelist` 未调用，`is_whitelisted` 会回退到内置白名单的 `starts_with` 前缀匹配，保证功能可用。

### 6.4 自定义白名单

通过 TOML `[auth] whitelist` 数组追加，无需修改源码、无需重新编译：

```
[auth]
whitelist = [
    "/api/public/**",      # 公开资源目录（多层级路径）
    "/api/v1/webhook",     # 支付回调（精确前缀）
    "/api/biz/*",          # 业务公开接口（单层路径）
    "/api/v?/docs",        # 版本号占位
]
```

**注意**：

- 启动时与内置白名单合并去重，**无需手工排除**已有条目
- 编译失败的规则**仅警告不中断启动**，便于临时配置错误修复
- 修改后**需重启服务生效**（当前版本不支持热更新）

### 6.5 常见用法示例

#### 公开首页 / 落地页

```
whitelist = ["/", "/index.html", "/static/**", "/favicon.ico"]
```

#### 支付回调（精确路径）

```
whitelist = ["/api/payment/notify/alipay", "/api/payment/notify/wechat"]
```

#### Webhook 端点（按版本）

```
whitelist = ["/api/v1/webhook/**", "/api/v2/webhook/**"]
```

#### API 文档

```
whitelist = ["/swagger-ui/**", "/api-docs/**", "/redoc/**"]
```

#### 内部健康检查

```
whitelist = ["/health", "/health/**", "/metrics"]
```

#### 业务公开接口（单层路径）

```
# 匹配 /api/biz/ping、/api/biz/version，但不匹配 /api/biz/user/123
whitelist = ["/api/biz/ping", "/api/biz/version"]
```

### 6.6 何时需要添加白名单

- 公开的 API（如健康检查、文档、注册、找回密码）
- 第三方回调（如支付回调、SSO 回调）
- 公开资源（无需登录的首页数据、CDN 资源）

### 6.7 白名单安全注意

- 白名单路径仍受 CORS / WAF / 限流保护
- **不要将管理类接口**（如 `/api/auth/revoke-all`）加入白名单
- 第三方回调路径必须验证 `state` 参数防 CSRF
- **避免过宽的 `/**` 规则**，如非必要不写 `/api/**`（会暴露所有 `/api` 路径）
- 定期审计白名单，删除不再需要的路径

### 6.8 强制下线接口的特殊权限要求

`POST /api/auth/revoke-all`（强制下线用户）**不在白名单中**，调用者必须具有 `system:auth:kick` 权限（管理员角色）。

***

## 七、第三方 OAuth2 Provider 对接

平台作为 **OAuth2 Client**，允许用户使用第三方账号（Google / GitHub / GitLab 等）登录。

### 7.1 适用场景

- C 端产品希望降低注册门槛（一键登录）
- 企业内部系统对接公司 SSO（GitLab / Okta）
- 海外业务需要 Google / GitHub 登录

### 7.2 标准对接流程（4 步）

#### 步骤 1：在 Provider 开发者后台注册应用

**Google**：访问 Google Cloud Console → API 和服务 → 凭据 → 创建 OAuth 客户端 ID

- 应用类型：Web 应用
- 已授权的重定向 URI：`https://your-domain.com/api/auth/oauth2/provider/google/callback`
- 记下 `Client ID` 和 `Client Secret`

**GitHub**：访问 Settings → Developer settings → OAuth Apps → New OAuth App

- Homepage URL：`https://your-domain.com`
- Authorization callback URL：`https://your-domain.com/api/auth/oauth2/provider/github/callback`
- 记下 `Client ID` 和 `Client Secret`

**GitLab**：参考 GitLab 文档创建 Application，redirect URI 同上。

#### 步骤 2：在 `config.toml` 中配置 Provider

参考 [5.11 完整配置示例](#511-完整配置示例) 中的 `[[auth.oauth2.providers]]` 段。**注意**：

- `redirect_uri` 必须与 Provider 后台注册的完全一致
- `client_secret` 仅服务端使用，不暴露给前端
- 生产环境建议通过环境变量注入 `client_secret`（`AUTH__OAUTH2__PROVIDERS_0__CLIENT_SECRET`）

#### 步骤 3：验证 Provider 已注册

启动后调用 `GET /api/auth/oauth2/providers`（在白名单中，无需认证），应返回已配置的 Provider 列表：

```
{
  "code": 0,
  "data": {
    "providers": [
      {
        "name": "google",
        "display_name": "Google",
        "scopes": ["openid", "email", "profile"],
        "icon_url": "https://www.gstatic.com/firebasejs/ui/identity/google.svg",
        "brand_color": "#4285F4"
      },
      {
        "name": "github",
        "display_name": "GitHub",
        "scopes": ["user:email", "read:user"],
        "icon_url": "https://github.githubassets.com/...",
        "brand_color": "#24292e"
      }
    ]
  }
}
```

**注意**：未启用的 Provider（`enabled = false`）不会出现在此列表中。

#### 步骤 4：前端发起授权流程

完整流程（前端 + 后端）：

1. **前端**：调用 `GET /api/auth/oauth2/providers` 获取 Provider 列表，展示登录按钮
2. **用户**：点击 "Google 登录" 按钮
3. **前端**：跳转浏览器到 `GET /api/auth/oauth2/provider/{provider}/authorize`（后端 302 重定向到 Provider 授权页）
4. **Provider**：用户在 Google 登录并同意授权
5. **Provider**：302 回调到 `https://your-domain.com/api/auth/oauth2/provider/{provider}/callback?code=XXX&state=YYY`
6. **后端**：
   - 原子消费 `state`（防重放，防 CSRF）
   - 用 `code` 向 Provider 换 `access_token`（含 ID Token）
   - 验证 Google ID Token 签名（JWKS）或调用 `/userinfo` 端点
   - 通过 `AccountLinker` 查找 / 关联 / 注册用户
   - 签发本平台 Token
   - 签发**一次性回调授权码**（30 秒 TTL，存 Redis）
7. **后端**：302 跳转到 `{frontend_callback_url}?code={one_time_code}&state={original_state}`
8. **前端**：在 `frontend_callback_url` 页面用 `code` 调用 `POST /api/auth/oauth2/provider/exchange {code, state}` 换取 Access + Refresh
9. **后端**：原子消费回调授权码（防重放），返回 Token 对

### 7.3 三种典型 Provider 配置

#### Google

内置 `google` 类型，自动配置端点 URL + JWKS 验证 ID Token 签名。

```
[[auth.oauth2.providers]]
name = "google"
display_name = "Google"
provider_type = "google"
client_id = "xxx.apps.googleusercontent.com"
client_secret = "xxx"
redirect_uri = "https://your-domain.com/api/auth/oauth2/provider/google/callback"
scopes = ["openid", "email", "profile"]
enabled = true
```

**特殊性**：

- 自动验证 Google ID Token 签名（JWKS 公钥从 Google endpoint 获取并缓存 24 小时）
- 自动解析 ID Token 中的 `email_verified` 字段
- 不需要手动配置端点 URL

#### GitHub

内置 `github` 类型，含 `/user/emails` API 调用以获取邮箱验证状态。

```
[[auth.oauth2.providers]]
name = "github"
display_name = "GitHub"
provider_type = "github"
client_id = "xxx"
client_secret = "xxx"
redirect_uri = "https://your-domain.com/api/auth/oauth2/provider/github/callback"
scopes = ["user:email", "read:user"]
enabled = true
```

**特殊性**：

- GitHub `/user` 接口不返回 `email_verified`，需额外调用 `/user/emails` API
- 必须包含 `user:email` scope 才能获取邮箱
- 自动处理 GitHub 的 number 类型 id（转为字符串）

#### GitLab（generic 通用配置）

非内置类型，需手动配置端点 URL 和字段映射。

```
[[auth.oauth2.providers]]
name = "gitlab"
display_name = "GitLab"
provider_type = "generic"
client_id = "xxx"
client_secret = "xxx"
redirect_uri = "https://your-domain.com/api/auth/oauth2/provider/gitlab/callback"
authorize_url = "https://gitlab.com/oauth/authorize"
token_url = "https://gitlab.com/oauth/token"
userinfo_url = "https://gitlab.com/api/v4/user"
scopes = ["read_user", "email"]
token_endpoint_auth_method = "client_secret_post"
field_mapping = { provider_user_id = "id", email = "email", username = "username", display_name = "name", avatar_url = "avatar_url" }
enabled = true
```

**关键点**：

- `field_mapping` 必须配置，因为不同 Provider 字段名不同
- 支持 `number` 类型自动转 `string`（如 GitLab 的 id 是 number）
- `token_endpoint_auth_method`：`client_secret_post`（默认，参数在 body）/ `client_secret_basic`（HTTP Basic Auth）

### 7.4 账号关联策略

#### 关联优先级

`AccountLinker::find_or_link` 按以下顺序处理：

1. **查询已关联**：根据 `provider + provider_user_id` 查 `cmx_auth_oauth2_account` 表
2. **邮箱自动关联**（`auto_link_by_email = true`）：如果 Provider 返回的邮箱**已验证**（`email_verified = true`）且与本地用户邮箱匹配，自动绑定
3. **自动注册**（`auto_register = true`）：无匹配用户时自动创建
4. **需要手动绑定**：返回错误，前端提示用户登录后手动绑定

#### 用户名生成策略（`username_strategy`）

| 策略 | 示例 | 适用 |
|------|------|------|
| `provider_prefix` | `google_1234567890` | 默认，最稳定，无重复风险 |
| `email_prefix` | `user`（从 `user@gmail.com`） | 用户体验好，但可能冲突 |
| `display_name` | 昵称 | 体验最好，但冲突最多 |

冲突处理：基础用户名冲突时追加 4 位十六进制随机后缀（如 `_a3f2`），最多重试 3 次，仍冲突则返回错误。

#### 安全限制

- 邮箱未验证时**不会**自动关联（防止账号接管）
- 默认 `auto_register = false`，避免自动创建大量用户
- 解绑时检查"最后一个登录绑定不可解绑"（用户至少保留密码或一个第三方绑定）

### 7.5 手动绑定 / 解绑

登录用户可在个人中心绑定 / 解绑第三方账号：

- **绑定**：`POST /api/auth/oauth2/provider/{provider}/link {code}` — 用户从 Provider 授权后回调到前端，前端拿授权 `code` 调此接口
- **解绑**：`DELETE /api/auth/oauth2/provider/{provider}/unlink` — 调用者必须是已登录用户

**解绑安全检查**：用户必须保留至少一种登录方式（密码或一个第三方绑定），否则解绑失败。

### 7.6 第三方 Token 安全

- **不持久化**：第三方 Provider 签发的 `access_token` / `refresh_token` 不在本平台存储
- **一次性使用**：仅在回调流程中使用，用后即弃
- **本平台 Token**：本平台只签发自己的 JWT Token（Access + Refresh）

### 7.7 已知不支持

- **WeChat**：非标准 OAuth2 流程（appid/secret 参数名、JSON body、openid+unionid），本期不支持
- **手机号 / 短信验证码**：cmx-auth 暂不包含此能力
- **SAML / OIDC IdP**：暂不支持，仅支持 OAuth2 Client 模式

***

## 八、API 端点参考

所有路径以 `/api/auth/` 为前缀。`[需要认证]` 表示必须携带 `Authorization: Bearer <token>` Header，`[白名单]` 表示无需认证。

### 8.1 核心认证接口

| 方法 | 路径 | 需要认证 | 用途 |
|------|------|---------|------|
| POST | `/api/auth/login` | 白名单 | 用户名密码登录，返回 Access + Refresh |
| POST | `/api/auth/refresh` | 白名单 | 用 Refresh Token 换新对 |
| POST | `/api/auth/logout` | 白名单 | 撤销指定 Token（登出） |
| POST | `/api/auth/validate` | 白名单 | 校验 Token，返回用户信息 |
| GET | `/api/auth/me` | 是 | 获取当前登录用户完整信息（含昵称、邮箱、角色、权限） |
| POST | `/api/auth/revoke-all` | 是 | 撤销用户所有 Token（强制下线，需 `system:auth:kick` 权限） |
| POST | `/api/auth/heartbeat` | 是 | 刷新会话活跃时间 |
| POST | `/api/auth/change-password` | 是 | 修改当前用户密码 |
| GET | `/api/auth/health` | 白名单 | 健康检查（Redis + JWT 密钥） |

### 8.2 OAuth2 Authorization Server（自建）

| 方法 | 路径 | 需要认证 | 用途 |
|------|------|---------|------|
| GET | `/api/auth/oauth2/authorize` | 白名单 | 验证 client_id，生成 CSRF state |
| POST | `/api/auth/oauth2/login` | 白名单 | 用户名密码登录并签发授权码 |
| POST | `/api/auth/oauth2/token` | 白名单 | 用授权码换本平台 Token |

### 8.3 第三方 OAuth2 Provider

| 方法 | 路径 | 需要认证 | 用途 |
|------|------|---------|------|
| GET | `/api/auth/oauth2/providers` | 白名单 | 列出已启用的 Provider 列表 |
| GET | `/api/auth/oauth2/provider/{provider}/authorize` | 白名单 | 重定向到 Provider 授权页 |
| GET | `/api/auth/oauth2/provider/{provider}/callback` | 白名单 | Provider 回调，签发一次性授权码 |
| POST | `/api/auth/oauth2/provider/exchange` | 白名单 | 用一次性授权码换 Token |
| POST | `/api/auth/oauth2/provider/{provider}/link` | 是 | 手动绑定第三方账号 |
| DELETE | `/api/auth/oauth2/provider/{provider}/unlink` | 是 | 解除第三方账号绑定 |

### 8.4 API Key 管理接口

运行时动态管理 API Key（区别于 5.9 节的静态配置导入）。所有接口需认证，建议仅管理员调用。

| 方法 | 路径 | 需要认证 | 用途 |
|------|------|---------|------|
| POST | `/api/auth/api-keys/create` | 是 | 创建 API Key，**明文仅返回一次** |
| GET | `/api/auth/api-keys/list` | 是 | 列出 API Key（支持 status / user_id / service_name 过滤） |
| POST | `/api/auth/api-keys/delete` | 是 | 删除指定 API Key（按 id） |
| POST | `/api/auth/api-keys/toggle-status` | 是 | 启用 / 禁用 API Key（status: 0-禁用 / 1-启用） |

**创建请求示例**：

```
POST /api/auth/api-keys/create
{
  "user_id": "1",                       // 可选，关联用户
  "service_name": "billing-service",    // 可选，关联服务
  "scopes": ["service:invoke"],         // 可选，权限范围
  "description": "计费服务调用"           // 可选
}
```

**创建响应**（`api_key` 字段为明文，仅此一次返回，后续不可查看）：

```
{
  "code": 0,
  "data": {
    "id": "...",
    "key_prefix": "cmx_abc1",
    "api_key": "cmx_abc12345def67890...",  // 明文，务必立即保存
    "user_id": "1",
    "service_name": "billing-service",
    "scopes": ["service:invoke"],
    "status": 1,
    "create_time": "2026-06-24T10:00:00Z"
  }
}
```

**安全提示**：
- 明文 `api_key` 仅在创建时返回一次，丢失只能重新创建
- 删除 / 禁用后通过 Pub/Sub 秒级失效两层缓存（见 4.2 节）
- 建议通过环境变量或密钥管理服务保存明文，不要硬编码

### 8.5 OAuth2 客户端管理接口

管理自建 Authorization Server 注册的第三方应用（`cmx_auth_client` 表）。所有接口需认证，建议仅管理员调用。

| 方法 | 路径 | 需要认证 | 用途 |
|------|------|---------|------|
| POST | `/api/auth/oauth2-clients/create` | 是 | 注册 OAuth2 客户端 |
| GET | `/api/auth/oauth2-clients/list` | 是 | 列出客户端（支持 status / client_id 过滤） |
| POST | `/api/auth/oauth2-clients/update` | 是 | 按 id 更新客户端（支持重置密钥） |
| POST | `/api/auth/oauth2-clients/delete` | 是 | 删除指定客户端（按 id） |

**创建请求示例**：

```
POST /api/auth/oauth2-clients/create
{
  "client_id": "my-app-001",
  "client_name": "我的应用",
  "client_secret": "my-secret",          // confidential 类型必填，public 可空
  "client_type": "confidential",          // public / confidential
  "redirect_uris": ["https://app.example.com/callback"],
  "grant_types": ["authorization_code"],
  "allowed_scopes": ["read", "write"],
  "pkce_required": true,
  "description": "第三方应用"
}
```

**更新请求**（按 id 更新，所有字段可选，`client_secret` 传则重置）：

```
POST /api/auth/oauth2-clients/update
{
  "id": "...",
  "client_name": "新名称",
  "client_secret": "new-secret",          // 传则重置密钥
  "status": 0                              // 0-禁用 / 1-启用
}
```

### 8.6 端点行为说明

- **登录接口**支持 `device_type`（web / mobile / desktop / api_key）和 `device_id` 字段，用于会话分类
- **refresh 接口**：每次调用都返回新的 Refresh Token，旧 Refresh 立即失效（Rotation）
- **revoke-all 接口**：撤销 Access + Refresh + 销毁所有会话 + 通过 Pub/Sub 广播本地缓存失效
- **heartbeat 接口**：仅刷新 `last_active_at`，不返回新 Token
- **change-password 接口**：改密成功后**自动撤销所有旧会话**（强制重新登录）
- **me 接口**：返回当前用户完整信息（含 `cmx_user` 表的昵称、邮箱、手机、头像等），并附加角色与权限列表
- **api-keys 接口**：明文 Key 仅创建时返回一次；删除 / 禁用通过 Pub/Sub 秒级失效两层缓存
- **oauth2-clients 接口**：`client_secret` 哈希存储；更新时传 `client_secret` 则重置

### 8.7 Swagger 文档

完整 API 文档（OpenAPI 3.0）：

- `GET /swagger-ui/` — Swagger UI
- `GET /api-docs/openapi.json` — OpenAPI JSON

***

## 九、数据库表

cmx-auth 涉及 6 张表（schema 均为 `public`）。

### 9.1 `cmx_user`（由 cmx-iam 管理）

- **用途**：用户基础信息
- **cmx-auth 读取字段**：`id`, `username`, `password_hash`, `status`, `nickname`
- **关联**：通过 `UserAuthQuery` trait 访问

### 9.2 `cmx_auth_client`

- **用途**：OAuth2 客户端注册表（自建 Authorization Server）
- **关键字段**：
  - `client_id` — 客户端标识（唯一）
  - `client_secret` — 客户端密钥（哈希存储）
  - `client_type` — `public`（无密钥，SPA）/ `confidential`（有密钥，服务端）
  - `redirect_uris` — 允许的回调地址（JSON 数组）
  - `grant_types` — 允许的授权类型（逗号分隔）
  - `allowed_scopes` — 允许请求的 scope
  - `pkce_required` — 是否强制 PKCE
  - `status` — 0-禁用 / 1-启用
  - `description` — 描述

### 9.3 `cmx_auth_token_event`

- **用途**：Token 签发/撤销/刷新等关键审计事件（append-only 事件流）
- **关键字段**：
  - `event_type` — 事件类型：`token_issued`/`token_revoked`/`token_refreshed`/`login_success`/`login_failed`/`password_changed`
  - `user_id` — 用户 ID
  - `jti` — JWT ID（关联 Token）
  - `detail` — 事件详情（JSON）
- **清理策略**：每天凌晨归档 30 天前的事件记录

### 9.4 `cmx_auth_api_key`

- **用途**：API Key 存储
- **关键字段**：
  - `key_prefix` — Key 前缀（人类可读标识，可选，配置未填时从 key 前 8 位提取）
  - `key_hash` — SHA256 哈希（明文不存储）
  - `user_id` — 关联用户 ID（可选）
  - `service_name` — 关联服务名
  - `scopes` — 允许的 scope（TOML 数组 / 数据库逗号分隔）
  - `rate_limit` — 速率限制（请求 / 秒，可选）
  - `expires_at` — 过期时间（NULL = 永不过期）
  - `status` — 0-禁用 / 1-启用

### 9.5 `cmx_auth_password_history`

- **用途**：密码历史（防重复使用）
- **关键字段**：
  - `user_id` — 用户 ID
  - `password_hash` — 密码哈希
- **策略**：保留最近 5 次密码历史

### 9.6 `cmx_auth_oauth2_account`

- **用途**：第三方 OAuth2 账号关联
- **关键字段**：
  - `user_id` — 本地用户 ID
  - `provider` — Provider 标识（`google` / `github` / 自定义）
  - `provider_user_id` — Provider 侧用户 ID
  - `provider_username` / `provider_email` / `provider_email_verified` — Provider 侧信息
  - `provider_display_name` / `provider_avatar_url` — Provider 侧展示信息
  - `last_login_at` — 最近一次通过此 Provider 登录时间
  - `status` — 0-禁用 / 1-启用
- **唯一约束**：`(provider, provider_user_id)` 联合唯一

### 9.7 表关系图（文字版）

- `cmx_user`（1）—（N）`cmx_auth_token_event`
- `cmx_user`（1）—（N）`cmx_auth_password_history`
- `cmx_user`（1）—（N）`cmx_auth_oauth2_account`
- `cmx_user`（1）—（N）`cmx_auth_api_key`（可选）
- `cmx_auth_client`（独立，无外键关联）

详细 DDL 参见架构方案 [§4.4 数据库表结构设计](.trae/documents/20260615_cmx-auth_企业级统一认证模块架构方案.md)。

***

## 十、Redis Key 设计

所有 Key 统一使用 `cmx:` 前缀（由 `cmx-buffer` 的 `CacheManager.build_key` 自动添加）。本节列出的 Key 不含前缀。

### 10.1 完整 Key 清单

| Key 模式 | 数据结构 | TTL | 用途 |
|----------|----------|-----|------|
| `auth:{user_id}:refresh:{jti}` | String (user_id) | refresh_ttl_secs (7天) | Refresh Token 存活标记 |
| `auth:{user_id}:refresh_index` | Set (jti) | 7 天 | 用户所有 refresh token 索引 |
| `auth:{user_id}:session` | Hash (device_type → JSON) | 手动管理 | 用户会话表 |
| `auth:{session_id}:session_detail` | String (JSON) | idle_timeout (24h) | 会话详情 |
| `auth:{jti}:blacklist` | String | = access token 剩余 TTL | 被撤销的 access token |
| `auth:online:users` | Set (user_id) | 无 | 全局在线用户集合 |
| `auth:oauth2:csrf:{state}` | String | 10 分钟 | OAuth2 授权码流程 CSRF 校验 |
| `auth:oauth2:authcode:{code}` | String (JSON) | 10 分钟 | OAuth2 授权码 |
| `auth:oauth2:authcode:{code}:used` | String | 10 分钟 | 已使用授权码标记（防重放） |
| `auth:oauth2:provider:state:{state}` | String (provider_name) | 600s | 第三方 OAuth2 CSRF state |
| `auth:oauth2:provider:callback:{code}` | String (TokenPair JSON) | 30s | 回调一次性授权码 → Token |
| `auth:{username}:login_fail` | String (count) | lock_duration (15min) | 登录失败计数 |
| `auth:{username}:locked` | String | lock_duration (15min) | 账号锁定标记 |
| `auth:api_key:{key_prefix}` | String (ApiKeyEntity JSON) | 60s | API Key 元数据缓存（第一层） |
| `auth:api_key_ctx:{key_prefix}` | String (AuthContext JSON) | 60s | API Key 认证上下文缓存（第二层） |
| `auth:cache:invalidate` | Pub/Sub Channel | — | 缓存失效广播 |

### 10.2 Hash Tag 说明（Redis Cluster 关键）

**所有同一 user_id 的 Key 统一使用 Hash Tag `{user_id}`**，保证 Redis Cluster 模式下路由到同一 slot，可执行事务 / Lua 原子操作。

例如：

- `auth:{user_id}:refresh:{jti}` — 用 `{user_id}` 作为 Hash Tag
- `auth:{user_id}:session` — 用 `{user_id}` 作为 Hash Tag
- `auth:oauth2:csrf:{state}` — 用 `{state}` 作为 Hash Tag

**注意**：`auth:online:users` 不绑定特定 user_id，是全局集合。

### 10.3 原子操作（Lua 脚本）

#### Refresh Token Rotation

```
-- rotate.lua
-- KEYS[1] = auth:{user_id}:refresh:{old_jti}
-- KEYS[2] = auth:{user_id}:refresh:{new_jti}
-- KEYS[3] = auth:{user_id}:refresh_index
-- ARGV[1] = user_id
-- ARGV[2] = ttl
-- ARGV[3] = old_jti
-- ARGV[4] = new_jti
local old = redis.call('GET', KEYS[1])
if not old then
  return nil  -- 已被使用或已撤销 → 重放攻击
end
redis.call('DEL', KEYS[1])
redis.call('SREM', KEYS[3], ARGV[3])
redis.call('SET', KEYS[2], ARGV[1], 'EX', ARGV[2])
redis.call('SADD', KEYS[3], ARGV[4])
return ARGV[4]
```

#### 第三方 OAuth2 State 消费

```
-- consume_oauth2_provider_state.lua
local value = redis.call('GET', KEYS[1])
if not value then return nil end
redis.call('DEL', KEYS[1])
return value
```

### 10.4 缓存清理任务

- **过期会话清理**：每 5 分钟扫描 `auth:online:users`，检查 `last_active_at`，过期则删除
- **Token 记录归档**：每天凌晨归档 30 天前的 `cmx_auth_token_event`
- **本地缓存失效**：通过 Pub/Sub `auth:cache:invalidate` 频道广播，秒级生效

### 10.5 Redis Cluster 部署建议

- 使用 Hash Tag 策略（同上）
- Slot 数量：建议 16384（默认）
- 主从配置：每 Master 至少 1 Slave
- 监控项：QPS、慢查询、内存使用、连接数

***

## 十一、安全机制

### 11.1 时序攻击防护（用户名枚举）

- 用户不存在时也执行 Argon2 dummy verify
- 消除"用户存在"与"用户不存在"的响应时间差异
- 攻击者无法通过时间判断用户名是否存在

### 11.2 密码安全

| 措施 | 实现 |
|------|------|
| Argon2id 哈希 | 64MB / 3 / 4（OWASP 推荐） |
| 密码策略 | 最少 8 位，含大小写 + 数字 + 特殊字符 |
| 密码历史 | 禁止最近 5 次密码重复 |
| 慢哈希 | Argon2 故意慢（~100ms），防暴力破解 |
| 明文不落库 | 哈希后存入 `cmx_user.password_hash` |

### 11.3 Token 安全

| 机制 | 实现 |
|------|------|
| 签名验证 | JWT 签名（HS256 / RS256） |
| 黑名单 | Access Token 加入 Redis 黑名单（TTL = 剩余有效期） |
| Rotation | Refresh Token 每次刷新轮换 |
| 重放检测 | Rotation Lua 检测到重放时撤销该用户所有 Token |
| 短期有效期 | Access Token 默认 30 分钟 |
| Session 校验 | 校验 Token 时检查会话是否活跃 |

### 11.4 CSRF 防护

- **OAuth2 授权码流程**：state 参数 + Redis 存储 + 一次性消费
- **第三方 Provider 登录**：state 参数 + 原子 Lua 消费
- **Token 传递**：Authorization Header（非 Cookie），天然免疫 CSRF

### 11.5 PKCE（防止授权码截获）

- OAuth2 授权码模式强制 PKCE
- 客户端生成 `code_verifier` → SHA256 → `code_challenge`
- 换 Token 时提交 `code_verifier`，服务端校验
- 授权码被截获也无法换取 Token

### 11.6 Open Redirect 防护

- `redirect_uri` 在服务端配置（`OAuth2ProviderConfig.redirect_uri`）
- 前端不可覆盖
- 授权码换 Token 时校验 `redirect_uri` 一致性

### 11.7 ID Token 签名验证（Google）

- Google ID Token 必须验证签名（RS256）
- JWKS 公钥从 `https://www.googleapis.com/oauth2/v3/certs` 获取并缓存 24 小时
- 验证 `iss`（签发者 = accounts.google.com）
- 验证 `aud`（受众 = 自己的 client_id）
- 验证 `exp`（未过期）
- 防止伪造 ID Token 冒充用户登录

### 11.8 账号锁定（暴力破解防护）

- 连续 `max_login_attempts`（默认 5）次登录失败 → 账号锁定
- 锁定时长 `lock_duration_secs`（默认 15 分钟）
- Redis 存储，支持分布式
- **建议**：生产环境结合 WAF / API Gateway 的 IP 限流

### 11.9 最后一个绑定不可解绑

- 用户解绑第三方账号前检查：是否还有密码 或 其他第三方绑定
- 若都没有，拒绝解绑（防止用户解绑后无法登录）

### 11.10 Pub/Sub 跨实例缓存失效

- 撤销 Token / 强制下线时通过 Redis Pub/Sub 广播 `invalidate` 事件
- 各实例收到后主动 `local_cache.invalidate()`
- 延迟从 30s（TTL 兜底）降到**秒级**

### 11.11 授权码一次性使用

- OAuth2 授权码只能用一次（换 Token 后立即删除 + 标记为 `used`）
- 第三方 Provider 回调授权码 30 秒 TTL + 一次性消费
- 防止重放攻击

### 11.12 时序安全的失败计数

- 登录失败计数用 `INCR` + `EXPIRE`（每次都 expire，幂等安全）
- 避免 `INCR` 后崩溃导致永久 key

### 11.13 白名单规则编译失败降级

- 启动时白名单规则编译失败仅记录警告，不中断启动
- 攻击者无法通过错误规则注入恶意路径

***

## 十二、可观测性

### 12.1 Prometheus 指标

`cmx-auth` 暴露 7 个指标（命名空间 `cmx`）：

| 指标名 | 类型 | 标签 | 含义 |
|--------|------|------|------|
| `cmx_auth_login_total` | IntCounterVec | `method` | 登录总数（`method` = `password` / `third_party_oauth2`） |
| `cmx_auth_login_failed_total` | IntCounterVec | `reason` | 登录失败数（`reason` = `invalid_credentials` 等） |
| `cmx_auth_token_validate_duration_seconds` | HistogramVec | `method` | Token 验证耗时（`method` = `jwt_bearer`） |
| `cmx_auth_active_sessions` | IntGauge | — | 活跃会话数 |
| `cmx_auth_online_users` | IntGauge | — | 在线用户数 |
| `cmx_auth_token_revoked_total` | IntCounterVec | `type` | Token 撤销数（`type` = `access` / `refresh`） |
| `cmx_auth_api_key_validations_total` | IntCounter | — | API Key 验证总数（M2M 场景，不计入 LOGIN_TOTAL） |

**指标初始化**：在 `web-server` 启动时调用 `cmx_auth::metrics::init_metrics()`，注册到 Prometheus 全局注册表。

**采集端点**：`GET /metrics`（由 cmx-api 暴露）

### 12.2 健康检查

`GET /api/auth/health` 返回：

```
{
  "code": 0,
  "data": {
    "redis": true,
    "jwt_keys": true,
    "status": "healthy"
  }
}
```

字段说明：

- `redis` — Redis 连通性
- `jwt_keys` — JWT 密钥可用性
- `status` — 整体状态（`healthy` / `degraded`）

### 12.3 Tracing Span

认证相关 Span 命名：

- `auth_login` — 用户名密码登录
- `auth_oauth2` — OAuth2 授权码认证
- `auth_third_party_oauth2` — 第三方 OAuth2 登录

Span 字段：

- `user_id` — 用户 ID
- `auth_method` — 认证方式
- `provider` — 第三方 Provider 名（仅第三方登录）
- `username` — 用户名（仅密码登录）

### 12.4 审计日志

通过 `cmx-audit` crate 集成，域为 `AuditDomain::Auth`：

| 操作 | 结果 | 说明 |
|------|------|------|
| `login` | Success / Failure | 登录 |
| `logout` | Success | 登出 |
| `change_password` | Success | 改密 |
| `verify_credentials` | Success | 凭据验证（OAuth2 流程） |
| `oauth2_login` | Success | 第三方 OAuth2 登录 |
| `oauth2_link` | Success | 第三方账号绑定 |
| `oauth2_unlink` | Success | 第三方账号解绑 |
| `token_issued` | Success | Token 签发 |
| `token_revoked` | Success | 单 Token 撤销 |
| `tokens_revoked` | Success | 全部 Token 撤销 |
| `password_changed` | Success | 密码修改 |

### 12.5 关键告警建议

| 告警项 | 阈值 | 含义 |
|--------|------|------|
| `cmx_auth_login_failed_total` 增长率 | > 100/分钟 | 可能的暴力破解 |
| `cmx_auth_token_validate_duration_seconds` P99 | > 50ms | 本地缓存命中率下降 |
| `cmx_auth_active_sessions` 突增 | 翻倍 | 异常活动 |
| Redis 连接失败 | 1 次 | 影响所有 Token 操作 |

***

## 十三、运维与排障

### 13.1 常见运维操作

#### 13.1.1 重置超管密码

- 方案 A：直接修改 `config.toml` 的 `password` 字段，重启服务（会自动 UPSERT 超管）
  - 适合：开发 / 测试环境
  - 影响：服务中断
- 方案 B：登录超管后通过改密接口修改（推荐）
  - 影响：无中断
- 方案 C：直接修改数据库 `cmx_user.password_hash`（用 `Argon2Hasher::hash` 生成新哈希）
  - 适合：忘记超管密码且无法登录的情况

#### 13.1.2 强制下线某个用户

- 管理员调用 `POST /api/auth/revoke-all {user_id}`（需 `system:auth:kick` 权限）
- 撤销该用户所有 Access + Refresh Token
- 销毁所有会话
- 通过 Pub/Sub 广播，所有实例秒级生效

#### 13.1.3 撤销单个 Token

- 调用 `POST /api/auth/logout {token}`，将指定 Token 加入黑名单
- 撤销 Access 时 TTL = 剩余有效期
- 撤销 Refresh 时直接删除 Redis

#### 13.1.4 轮换 JWT 密钥

**场景**：怀疑密钥泄露 / 定期轮换

1. 生成新 RSA 密钥对：`openssl genpkey -algorithm RSA -out new-private.pem -pkeyopt rsa_keygen_bits:2048` 和 `openssl rsa -in new-private.pem -pubout -out new-public.pem`
2. 更新 `config.toml`：
   - `private_key` / `public_key` 指向新密钥
   - `current_kid` 设置为新值（如 `"key-2026-07"`）
3. 将旧公钥加入 `[[auth.jwt.legacy_public_keys]]`：
   - `kid` = 旧 kid
   - `pem` = 旧公钥
4. 滚动重启服务（先启动新实例再停旧实例）
5. **宽限期内**：旧 kid 签发的 Token 仍可通过旧公钥验签
6. 宽限期结束（建议 = `refresh_ttl_secs`，7 天）：所有旧 Token 已过期或刷新，可移除 `legacy_public_keys`

#### 13.1.5 清理过期会话

**自动清理**：`start_cleanup_task` 启动时启动定时任务（间隔 ≥ 5 分钟），扫描 `auth:online:users`，清理 `last_active_at` 超过 `idle_timeout_secs` 的会话。

**手动清理**：直接删除 Redis 中以下 Key：

- `auth:online:users` — 清空在线用户集合
- `auth:{user_id}:session` — 删除单个用户的所有会话
- `auth:{session_id}:session_detail` — 删除单个会话详情

#### 13.1.6 重建 Redis 缓存

如果 Redis 重启或数据损坏，需重建：

1. Access Token 黑名单：无需重建（短期有效，最长 30 分钟过期）
2. Refresh Token 索引：用户需重新登录（无法重建）
3. 在线用户集合：从 `cmx_auth_token_event` 表查最近活跃用户写回（可选）
4. 会话详情：同 3

**建议**：Redis 启用 AOF + 定期 RDB 备份，避免重启导致认证状态丢失。

#### 13.1.7 调整白名单

**新增 / 修改**：编辑 `config.toml` 的 `[auth].whitelist`，重启服务生效。

**验证生效**：

- 查看启动日志 `认证白名单初始化完成` 与规则数量
- 调用受保护路径应不再要求 Token
- 调用非白名单路径应仍要求 Token

**回滚**：删除 `config.toml` 中的 `whitelist` 配置项，重启服务即可恢复仅内置白名单。

### 13.2 常见问题排查

#### Q1：登录返回 401 "用户名或密码错误"

排查步骤：

1. 确认用户名 / 密码正确
2. 检查 `cmx_user.status` 是否为 1（启用）
3. 检查 `cmx_user.password_hash` 是否存在
4. 确认用户未被锁定（Redis `auth:{username}:locked` 是否存在）
5. 确认 `UserAuthQuery` 实现正常工作（cmx-iam 是否启动）

#### Q2：调用受保护接口返回 401 "Token 无效"

排查步骤：

1. 确认请求头 `Authorization: Bearer <token>` 格式正确
2. 检查 Token 是否过期（`access_expires_at`）
3. 检查 Token 是否被撤销（Redis `auth:{jti}:blacklist`）
4. 检查会话是否过期（`cmx-auth::start_cleanup_task` 是否启动）
5. 确认路径不在白名单中：检查 `cmx_auth::config::BUILTIN_WHITELIST` 和 `config.toml` 的 `[auth].whitelist` 合并结果（详见 [六章](#六路由白名单)）

#### Q3：JWT 签名验证失败

排查步骤：

1. 确认 Token Header 中的 `kid` 在 `current_kid` 或 `legacy_public_keys` 中
2. 检查密钥文件路径 / 权限（生产建议 `chmod 600`）
3. 确认 `algorithm` 配置正确（HS256 对称 / RS256 非对称）
4. 检查密钥是否在轮换宽限期内

#### Q4：第三方 OAuth2 登录失败

排查步骤：

1. 确认 Provider 在 `[[auth.oauth2.providers]]` 中配置且 `enabled = true`
2. 确认 `redirect_uri` 与 Provider 后台注册完全一致
3. 确认 `client_id` / `client_secret` 正确
4. 检查 Google JWKS 缓存（24h TTL）是否过期
5. 查看 Provider 端点可达性（`token_url` / `userinfo_url`）

#### Q5：账号频繁被锁定

可能原因：

- 用户忘记密码
- 自动化脚本密码错误
- 暴力破解攻击

解决方案：

- 临时调高 `max_login_attempts` 或 `lock_duration_secs`
- 部署 WAF / API Gateway 增加 IP 限流
- 通过 API 主动解锁：删除 Redis Key `auth:{username}:locked`

#### Q6：本地缓存导致强制下线不生效

- 检查 `enable_local_cache` 配置（应为 `true`）
- 检查 Pub/Sub 订阅是否启动（`GlobalSubscriberManager::initialize`）
- 等待 `local_ttl_secs`（默认 30 秒）后自然失效
- 主动触发：调用 `revoke_all_tokens` 后通过 Pub/Sub 广播

#### Q7：Token 验证慢

排查步骤：

1. 检查 Redis 连接（网络延迟、QPS 限制）
2. 检查本地缓存命中率（`cmx_auth_token_validate_duration_seconds` P99）
3. 检查 moka 缓存大小（`local_cache_max_entries`）
4. 考虑调高 `local_ttl_secs`（延长本地缓存时间，提高命中率）

#### Q8：Redis Cluster 下 Lua 脚本失败

原因：Lua 脚本操作的多个 Key 不在同一 slot。

解决方案：

- 确认所有 Key 使用 Hash Tag（参考 [10.2](#102-hash-tag-说明redis-cluster-关键)）
- Refresh Token Rotation：`auth:{user_id}:refresh:{jti}` 和 `auth:{user_id}:refresh_index` 必须用同一 `{user_id}`
- 第三方 OAuth2 State 消费：`auth:oauth2:provider:state:{state}` 用 `{state}` 作为 Hash Tag

#### Q9：白名单规则不生效

排查步骤：

1. 检查 TOML `[auth].whitelist` 拼写是否正确（注意是 `auth.whitelist` 而非 `[auth.whitelist]`）
2. 查看启动日志中"白名单规则编译失败"警告，定位语法错误
3. 确认路径规则**与请求路径完全匹配**（区分前缀 / 单层 / 多层）
4. 重启服务生效

### 13.3 性能调优

| 项 | 默认 | 调优建议 |
|----|------|---------|
| Access Token TTL | 30min | 调低 → 安全性↑、刷新频率↑、性能↓ |
| Argon2 memory_cost | 64MB | 调高 → 安全性↑、登录延迟↑；调低 → 反之 |
| local_ttl_secs | 30s | 调高 → 命中率↑、实时性↓；调低 → 反之 |
| max_login_attempts | 5 | 调低 → 更严格；调高 → 容忍误操作 |
| local_cache_max_entries | 10000 | 调高 → 命中率↑、内存↑；调低 → 反之 |
| 白名单规则数量 | 无限制 | 规则越多正则匹配耗时越长，建议 < 200 条 |

**基准性能**（参考）：

- 登录（含 Argon2）：< 200ms
- Token 校验（本地缓存命中）：< 1ms
- Token 校验（缓存未命中，2 次 Redis）：< 10ms
- 白名单正则匹配：< 0.1ms / 请求
- QPS（本地缓存命中）：> 10000
- QPS（缓存未命中）：> 5000

### 13.4 监控面板建议

Grafana 面板推荐指标：

- `rate(cmx_auth_login_total[5m])` — 登录速率
- `rate(cmx_auth_login_failed_total[5m])` — 登录失败速率（告警）
- `cmx_auth_online_users` — 在线用户数
- `cmx_auth_active_sessions` — 活跃会话数
- `histogram_quantile(0.99, rate(cmx_auth_token_validate_duration_seconds_bucket[5m]))` — Token 验证 P99 延迟

### 13.5 安全检查清单

部署到生产前确认：

- JWT 密钥已替换为随机长字符串（或使用 RS256 + 文件路径 + chmod 600）
- `config.toml` 设置 `chmod 600`，仅运行用户可读
- 超管密码已修改（部署后立即）
- HTTPS 已配置（避免 Token 明文传输）
- Redis 已设置密码（生产建议）
- 数据库已限制访问（最小权限原则）
- 监控告警已配置（登录失败、Token 验证延迟等）
- WAF / API Gateway 已部署（防 CC / 暴力破解）
- 白名单规则已审计，避免过宽规则（如 `/api/**`）

### 13.6 升级与迁移

#### 13.6.1 数据库迁移

启用 `[migration] enabled = true` 后，启动时自动执行待执行的 SQL 迁移文件。

新增表（如 `cmx_auth_oauth2_account`）：直接执行迁移，无需手工建表。

#### 13.6.2 配置兼容

- **白名单迁移**：从源码 `AUTH_WHITELIST` 迁移到 TOML `[auth].whitelist` 时，将原硬编码路径复制到 TOML 即可（合并去重逻辑已兼容）
- **API Key 兼容**：`key_prefix` 字段**保持可选**，已有配置无需修改
- **新增配置项**：使用默认值，向后兼容
- **修改配置项行为**：重大变更前发布迁移指南
- **删除配置项**：保留 1-2 个版本作为警告，再删除

#### 13.6.3 密钥轮换

参考 [13.1.4 轮换 JWT 密钥](#1314-轮换-jwt-密钥)。生产环境建议**至少每 90 天轮换一次**。

***

## 附录 A：环境变量覆盖格式

所有 `[auth.*]` 配置项支持环境变量覆盖，格式为 `<SECTION>__<KEY>=<VALUE>`（双下划线分隔段名，TOML 数组使用数字索引）。

| TOML 路径 | 环境变量 |
|-----------|---------|
| `auth.jwt.algorithm` | `AUTH__JWT__ALGORITHM` |
| `auth.jwt.secret` | `AUTH__JWT__SECRET` |
| `auth.token.access_ttl_secs` | `AUTH__TOKEN__ACCESS_TTL_SECS` |
| `auth.super_admin.password` | `AUTH__SUPER_ADMIN__PASSWORD` |
| `auth.static_api_keys[0].key` | `AUTH__STATIC_API_KEYS_0__KEY` |
| `auth.oauth2.providers[0].client_secret` | `AUTH__OAUTH2__PROVIDERS_0__CLIENT_SECRET` |
| `auth.oauth2.frontend_callback_url` | `AUTH__OAUTH2__FRONTEND_CALLBACK_URL` |
| `auth.whitelist` | `AUTH__WHITELIST__0` / `AUTH__WHITELIST__1`（数组索引） |

完整说明参考 [config/ENV_MANUAL.md](../../../config/ENV_MANUAL.md)。

## 附录 B：相关文档

- **架构方案**：[.trae/documents/20260615_cmx-auth_企业级统一认证模块架构方案.md](../../documents/20260615_cmx-auth_企业级统一认证模块架构方案.md)
- **第三方 OAuth2 对接方案**：[.trae/documents/20260615_cmx-auth_第三方OAuth2Provider对接方案.md](../../documents/20260615_cmx-auth_第三方OAuth2Provider对接方案.md)
- **配置项字典**：[config/CONFIG_MANUAL.md](../../../config/CONFIG_MANUAL.md)
- **配置模板**：[config/config_template.toml](../../../config/config_template.toml)
- **环境变量**：[config/.env.template](../../../config/.env.template)
- **源码**：
  - [crates/libs/cmx-infra/cmx-auth/src/lib.rs](../src/lib.rs) — 模块声明
  - [crates/libs/cmx-infra/cmx-auth/src/auth_service_impl.rs](../src/auth_service_impl.rs) — AuthService 实现（含 API Key 两层缓存）
  - [crates/libs/cmx-infra/cmx-auth/src/config.rs](../src/config.rs) — 配置定义
  - [crates/libs/cmx-infra/cmx-auth/src/metrics.rs](../src/metrics.rs) — Prometheus 指标
  - [crates/libs/cmx-infra/cmx-auth/src/api_key/manager.rs](../src/api_key/manager.rs) — API Key 管理器（第一层缓存）
  - [crates/libs/cmx-api/src/middleware/mw_auth.rs](../../cmx-api/src/middleware/mw_auth.rs) — 路由白名单（编译正则、合并、查询）
  - [crates/libs/cmx-api/src/handlers/auth/handler.rs](../../cmx-api/src/handlers/auth/handler.rs) — 核心认证接口（含 `/api/auth/me`）
  - [crates/libs/cmx-api/src/handlers/auth/api_key_handler.rs](../../cmx-api/src/handlers/auth/api_key_handler.rs) — API Key 管理接口
  - [crates/libs/cmx-api/src/handlers/auth/oauth2_client_handler.rs](../../cmx-api/src/handlers/auth/oauth2_client_handler.rs) — OAuth2 客户端管理接口
  - [crates/libs/cmx-api/src/handlers/auth/oauth2_provider_handler.rs](../../cmx-api/src/handlers/auth/oauth2_provider_handler.rs) — 第三方 OAuth2 Provider 接口

## 附录 C：版本与变更

| 版本 | 日期 | 变更 |
|------|------|------|
| v1 | 2026-06-16 | 初版，基于 cmx-auth v6 架构方案 |
| v2 | 2026-06-16 | 更新：**白名单支持通配符**（`*` / `**` / `?`，编译为正则）；**`static_api_keys.key_prefix` 改为可选**（自动从 `key` 前 8 位提取）；新增 5.10 `[auth] whitelist` 配置段；重写第六章路由白名单 |
| v3 | 2026-06-24 | 更新：新增 `GET /api/auth/me` 接口；新增 8.4 API Key 管理接口（create / list / delete / toggle-status）；新增 8.5 OAuth2 客户端管理接口（create / list / update / delete）；补充 4.2 API Key **两层缓存**说明（第一层 `ApiKeyEntity` + 第二层 `AuthContext`，TTL 60s，Pub/Sub 失效）；Redis Key 清单新增 `auth:api_key:{key_prefix}` 和 `auth:api_key_ctx:{key_prefix}` |

***

> 本文档为使用与配置指南，不含代码实现。如需了解实现细节，请参考架构方案与源码。
