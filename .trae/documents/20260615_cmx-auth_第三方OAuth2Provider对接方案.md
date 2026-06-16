# cmx-auth 第三方 OAuth2 Provider 对接方案

> 日期：2026-06-15 | 模块：cmx-auth | 状态：方案设计 v6
>
> 关联文档：
>
> * `20260615_cmx-auth_企业级统一认证模块架构方案.md`（原方案，本文为其补充/完善）
> * `20260615_cmx-auth_第三方OAuth2Provider对接方案_评审报告.md`（v1/v2/v3 评审报告）
>
> 变更摘要：原方案中 OAuth2 仅实现 Authorization Server（自建授权服务），本次扩展为同时支持 **OAuth2 Client**（对接第三方 Provider 如 Google/GitHub 等 Social Login），并在 TOML 配置中支持声明多个第三方 OAuth2 Provider 的连接信息。
>
> v2 变更（基于 v1 评审报告 21 项问题）：
>
> | 评审编号 | 问题 | v2 处理 |
> |----------|------|---------|
> | 冲突-01 | `get_user_by_id` 已存在 | 从新增方法列表移除，标注为已有方法 |
> | 冲突-02 | `UserAuthData` 缺 `email` | 经核实 `UserAuthData` 已有 `email` 字段，无需扩展 |
> | 冲突-03 | `user.org_id` 不存在 | 统一传 `None`，与现有密码/APIKey/OAuth2 分支一致 |
> | 冲突-04 | 白名单路径参数不匹配 | 改用前缀匹配 `/api/auth/oauth2/provider` |
> | 安全-01 | Token 通过 URL Query 传递 | 改为授权码模式：回调签发一次性授权码，前端 POST 换 Token |
> | 安全-02 | Google ID Token 未验证签名 | 增加 JWKS 公钥验证，启动时获取并缓存 |
> | 安全-03 | `email_verified` 检查未实现 | `ProviderUserInfo` 增加 `email_verified` 字段，`AccountLinker` 增加前置检查 |
> | 安全-04 | State 消费非原子 | 参照现有授权码消费模式，编写 Lua 脚本原子消费 |
> | 遗漏-01 | WeChat 非标准流程 | 延后至下期，本期仅支持标准 OAuth2 Provider |
> | 遗漏-02 | `redirect_uri` 配置缺失 | `OAuth2ProviderConfig` 增加 `redirect_uri`，服务端生成 |
> | 遗漏-03 | Generic userinfo 解析脆弱 | 增加 `field_mapping` 配置，支持 number→string 自动转换 |
> | 遗漏-04 | 用户名生成策略未说明 | `AccountLinkConfig` 增加 `username_strategy`，含冲突重试逻辑 |
> | 遗漏-05 | 解绑安全逻辑未实现 | 增加"最后一个绑定不可解绑"检查 |
> | 遗漏-06 | Provider 列表缺图标/品牌 | `ProviderInfo` 增加 `icon_url` / `brand_color` |
> | 遗漏-07 | 缺 `client_secret_basic` 认证 | 增加 `token_endpoint_auth_method` 配置 |
> | 架构-01 | Registry 全局注册不明确 | 纳入 `GlobalAuthService`，使用 `OnceLock` 模式 |
> | 架构-02 | 配置加载方式不可行 | 改为直接从 TOML 反序列化 `auth.oauth2` 段 |
> | 架构-03 | `trait.rs` 拆分过度 | 合并至 `provider/mod.rs` |
> | 架构-04 | 缺少 tracing 日志 | 关键节点增加结构化日志 |
> | 数据库-01 | `last_token_at` 写入频率高 | 改为 `last_login_at`，仅在会话创建时更新 |
> | 数据库-02 | 缺 `email_verified` 字段 | 表增加 `provider_email_verified` 列 |
>
> v3 变更（基于 v2 评审报告 8 项问题）：
>
> | 评审编号 | 问题 | v3 处理 |
> |----------|------|---------|
> | 新-01 | JWKS 缓存无过期刷新 | 增加 `JwksCache` 含 `expires_at`，24h TTL；kid 不匹配时强制刷新重试 |
> | 新-02 | `LinkAccountRequest` 仍含 `redirect_uri` | 移除，从 Provider 配置获取 |
> | 新-03 | 前端回调 URL 未配置 | `OAuth2Config` 增加 `frontend_callback_url` 字段 |
> | 新-04 | `generate_username` 冲突重试仅为 TODO | 完整实现：检查冲突 → 追加随机后缀 → 最多 3 次重试 → 失败返回错误 |
> | 新-05 | GitHub `email_verified` 始终 None | 额外调用 `/user/emails` API 获取验证状态 |
> | 新-06 | TOML `field_mapping` 写法错误 | 改为内联表写法 `{ key = "value", ... }` |
> | 新-07 | `issue_token_pair` 参数类型不一致 | 修正为 `device_info.as_ref()` 传引用 |
> | 新-08 | AccountLinker 数据访问方法未定义 | 补充 4 个方法签名 + SQL + `OAuth2Account` 模型 |
>
> v4 变更（基于 v3 评审报告 2 项问题）：
>
> | 评审编号 | 问题 | v4 处理 |
> |----------|------|---------|
> | 新-09 | `create_account` INSERT 缺少 `id` 列 | 改用 `GenericCrudService::create`，id 由 `HasSeaFields` 自动生成 |
> | 新-10 | 数据访问方法 API 与 `DatabaseManager` 不匹配 | 全部改用 `GenericCrudService` 风格（`list`/`create`/`count`/`delete`），与 `UserAuthQueryImpl` 一致；补充 `OAuth2AccountBmc`/`OAuth2AccountForCreate`/`OAuth2AccountFilter` 定义 |
>
> v5 变更（基于代码现状校验 4 项问题）：
>
> | 问题 | 修正 |
> |------|------|
> | `OAuth2AccountFilter` 写成 enum | 改为 struct + `#[derive(FilterNodes)]`，字段类型 `Option<OpValsString>` / `Option<OpValsInt64>`，"不等于"用 `OpValString::Not(value)` |
> | `from_dataset_row` 不存在 | 改为 `extract_oauth2_account(dataset)` 函数，使用 `row.get_by_name_as(schema, "col")` 逐字段提取 |
> | `OAuth2AccountForCreate` 缺 derive | 加 `#[derive(Fields)]` 自动生成 `HasSeaFields`，加 `#[serde(skip_serializing_if)]` |
> | `AccountLinker` 冗余 `db` 字段 | 移除 `db: DatabaseManager`，改用 `Self::get_db_manager()` + `Self::default_db_id()` 静态方法 |
>
> v6 变更（基于 v5 评审报告 1 项问题）：
>
> | 评审编号 | 问题 | v6 处理 |
> |----------|------|---------|
> | 新-11 | `modql` 依赖未列入 §8 新增依赖 | §8.2 补充 `modql = { workspace = true }`，用于 `FilterNodes`/`Fields` derive 宏 |

***

## 一、背景与目标

### 1.1 现状

当前 cmx-auth 的 OAuth2 模块仅实现了 **Authorization Server** 角色：

* `OAuth2FlowService`：管理本平台的授权码签发 + PKCE 验证
* `OAuth2Policy`：作为 AuthPolicy 策略之一，用本平台授权码换 user_id
* `OAuth2Config`：仅含 `auth_code_ttl_secs` + `pkce_required` 两个字段
* `Credentials::AuthorizationCode`：语义为"用本平台签发的授权码换 Token"

**无任何第三方 OAuth2 Provider 对接能力。**

### 1.2 目标

扩展 cmx-auth 支持 **OAuth2 Client** 角色，使用户可通过第三方 Provider（Google、GitHub、GitLab 等）登录，平台作为 OAuth2 Client 向 Provider 发起授权请求、交换 Token、获取用户信息，最终签发本平台 Token。

> **范围说明**：本期仅支持标准 OAuth2 Authorization Code Flow 的 Provider。WeChat 等非标准 OAuth2 Provider 延后至下期实现。

核心需求：

1. **配置化多 Provider**：在 TOML 中声明多个第三方 OAuth2 Provider 的 client_id/secret/endpoint 等信息
2. **标准 OAuth2 Authorization Code Flow**：支持向第三方 Provider 发起授权码流程
3. **用户关联**：第三方账号与本地用户的绑定/自动注册
4. **统一认证入口**：第三方 OAuth2 登录纳入现有 `AuthService::authenticate()` 策略模式
5. **安全 Token 传递**：回调采用授权码模式，避免 Token 暴露在 URL 中

***

## 二、架构设计

### 2.1 整体架构图（扩展部分）

```mermaid
graph TB
    subgraph "客户端"
        C1[Web 前端]
    end

    subgraph "协议层 (cmx-api)"
        AH_NEW[OAuth2 Provider Handler<br/>/api/auth/oauth2/provider/*]
    end

    subgraph "cmx-auth 认证核心（新增/扩展）"
        PROV[OAuth2ProviderRegistry<br/>Provider 注册表]
        GOOGLE[GoogleProvider<br/>OAuth2Provider trait 实现]
        GITHUB[GitHubProvider<br/>OAuth2Provider trait 实现]
        CUSTOM[CustomProvider<br/>通用 OAuth2 Provider]
        LINK[AccountLinker<br/>第三方账号关联/注册]
    end

    subgraph "外部服务"
        GP[Google OAuth2<br/>accounts.google.com]
        GHP[GitHub OAuth2<br/>github.com]
    end

    subgraph "存储层"
        PG[(PostgreSQL<br/>cmx_auth_oauth2_account)]
        RD[(Redis<br/>state 暂存 + 授权码)]
    end

    C1 -->|1. 点击"Google 登录"| AH_NEW
    AH_NEW -->|2. 获取 authorize URL| PROV
    PROV --> GOOGLE / GITHUB / CUSTOM
    C1 -->|3. 重定向到 Provider| GP / GHP
    GP / GHP -->|4. 回调带 code| AH_NEW
    AH_NEW -->|5. 交换 Token + 获取用户信息| PROV
    PROV --> LINK
    LINK --> PG
    AH_NEW -->|6. 签发一次性授权码| RD
    AH_NEW -->|7. 302 重定向前端| C1
    C1 -->|8. POST exchange 换 Token| AH_NEW
    AH_NEW -->|9. 返回 TokenPair| C1
```

### 2.2 核心交互时序：第三方 OAuth2 登录（授权码模式）

> **安全改进**（v2）：回调不再通过 URL Query String 传递 Token，改为签发一次性短生命周期授权码，前端通过 POST 交换 Token，避免 Token 泄露到浏览器历史/Referer/代理日志。

