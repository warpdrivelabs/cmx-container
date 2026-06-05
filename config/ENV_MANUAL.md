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
