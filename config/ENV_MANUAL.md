# CMX Container 环境变量配置手册

本手册包含所有通过环境变量注入的配置项，TOML 配置项详见 [CONFIG_MANUAL.md](CONFIG_MANUAL.md)。

---

## 目录

- [注册中心与配置中心环境变量](#注册中心与配置中心环境变量)
  - [推荐环境变量](#推荐环境变量)
  - [Nacos 连接配置](#nacos-连接配置)
  - [兼容旧环境变量（不推荐）](#兼容旧环境变量不推荐)
  - [服务注册地址解析优先级](#服务注册地址解析优先级)
  - [DEPLOY__MODE 部署模式](#deploy_mode-部署模式)
  - [app_id 获取优先级](#app_id-获取优先级)
- [基础服务中心环境变量覆盖](#基础服务中心环境变量覆盖)
- [服务对外身份环境变量覆盖](#服务对外身份环境变量覆盖)
- [认证配置环境变量覆盖](#认证配置环境变量覆盖)
- [通用环境变量](#通用环境变量)
- [配置优先级](#配置优先级)

---

## 注册中心与配置中心环境变量

> **重要**: 注册中心与配置中心相关配置必须通过环境变量注入，不支持在 TOML 文件中配置。这是出于安全考虑，防止远程配置覆盖敏感连接信息。

### 推荐环境变量

以下为推荐使用的新环境变量，支持注册中心与配置中心的解耦配置：

| 环境变量 | 类型 | 默认值 | 说明 |
|----------|------|--------|------|
| `SERVICE_REGISTRY_TYPE` | String | `mock` | 注册中心类型：`mock` / `nacos` |
| `SERVICE_REGISTRY_ENABLED` | Boolean | `false` | 是否启用服务注册 |
| `SERVICE_REGISTRY_NAME` | String | `cmx-server` | 注册的服务名称（同时作为 `APP_ID` 的回退值） |
| `SERVICE_REGISTRY_GROUP` | String | `DEFAULT_GROUP` | 分组名称 |
| `SERVICE_REGISTRY_CLUSTER` | String | `DEFAULT` | 集群名称 |
| `SERVICE_REGISTRY_WEIGHT` | Float | `1.0` | 实例权重 |
| `SERVICE_REGISTRY_IP` | String | 自动检测 | 注册使用的 IP 地址 |
| `SERVICE_REGISTRY_PORT` | Integer | `SERVICE_REGISTRY_PORT` > `NACOS_REGISTER_SERVER_PORT` > `server.port` > `8080` | 注册使用的端口号。**独立微服务（cmx-flow-server / cmx-rpt-server / cmx-rule-server 等）开启注册时必填**——服务 HTTP 端口来自 toml `[server]` 段（或 `SERVER__PORT` env，其经 ConfigManager env 层自动并入 `server.port`）、不写注册中心的 `server.port`，漏配会注册成 8080 错端口，服务发现反代将打错地址 |
| `CONFIG_CENTER_TYPE` | String | `mock` | 配置中心类型：`mock` / `nacos` |
| `CONFIG_CENTER_ENABLED` | Boolean | `false` | 是否启用配置中心 |
| `APP_ID` | String | `default` | 应用隔离标识（仅 micro 模式生效，详见 [`DEPLOY__MODE`](#deploy_mode-部署模式)） |
| `DEPLOY__MODE` | String | `mono` | 部署模式：`mono`（单体）/ `micro`（微服务）。详见 [部署模式](#deploy_mode-部署模式) |

> **独立微服务接入说明**（cmx-flow-server / cmx-rpt-server / cmx-rule-server）：
> 三个服务经 `cmx-service-base::init_infra()` 接入注册中心与配置中心，开关与门户同构——
> **推荐新前缀**：`SERVICE_REGISTRY_ENABLED` / `CONFIG_CENTER_ENABLED`（需配
> `SERVICE_REGISTRY_TYPE=nacos` / `CONFIG_CENTER_TYPE=nacos`，两中心可独立开启；旧前缀
> `NACOS_ENABLED` 主开关 + `NACOS_NAMING_ENABLED` / `NACOS_CONFIG_ENABLED` 子开关仍兼容），
> 默认全关（纯本地 toml+env，行为与独立部署前一致）。开启后 **create 阶段强依赖** Nacos 可达
> （客户端创建失败即中止启动）；register 阶段失败仅 warn 不阻塞。`SERVICE_REGISTRY_NAME` 优先级高于
> `NACOS_NAMING_SERVICE_NAME`，注册名约定：`cmx-flow-server` / `cmx-rpt-server` / `cmx-rule-server`
>（门户 `[center_client.services]` 的 `{flow,report,rules}.discovery` 按同名键发现）。配置中心开启后
> 配置优先级为本地 toml ← 远程配置中心 ← 环境变量（env 最高）。

### Nacos 连接配置

当 `SERVICE_REGISTRY_TYPE=nacos` 或 `CONFIG_CENTER_TYPE=nacos` 时，以下 Nacos 连接变量生效：

| 环境变量 | 类型 | 默认值 | 说明 |
|----------|------|--------|------|
| `NACOS_SERVER_ADDR` | String | `127.0.0.1:8848` | Nacos 服务器地址 |
| `NACOS_NAMESPACE` | String | `""` | 命名空间 |
| `NACOS_APP_NAME` | String | `cmx-container` | 应用名称 |
| `NACOS_USERNAME` | String | - | 认证用户名（可选） |
| `NACOS_PASSWORD` | String | - | 认证密码（可选） |
| `NACOS_CONFIG_DATA_ID` | String | - | 配置 Data ID |
| `NACOS_CONFIG_GROUP` | String | `DEFAULT_GROUP` | 配置 Group |

### 兼容旧环境变量（不推荐）

以下旧 `NACOS_*` 环境变量仍然有效，但**不推荐使用**，建议迁移到新变量：

| 旧变量 | 新变量 | 说明 |
|--------|--------|------|
| `NACOS_ENABLED=true` | `SERVICE_REGISTRY_ENABLED=true` + `CONFIG_CENTER_ENABLED=true` | 自动启用并设为 nacos 类型 |
| `NACOS_NAMING_ENABLED` | `SERVICE_REGISTRY_ENABLED` | 是否启用服务注册 |
| `NACOS_CONFIG_ENABLED` | `CONFIG_CENTER_ENABLED` | 是否启用配置中心 |
| `NACOS_NAMING_SERVICE_NAME` | `SERVICE_REGISTRY_NAME` | 注册的服务名称 |
| `NACOS_NAMING_GROUP_NAME` | `SERVICE_REGISTRY_GROUP` | 分组名称 |
| `NACOS_REGISTER_SERVER_IP` | `SERVICE_REGISTRY_IP` | 注册 IP |
| `NACOS_REGISTER_SERVER_PORT` | `SERVICE_REGISTRY_PORT` | 注册端口 |

### 服务注册地址解析优先级

注册 IP 和端口的解析优先级（从高到低）：

1. `SERVICE_REGISTRY_IP` / `SERVICE_REGISTRY_PORT`
2. `NACOS_REGISTER_SERVER_IP` / `NACOS_REGISTER_SERVER_PORT`（兼容旧变量）
3. 配置文件中的 `server.ip` / `server.port`
4. 自动检测本机 IP / 默认端口 `8080`

### DEPLOY__MODE 部署模式

> 对应 TOML 配置 `[deploy] mode`（详见 [CONFIG_MANUAL.md](CONFIG_MANUAL.md#部署模式配置)）。

| 环境变量 | 类型 | 默认值 | 说明 |
|----------|------|--------|------|
| `DEPLOY__MODE` | String | `mono` | 部署模式：`mono`（单体）/ `micro`（微服务）。可选别名：`monolithic`/`single`、`microservice` |

**双模行为对照**：

| 维度 | `mono`（默认） | `micro` |
|---|---|---|
| 数据源加载 | 加载全部 `status=1 AND archived=0` 的记录 | 按 `[app]` 三元组精确过滤 |
| `get_app_id()` 返回值 | 固定 `"default"`（不读 `[app].module_code`） | `[app].module_code`（维持现状） |
| 模块导入守卫 | 放宽（允许任意 `module_code`） | 保留（`module_code != app_id` 拒绝） |
| `[app]` 块 | 整体不生效 | 必需，不能为 `default` |
| 启动期校验 | 无（允许主库 + 业务库分库） | 不校验 |

**mono 切换的数据迁移**：从 micro 切到 mono 时，需执行 `docs/sql/migrations/20260721_001_deploy_mode_mono_app_id_unification.up.sql` 把历史 `app_id` 统一为 `'default'`。

### app_id 获取优先级

应用标识 `app_id` 的获取优先级（按 `[deploy] mode` 分支）：

**mono 模式**：固定返回 `"default"`，不读任何配置。

**micro 模式**（从高到低）：

1. 配置项 `app.module_code`（即 `[app]` 块的 `module_code`）
2. 环境变量 `APP_ID`
3. 环境变量 `SERVICE_REGISTRY_NAME`
4. 环境变量 `NACOS_NAMING_SERVICE_NAME`（兼容旧变量）
5. 默认值 `"default"`

---

## OpenCode AI 中继环境变量

cmx-ai 薄代理连接 OpenCode 的配置，优先级高于 `config.toml` 的 `[opencode]` 段。

| 环境变量 | 类型 | 默认值 | 说明 |
|----------|------|--------|------|
| `OPENCODE_BASE_URL` | String | `http://127.0.0.1:4096` | OpenCode 服务地址（含协议与端口） |
| `OPENCODE_SERVER_PASSWORD` | String | 空 | OpenCode 访问凭证；留空表示不启用鉴权（仅开发环境）。生产必须配置强密码 |

> **鉴权说明**：OpenCode `serve` 默认挂载 Authorization 中间件；未设置 `OPENCODE_SERVER_PASSWORD`
> 时打印 `server is unsecured` 警告。cmx-ai 调用 OpenCode 的所有请求（含 SSE 连接）以
> `Authorization: Bearer <password>` 携带该凭证。该密码也是 OpenCode 面向 cmx-ai 的唯一访问边界。

---

## 基础服务中心环境变量覆盖

`center_client` 配置节支持通过环境变量覆盖（v2 services 单表形态），格式为
`CENTER_CLIENT__<KEY>` 或 `CENTER_CLIENT__SERVICES__<KEY>__<FIELD>`：

| 环境变量 | 类型 | 说明 |
|----------|------|------|
| `CENTER_CLIENT__DEFAULT_TRANSPORT` | String | 服务间调用全局传输缺省（`http` / `grpc`，默认 `http`；grpc 需启用 `[rpc]` 且对应键配 `discovery`） |
| `CENTER_CLIENT__NACOS_GROUP` | String | Nacos 分组（discovery 定位的键共用，默认 `DEFAULT_GROUP`） |
| `CENTER_CLIENT__TIMEOUT_MS` | Integer | 请求超时时间（毫秒，默认 30000） |
| `CENTER_CLIENT__SERVICES__MENU__URL` | String | 门户中心静态基址（纯基址无路径，需带 scheme；与 `DISCOVERY` 并存时 URL 优先） |
| `CENTER_CLIENT__SERVICES__MENU__DISCOVERY` | String | 门户中心 Nacos 服务名 |
| `CENTER_CLIENT__SERVICES__PERM__TRANSPORT` | String | 权限中心传输覆盖（`http` / `grpc`，仅服务间导入调用生效） |
| `CENTER_CLIENT__SERVICES__FLOW__URL` | String | 流程微服务静态基址（反代目标） |

> 注：`url` 值需带 scheme（如 `http://...`）；纯数字值会被环境变量层解析为整数导致反序列化失败。
> 旧形态（`CENTER_CLIENT__MODE` / `CENTER_CLIENT__URLS__*`）已废弃，出现时被忽略并打迁移 warn。

---

## 框架级环境变量（SERVER__* 族，与 ConfigManager `__` 约定同名）

对应各服务 toml 的 `[server]` 段（`cmx-web-chassis::ChassisConfig` 读取）。命名与 ConfigManager 的 `__` 约定**同名**（`SERVER__PORT` → `server.port`）：chassis 在 ConfigManager 初始化前直读同名变量（门户日志初始化的时序要求），ConfigManager 就绪后其 env 层把同一变量合并到同一键——注册中心等 `get_string("server.port")` 消费方与实际监听端口永远一致。旧的每服务前缀（FLOW_/RPT_/RULE_/MDM_/MODEL_）与统一前缀 `CMX_*` 均已废弃：

| 环境变量 | 类型 | 说明 |
|----------|------|------|
| `SERVER__HOST` | String | 监听地址（默认 `0.0.0.0`；`[server].host` 覆盖） |
| `SERVER__PORT` | Integer | 监听端口（默认 `8080`；各引擎 toml 自配 8091~8095；`[server].port` 覆盖） |
| `SERVER__LOG_DIR` | String | 日志目录（默认 `logs`；`[server].log_dir` 覆盖） |
| `SERVER__LOG_LEVEL` | String | 默认日志级别（RUST_LOG 未设时用，默认 `info`；`[server].log_level` 覆盖） |
| `SERVER__GRACEFUL_TIMEOUT_SECS` | Integer | 优雅关闭最长等待秒数（默认 `10`；`[server].graceful_timeout_secs` 覆盖；旧名 `CMX_GRACEFUL_SHUTDOWN_TIMEOUT_SECS` / `{PREFIX}_GRACEFUL_SECS` 已废弃） |

> ⚠️ 多服务共存同一环境（同一 shell / 共享 env 的编排）时这些变量会同时命中多个服务；需单独改某服务时请改其 toml 的 `[server]` 段。

---

## 页面/内容资产环境变量（ASSETS__* 族）

对应 toml 的 `[assets]` 段（ConfigManager `__` 分隔约定，段名 + `__` + 键名大写）：

| 环境变量 | 消费方 | 说明 |
|----------|--------|------|
| `ASSETS__ROOT` | 门户 / model | 内容根（jsonstore：页面/元数据/字典/菜单 JSON；默认 `./data`） |
| `ASSETS__UI_NATIVE_DIR` | 五引擎 | native 页面投递目录（默认 `web/ui-native`） |
| `ASSETS__UI_HTML_DIR` | 五引擎 | html 页面投递目录（默认 `web/ui-html`） |
| `ASSETS__WEB_PORTAL_DIST` / `ASSETS__WEB_HTML_DIST` / `ASSETS__WEB_SHARED_DIST` | 门户 | 同源托管的前端 dist 目录 |
| `ASSETS__PAGE_CACHE_ENABLED` / `ASSETS__PAGE_CACHE_TTL_SECS` / `ASSETS__PAGE_CACHE_MAX_ENTRIES` | 门户 | 页面 moka L1 缓存开关/TTL/容量 |

> 旧名 `CMX_PORTAL_DATA_ROOT`、各引擎 `{PREFIX}_UI_DIR` / `{PREFIX}_UI_HTML_DIR` 已废弃。

---

## 引擎认证环境变量（AUTH__* 族，flow / rules 等）

对应引擎微服务 toml 的 `[auth]` 扁平段（ConfigManager 直读）：`AUTH__MODE` / `AUTH__JWT_ALG` / `AUTH__JWT_SECRET` / `AUTH__JWT_PUBLIC_KEY` / `AUTH__JWT_TENANT_CLAIM` / `AUTH__JWT_ROLES_CLAIM` / `AUTH__API_KEYS` / `AUTH__TENANCY`。

> ⚠️ **一字之差警示**：`AUTH__JWT_SECRET`（引擎 `[auth]` 扁平段 → `auth.jwt_secret`）与平台 `[auth.jwt]` 段的 `AUTH__JWT__SECRET`（→ `auth.jwt.secret`）是**两个不同配置**。写错不报错、只是不生效，排查时先数下划线。
> 旧名 `FLOW_AUTH_MODE` / `FLOW_JWT_*` / `FLOW_API_KEYS` / `RULE_AUTH_MODE` 等 env 注入链已废弃（数据源同理：`FLOW_PG_URL` / `IAM_PG_URL` / `RPT_PG_URL` / `RULE_PG_URL` 已废弃，统一 `[[databases]]` 段且缺段启动失败）。

---

## 服务对外身份环境变量覆盖

对应 TOML 配置节 `[service_auth]`。本服务作为调用方时携带的服务级凭证，
用于 gRPC / HTTP 跨服务调用的 M2M 鉴权。

| 环境变量 | 类型 | 说明 |
|---------|------|------|
| `SERVICE_AUTH__OUTGOING_API_KEY` | String | 本服务对外服务级凭证（`cmx_sk_xxx`），须同时在 `[[auth.static_api_keys]]` 注册。留空表示不配置服务身份（仅单体无跨服务调用场景）。生产环境优先用此环境变量注入，避免明文写入 TOML。 |

### 凭证与认证流程

1. gRPC 出站：客户端自动携带 `X-API-Key: <cmx_sk_xxx>` + 委托用户 token（若有）+ 请求 ID。
2. HTTP 出站（`RemoteImporterContext`）：携带同样三层 header。
3. 接收端 `mw_auth` / gRPC `AuthVerifier` 按 `X-API-Key` header 识别为 API Key，走 `validate_api_key` 验证。

详见 `docs/20260707_跨服务认证与认证上下文传播设计.md`。

---

## 认证配置环境变量覆盖

`auth` 配置节支持通过环境变量覆盖，格式为 `AUTH__<SECTION>__<KEY>`（双下划线 `__` 分隔层级，对应 TOML 中的点分隔键名）。

> **安全提示**：JWT 密钥（`AUTH__JWT__SECRET`）和 RS256 私钥（`AUTH__JWT__PRIVATE_KEY`）属于敏感信息，**务必通过环境变量注入**，不要写入 TOML 配置文件或版本控制。

### JWT 配置

| 环境变量 | 类型 | 默认值 | 说明 |
|----------|------|--------|------|
| `AUTH__JWT__ALGORITHM` | String | `HS256` | JWT 签名算法（`HS256` / `RS256`） |
| `AUTH__JWT__SECRET` | String | `a7k9m2p4x8q1w5e3r6t0y7u2i9o4p1` | HMAC 密钥（HS256 模式，生产环境务必修改） |
| `AUTH__JWT__ISSUER` | String | `cmx-auth` | JWT 签发者标识 |
| `AUTH__JWT__AUDIENCE` | String | `cmx-platform` | JWT 受众标识 |
| `AUTH__JWT__PRIVATE_KEY` | String | - | RS256 私钥（文件路径或 PEM 内容） |
| `AUTH__JWT__PUBLIC_KEY` | String | - | RS256 公钥（文件路径或 PEM 内容） |
| `AUTH__JWT__CURRENT_KID` | String | - | 当前签发使用的密钥 ID（密钥轮换标识） |
| `AUTH__JWT__LEGACY_PUBLIC_KEYS_0_KID` | String | - | 旧密钥 0 的 kid（密钥轮换宽限期） |
| `AUTH__JWT__LEGACY_PUBLIC_KEYS_0_PEM` | String | - | 旧密钥 0 的 PEM（文件路径或内容） |
| `AUTH__JWT__LEGACY_PUBLIC_KEYS_1_KID` | String | - | 旧密钥 1 的 kid |
| `AUTH__JWT__LEGACY_PUBLIC_KEYS_1_PEM` | String | - | 旧密钥 1 的 PEM |

> 旧密钥列表支持索引 0-4，最多 5 个。

### Argon2 密码哈希配置

| 环境变量 | 类型 | 默认值 | 说明 |
|----------|------|--------|------|
| `AUTH__ARGON2__MEMORY_COST` | Integer | `65536` | 内存开销（KB），值越大抗 GPU 破解能力越强 |
| `AUTH__ARGON2__TIME_COST` | Integer | `3` | 时间开销（迭代次数） |
| `AUTH__ARGON2__PARALLELISM` | Integer | `4` | 并行线程数 |

### Token 有效期配置

| 环境变量 | 类型 | 默认值 | 说明 |
|----------|------|--------|------|
| `AUTH__TOKEN__ACCESS_TTL_SECS` | Integer | `1800` | Access Token 有效期（秒） |
| `AUTH__TOKEN__REFRESH_TTL_SECS` | Integer | `604800` | Refresh Token 有效期（秒） |

### 会话配置

| 环境变量 | 类型 | 默认值 | 说明 |
|----------|------|--------|------|
| `AUTH__SESSION__SINGLE_SESSION_PER_DEVICE_TYPE` | Boolean | `false` | 同一设备类型是否仅允许一个会话 |
| `AUTH__SESSION__MAX_SESSIONS` | Integer | `0` | 最大并发会话数（0 = 不限制） |
| `AUTH__SESSION__IDLE_TIMEOUT_SECS` | Integer | `86400` | 会话空闲超时（秒） |
| `AUTH__SESSION__HEARTBEAT_INTERVAL_SECS` | Integer | `300` | 心跳间隔（秒） |

### 认证缓存配置

| 环境变量 | 类型 | 默认值 | 说明 |
|----------|------|--------|------|
| `AUTH__CACHE__ENABLE_LOCAL_CACHE` | Boolean | `true` | 是否启用本地缓存 |
| `AUTH__CACHE__LOCAL_TTL_SECS` | Integer | `30` | 本地缓存 TTL（秒） |
| `AUTH__CACHE__LOCAL_CACHE_MAX_ENTRIES` | Integer | `10000` | 本地缓存最大容量 |
| `AUTH__CACHE__MAX_LOGIN_ATTEMPTS` | Integer | `5` | 登录失败锁定阈值 |
| `AUTH__CACHE__LOCK_DURATION_SECS` | Integer | `900` | 账号锁定时长（秒） |

### 超管初始化环境变量

| 环境变量 | 类型 | 默认值 | 说明 |
|----------|------|--------|------|
| `AUTH__SUPER_ADMIN__USERNAME` | String | - | 超管用户名（必需） |
| `AUTH__SUPER_ADMIN__PASSWORD` | String | - | 超管初始密码（必需，生产环境务必通过环境变量注入） |
| `AUTH__SUPER_ADMIN__EMAIL` | String | - | 超管邮箱（可选） |
| `AUTH__SUPER_ADMIN__ROLES` | String | `admin` | 超管角色编码（逗号分隔） |

> 超管账号仅在首次启动时创建，已存在则跳过。

### OAuth2 配置环境变量覆盖

| 环境变量 | 类型 | 默认值 | 说明 |
|----------|------|--------|------|
| `AUTH__OAUTH2__AUTH_CODE_TTL_SECS` | Integer | `600` | 授权码有效期（秒） |
| `AUTH__OAUTH2__PKCE_REQUIRED` | Boolean | `true` | 是否强制 PKCE（生产环境建议开启） |

---

## 通用环境变量

### CONFIG_FILE

通过 `CONFIG_FILE` 环境变量指定配置文件路径：

```bash
CONFIG_FILE=/path/to/config.toml ./cmx-server
```

---

## 配置优先级

配置优先级从高到低：

1. **环境变量** - 最高优先级，不可被覆盖
2. **Nacos 远程配置** - 从 Nacos 配置中心拉取的配置
3. **本地 TOML 文件** - 配置文件中的配置
4. **代码默认值** - 代码中定义的默认值