```mermaid
sequenceDiagram
    participant C as 客户端(前端)
    participant H as OAuth2 Provider Handler
    participant REG as OAuth2ProviderRegistry
    participant PROV as 具体 Provider(如 Google)
    participant EXT as 第三方 OAuth2 Provider
    participant LINK as AccountLinker
    participant UAQ as UserAuthQuery trait
    participant AS as AuthService
    participant RD as Redis
    participant PG as PostgreSQL

    Note over C: 1. 用户点击"Google 登录"
    C->>H: GET /api/auth/oauth2/provider/{provider}/authorize
    H->>REG: get_provider("google")
    REG-->>H: GoogleProvider
    H->>PROV: generate_state() → state
    H->>RD: SET auth:oauth2:provider:state:{state} {provider} (EX 600)
    H->>PROV: build_authorize_url(state, redirect_uri)
    PROV-->>H: https://accounts.google.com/o/oauth2/v2/auth?...
    H-->>C: 302 Redirect to Provider authorize URL

    Note over C: 2. 用户在 Provider 页面授权
    EXT->>C: 302 Redirect to callback URL?code=XXX&state=YYY

    C->>H: GET /api/auth/oauth2/provider/{provider}/callback?code=XXX&state=YYY
    H->>RD: Lua 原子消费 auth:oauth2:provider:state:{state} → 验证 state + 获取 provider
    H->>REG: get_provider(provider)
    H->>PROV: exchange_code(code, redirect_uri)
    PROV->>EXT: POST /token {code, client_id, client_secret, redirect_uri}
    EXT-->>PROV: {access_token, id_token, ...}
    PROV-->>H: ProviderTokenResponse

    H->>PROV: get_user_info(access_token/id_token)
    Note over PROV: Google: JWKS 验证 ID Token 签名后解析 claims
    PROV->>EXT: GET /userinfo (Authorization: Bearer ...) [降级路径]
    EXT-->>PROV: {sub, email, email_verified, name, ...}
    PROV-->>H: ProviderUserInfo {provider_user_id, email, email_verified, name, avatar_url}

    H->>LINK: find_or_link(provider, provider_user_id, user_info)
    LINK->>PG: SELECT FROM cmx_auth_oauth2_account WHERE provider + provider_user_id
    alt 已关联本地用户
        PG-->>LINK: existing_account (含 user_id)
        LINK-->>H: (user_id, is_new=false)
    else 未关联
        LINK->>UAQ: get_user_by_email(email) ← 自动关联匹配
        alt 邮箱匹配到本地用户 且 email_verified=true
            UAQ-->>LINK: Some(User)
            LINK->>PG: INSERT cmx_auth_oauth2_account (绑定)
            LINK-->>H: (user_id, is_new=false)
        else 无匹配 或 邮箱未验证
            LINK->>UAQ: create_user_from_oauth2(...) ← 自动注册
            UAQ-->>LINK: new_user_id
            LINK->>PG: INSERT cmx_auth_oauth2_account (绑定)
            LINK-->>H: (user_id, is_new=true)
        end
    end

    H->>AS: authenticate(Credentials::ThirdPartyOAuth2{...}, device_info)
    AS->>AS: 签发本平台 TokenPair（跳过密码验证）

    Note over H: v2: 签发一次性授权码，而非直接返回 Token
    H->>RD: SET auth:oauth2:provider:callback:{code} → TokenPair JSON (EX 30)
    H-->>C: 302 Redirect to {frontend_callback_url}?code={one_time_code}&state={original_state}

    Note over C: 3. 前端用授权码换 Token
    C->>H: POST /api/auth/oauth2/provider/exchange {code, state}
    H->>RD: Lua 原子消费 auth:oauth2:provider:callback:{code} → TokenPair JSON
    H-->>C: {access_token, refresh_token, is_new, ...}
```

### 2.3 模块结构扩展

在现有 `oauth2/` 目录下新增 `provider/` 子模块：

```
crates/libs/cmx-infra/cmx-auth/src/oauth2/
  ├── mod.rs                    # 模块声明（新增 provider 子模块导出）
  ├── flows.rs                  # Authorization Server 流程（不变）
  ├── pkce.rs                   # PKCE 验证器（不变）
  ├── store.rs                  # Authorization Server 存储（不变）
  └── provider/                 # 🆕 第三方 OAuth2 Provider Client
      ├── mod.rs                # OAuth2Provider trait 定义 + 导出
      ├── registry.rs           # OAuth2ProviderRegistry（名称 → Provider 实例映射）
      ├── google.rs             # Google OAuth2 实现（含 JWKS 验证）
      ├── github.rs             # GitHub OAuth2 实现
      ├── generic.rs            # 通用 OAuth2 Provider（适用于任何标准 OAuth2 Provider）
      └── account_linker.rs     # 第三方账号关联/注册逻辑
```

> **v2 变更**：移除 `trait.rs`，trait 定义直接放在 `provider/mod.rs`，与项目现有风格一致。移除 `wechat.rs`，WeChat 延后至下期。

***

## 三、核心设计

### 3.1 OAuth2Provider Trait

> 遵循现有 Strategy Pattern，定义统一的第三方 Provider 抽象。

```rust
// crates/libs/cmx-infra/cmx-auth/src/oauth2/provider/mod.rs
use async_trait::async_trait;
use cmx_traits::AuthError;
use serde::{Deserialize, Serialize};

/// 第三方 OAuth2 Provider 统一接口
#[async_trait]
pub trait OAuth2Provider: Send + Sync {
    /// Provider 唯一标识（如 "google", "github"）
    fn name(&self) -> &str;

    /// Provider 显示名称（如 "Google", "GitHub"）
    fn display_name(&self) -> &str;

    /// Provider 图标 URL（内置 Provider 提供默认值）
    fn icon_url(&self) -> Option<&str> { None }

    /// 品牌色（用于前端按钮样式，如 "#4285F4"）
    fn brand_color(&self) -> Option<&str> { None }

    /// 构建授权 URL（第一步：重定向用户到 Provider 授权页面）
    fn build_authorize_url(&self, state: &str, redirect_uri: &str, scopes: &[String]) -> String;

    /// 用授权码交换 Token（第二步：POST 到 Provider token endpoint）
    async fn exchange_code(
        &self,
        code: &str,
        redirect_uri: &str,
    ) -> Result<ProviderTokenResponse, AuthError>;

    /// 获取用户信息（第三步：用 access_token/id_token 获取用户信息）
    async fn get_user_info(
        &self,
        token_response: &ProviderTokenResponse,
    ) -> Result<ProviderUserInfo, AuthError>;

    /// Provider 特有的 scope 列表（默认值）
    fn default_scopes(&self) -> Vec<String>;
}

/// 第三方 Provider Token 响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderTokenResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: Option<u64>,
    pub refresh_token: Option<String>,
    pub scope: Option<String>,
    /// ID Token（OIDC Provider 如 Google 会返回）
    pub id_token: Option<String>,
}

/// 第三方 Provider 用户信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderUserInfo {
    /// Provider 侧的用户唯一标识
    pub provider_user_id: String,
    /// 邮箱（可能为空，取决于 scope）
    pub email: Option<String>,
    /// 邮箱是否已验证（v2 新增：auto_link_by_email 安全检查必需）
    pub email_verified: Option<bool>,
    /// 用户名
    pub username: Option<String>,
    /// 昵称/显示名
    pub display_name: Option<String>,
    /// 头像 URL
    pub avatar_url: Option<String>,
}
```

### 3.2 OAuth2ProviderRegistry

> **v2 变更**：Registry 纳入 `GlobalAuthService`，使用 `OnceLock` 全局单例模式，与现有 `GLOBAL_AUTH_SERVICE` / `GLOBAL_OAUTH2_POLICY` 一致。

```rust
// crates/libs/cmx-api/src/middleware/mw_auth.rs（扩展 GlobalAuthService）
use std::sync::OnceLock;
use cmx_auth::oauth2::provider::OAuth2ProviderRegistry;

static GLOBAL_AUTH_SERVICE: OnceLock<Arc<dyn AuthService>> = OnceLock::new();
static GLOBAL_OAUTH2_POLICY: OnceLock<Arc<cmx_auth::policy::OAuth2Policy>> = OnceLock::new();
// 🆕 第三方 OAuth2 Provider 注册表
static GLOBAL_OAUTH2_PROVIDER_REGISTRY: OnceLock<OAuth2ProviderRegistry> = OnceLock::new();

pub struct GlobalAuthService;

impl GlobalAuthService {
    // ... 现有方法不变 ...

    /// 🆕 初始化第三方 OAuth2 Provider 注册表
    pub fn initialize_provider_registry(registry: OAuth2ProviderRegistry) -> Result<(), String> {
        GLOBAL_OAUTH2_PROVIDER_REGISTRY.set(registry)
            .map_err(|_| "OAuth2 Provider 注册表已初始化".to_string())
    }

    /// 🆕 获取第三方 OAuth2 Provider 注册表
    pub fn get_provider_registry() -> Option<&'static OAuth2ProviderRegistry> {
        GLOBAL_OAUTH2_PROVIDER_REGISTRY.get()
    }
}
```

```rust
// crates/libs/cmx-infra/cmx-auth/src/oauth2/provider/registry.rs
use std::collections::HashMap;
use std::sync::Arc;
use cmx_traits::AuthError;
use serde::{Serialize, Deserialize};
use super::OAuth2Provider;

/// Provider 注册表
pub struct OAuth2ProviderRegistry {
    providers: HashMap<String, Arc<dyn OAuth2Provider>>,
}

impl OAuth2ProviderRegistry {
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
        }
    }

    /// 注册 Provider
    pub fn register(&mut self, provider: Arc<dyn OAuth2Provider>) {
        tracing::info!(provider = %provider.name(), display_name = %provider.display_name(), "注册 OAuth2 Provider");
        self.providers.insert(provider.name().to_string(), provider);
    }

    /// 获取 Provider
    pub fn get_provider(&self, name: &str) -> Result<Arc<dyn OAuth2Provider>, AuthError> {
        self.providers.get(name).cloned().ok_or_else(|| {
            AuthError::OAuth2ProviderNotFound(name.to_string())
        })
    }

    /// 列出所有已注册的 Provider
    pub fn list_providers(&self) -> Vec<ProviderInfo> {
        self.providers.values().map(|p| ProviderInfo {
            name: p.name().to_string(),
            display_name: p.display_name().to_string(),
            scopes: p.default_scopes(),
            icon_url: p.icon_url().map(String::from),
            brand_color: p.brand_color().map(String::from),
        }).collect()
    }
}

/// Provider 信息（供前端展示登录按钮）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderInfo {
    pub name: String,
    pub display_name: String,
    pub scopes: Vec<String>,
    /// Provider 图标 URL
    pub icon_url: Option<String>,
    /// 品牌色（用于按钮样式）
    pub brand_color: Option<String>,
}
```

### 3.3 通用 OAuth2 Provider 实现

> 适用于任何标准 OAuth2 Provider（未内置专用实现的 Provider），通过配置驱动。

> **v2 变更**：增加 `field_mapping` 支持，解决不同 Provider 字段名差异问题；增加 `token_endpoint_auth_method` 支持；增加 number→string 自动转换；增加 tracing 日志。

