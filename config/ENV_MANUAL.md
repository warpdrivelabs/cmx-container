# CMX Container 环境变量配置手册

本手册包含所有通过环境变量注入的配置项，TOML 配置项详见 [CONFIG_MANUAL.md](CONFIG_MANUAL.md)。

---

## 目录

- [注册中心与配置中心环境变量](#注册中心与配置中心环境变量)
  - [推荐环境变量](#推荐环境变量)
  - [Nacos 连接配置](#nacos-连接配置)
  - [兼容旧环境变量（不推荐）](#兼容旧环境变量不推荐)
  - [服务注册地址解析优先级](#服务注册地址解析优先级)
  - [app_id 获取优先级](#app_id-获取优先级)
- [基础服务中心环境变量覆盖](#基础服务中心环境变量覆盖)
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
| `SERVICE_REGISTRY_PORT` | Integer | `server.port` 配置值 | 注册使用的端口号 |
| `CONFIG_CENTER_TYPE` | String | `mock` | 配置中心类型：`mock` / `nacos` |
| `CONFIG_CENTER_ENABLED` | Boolean | `false` | 是否启用配置中心 |
| `APP_ID` | String | `default` | 应用隔离标识 |

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

### app_id 获取优先级

应用标识 `app_id` 的获取优先级（从高到低）：

1. 配置文件 `app.id`
2. 环境变量 `APP_ID`
3. 环境变量 `SERVICE_REGISTRY_NAME`
4. 环境变量 `NACOS_NAMING_SERVICE_NAME`（兼容旧变量）
5. 默认值 `"default"`

---

## 基础服务中心环境变量覆盖

`center_client` 配置节支持通过环境变量覆盖，格式为 `CENTER_CLIENT__<KEY>` 或 `CENTER_CLIENT__<SECTION>__<KEY>`：

| 环境变量 | 类型 | 说明 |
|----------|------|------|
| `CENTER_CLIENT__MODE` | String | 访问模式 (`mock` / `url` / `discovery`) |
| `CENTER_CLIENT__TIMEOUT_MS` | Integer | 请求超时时间（毫秒） |
| `CENTER_CLIENT__URLS__MENU` | String | 门户中心 URL |
| `CENTER_CLIENT__URLS__PERM` | String | 权限中心 URL |
| `CENTER_CLIENT__URLS__FORM` | String | 表单中心 URL |
| `CENTER_CLIENT__URLS__FLOW` | String | 流程中心 URL |

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