```rust
// crates/libs/cmx-infra/cmx-auth/src/oauth2/provider/generic.rs
use async_trait::async_trait;
use cmx_traits::AuthError;
use super::{OAuth2Provider, ProviderTokenResponse, ProviderUserInfo};
use crate::config::OAuth2ProviderConfig;
use std::collections::HashMap;

/// 通用 OAuth2 Provider（配置驱动）
pub struct GenericOAuth2Provider {
    config: OAuth2ProviderConfig,
    http_client: reqwest::Client,
}

impl GenericOAuth2Provider {
    pub fn new(config: OAuth2ProviderConfig) -> Self {
        let http_client = reqwest::Client::new();
        Self { config, http_client }
    }
}

#[async_trait]
impl OAuth2Provider for GenericOAuth2Provider {
    fn name(&self) -> &str { &self.config.name }
    fn display_name(&self) -> &str { &self.config.display_name }
    fn icon_url(&self) -> Option<&str> { self.config.icon_url.as_deref() }
    fn brand_color(&self) -> Option<&str> { self.config.brand_color.as_deref() }

    fn build_authorize_url(&self, state: &str, redirect_uri: &str, scopes: &[String]) -> String {
        let scopes_str = if scopes.is_empty() {
            self.config.scopes.join(" ")
        } else {
            scopes.join(" ")
        };
        format!(
            "{}?response_type=code&client_id={}&redirect_uri={}&scope={}&state={}",
            self.config.authorize_url,
            self.config.client_id,
            urlencoding::encode(redirect_uri),
            urlencoding::encode(&scopes_str),
            state,
        )
    }

    async fn exchange_code(
        &self,
        code: &str,
        redirect_uri: &str,
    ) -> Result<ProviderTokenResponse, AuthError> {
        tracing::info!(provider = %self.name(), "向第三方 Provider 交换 Token");
        tracing::debug!(provider = %self.name(), code = %code, "Token 交换请求参数");

        let mut req = self.http_client
            .post(&self.config.token_url);

        // 根据认证方式构建请求
        req = match self.config.token_endpoint_auth_method.as_deref() {
            "client_secret_basic" => {
                // HTTP Basic Auth: Base64(client_id:client_secret)
                req.basic_auth(&self.config.client_id, Some(&self.config.client_secret))
                    .form(&[
                        ("grant_type", "authorization_code"),
                        ("code", code),
                        ("redirect_uri", redirect_uri),
                    ])
            }
            _ => {
                // 默认 client_secret_post: form body 传递
                req.form(&[
                    ("grant_type", "authorization_code"),
                    ("code", code),
                    ("client_id", &self.config.client_id),
                    ("client_secret", &self.config.client_secret),
                    ("redirect_uri", redirect_uri),
                ])
            }
        };

        let resp = req.send().await
            .map_err(|e| {
                tracing::warn!(provider = %self.name(), error = %e, "Provider 服务不可达");
                AuthError::OAuth2ProviderUnavailable(e.to_string())
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            tracing::warn!(provider = %self.name(), status = %status, "Token 交换失败");
            return Err(AuthError::OAuth2ProviderTokenError(format!(
                "HTTP {}: {}", status, body
            )));
        }

        resp.json::<ProviderTokenResponse>().await
            .map_err(|e| {
                tracing::warn!(provider = %self.name(), error = %e, "Token 响应解析失败");
                AuthError::OAuth2ProviderTokenError(e.to_string())
            })
    }

    async fn get_user_info(
        &self,
        token_response: &ProviderTokenResponse,
    ) -> Result<ProviderUserInfo, AuthError> {
        tracing::info!(provider = %self.name(), "获取第三方 Provider 用户信息");

        let resp = self.http_client
            .get(&self.config.userinfo_url)
            .bearer_auth(&token_response.access_token)
            .send()
            .await
            .map_err(|e| {
                tracing::warn!(provider = %self.name(), error = %e, "用户信息请求失败");
                AuthError::OAuth2ProviderUnavailable(e.to_string())
            })?;

        if !resp.status().is_success() {
            return Err(AuthError::OAuth2ProviderUserInfoError(
                format!("HTTP {}", resp.status())
            ));
        }

        let json: serde_json::Value = resp.json().await
            .map_err(|e| AuthError::OAuth2ProviderUserInfoError(e.to_string()))?;

        // 使用 field_mapping 配置提取字段，支持 number→string 自动转换
        let mapping = &self.config.field_mapping;
        Ok(ProviderUserInfo {
            provider_user_id: Self::extract_string(&json, mapping, "provider_user_id"),
            email: Self::extract_string_opt(&json, mapping, "email"),
            email_verified: Self::extract_bool_opt(&json, mapping, "email_verified"),
            username: Self::extract_string_opt(&json, mapping, "username"),
            display_name: Self::extract_string_opt(&json, mapping, "display_name"),
            avatar_url: Self::extract_string_opt(&json, mapping, "avatar_url"),
        })
    }

    fn default_scopes(&self) -> Vec<String> {
        self.config.scopes.clone()
    }
}

impl GenericOAuth2Provider {
    /// 从 JSON 中提取字符串字段，支持 number→string 自动转换
    fn extract_string(json: &serde_json::Value, mapping: &HashMap<String, String>, field: &str) -> String {
        let json_key = mapping.get(field).map(|s| s.as_str()).unwrap_or(field);
        match json.get(json_key) {
            Some(v) => match v {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Number(n) => n.to_string(),
                _ => String::new(),
            },
            None => String::new(),
        }
    }

    /// 从 JSON 中提取可选字符串字段
    fn extract_string_opt(json: &serde_json::Value, mapping: &HashMap<String, String>, field: &str) -> Option<String> {
        let json_key = mapping.get(field).map(|s| s.as_str()).unwrap_or(field);
        json.get(json_key).and_then(|v| match v {
            serde_json::Value::String(s) => Some(s.clone()),
            serde_json::Value::Number(n) => Some(n.to_string()),
            _ => None,
        })
    }

    /// 从 JSON 中提取可选布尔字段
    fn extract_bool_opt(json: &serde_json::Value, mapping: &HashMap<String, String>, field: &str) -> Option<bool> {
        let json_key = mapping.get(field).map(|s| s.as_str()).unwrap_or(field);
        json.get(json_key).and_then(|v| v.as_bool())
    }
}
```

### 3.4 内置 Provider 实现（Google）

> **v2 变更**：增加 JWKS 公钥验证 ID Token 签名，解决伪造 ID Token 的安全漏洞。增加 tracing 日志。

```rust
// crates/libs/cmx-infra/cmx-auth/src/oauth2/provider/google.rs
use async_trait::async_trait;
use super::{OAuth2Provider, ProviderTokenResponse, ProviderUserInfo};
use crate::config::OAuth2ProviderConfig;

pub struct GoogleProvider {
    config: OAuth2ProviderConfig,
    http_client: reqwest::Client,
    /// Google JWKS 公钥缓存（含过期时间）
    jwks: tokio::sync::RwLock<JwksCache>,
}

/// JWKS 缓存（v3：增加过期刷新机制）
///
/// Google JWKS 密钥会定期轮换（通常每 24-48 小时），
/// 缓存过期后需重新获取，密钥轮换后旧缓存会导致验证失败。
struct JwksCache {
    keys: Option<serde_json::Value>,
    /// 缓存过期时间
    expires_at: Option<std::time::Instant>,
}

impl JwksCache {
    fn new() -> Self {
        Self { keys: None, expires_at: None }
    }

    fn is_valid(&self) -> bool {
        self.keys.is_some()
            && self.expires_at.map_or(false, |t| std::time::Instant::now() < t)
    }

    fn set(&mut self, keys: serde_json::Value, ttl: std::time::Duration) {
        self.keys = Some(keys);
        self.expires_at = Some(std::time::Instant::now() + ttl);
    }

    fn invalidate(&mut self) {
        self.keys = None;
        self.expires_at = None;
    }
}

impl GoogleProvider {
    pub fn new(config: OAuth2ProviderConfig) -> Self {
        Self {
            config,
            http_client: reqwest::Client::new(),
            jwks: tokio::sync::RwLock::new(JwksCache::new()),
        }
    }

    /// Google JWKS endpoint
    const JWKS_URL: &'static str = "https://www.googleapis.com/oauth2/v3/certs";
    /// JWKS 缓存有效期（24 小时）
    const JWKS_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(24 * 3600);

    /// 获取 Google JWKS 公钥（带缓存 + 过期刷新）
    async fn fetch_jwks(&self) -> Result<serde_json::Value, AuthError> {
        // 先读缓存
        {
            let cache = self.jwks.read().await;
            if cache.is_valid() {
                return Ok(cache.keys.as_ref().unwrap().clone());
            }
        }
        // 缓存过期或未命中，远程获取
        tracing::info!("获取 Google JWKS 公钥（缓存过期或首次获取）");
        let resp = self.http_client
            .get(Self::JWKS_URL)
            .send()
            .await
            .map_err(|e| AuthError::OAuth2ProviderUnavailable(e.to_string()))?;

        let jwks: serde_json::Value = resp.json().await
            .map_err(|e| AuthError::OAuth2ProviderUnavailable(e.to_string()))?;

        // 写入缓存
        let mut cache = self.jwks.write().await;
        cache.set(jwks.clone(), Self::JWKS_CACHE_TTL);

        Ok(jwks)
    }

    /// 强制刷新 JWKS 缓存（当 kid 不匹配时调用，Google 文档推荐做法）
    async fn force_refresh_jwks(&self) -> Result<serde_json::Value, AuthError> {
        {
            let mut cache = self.jwks.write().await;
            cache.invalidate();
        }
        self.fetch_jwks().await
    }

    /// 验证 ID Token 签名并解析 claims
    ///
    /// 验证步骤：
    /// 1. 解码 JWT header 获取 kid
    /// 2. 从 JWKS 查找匹配的公钥
    /// 3. 验证签名（RS256）
    /// 4. 验证 iss = "accounts.google.com"
    /// 5. 验证 aud = 自己的 client_id
    /// 6. 验证 exp 未过期
    async fn verify_id_token(&self, id_token: &str) -> Result<GoogleIdTokenClaims, AuthError> {
        use base64::engine::general_purpose::URL_SAFE_NO_PAD;
        use base64::Engine;

        // 1. 解码 JWT header
        let parts: Vec<&str> = id_token.split('.').collect();
        if parts.len() != 3 {
            return Err(AuthError::OAuth2ProviderTokenError("ID Token 格式无效".into()));
        }

        let header_json = URL_SAFE_NO_PAD.decode(parts[0])
            .map_err(|e| AuthError::OAuth2ProviderTokenError(format!("Header 解码失败: {}", e)))?;
        let header: serde_json::Value = serde_json::from_slice(&header_json)
            .map_err(|e| AuthError::OAuth2ProviderTokenError(format!("Header 解析失败: {}", e)))?;

        let kid = header["kid"].as_str()
            .ok_or_else(|| AuthError::OAuth2ProviderTokenError("ID Token 缺少 kid".into()))?;

        // 2. 获取 JWKS 并查找匹配公钥
        let jwks = self.fetch_jwks().await?;
        let key = jwks["keys"].as_array()
            .and_then(|keys| keys.iter().find(|k| k["kid"].as_str() == Some(kid)));

        // v3: kid 不匹配时强制刷新 JWKS 再重试一次（Google 文档推荐做法）
        let key = match key {
            Some(k) => k,
            None => {
                tracing::warn!(kid = %kid, "JWKS 中未找到匹配的 kid，强制刷新 JWKS");
                let refreshed_jwks = self.force_refresh_jwks().await?;
                refreshed_jwks["keys"].as_array()
                    .and_then(|keys| keys.iter().find(|k| k["kid"].as_str() == Some(kid)))
                    .ok_or_else(|| AuthError::OAuth2ProviderTokenError("JWKS 中未找到匹配的公钥（刷新后仍无）".into()))?
            }
        };

        let n = key["n"].as_str().ok_or_else(|| AuthError::OAuth2ProviderTokenError("JWKS 公钥缺少 n".into()))?;
        let e = key["e"].as_str().ok_or_else(|| AuthError::OAuth2ProviderTokenError("JWKS 公钥缺少 e".into()))?;

        // 3. 使用 RSA 公钥验证签名（借助 jsonwebtoken crate）
        // 注意：实际实现需引入 jsonwebtoken 依赖
        let decoding_key = jsonwebtoken::DecodingKey::from_rsa_components(n, e)
            .map_err(|e| AuthError::OAuth2ProviderTokenError(format!("公钥构建失败: {}", e)))?;

        let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::RS256);
        validation.set_issuer(&["accounts.google.com", "https://accounts.google.com"]);
        validation.set_audience(&[&self.config.client_id]);

        let claims: GoogleIdTokenClaims = jsonwebtoken::decode(id_token, &decoding_key, &validation)
            .map_err(|e| {
                tracing::warn!(error = %e, "Google ID Token 验证失败");
                AuthError::OAuth2ProviderTokenError(format!("ID Token 验证失败: {}", e))
            })?
            .claims;

        Ok(claims)
    }
}

/// Google ID Token Claims
#[derive(Debug, serde::Deserialize)]
struct GoogleIdTokenClaims {
    sub: String,
    email: Option<String>,
    email_verified: Option<bool>,
    name: Option<String>,
    picture: Option<String>,
}

#[async_trait]
impl OAuth2Provider for GoogleProvider {
    fn name(&self) -> &str { "google" }
    fn display_name(&self) -> &str { "Google" }
    fn icon_url(&self) -> Option<&str> { Some("https://www.gstatic.com/firebasejs/ui/identity/google.svg") }
    fn brand_color(&self) -> Option<&str> { Some("#4285F4") }

    fn build_authorize_url(&self, state: &str, redirect_uri: &str, scopes: &[String]) -> String {
        let scopes_str = if scopes.is_empty() {
            self.config.scopes.join(" ")
        } else {
            scopes.join(" ")
        };
        format!(
            "https://accounts.google.com/o/oauth2/v2/auth?response_type=code&client_id={}&redirect_uri={}&scope={}&state={}&access_type=offline&prompt=consent",
            self.config.client_id,
            urlencoding::encode(redirect_uri),
            urlencoding::encode(&scopes_str),
            state,
        )
    }

    async fn exchange_code(&self, code: &str, redirect_uri: &str) -> Result<ProviderTokenResponse, AuthError> {
        tracing::info!(provider = "google", "向 Google 交换 Token");

        let resp = self.http_client
            .post("https://oauth2.googleapis.com/token")
            .form(&[
                ("grant_type", "authorization_code"),
                ("code", code),
                ("client_id", &self.config.client_id),
                ("client_secret", &self.config.client_secret),
                ("redirect_uri", redirect_uri),
            ])
            .send()
            .await
            .map_err(|e| {
                tracing::warn!(provider = "google", error = %e, "Google Token 端点不可达");
                AuthError::OAuth2ProviderUnavailable(e.to_string())
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(AuthError::OAuth2ProviderTokenError(format!(
                "HTTP {}: {}", status, body
            )));
        }

        resp.json::<ProviderTokenResponse>().await
            .map_err(|e| AuthError::OAuth2ProviderTokenError(e.to_string()))
    }

    async fn get_user_info(&self, token_response: &ProviderTokenResponse) -> Result<ProviderUserInfo, AuthError> {
        // Google 优先从 ID Token 解析用户信息（需验证签名）
        if let Some(id_token) = &token_response.id_token {
            match self.verify_id_token(id_token).await {
                Ok(claims) => {
                    tracing::info!(provider = "google", sub = %claims.sub, "ID Token 验证成功，解析用户信息");
                    return Ok(ProviderUserInfo {
                        provider_user_id: claims.sub,
                        email: claims.email,
                        email_verified: claims.email_verified,
                        username: None,
                        display_name: claims.name,
                        avatar_url: claims.picture,
                    });
                }
                Err(e) => {
                    tracing::warn!(provider = "google", error = %e, "ID Token 验证失败，降级到 userinfo endpoint");
                }
            }
        }
        // 降级：调用 userinfo endpoint
        let resp = self.http_client
            .get("https://www.googleapis.com/oauth2/v3/userinfo")
            .bearer_auth(&token_response.access_token)
            .send()
            .await
            .map_err(|e| AuthError::OAuth2ProviderUnavailable(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(AuthError::OAuth2ProviderUserInfoError(
                format!("HTTP {}", resp.status())
            ));
        }

        let json: serde_json::Value = resp.json().await
            .map_err(|e| AuthError::OAuth2ProviderUserInfoError(e.to_string()))?;

        Ok(ProviderUserInfo {
            provider_user_id: json["sub"].as_str().unwrap_or("").to_string(),
            email: json["email"].as_str().map(String::from),
            email_verified: json["email_verified"].as_bool(),
            username: None,
            display_name: json["name"].as_str().map(String::from),
            avatar_url: json["picture"].as_str().map(String::from),
        })
    }

    fn default_scopes(&self) -> Vec<String> {
        vec!["openid".into(), "email".into(), "profile".into()]
    }
}
```

### 3.5 内置 Provider 实现（GitHub）

```rust
// crates/libs/cmx-infra/cmx-auth/src/oauth2/provider/github.rs
use async_trait::async_trait;
use super::{OAuth2Provider, ProviderTokenResponse, ProviderUserInfo};
use crate::config::OAuth2ProviderConfig;

pub struct GitHubProvider {
    config: OAuth2ProviderConfig,
    http_client: reqwest::Client,
}

impl GitHubProvider {
    pub fn new(config: OAuth2ProviderConfig) -> Self {
        Self { config, http_client: reqwest::Client::new() }
    }
}

#[async_trait]
impl OAuth2Provider for GitHubProvider {
    fn name(&self) -> &str { "github" }
    fn display_name(&self) -> &str { "GitHub" }
    fn icon_url(&self) -> Option<&str> { Some("https://github.githubassets.com/favicons/favicon-dark.svg") }
    fn brand_color(&self) -> Option<&str> { Some("#24292e") }

    fn build_authorize_url(&self, state: &str, redirect_uri: &str, scopes: &[String]) -> String {
        let scopes_str = if scopes.is_empty() {
            self.config.scopes.join(" ")
        } else {
            scopes.join(" ")
        };
        format!(
            "https://github.com/login/oauth/authorize?client_id={}&redirect_uri={}&scope={}&state={}",
            self.config.client_id,
            urlencoding::encode(redirect_uri),
            urlencoding::encode(&scopes_str),
            state,
        )
    }

    async fn exchange_code(&self, code: &str, redirect_uri: &str) -> Result<ProviderTokenResponse, AuthError> {
        tracing::info!(provider = "github", "向 GitHub 交换 Token");

        let resp = self.http_client
            .post("https://github.com/login/oauth/access_token")
            .header("Accept", "application/json")
            .form(&[
                ("client_id", &self.config.client_id),
                ("client_secret", &self.config.client_secret),
                ("code", code),
                ("redirect_uri", redirect_uri),
            ])
            .send()
            .await
            .map_err(|e| {
                tracing::warn!(provider = "github", error = %e, "GitHub Token 端点不可达");
                AuthError::OAuth2ProviderUnavailable(e.to_string())
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(AuthError::OAuth2ProviderTokenError(format!(
                "HTTP {}: {}", status, body
            )));
        }

        resp.json::<ProviderTokenResponse>().await
            .map_err(|e| AuthError::OAuth2ProviderTokenError(e.to_string()))
    }

    async fn get_user_info(&self, token_response: &ProviderTokenResponse) -> Result<ProviderUserInfo, AuthError> {
        tracing::info!(provider = "github", "获取 GitHub 用户信息");

        let resp = self.http_client
            .get("https://api.github.com/user")
            .bearer_auth(&token_response.access_token)
            .header("User-Agent", "cmx-auth")
            .send()
            .await
            .map_err(|e| AuthError::OAuth2ProviderUnavailable(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(AuthError::OAuth2ProviderUserInfoError(
                format!("HTTP {}", resp.status())
            ));
        }

        let json: serde_json::Value = resp.json().await
            .map_err(|e| AuthError::OAuth2ProviderUserInfoError(e.to_string()))?;

        // GitHub 的 id 是数字类型，需转为字符串
        let provider_user_id = json["id"].as_i64().map(|n| n.to_string()).unwrap_or_default();
        let email = json["email"].as_str().map(String::from);
        let username = json["login"].as_str().map(String::from);
        let display_name = json["name"].as_str().map(String::from);
        let avatar_url = json["avatar_url"].as_str().map(String::from);

        // GitHub /user 接口不返回 email_verified，需额外调用 /user/emails API
        // 需 user:email scope（方案已配置）
        let email_verified = if email.is_some() {
            self.fetch_github_email_verified(&token_response.access_token, &email).await
        } else {
            None
        };

        Ok(ProviderUserInfo {
            provider_user_id,
            email,
            email_verified,
            username,
            display_name,
            avatar_url,
        })
    }

    /// 调用 GitHub /user/emails API 获取邮箱验证状态
    async fn fetch_github_email_verified(
        &self,
        access_token: &str,
        primary_email: &Option<String>,
    ) -> Option<bool> {
        let resp = self.http_client
            .get("https://api.github.com/user/emails")
            .bearer_auth(access_token)
            .header("User-Agent", "cmx-auth")
            .send()
            .await;

        match resp {
            Ok(resp) if resp.status().is_success() => {
                match resp.json::<Vec<serde_json::Value>>().await {
                    Ok(emails) => {
                        // 查找主邮箱的 verified 状态
                        if let Some(primary) = primary_email {
                            for e in &emails {
                                if e["email"].as_str() == Some(primary.as_str()) {
                                    return e["verified"].as_bool();
                                }
                            }
                        }
                        // 未找到匹配邮箱，取 primary=true 的第一个
                        for e in &emails {
                            if e["primary"].as_bool() == Some(true) {
                                return e["verified"].as_bool();
                            }
                        }
                        None
                    }
                    Err(e) => {
                        tracing::warn!(provider = "github", error = %e, "GitHub emails 响应解析失败");
                        None
                    }
                }
            }
            Ok(resp) => {
                tracing::warn!(provider = "github", status = %resp.status(), "GitHub /user/emails 请求失败");
                None
            }
            Err(e) => {
                tracing::warn!(provider = "github", error = %e, "GitHub /user/emails 请求失败");
                None
            }
        }
    }

    fn default_scopes(&self) -> Vec<String> {
        vec!["user:email".into(), "read:user".into()]
    }
}
```

### 3.6 AccountLinker — 第三方账号关联

> **v2 变更**：
> - 增加 `email_verified` 前置检查（安全-03）
> - 增加用户名生成策略配置（遗漏-04）
> - 增加解绑安全检查逻辑（遗漏-05）
> - 增加 tracing 日志

```rust
// crates/libs/cmx-infra/cmx-auth/src/oauth2/provider/account_linker.rs
use cmx_database::{DatabaseManager, GenericCrudService, DbBmc, get_default_db_manager};
use cmx_traits::{AuthError, UserAuthQuery};
use modql::filter::{OpValsString, OpValsInt64, OpValString};
use modql::field::Fields;
use serde::{Serialize, Deserialize};
use super::ProviderUserInfo;
use crate::config::AccountLinkConfig;
use std::sync::Arc;

/// 第三方账号关联结果
pub enum LinkResult {
    /// 关联已有用户
    Linked { user_id: String, is_new: bool },
    /// 需要前端完成绑定（邮箱未验证或配置为手动绑定）
    BindingRequired { provider: String, provider_user_id: String, email: Option<String> },
}

/// 第三方账号关联/注册逻辑
pub struct AccountLinker {
    user_query: Arc<dyn UserAuthQuery>,
    config: AccountLinkConfig,
}

impl AccountLinker {
    /// 查找或关联本地用户
    pub async fn find_or_link(
        &self,
        provider: &str,
        provider_user_id: &str,
        user_info: &ProviderUserInfo,
    ) -> Result<LinkResult, AuthError> {
        // 1. 查询是否已关联
        let existing = self.find_account(provider, provider_user_id).await?;
        if let Some(account) = existing {
            tracing::info!(provider = %provider, provider_user_id = %provider_user_id, user_id = %account.user_id, "第三方账号已关联");
            return Ok(LinkResult::Linked {
                user_id: account.user_id,
                is_new: false,
            });
        }

        // 2. 自动关联策略（根据邮箱匹配）
        if self.config.auto_link_by_email {
            if let Some(email) = &user_info.email {
                // v2: 必须验证邮箱已验证
                if user_info.email_verified != Some(true) {
                    tracing::warn!(provider = %provider, email = %email, "Provider 邮箱未验证，跳过自动关联");
                    return Ok(LinkResult::BindingRequired {
                        provider: provider.to_string(),
                        provider_user_id: provider_user_id.to_string(),
                        email: Some(email.clone()),
                    });
                }
                let user = self.user_query.get_user_by_email(email).await?;
                if let Some(user) = user {
                    tracing::info!(provider = %provider, email = %email, user_id = %user.user_id, "邮箱匹配，自动关联");
                    self.create_account(provider, provider_user_id, &user.user_id, user_info).await?;
                    return Ok(LinkResult::Linked {
                        user_id: user.user_id,
                        is_new: false,
                    });
                }
            }
        }

        // 3. 自动注册策略
        if self.config.auto_register {
            let user_id = self.register_user_from_oauth2(provider, user_info).await?;
            self.create_account(provider, provider_user_id, &user_id, user_info).await?;
            tracing::info!(provider = %provider, provider_user_id = %provider_user_id, user_id = %user_id, "自动注册并关联");
            return Ok(LinkResult::Linked {
                user_id,
                is_new: true,
            });
        }

        // 4. 不自动注册，返回需要绑定
        tracing::info!(provider = %provider, provider_user_id = %provider_user_id, "需手动绑定");
        Ok(LinkResult::BindingRequired {
            provider: provider.to_string(),
            provider_user_id: provider_user_id.to_string(),
            email: user_info.email.clone(),
        })
    }

    /// 从第三方 OAuth2 信息自动注册用户
    async fn register_user_from_oauth2(
        &self,
        provider: &str,
        user_info: &ProviderUserInfo,
    ) -> Result<String, AuthError> {
        let username = self.generate_username(provider, user_info).await?;
        let user_id = self.user_query.create_user_from_oauth2(
            provider,
            &cmx_traits::OAuth2UserInfo {
                provider: provider.to_string(),
                provider_user_id: user_info.provider_user_id.clone(),
                email: user_info.email.clone(),
                username: Some(username),
                display_name: user_info.display_name.clone(),
                avatar_url: user_info.avatar_url.clone(),
            },
        ).await?;
        Ok(user_id)
    }

    /// 根据配置策略生成用户名（含冲突重试）
    ///
    /// 策略：
    /// 1. 根据配置生成基础用户名
    /// 2. 调用 get_user_by_username 检查冲突
    /// 3. 冲突时追加 4 位随机后缀（如 `_a3f2`），最多重试 3 次
    /// 4. 全部重试失败后返回 OAuth2UsernameConflict 错误
    async fn generate_username(&self, provider: &str, user_info: &ProviderUserInfo) -> Result<String, AuthError> {
        const MAX_RETRIES: usize = 3;

        let base = match self.config.username_strategy.as_deref() {
            "provider_prefix" => format!("{}_{}", provider, user_info.provider_user_id),
            "email_prefix" => {
                user_info.email
                    .as_ref()
                    .map(|e| e.split('@').next().unwrap_or(e).to_string())
                    .unwrap_or_else(|| format!("{}_{}", provider, user_info.provider_user_id))
            }
            _ => {
                // 默认：display_name 优先，否则 provider_prefix
                user_info.display_name.clone()
                    .unwrap_or_else(|| format!("{}_{}", provider, user_info.provider_user_id))
            }
        };

        // 首次尝试基础用户名
        if self.user_query.get_user_by_username(&base).await?.is_none() {
            return Ok(base);
        }

        // 冲突时追加随机后缀重试
        tracing::info!(base = %base, "用户名冲突，追加随机后缀重试");
        for i in 0..MAX_RETRIES {
            let suffix = Self::random_suffix();
            let candidate = format!("{}_{}", base, suffix);
            if self.user_query.get_user_by_username(&candidate).await?.is_none() {
                tracing::info!(candidate = %candidate, attempt = i + 1, "用户名冲突重试成功");
                return Ok(candidate);
            }
        }

        // 全部重试失败
        tracing::warn!(base = %base, retries = MAX_RETRIES, "用户名冲突重试耗尽");
        Err(AuthError::OAuth2UsernameConflict(base))
    }

    /// 生成 4 位随机十六进制后缀
    fn random_suffix() -> String {
        use std::fmt::Write;
        let mut buf = String::with_capacity(4);
        let val = rand::random::<u16>();
        write!(buf, "{:04x}", val).unwrap();
        buf
    }

    /// 解绑第三方账号（含安全检查）
    pub async fn unlink_account(
        &self,
        user_id: &str,
        provider: &str,
    ) -> Result<(), AuthError> {
        // 1. 检查用户是否设置了密码
        let user = self.user_query.get_user_by_id(user_id).await?
            .ok_or(AuthError::OAuth2AccountNotLinked {
                provider: provider.to_string(),
                provider_user_id: String::new(),
            })?;
        let has_password = user.password_hash.is_some();

        // 2. 检查是否还绑定了其他第三方 Provider
        let other_bindings = self.count_other_bindings(user_id, provider).await?;

        // 3. 如果既没有密码也没有其他绑定，拒绝解绑
        if !has_password && other_bindings == 0 {
            tracing::warn!(user_id = %user_id, provider = %provider, "无法解除最后一个登录绑定");
            return Err(AuthError::OAuth2LastBindingCannotRemove);
        }

        // 4. 执行解绑
        self.remove_account(user_id, provider).await?;
        tracing::info!(user_id = %user_id, provider = %provider, "第三方账号解绑成功");
        Ok(())
    }

    // ========== 数据访问方法（通过 GenericCrudService 操作 cmx_auth_oauth2_account 表） ==========

    /// 获取 DatabaseManager 引用（与 UserAuthQueryImpl 风格一致）
    fn get_db_manager() -> &'static DatabaseManager {
        get_default_db_manager()
    }
    fn default_db_id() -> &'static str { "default" }

    /// 查询第三方账号关联记录
    ///
    /// 使用 GenericCrudService::list 按 provider + provider_user_id 过滤查询
    async fn find_account(
        &self,
        provider: &str,
        provider_user_id: &str,
    ) -> Result<Option<OAuth2Account>, AuthError> {
        let filters = Some(vec![OAuth2AccountFilter {
            provider: Some(OpValsString(vec![OpValString::Eq(provider.to_string())])),
            provider_user_id: Some(OpValsString(vec![OpValString::Eq(provider_user_id.to_string())])),
            ..Default::default()
        }]);
        let dataset = GenericCrudService::<OAuth2AccountBmc, OAuth2AccountFilter>::list(
            Self::get_db_manager(),
            Self::default_db_id(),
            None,
            filters,
            None,
        ).await.map_err(|e| AuthError::Internal(e.to_string()))?;

        Ok(extract_oauth2_account(dataset))
    }

    /// 创建第三方账号关联记录
    async fn create_account(
        &self,
        provider: &str,
        provider_user_id: &str,
        user_id: &str,
        user_info: &ProviderUserInfo,
    ) -> Result<(), AuthError> {
        let data = OAuth2AccountForCreate {
            user_id: user_id.to_string(),
            provider: provider.to_string(),
            provider_user_id: provider_user_id.to_string(),
            provider_username: user_info.username.clone(),
            provider_email: user_info.email.clone(),
            provider_email_verified: user_info.email_verified,
            provider_display_name: user_info.display_name.clone(),
            provider_avatar_url: user_info.avatar_url.clone(),
        };
        // id 由 GenericCrudService::create 自动生成（#[derive(Fields)] 生成 HasSeaFields 实现）
        GenericCrudService::<OAuth2AccountBmc>::create(
            Self::get_db_manager(),
            Self::default_db_id(),
            None,
            data,
        ).await.map_err(|e| AuthError::Internal(e.to_string()))?;
        Ok(())
    }

    /// 统计用户绑定的其他第三方 Provider 数量（排除指定 Provider）
    async fn count_other_bindings(
        &self,
        user_id: &str,
        exclude_provider: &str,
    ) -> Result<usize, AuthError> {
        let filters = Some(vec![OAuth2AccountFilter {
            user_id: Some(OpValsString(vec![OpValString::Eq(user_id.to_string())])),
            provider: Some(OpValsString(vec![OpValString::Not(exclude_provider.to_string())])),
            status: Some(OpValsInt64(vec![modql::filter::OpValInt64::Eq(1)])),
            ..Default::default()
        }]);
        let count = GenericCrudService::<OAuth2AccountBmc, OAuth2AccountFilter>::count(
            Self::get_db_manager(),
            Self::default_db_id(),
            None,
            filters,
        ).await.map_err(|e| AuthError::Internal(e.to_string()))?;
        Ok(count as usize)
    }

    /// 删除第三方账号关联记录
    async fn remove_account(
        &self,
        user_id: &str,
        provider: &str,
    ) -> Result<(), AuthError> {
        // 先查找记录获取主键 id，再按主键删除
        let filters = Some(vec![OAuth2AccountFilter {
            user_id: Some(OpValsString(vec![OpValString::Eq(user_id.to_string())])),
            provider: Some(OpValsString(vec![OpValString::Eq(provider.to_string())])),
            ..Default::default()
        }]);
        let dataset = GenericCrudService::<OAuth2AccountBmc, OAuth2AccountFilter>::list(
            Self::get_db_manager(),
            Self::default_db_id(),
            None,
            filters,
            None,
        ).await.map_err(|e| AuthError::Internal(e.to_string()))?;

        let schema = dataset.schema.as_ref();
        let ids: Vec<serde_json::Value> = dataset.iter()
            .filter_map(|row| row.get_by_name_as::<String>(schema, "id"))
            .map(|id| serde_json::Value::String(id))
            .collect();

        if !ids.is_empty() {
            GenericCrudService::<OAuth2AccountBmc>::delete(
                Self::get_db_manager(),
                Self::default_db_id(),
                None,
                ids,
            ).await.map_err(|e| AuthError::Internal(e.to_string()))?;
        }
        Ok(())
    }
}

/// 从 DataSet 提取 OAuth2Account（与 UserAuthQueryImpl 的 extract_user 模式一致）
fn extract_oauth2_account(dataset: cmx_database::DataSet) -> Option<OAuth2Account> {
    let schema = dataset.schema.as_ref();
    let row = dataset.iter().next()?;

    Some(OAuth2Account {
        id: row.get_by_name_as(schema, "id").unwrap_or_default(),
        user_id: row.get_by_name_as(schema, "user_id").unwrap_or_default(),
        provider: row.get_by_name_as(schema, "provider").unwrap_or_default(),
        provider_user_id: row.get_by_name_as(schema, "provider_user_id").unwrap_or_default(),
        provider_username: row.get_by_name_as(schema, "provider_username"),
        provider_email: row.get_by_name_as(schema, "provider_email"),
        provider_email_verified: row.get_by_name_as(schema, "provider_email_verified"),
        provider_display_name: row.get_by_name_as(schema, "provider_display_name"),
        provider_avatar_url: row.get_by_name_as(schema, "provider_avatar_url"),
    })
}

/// 第三方 OAuth2 账号关联表 Bmc（DbBmc 实现）
struct OAuth2AccountBmc;
impl DbBmc for OAuth2AccountBmc {
    const TABLE: &'static str = "cmx_auth_oauth2_account";
    const PK_COLUMN: &'static str = "id";
}

/// 第三方 OAuth2 账号关联记录（数据库模型）
struct OAuth2Account {
    id: String,
    user_id: String,
    provider: String,
    provider_user_id: String,
    provider_username: Option<String>,
    provider_email: Option<String>,
    provider_email_verified: Option<bool>,
    provider_display_name: Option<String>,
    provider_avatar_url: Option<String>,
}

/// 创建第三方账号关联的输入结构体
/// #[derive(Fields)] 自动生成 HasSeaFields 实现，GenericCrudService::create 依赖此 trait
#[derive(Debug, Clone, Serialize, Deserialize, Fields)]
pub struct OAuth2AccountForCreate {
    pub user_id: String,
    pub provider: String,
    pub provider_user_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_email: Option<String>,
    pub provider_email_verified: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_avatar_url: Option<String>,
}

/// 第三方账号过滤条件
/// #[derive(FilterNodes)] 自动生成 IntoFilterNodes 实现，GenericCrudService::list/count 依赖此 trait
/// "不等于"通过 OpValString::Not(value) 表达，映射到 SQL 的 <> 操作符
#[derive(Debug, Clone, FilterNodes, Deserialize, Default)]
pub struct OAuth2AccountFilter {
    pub provider: Option<OpValsString>,
    pub provider_user_id: Option<OpValsString>,
    pub user_id: Option<OpValsString>,
    pub status: Option<OpValsInt64>,
}
```

### 3.7 Credentials 枚举扩展

在 `cmx-traits/src/auth_service.rs` 中新增第三方 OAuth2 凭证变体：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum Credentials {
    // ... 现有变体不变 ...

    /// 🆕 第三方 OAuth2 登录（Provider 已验证通过，直接签发本平台 Token）
    ThirdPartyOAuth2 {
        /// Provider 名称（如 "google", "github"）
        provider: String,
        /// Provider 侧用户唯一标识
        provider_user_id: String,
        /// 本平台用户 ID（已通过 AccountLinker 关联）
        user_id: String,
    },
}
```

### 3.8 AuthError 扩展

在 `cmx-traits/src/auth_error.rs` 中新增第三方 Provider 相关错误：

```rust
#[derive(Debug, Error)]
pub enum AuthError {
    // ... 现有变体不变 ...

    /// 🆕 OAuth2 Provider 不存在
    #[error("OAuth2 Provider 不存在: {0}")]
    OAuth2ProviderNotFound(String),

    /// 🆕 OAuth2 Provider 服务不可达
    #[error("OAuth2 Provider 服务不可达: {0}")]
    OAuth2ProviderUnavailable(String),

    /// 🆕 OAuth2 Provider Token 交换失败
    #[error("OAuth2 Provider Token 交换失败: {0}")]
    OAuth2ProviderTokenError(String),

    /// 🆕 OAuth2 Provider 用户信息获取失败
    #[error("OAuth2 Provider 用户信息获取失败: {0}")]
    OAuth2ProviderUserInfoError(String),

    /// 🆕 第三方账号未绑定本地用户
    #[error("第三方账号未绑定本地用户: {provider}:{provider_user_id}")]
    OAuth2AccountNotLinked {
        provider: String,
        provider_user_id: String,
    },

    /// 🆕 Provider 邮箱未验证，无法自动关联
    #[error("Provider 邮箱未验证，无法自动关联")]
    OAuth2EmailNotVerified,

    /// 🆕 无法解除最后一个登录绑定
    #[error("无法解除最后一个登录绑定")]
    OAuth2LastBindingCannotRemove,

    /// 🆕 用户名冲突，自动注册失败
    #[error("用户名冲突，自动注册失败: {0}")]
    OAuth2UsernameConflict(String),

    /// 🆕 回调授权码无效或已过期
    #[error("第三方 OAuth2 回调授权码无效或已过期")]
    OAuth2CallbackCodeInvalid,
}
```

***

## 四、配置设计

### 4.1 OAuth2ProviderConfig

> **v2 变更**：增加 `redirect_uri`、`field_mapping`、`token_endpoint_auth_method`、`icon_url`、`brand_color` 字段。

```rust
// crates/libs/cmx-infra/cmx-auth/src/config.rs（扩展）

/// 第三方 OAuth2 Provider 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuth2ProviderConfig {
    /// Provider 唯一标识（如 "google", "github"）
    pub name: String,
    /// Provider 显示名称（如 "Google", "GitHub"）
    #[serde(default)]
    pub display_name: String,
    /// Provider 类型（内置实现名称，如 "google", "github"；或 "generic" 使用通用实现）
    #[serde(default = "default_provider_type")]
    pub provider_type: String,
    /// OAuth2 Client ID
    pub client_id: String,
    /// OAuth2 Client Secret
    pub client_secret: String,
    /// 🆕 回调地址（服务端生成，前端不可覆盖，防止 Open Redirect）
    /// 格式：https://your-domain.com/api/auth/oauth2/provider/{name}/callback
    #[serde(default)]
    pub redirect_uri: String,
    /// 授权端点 URL（generic 类型必需，内置类型可省略）
    #[serde(default)]
    pub authorize_url: String,
    /// Token 端点 URL（generic 类型必需，内置类型可省略）
    #[serde(default)]
    pub token_url: String,
    /// 用户信息端点 URL（generic 类型必需，内置类型可省略）
    #[serde(default)]
    pub userinfo_url: String,
    /// 请求的 scope 列表
    #[serde(default)]
    pub scopes: Vec<String>,
    /// 🆕 用户信息字段映射（标准字段名 → JSON 字段名，支持 number→string 自动转换）
    #[serde(default)]
    pub field_mapping: std::collections::HashMap<String, String>,
    /// 🆕 Token 端点认证方式："client_secret_post"（默认）| "client_secret_basic"
    #[serde(default = "default_auth_method")]
    pub token_endpoint_auth_method: String,
    /// 🆕 Provider 图标 URL
    #[serde(default)]
    pub icon_url: Option<String>,
    /// 🆕 品牌色（用于前端按钮样式）
    #[serde(default)]
    pub brand_color: Option<String>,
    /// 是否启用
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_provider_type() -> String { "generic".to_string() }
fn default_enabled() -> bool { true }
fn default_auth_method() -> String { "client_secret_post".to_string() }
```

### 4.2 OAuth2Config 扩展

```rust
/// OAuth2 配置（扩展）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OAuth2Config {
    // === Authorization Server 配置（原有） ===
    /// 授权码有效期（秒），默认 600（10 分钟）
    #[serde(default = "default_auth_code_ttl")]
    pub auth_code_ttl_secs: u64,
    /// 是否强制 PKCE
    #[serde(default = "default_pkce_required")]
    pub pkce_required: bool,

    // === 🆕 OAuth2 Client 配置（第三方 Provider 对接） ===
    /// 第三方 OAuth2 Provider 列表
    #[serde(default)]
    pub providers: Vec<OAuth2ProviderConfig>,
    /// 🆕 第三方账号关联配置
    #[serde(default)]
    pub account_link: AccountLinkConfig,
    /// 🆕 Provider state 有效期（秒），默认 600
    #[serde(default = "default_state_ttl")]
    pub state_ttl_secs: u64,
    /// 🆕 回调授权码有效期（秒），默认 30
    #[serde(default = "default_callback_code_ttl")]
    pub callback_code_ttl_secs: u64,
    /// 🆕 第三方 OAuth2 登录成功后重定向到前端的 URL
    /// 例如：https://app.example.com/auth/callback
    /// 回调时拼接为：{frontend_callback_url}?code={one_time_code}&state={original_state}
    #[serde(default)]
    pub frontend_callback_url: String,
}

/// 第三方账号关联配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountLinkConfig {
    /// 是否根据邮箱自动关联已有本地用户
    #[serde(default = "default_auto_link_by_email")]
    pub auto_link_by_email: bool,
    /// 是否自动注册新用户（当无匹配的本地用户时）
    #[serde(default = "default_auto_register")]
    pub auto_register: bool,
    /// 自动注册时的默认角色（role_code）
    #[serde(default)]
    pub default_role: Option<String>,
    /// 🆕 用户名生成策略："provider_prefix" | "email_prefix" | "display_name"
    #[serde(default = "default_username_strategy")]
    pub username_strategy: String,
}

impl Default for AccountLinkConfig {
    fn default() -> Self {
        Self {
            auto_link_by_email: true,
            auto_register: false,
            default_role: None,
            username_strategy: "provider_prefix".to_string(),
        }
    }
}

fn default_auto_link_by_email() -> bool { true }
fn default_auto_register() -> bool { false }
fn default_state_ttl() -> u64 { 600 }
fn default_callback_code_ttl() -> u64 { 30 }
fn default_username_strategy() -> String { "provider_prefix".to_string() }
```

### 4.3 TOML 配置示例

在 `config/config_template.toml` 的 `[auth.oauth2]` 段扩展：

```toml
# ==================== OAuth2 配置（可选） ====================
[auth.oauth2]
# === Authorization Server 配置 ===
# 授权码有效期（秒），默认 600（10 分钟）
# auth_code_ttl_secs = 600
# 是否强制 PKCE（默认 true，生产环境建议开启）
# pkce_required = true

# === 第三方 OAuth2 Provider 对接 ===
# Provider state 有效期（秒），默认 600
# state_ttl_secs = 600
# 回调授权码有效期（秒），默认 30
# callback_code_ttl_secs = 30
# 第三方 OAuth2 登录成功后重定向到前端的 URL
# 回调时拼接为：{frontend_callback_url}?code={one_time_code}&state={original_state}
frontend_callback_url = "https://app.example.com/auth/callback"

# 第三方账号关联策略
[auth.oauth2.account_link]
# 是否根据邮箱自动关联已有本地用户（默认 true，要求 Provider 邮箱已验证）
auto_link_by_email = true
# 是否自动注册新用户（默认 false，需手动绑定或管理员创建）
auto_register = false
# 自动注册时的默认角色（role_code，如 "user"）
# default_role = "user"
# 用户名生成策略：provider_prefix | email_prefix | display_name
# - provider_prefix: {provider}_{provider_user_id}，如 google_1234567890
# - email_prefix: 邮箱 @ 前部分，如 user from user@gmail.com
# - display_name: 使用昵称，冲突时追加随机后缀
username_strategy = "provider_prefix"

# === Google OAuth2 ===
[[auth.oauth2.providers]]
name = "google"
display_name = "Google"
provider_type = "google"
client_id = "your-google-client-id.apps.googleusercontent.com"
client_secret = "your-google-client-secret"
# 回调地址（必须与 Google Console 注册的一致，服务端生成，前端不可覆盖）
redirect_uri = "https://your-domain.com/api/auth/oauth2/provider/google/callback"
scopes = ["openid", "email", "profile"]
enabled = true

# === GitHub OAuth2 ===
[[auth.oauth2.providers]]
name = "github"
display_name = "GitHub"
provider_type = "github"
client_id = "your-github-client-id"
client_secret = "your-github-client-secret"
redirect_uri = "https://your-domain.com/api/auth/oauth2/provider/github/callback"
scopes = ["user:email", "read:user"]
enabled = true

# === 通用 OAuth2 Provider 示例（适用于任何标准 OAuth2 服务） ===
[[auth.oauth2.providers]]
name = "gitlab"
display_name = "GitLab"
provider_type = "generic"
client_id = "your-gitlab-application-id"
client_secret = "your-gitlab-secret"
redirect_uri = "https://your-domain.com/api/auth/oauth2/provider/gitlab/callback"
authorize_url = "https://gitlab.com/oauth/authorize"
token_url = "https://gitlab.com/oauth/token"
userinfo_url = "https://gitlab.com/api/v4/user"
scopes = ["read_user", "email"]
enabled = false
# 字段映射使用内联表（标准字段名 = JSON 字段名，支持 number 类型自动转 string）
field_mapping = { provider_user_id = "id", email = "email", username = "username", display_name = "name", avatar_url = "avatar_url" }
```

### 4.4 配置项速查

| 分类 | 配置路径 | 关键项 | 默认值 |
|------|----------|--------|--------|
| Auth Server | `auth.oauth2` | auth_code_ttl_secs / pkce_required | 600 / true |
| State | `auth.oauth2` | state_ttl_secs / callback_code_ttl_secs / frontend_callback_url | 600 / 30 / "" |
| 账号关联 | `auth.oauth2.account_link` | auto_link_by_email / auto_register / default_role / username_strategy | true / false / None / provider_prefix |
| **Provider 列表** | `auth.oauth2.providers[]` | name / provider_type / client_id / client_secret / redirect_uri / scopes / enabled | - |
| **Provider 端点** | `auth.oauth2.providers[]` | authorize_url / token_url / userinfo_url | 内置 Provider 自动填充 |
| **Provider 类型** | `auth.oauth2.providers[].provider_type` | google / github / generic | generic |
| **Provider 安全** | `auth.oauth2.providers[]` | redirect_uri / token_endpoint_auth_method | "" / client_secret_post |
| **Provider 映射** | `auth.oauth2.providers[].field_mapping` | provider_user_id / email / username / display_name / avatar_url | 与标准字段同名 |
| **Provider 品牌** | `auth.oauth2.providers[]` | icon_url / brand_color | None / None |

***

## 五、API 设计

### 5.1 新增 API 端点

> **v2 变更**：新增 `exchange` 端点（授权码模式）；`authorize` 和 `callback` 不再接受 `redirect_uri` 参数（服务端配置生成）。

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/api/auth/oauth2/providers` | 列出所有已启用的 Provider（供前端展示登录按钮） |
| GET | `/api/auth/oauth2/provider/{provider}/authorize` | 获取 Provider 授权 URL 并重定向 |
| GET | `/api/auth/oauth2/provider/{provider}/callback` | Provider 回调（交换 Token + 获取用户信息 + 签发一次性授权码 + 重定向前端） |
| **POST** | `/api/auth/oauth2/provider/exchange` | **🆕 用一次性授权码换 TokenPair** |
| POST | `/api/auth/oauth2/provider/{provider}/link` | 手动绑定第三方账号到已登录用户 |
| DELETE | `/api/auth/oauth2/provider/{provider}/unlink` | 解除第三方账号绑定 |

### 5.2 路由白名单补充

> **v2 变更**：使用前缀匹配，与现有 `starts_with` 机制一致。

```rust
const AUTH_WHITELIST: &[&str] = &[
    // ... 现有白名单 ...
    "/api/auth/oauth2/providers",       // 列出 Provider
    "/api/auth/oauth2/provider",         // 所有 Provider 子路径（authorize/callback/link/exchange）
];
```

### 5.3 请求/响应结构

```rust
// 列出 Providers 响应
#[derive(Serialize)]
pub struct ListProvidersResponse {
    pub providers: Vec<ProviderInfo>,
}

// Provider 回调响应（v2: 重定向前端，携带一次性授权码）
// 302 Redirect to {frontend_callback_url}?code={one_time_code}&state={original_state}
// frontend_callback_url 由 OAuth2Config.frontend_callback_url 配置
// 前端收到后 POST /api/auth/oauth2/provider/exchange 换 Token

// 🆕 授权码换 Token 请求
#[derive(Deserialize)]
pub struct ExchangeCodeRequest {
    /// 一次性授权码（回调返回的 code）
    pub code: String,
    /// 原始 state（用于前端校验）
    pub state: String,
}

// 🆕 授权码换 Token 响应
#[derive(Serialize)]
pub struct ExchangeCodeResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: String,
    pub expires_in: u64,
    /// 是否为新注册用户
    pub is_new: bool,
    /// Provider 名称
    pub provider: String,
}

// 手动绑定请求
#[derive(Deserialize)]
pub struct LinkAccountRequest {
    pub code: String,
    // redirect_uri 由服务端根据 Provider 配置生成，前端不可覆盖
}

// 解除绑定请求
#[derive(Deserialize)]
pub struct UnlinkAccountRequest {
    // 无额外参数，provider 从路径获取
}
```

***

## 六、数据库表设计

### 6.1 第三方 OAuth2 账号关联表

> **v2 变更**：`last_token_at` 改为 `last_login_at`（仅在会话创建时更新）；新增 `provider_email_verified` 字段。

```sql
CREATE TABLE "public"."cmx_auth_oauth2_account" (
    "id" varchar(64) NOT NULL,
    "user_id" varchar(64) NOT NULL,
    "provider" varchar(50) NOT NULL,
    "provider_user_id" varchar(255) NOT NULL,
    "provider_username" varchar(200),
    "provider_email" varchar(255),
    "provider_email_verified" bool,
    "provider_display_name" varchar(200),
    "provider_avatar_url" varchar(1000),
    "last_login_at" timestamp,
    "status" int4 DEFAULT 1,
    "archived" int4 DEFAULT 0,
    "create_time" timestamp DEFAULT CURRENT_TIMESTAMP,
    "update_time" timestamp DEFAULT CURRENT_TIMESTAMP,
    "create_by" varchar(100),
    "create_name" varchar(100),
    "update_by" varchar(100),
    "update_name" varchar(100),
    CONSTRAINT "pk_cmx_auth_oauth2_account" PRIMARY KEY ("id"),
    CONSTRAINT "uk_cmx_auth_oauth2_account_provider_user" UNIQUE ("provider", "provider_user_id")
);

CREATE INDEX "idx_cmx_auth_oauth2_account_user" ON "public"."cmx_auth_oauth2_account" ("user_id");
CREATE INDEX "idx_cmx_auth_oauth2_account_provider_email" ON "public"."cmx_auth_oauth2_account" ("provider", "provider_email");

COMMENT ON TABLE "public"."cmx_auth_oauth2_account" IS '第三方 OAuth2 账号关联表';
COMMENT ON COLUMN "public"."cmx_auth_oauth2_account"."user_id" IS '本地用户 ID';
COMMENT ON COLUMN "public"."cmx_auth_oauth2_account"."provider" IS 'OAuth2 Provider 标识（google/github 等）';
COMMENT ON COLUMN "public"."cmx_auth_oauth2_account"."provider_user_id" IS 'Provider 侧用户唯一标识';
COMMENT ON COLUMN "public"."cmx_auth_oauth2_account"."provider_username" IS 'Provider 侧用户名';
COMMENT ON COLUMN "public"."cmx_auth_oauth2_account"."provider_email" IS 'Provider 侧邮箱';
COMMENT ON COLUMN "public"."cmx_auth_oauth2_account"."provider_email_verified" IS 'Provider 侧邮箱是否已验证';
COMMENT ON COLUMN "public"."cmx_auth_oauth2_account"."provider_display_name" IS 'Provider 侧显示名';
COMMENT ON COLUMN "public"."cmx_auth_oauth2_account"."provider_avatar_url" IS 'Provider 侧头像 URL';
COMMENT ON COLUMN "public"."cmx_auth_oauth2_account"."last_login_at" IS '最近一次通过此 Provider 登录时间';
COMMENT ON COLUMN "public"."cmx_auth_oauth2_account"."status" IS '状态：0-禁用，1-启用';
```

### 6.2 Redis Key 新增

| Key 模式 | 数据结构 | TTL | 用途 |
|----------|----------|-----|------|
| `auth:oauth2:provider:state:{state}` | String (provider_name) | 600s | 第三方 OAuth2 CSRF state |
| `auth:oauth2:provider:callback:{code}` | String (TokenPair JSON) | 30s | 回调一次性授权码 → TokenPair |

### 6.3 State 原子消费 Lua 脚本

> **v2 新增**：参照现有授权码消费模式（`oauth2/store.rs`），编写 Lua 脚本原子消费 state，防止重放攻击。

```lua
-- consume_oauth2_provider_state.lua
-- 原子操作：读取并删除 state，防止并发重放
local value = redis.call('GET', KEYS[1])
if not value then
    return nil
end
redis.call('DEL', KEYS[1])
return value
```

### 6.4 回调授权码原子消费 Lua 脚本

> **v2 新增**：回调签发的一次性授权码同样需要原子消费。

```lua
-- consume_oauth2_callback_code.lua
-- 原子操作：读取并删除回调授权码，防止并发重放
local value = redis.call('GET', KEYS[1])
if not value then
    return nil
end
redis.call('DEL', KEYS[1])
return value
```

***

## 七、AuthServiceImpl 集成

### 7.1 authenticate() 新增分支

> **v2 变更**：`org_id` 统一传 `None`，与现有密码/APIKey/OAuth2 授权码分支一致。增加 tracing 日志。

```rust
// auth_service_impl.rs 中的 authenticate 方法扩展
async fn authenticate(
    &self,
    credentials: Credentials,
    device_info: Option<DeviceInfo>,
) -> Result<TokenPair, AuthError> {
    match credentials {
        // ... 现有 Password / RefreshToken / ApiKey / AuthorizationCode 分支不变 ...

        Credentials::ThirdPartyOAuth2 { user_id, provider, provider_user_id } => {
            tracing::info!(provider = %provider, user_id = %user_id, "第三方 OAuth2 登录");
            self.authenticate_third_party(&user_id, &provider, &provider_user_id, device_info).await
        }
    }
}

async fn authenticate_third_party(
    &self,
    user_id: &str,
    provider: &str,
    provider_user_id: &str,
    device_info: Option<DeviceInfo>,
) -> Result<TokenPair, AuthError> {
    // 1. 通过 UserAuthQuery 获取用户数据（验证用户存在且启用）
    let user = self.user_query.get_user_by_id(user_id).await?
        .ok_or(AuthError::OAuth2AccountNotLinked {
            provider: provider.to_string(),
            provider_user_id: provider_user_id.to_string(),
        })?;

    if user.status == 0 {
        return Err(AuthError::UserDisabled);
    }

    // 2. 获取角色和权限
    let roles = self.user_query.get_user_role_codes(user_id).await?;
    let permissions = self.user_query.get_user_permissions(user_id).await?;

    // 3. 创建会话
    let device_type = device_info.as_ref().map(|d| d.device_type.as_str()).unwrap_or("oauth2");
    let session = self.session_manager.create_session(
        user_id, device_type,
        device_info.as_ref().and_then(|d| d.device_id.as_deref()).unwrap_or(""),
        device_info.as_ref().and_then(|d| d.ip.as_deref()),
        device_info.as_ref().and_then(|d| d.user_agent.as_deref()),
    ).await?;

    // 4. 签发 TokenPair（org_id 传 None，与现有密码/APIKey/OAuth2 分支一致）
    let token_pair = self.issue_token_pair(
        user_id, &user.username, &roles, &permissions,
        None,  // org_id: 当前所有认证分支均传 None
        device_info.as_ref(),  // 传引用，匹配 issue_token_pair 签名 Option<&DeviceInfo>
    ).await?;

    tracing::info!(provider = %provider, user_id = %user_id, "第三方 OAuth2 登录成功");
    Ok(token_pair)
}
```

### 7.2 AuthService Trait 扩展

```rust
// 如需在 AuthService trait 层面暴露第三方 OAuth2 能力，可新增方法：
#[async_trait]
pub trait AuthService: Send + Sync {
    // ... 现有方法 ...

    /// 🆕 列出已启用的第三方 OAuth2 Provider
    async fn list_oauth2_providers(&self) -> Result<Vec<ProviderInfo>, AuthError>;

    /// 🆕 处理第三方 OAuth2 回调（交换 Token + 获取用户信息 + 关联/注册 + 签发本平台 Token）
    async fn handle_oauth2_callback(
        &self,
        provider: &str,
        code: &str,
        state: &str,
        device_info: Option<DeviceInfo>,
    ) -> Result<TokenPair, AuthError>;

    /// 🆕 用回调授权码交换 TokenPair
    async fn exchange_oauth2_callback_code(
        &self,
        code: &str,
    ) -> Result<TokenPair, AuthError>;

    /// 🆕 绑定第三方 OAuth2 账号到已登录用户
    async fn link_oauth2_account(
        &self,
        user_id: &str,
        provider: &str,
        code: &str,
    ) -> Result<(), AuthError>;

    /// 🆕 解除第三方 OAuth2 账号绑定（含安全检查：最后一个绑定不可解绑）
    async fn unlink_oauth2_account(
        &self,
        user_id: &str,
        provider: &str,
    ) -> Result<(), AuthError>;
}
```

***

## 八、新增依赖

### 8.1 第三方依赖

```toml
# workspace Cargo.toml 新增
# URL 编码
urlencoding = "2.1"
# JWT 验证（Google ID Token 签名验证）
jsonwebtoken = "9.3"
# Base64 解码（JWT header 解析）
base64 = "0.22"
```

> **注意**：`reqwest` 已在 workspace 中定义（v0.12），无需新增。`oauth2` crate 过于重量级且与现有手写 OAuth2 flow 风格不一致，选择不引入，直接用 reqwest 调用 Provider endpoint 即可。

### 8.2 cmx-auth Cargo.toml 新增

```toml
# HTTP 客户端（与第三方 Provider 通信）
reqwest = { workspace = true }
# URL 编码
urlencoding = { workspace = true }
# JWT 验证
jsonwebtoken = { workspace = true }
# Base64 解码
base64 = { workspace = true }
# 内部依赖 - 查询过滤/字段映射（GenericCrudService 的 FilterNodes + Fields）
modql = { workspace = true }
```

***

## 九、UserAuthQuery Trait 扩展

> **v2 变更**：`get_user_by_id` 已存在，从新增列表移除；`get_user_by_email` 返回 `UserAuthData`（已有 `email` 字段，无需定义专用类型）。

为支持 AccountLinker 的邮箱查找和自动注册功能，需在 `UserAuthQuery` trait 新增方法：

```rust
// crates/libs/cmx-traits/src/user_auth_query.rs

#[async_trait]
pub trait UserAuthQuery: Send + Sync {
    // ... 现有方法不变（含 get_user_by_id，供第三方 OAuth2 复用） ...

    /// 🆕 根据邮箱查询用户认证数据（用于第三方 OAuth2 自动关联）
    /// 返回 UserAuthData（已含 email 字段）
    async fn get_user_by_email(
        &self,
        email: &str,
    ) -> Result<Option<UserAuthData>, crate::error::TraitError>;

    /// 🆕 从第三方 OAuth2 信息自动注册用户（当 auto_register=true 时调用）
    async fn create_user_from_oauth2(
        &self,
        provider: &str,
        user_info: &OAuth2UserInfo,
    ) -> Result<String, crate::error::TraitError>;
}

/// 第三方 OAuth2 用户信息（用于自动注册）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuth2UserInfo {
    pub provider: String,
    pub provider_user_id: String,
    pub email: Option<String>,
    pub username: Option<String>,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
}
```

***

## 十、启动初始化流程扩展

> **v2 变更**：配置加载改为直接从 TOML 反序列化；Registry 纳入 GlobalAuthService；移除 WeChat。

```
web-server main.rs:
  1. DatabaseManager::initialize()
  2. CacheManager::initialize()
  3. GlobalAuthService::initialize()
     ├── 加载 AuthConfig（含 OAuth2 Provider 配置）
     ├── import_static_api_keys()
     ├── 🆕 初始化 OAuth2ProviderRegistry
     │   ├── 根据 providers 配置创建 Provider 实例
     │   ├── provider_type="google" → GoogleProvider::new(config)
     │   ├── provider_type="github" → GitHubProvider::new(config)
     │   └── provider_type="generic" → GenericOAuth2Provider::new(config)
     ├── 🆕 注册 Provider 到 Registry
     ├── 🆕 GlobalAuthService::initialize_provider_registry(registry)
     └── setup_cache_invalidation_handler()
  4. GlobalIamService::initialize()
  5. ... 其他服务
```

### 10.1 load_auth_config() 扩展

> **v2 变更**：改为直接从 TOML 反序列化 `auth.oauth2` 段，而非 `get_string + serde_json`。

```rust
// web-server/src/config/auth.rs 中的 load_auth_config() 新增：
fn load_auth_config() -> AuthConfig {
    // ... 现有配置加载 ...

    // 🆕 OAuth2 Provider 配置：直接从 TOML 反序列化
    let config_mgr = cmx_utils::ConfigManager::global();

    // 直接反序列化 auth.oauth2 段为 OAuth2Config
    if let Some(oauth2_table) = config_mgr.get_table("auth.oauth2") {
        let oauth2_config: OAuth2Config = oauth2_table.try_into()
            .map_err(|e| tracing::warn!("OAuth2 配置解析失败: {}", e))
            .unwrap_or_default();
        auth_config.oauth2 = Some(oauth2_config);
    }

    auth_config
}
```

***

## 十一、安全考量

### 11.1 CSRF 防护

* 第三方 OAuth2 回调必须验证 `state` 参数（与原 Authorization Server 的 CSRF state 机制一致）
* State 存储在 Redis，设置 600s TTL，**使用 Lua 脚本原子消费**（一次性，防重放）

### 11.2 Client Secret 安全

* `client_secret` 仅在服务端使用（Token 交换），不暴露给前端
* 生产环境建议 `config.toml` 设置 `chmod 600`
* 远期可支持从环境变量或密钥管理服务读取

### 11.3 Token 不持久化

* 第三方 Provider 签发的 `access_token` / `refresh_token` **不在本平台持久化存储**
* 仅在回调流程中使用，用后即弃
* 本平台只签发自己的 JWT Token

### 11.4 回调 Token 传递安全（v2 新增）

* 回调**不再通过 URL Query String 传递 Token**，避免 Token 泄露到浏览器历史/Referer/代理日志
* 改为**授权码模式**：回调签发一次性短生命周期（30s）授权码，前端 POST `/api/auth/oauth2/provider/exchange` 换 Token
* 回调授权码使用 Lua 脚本原子消费，防止重放

### 11.5 ID Token 签名验证（v2 新增）

* Google 等 OIDC Provider 返回的 ID Token **必须验证签名**，不可仅解码 payload
* 使用 Provider 的 JWKS endpoint 获取公钥，验证 RS256 签名
* 验证 `iss`（签发者）和 `aud`（受众 = 自己的 client_id）
* 验证 `exp`（过期时间）
* JWKS 公钥启动时获取并缓存

### 11.6 账号关联安全

* `auto_link_by_email` 默认 true 但**要求邮箱已验证**（Provider 返回 `email_verified=true`）
* 若邮箱未验证，跳过自动关联，返回 `BindingRequired`
* `auto_register` 默认 false，避免自动创建大量用户
* 解绑需验证用户仍有其他登录方式（至少保留密码或一个第三方绑定），否则拒绝解绑

### 11.7 redirect_uri 安全（v2 新增）

* `redirect_uri` 在服务端配置（`OAuth2ProviderConfig.redirect_uri`），前端不可覆盖
* 防止 Open Redirect 攻击
* `redirect_uri` 必须与 Provider 注册时一致，否则 Provider 会拒绝

***

## 十二、原方案文档修改清单

以下为原方案 `20260615_cmx-auth_企业级统一认证模块架构方案.md` 需同步修改的位置：

| 章节 | 修改内容 |
|------|----------|
| §1.1 定位 | 新增"第三方 OAuth2 Provider 对接（Social Login）" |
| §2.1 架构图 | 新增 OAuth2ProviderRegistry + 具体 Provider + AccountLinker |
| §2.4 模块结构 | oauth2/ 下新增 provider/ 子模块 |
| §2.5.1 UserAuthQuery | 新增 get_user_by_email / create_user_from_oauth2（get_user_by_id 已存在，复用） |
| §2.5.2 AuthError | 新增 8 个 Provider 相关错误变体 |
| §2.5.3 Credentials | 新增 ThirdPartyOAuth2 变体 |
| §2.5.3 AuthService | 新增 5 个 OAuth2 Client 方法 |
| §3.2 OAuth2 流程 | 新增 §3.5 第三方 OAuth2 Client 流程（授权码模式） |
| §4.1.3 OAuth2 客户端 | 新增 ProviderUserInfo / ProviderTokenResponse |
| §4.1.5 AuthConfig | 扩展 OAuth2Config（providers + account_link + state_ttl + callback_code_ttl） |
| §4.3 Redis Key | 新增 `auth:oauth2:provider:state:{state}` + `auth:oauth2:provider:callback:{code}` |
| §4.4 数据库 | 新增 §4.4.8 cmx_auth_oauth2_account 表 |
| §7 路由白名单 | 新增 2 个前缀路径（/api/auth/oauth2/providers + /api/auth/oauth2/provider） |
| §9.1 依赖 | 新增 reqwest / urlencoding / jsonwebtoken / base64 |
| §9.2 Cargo.toml | 新增 reqwest / urlencoding / jsonwebtoken / base64 |
| §10 TOML 配置 | 扩展 [auth.oauth2] 段，新增 providers + account_link + callback_code_ttl |
| §10.2 配置速查 | 新增 Provider 和 AccountLink 行 |
| §12 实施计划 | Phase 7 细化，新增 Phase 8（第三方 OAuth2 Client） |
| §13 假设与决策 | 新增第三方 OAuth2 相关决策 |
| §14 验证步骤 | 新增第三方 OAuth2 登录流程测试 |

***

## 十三、实施计划

| 阶段 | 内容 | 产出 |
|------|------|------|
| **Phase 8a** | 配置扩展：OAuth2ProviderConfig + AccountLinkConfig + AuthConfig 扩展 + TOML 配置 | 编译通过 |
| **Phase 8b** | OAuth2Provider trait + GenericOAuth2Provider + Registry + GlobalAuthService 扩展 | 单元测试通过 |
| **Phase 8c** | Credentials/AuthError/AuthService trait 扩展 + UserAuthQuery 扩展 | 编译通过 |
| **Phase 8d** | AccountLinker + 数据库表 cmx_auth_oauth2_account + State/Code Lua 脚本 | 集成测试通过 |
| **Phase 8e** | 内置 Provider 实现（Google 含 JWKS 验证 / GitHub） | 单元测试通过 |
| **Phase 8f** | AuthServiceImpl 集成 + Handler + 路由注册 + 授权码模式回调 | API 可调用 |
| **Phase 8g** | config_template.toml 更新 + load_auth_config() 扩展（TOML 直接反序列化） | 配置可加载 |
| **Phase 8h** | 端到端测试（Google/GitHub 模拟回调 + 授权码换 Token） | 集成测试通过 |

***

## 十四、假设与决策

| 决策 | 理由 |
|------|------|
| 不引入 `oauth2` crate，直接用 reqwest | 与现有手写 OAuth2 flow 风格一致，避免重量级依赖 |
| Provider 实现用 trait + 注册表模式 | 遵循现有 Strategy Pattern，支持动态扩展 |
| 通用 Provider 支持配置驱动 + field_mapping | 避免为每个 Provider 写代码，配置即可对接标准 OAuth2 服务；field_mapping 解决字段名差异 |
| 第三方 Token 不持久化 | 减少攻击面，避免密钥泄露风险 |
| auto_register 默认 false | 安全优先，避免自动创建大量用户 |
| auto_link_by_email 默认 true 但要求 email_verified | 用户体验优先，同一邮箱自动关联；但必须验证邮箱，防止账号接管 |
| state 存 Redis + Lua 原子消费 | 与现有 CSRF state 机制一致，支持一次性消费，防重放 |
| 回调用授权码模式而非 URL Query 传 Token | 遵循 OAuth2 BCP (RFC 6819 §5.3.2)，避免 Token 泄露到浏览器历史/Referer/代理日志 |
| Google ID Token 必须验证签名（JWKS） | 防止伪造 ID Token 冒充用户登录 |
| redirect_uri 服务端配置，前端不可覆盖 | 防止 Open Redirect 攻击，确保与 Provider 注册一致 |
| WeChat 延后至下期 | WeChat 非标准 OAuth2 流程差异大（appid/secret 参数名、JSON body、openid+unionid），需独立设计 |
| org_id 统一传 None | 与现有密码/APIKey/OAuth2 授权码分支一致，org_id 数据管道当前为空 |
| Registry 纳入 GlobalAuthService（OnceLock） | 与现有 GLOBAL_AUTH_SERVICE / GLOBAL_OAUTH2_POLICY 模式一致 |
| 配置加载直接从 TOML 反序列化 | ConfigManager 的 get_string 无法处理 TOML array-of-tables |
| 解绑需检查最后一个绑定 | 防止用户解绑后无法登录 |

***

## 十五、远期规划（非本期范围）

| 功能 | 说明 | 优先级 |
|------|------|--------|
| WeChat OAuth2 支持 | 非标准流程（appid/secret、JSON body、openid+unionid），需独立 Provider 实现 | 高 |
| OIDC Discovery | 自动发现 Provider 端点 URL，减少手动配置 | 中 |
| Provider 健康检查 | 启动时检测 Provider 可达性，不可达时 enabled=false 并告警 | 中 |
| 回调速率限制 | 基于 IP 的速率限制，防止枚举/DoS/暴力破解 | 中 |
| client_secret 从环境变量/密钥管理服务读取 | 避免明文存储在 config.toml | 低 |
