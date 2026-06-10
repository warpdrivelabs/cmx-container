# CMX Container 配置手册

本文档详细介绍 CMX Container 应用的所有配置项，包括配置含义、默认值、使用建议等信息。

---

## 目录

- [服务器配置](#服务器配置)
- [Web 服务配置](#web-服务配置)
- [数据库配置](#数据库配置)
- [Redis 配置](#redis-配置)
- [WASM 运行时配置](#wasm-运行时配置)
- [插件配置](#插件配置)
- [数据库迁移配置](#数据库迁移配置)
- [文件存储配置](#文件存储配置)
- [模板配置](#模板配置)
- [Code Server 配置](#code-server-配置)
- [节点配置](#节点配置)
- [基础服务中心配置](#基础服务中心配置)
- [RPC 配置](#rpc-配置)
- [注册中心 Metadata 配置](#注册中心-metadata-配置)
- [配置优先级](#配置优先级)
- [配置文件位置](#配置文件位置)

---

## 服务器配置

### `[server]`

服务器监听配置。

#### `host`

- **类型**: String
- **必需**: 否
- **默认值**: `"0.0.0.0"`
- **说明**: 监听地址，设置服务器绑定的 IP 地址
- **可选值**:
  - `"0.0.0.0"` - 监听所有网络接口
  - `"127.0.0.1"` - 仅监听本地回环地址
  - `"192.168.1.100"` - 监听指定 IP
- **示例**: `"0.0.0.0"`

#### `port`

- **类型**: Integer
- **必需**: 否
- **默认值**: `8080`
- **说明**: 监听端口，设置服务器监听的 TCP 端口号
- **示例**: `8080`

#### `ip`

- **类型**: String
- **必需**: 否
- **默认值**: 自动获取本机 IP
- **说明**: Nacos 注册 IP，用于服务注册到 Nacos 时的 IP 地址
- **用途**: 当服务器有多个网络接口时，可指定注册到 Nacos 的 IP 地址
- **行为**: 如果配置为空或不存在，系统会自动获取本机 IP
- **示例**: `"192.168.1.100"`

---

## Web 服务配置

### `web_folder`

- **类型**: String
- **必需**: 是
- **说明**: Web 静态文件目录路径，用于存放前端静态资源
- **示例**: `"/app/web-folder"` 或 `"./web-folder"`

---

## 数据库配置

### `[[databases]]`

数据库配置数组，支持配置多个数据源。

#### `db_id`

- **类型**: String
- **必需**: 是
- **说明**: 数据源唯一标识符，用于程序内部引用不同的数据库
- **示例**: `"primary"`, `"secondary"`, `"postgres2"`

#### `db_type`

- **类型**: String (enum)
- **必需**: 是
- **说明**: 数据库类型
- **可选值**:
    - `postgres` / `postgresql` / `pgsql` - PostgreSQL
    - `mysql` / `mariadb` - MySQL
    - `sqlite` / `sqlite3` - SQLite
- **示例**: `"postgres"`

#### `db_url`

- **类型**: String
- **必需**: 是
- **说明**: 数据库连接 URL
- **格式**: `<driver>://<user>:<password>@<host>:<port>/<database>`
- **示例**: `"postgresql://postgres:postgres@192.168.1.100:5432/cmx"`

#### `default`

- **类型**: Boolean
- **必需**: 否
- **默认值**: `false`
- **说明**: 是否为默认数据库。多个数据源时只能有一个为 `true`

#### `db_schema`

- **类型**: String
- **必需**: 否
- **默认值**: `"public"` (PostgreSQL)
- **说明**: 数据库 schema 名称，主要用于 PostgreSQL

---

### `[databases.pool_config]`

连接池配置。

#### `max_connections`

- **类型**: Integer
- **必需**: 否
- **默认值**: `10`（测试环境为 `1`）
- **说明**: 连接池最大连接数
- **建议**: 生产环境建议 `20-50`，根据并发量调整

#### `min_connections`

- **类型**: Integer
- **必需**: 否
- **默认值**: `2`
- **说明**: 连接池最小空闲连接数

#### `connect_timeout`

- **类型**: Integer (秒)
- **必需**: 否
- **默认值**: `30`
- **说明**: 获取连接的超时时间

#### `idle_timeout`

- **类型**: Integer (秒)
- **必需**: 否
- **默认值**: `600`
- **说明**: 空闲连接超时时间，超过此时间的空闲连接会被关闭

#### `max_lifetime`

- **类型**: Integer (秒)
- **必需**: 否
- **默认值**: `1800`
- **说明**: 连接最大生命周期，超过此时间的连接会被替换

---

### 健康检查配置

#### `health_check_interval`

- **类型**: Integer (秒)
- **必需**: 否
- **默认值**: `60`
- **说明**: 健康检查间隔时间

#### `health_check_timeout`

- **类型**: Integer (秒)
- **必需**: 否
- **默认值**: `5`
- **说明**: 健康检查超时时间

---

## Redis 配置

### `[redis]`

#### `url`

- **类型**: String
- **必需**: 是
- **说明**: Redis 连接地址
- **格式**: `redis://<host>:<port>/<db>`
- **示例**: `"redis://localhost:6379/13"`

#### `mode`

- **类型**: String (enum)
- **必需**: 否
- **默认值**: `"standalone"`
- **说明**: Redis 运行模式
- **可选值**:
    - `standalone` - 单机模式
    - `cluster` - 集群模式

#### `cluster_urls`

- **类型**: Array of String
- **必需**: 集群模式时必填
- **说明**: 集群节点地址列表，逗号分隔
- **示例**: `"redis://node1:6379,redis://node2:6379,redis://node3:6379"`

#### `heartbeat_interval`

- **类型**: Integer (秒)
- **必需**: 否
- **默认值**: `30`
- **说明**: Pub/Sub 心跳间隔，`0` 表示禁用心跳

#### `connection_timeout`

- **类型**: Integer (秒)
- **必需**: 否
- **默认值**: `5`
- **说明**: 连接超时时间

#### `operation_timeout`

- **类型**: Integer (秒)
- **必需**: 否
- **默认值**: `3`
- **说明**: 操作超时时间

#### `key_prefix`

- **类型**: String
- **必需**: 否
- **默认值**: `"cmx:"`
- **说明**: 默认键前缀

#### `subscribe_channels`

- **类型**: Array of String
- **必需**: 否
- **默认值**: `[]`
- **说明**: 启动时自动订阅的频道列表，需填写完整频道名，不会自动加前缀

#### `subscribe_patterns`

- **类型**: Array of String
- **必需**: 否
- **默认值**: `[]`
- **说明**: 启动时自动订阅的模式列表，支持通配符 `* ? []`，需填写完整模式

---

## WASM 运行时配置

### `[runtime]`

#### `memory_max`

- **类型**: Integer (页数)
- **必需**: 否
- **默认值**: `4096` (256MB)
- **说明**: 内存限制，每页 64KB
- **计算公式**: 内存大小 = 页数 × 64KB
- **示例**: `4096` 页 = 256MB

#### `timeout`

- **类型**: Integer (秒)
- **必需**: 否
- **默认值**: `30`
- **说明**: 单次调用超时时间

#### `pool_max_instances`

- **类型**: Integer
- **必需**: 否
- **默认值**: CPU 核心数
- **说明**: 实例池最大实例数

#### `fuel_limit`

- **类型**: Integer
- **必需**: 否
- **默认值**: `0` (不限制)
- **说明**: Fuel 限制（单位：Wasm 指令数），设置为 `0` 表示不限制
- **用途**: 防止死循环和恶意代码消耗过多 CPU

---

## 插件配置

### `[plugin]`

#### `install_root`

- **类型**: String
- **必需**: 是
- **说明**: 插件安装根目录
- **示例**: `"plugins/root"` 或 `/app/plugins/root`

#### `backup_root`

- **类型**: String
- **必需**: 是
- **说明**: 插件备份目录，用于插件升级时备份旧版本

#### `temp_root`

- **类型**: String
- **必需**: 是
- **说明**: 插件解压临时目录

#### `upload_root`

- **类型**: String
- **必需**: 是
- **说明**: 插件上传临时目录



#### `reconciliation_interval_secs`

- **类型**: Integer (秒)
- **必需**: 否
- **默认值**: `60`
- **说明**: 定时一致性校验间隔，对比数据库与本地 Registry，自动补偿差异
- **用途**: 确保节点运行时状态与数据库一致，防止通知丢失导致的状态不同步
- **可选值**: `0` 禁用定时校验，`10` 最小值（自动修正），建议 `30-120`

---

### `[plugin.auto_install]`

自动安装配置。

#### `enabled`

- **类型**: Boolean
- **必需**: 否
- **默认值**: `false`
- **说明**: 是否启用自动安装

#### 自动安装插件列表

```toml
[[plugin.auto_install.plugins]]
plugin_id = "cmx-debug"
version = "1.0.0"
source_type = "local"
source_path = "plugins/source/cmx-debug.zip"
is_critical = true
```

- `plugin_id`: 插件唯一标识符
- `version`: 插件版本号
- `source_type`: 插件来源类型 (`local` / `remote` / `marketplace`/`storage`)
- `source_path`: 插件包路径（文件路径或者url）
- `is_critical`: 是否为关键插件，关键插件安装失败会导致应用启动失败

---

## 数据库迁移配置

### `[migration]`

#### `enabled`

- **类型**: Boolean
- **必需**: 否
- **默认值**: `false`
- **说明**: 是否启用数据库迁移
- **用途**: 启用后会在应用启动时自动执行待执行的数据库迁移，禁用则跳过

#### `dir`

- **类型**: String
- **必需**: 是
- **说明**: 迁移文件目录路径
- **示例**: `"docs/sql/migrations"` 或 `/app/docs/sql/migrations`

#### `validate_checksum`

- **类型**: Boolean
- **必需**: 否
- **默认值**: `true`
- **说明**: 是否校验文件内容是否被修改
- **用途**: 启用后会校验迁移文件的 MD5 校验和，防止手动修改导致的不一致

#### `lock_timeout`

- **类型**: Integer (秒)
- **必需**: 否
- **默认值**: `60`
- **说明**: 分布式锁超时时间，多节点部署时用于防止并发执行迁移

---

## 文件存储配置

### `[storage]`

#### `default_platform`

- **类型**: String
- **必需**: 否
- **说明**: 默认存储平台标识符，不设置则自动选择第一个已启用的存储实例

---

### `[[storage.instances]]`

存储实例配置数组，支持配置多个存储平台。

#### `platform`

- **类型**: String
- **必需**: 是
- **说明**: 存储平台唯一标识符
- **示例**: `"local-1"`, `"amazon-s3-1"`

#### `storage_type`

- **类型**: String (enum)
- **必需**: 是
- **说明**: 存储类型
- **可选值**:
    - `local` - 本地文件系统存储
    - `s3` - S3 兼容对象存储

#### `enable_storage`

- **类型**: Boolean
- **必需**: 否
- **默认值**: `true`
- **说明**: 是否启用该存储平台

#### `domain`

- **类型**: String
- **必需**: 否
- **说明**: 文件访问基础域名，注意应以 `/` 结尾
- **用途**: 用于拼接生成文件的访问 URL
- **示例**: `"http://localhost:8080/files/"`

#### `base_path`

- **类型**: String
- **必需**: 否
- **默认值**: `""`
- **说明**: 存储路径基础前缀，所有上传文件路径都会以此为前缀

---

### Local 类型字段

#### `enable_access`

- **类型**: Boolean
- **必需**: 否
- **默认值**: `false`
- **说明**: 是否启用直接访问
- **用途**: 启用后可通过 `storage_path` 直接访问文件，线上环境建议使用 Nginx 代理

#### `path_patterns`

- **类型**: String
- **必需**: 否
- **说明**: 文件路径匹配模式
- **示例**: `"**/*"`

#### `storage_path`

- **类型**: String
- **必需**: Local 类型时必填
- **说明**: 本地存储的物理根目录路径
- **示例**: `"./storage"` 或 `/app/storage`

---

### S3 类型字段

#### `access_key`

- **类型**: String
- **必需**: S3 类型时必填
- **说明**: S3 Access Key ID

#### `secret_key`

- **类型**: String
- **必需**: S3 类型时必填
- **说明**: S3 Secret Access Key

#### `region`

- **类型**: String
- **必需**: S3 类型时必填
- **说明**: S3 区域
- **示例**: `"us-east-1"`, `"ap-northeast-1"`

#### `endpoint`

- **类型**: String
- **必需**: S3 类型时可选
- **说明**: S3 API 端点 URL，支持 MinIO、腾讯云 COS、阿里云 OSS 等 S3 兼容服务
- **示例**: `"http://localhost:9000/"`, `"http://192.168.1.100:9000/"`

#### `bucket_name`

- **类型**: String
- **必需**: S3 类型时必填
- **说明**: S3 桶名称

---

## 模板配置

### `[templates]`

#### `path`

- **类型**: String
- **必需**: 否
- **默认值**: `"crates/libs/cmx-dev/templates"`
- **说明**: 模板目录路径

---

## Code Server 配置

### `[code_server]`

#### `url`

- **类型**: String
- **必需**: 否
- **说明**: code-server 服务 URL，用于在线开发功能

---

### `[code_server_extension_server]`

#### `url`

- **类型**: String
- **必需**: 否
- **说明**: Code Server 扩展服务器 URL，用于自调试

---

## 节点配置

### `[node]`

#### `node_id`

- **类型**: String
- **必需**: 否
- **说明**: 节点唯一标识符，用于分布式迁移锁等场景

---

## 基础服务中心配置

插件生命周期与外部基础服务中心（门户中心、权限中心、表单中心、流程中心）之间的数据交互配置。

### `[center_client]`

#### `mode`

- **类型**: String (enum)
- **必需**: 否
- **默认值**: `"mock"`
- **说明**: 基础服务中心访问模式
- **可选值**:
    - `mock` - Mock 模式，所有接口调用返回成功结果（当前阶段）
    - `url` - URL 直连模式，通过显式配置的服务地址访问各中心
    - `discovery` - 服务发现模式，通过 Nacos 服务发现获取各中心地址

#### `timeout_ms`

- **类型**: Integer (毫秒)
- **必需**: 否
- **默认值**: `30000`
- **说明**: 各基础服务中心 HTTP 请求的超时时间

---

### `[center_client.urls]`

URL 直连模式配置（`mode = "url"` 时生效）。每个中心对应一个独立的 URL 配置项。

#### `menu`

- **类型**: String
- **必需**: `mode = "url"` 时必需
- **说明**: 门户中心（菜单数据）导入接口 URL
- **示例**: `"http://portal-center:8080/api/plugin/menu/import"`

#### `perm`

- **类型**: String
- **必需**: `mode = "url"` 时必需
- **说明**: 权限中心（权限数据）导入接口 URL
- **示例**: `"http://perm-center:8080/api/plugin/perm/import"`

#### `form`

- **类型**: String
- **必需**: `mode = "url"` 时必需
- **说明**: 表单中心（表单数据）导入接口 URL
- **示例**: `"http://form-center:8080/api/plugin/form/import"`

#### `flow`

- **类型**: String
- **必需**: `mode = "url"` 时必需
- **说明**: 流程中心（流程定义）导入接口 URL
- **示例**: `"http://flow-center:8080/api/plugin/flow/import"`

---

### `[center_client.discovery]`

服务发现模式配置（`mode = "discovery"` 时生效）。通过 Nacos 服务发现获取各中心的实例地址。

#### `nacos_group`

- **类型**: String
- **必需**: 否
- **默认值**: `"DEFAULT_GROUP"`
- **说明**: Nacos 服务分组

#### `menu_service`

- **类型**: String
- **必需**: `mode = "discovery"` 时必需
- **说明**: 门户中心在 Nacos 中注册的服务名
- **示例**: `"cmx-portal-center"`

#### `perm_service`

- **类型**: String
- **必需**: `mode = "discovery"` 时必需
- **说明**: 权限中心在 Nacos 中注册的服务名
- **示例**: `"cmx-perm-center"`

#### `form_service`

- **类型**: String
- **必需**: `mode = "discovery"` 时必需
- **说明**: 表单中心在 Nacos 中注册的服务名
- **示例**: `"cmx-form-center"`

#### `flow_service`

- **类型**: String
- **必需**: `mode = "discovery"` 时必需
- **说明**: 流程中心在 Nacos 中注册的服务名
- **示例**: `"cmx-flow-center"`

---

### 环境变量覆盖

`center_client` 配置节支持通过环境变量覆盖，详见 [ENV_MANUAL.md](ENV_MANUAL.md#基础服务中心环境变量)。

---

## RPC 配置

### `[rpc]`

#### `enabled`

- **类型**: Boolean
- **必需**: 否
- **默认值**: `false`
- **说明**: 是否启用 RPC 功能

#### `protocol`

- **类型**: String
- **必需**: 否
- **默认值**: `"grpc"`
- **说明**: RPC 通信协议，目前仅支持 `"grpc"`

#### `warmup_services`

- **类型**: Array\<String\>
- **必需**: 否
- **默认值**: `[]`
- **说明**: 启动时预先发现的服务名列表，对这些服务会主动拉取实例并缓存

#### `service_sync_interval_secs`

- **类型**: Integer (秒)
- **必需**: 否
- **默认值**: `30`
- **说明**: 服务列表定时同步间隔（秒），0 表示禁用定时同步

### `[rpc.grpc]`

#### `port`

- **类型**: Integer
- **必需**: 是
- **说明**: gRPC Server 监听端口

#### `timeout_ms`

- **类型**: Integer (毫秒)
- **必需**: 否
- **默认值**: `5000`
- **说明**: RPC 调用超时时间，通过 volo `rpc_timeout` 设置

#### `connect_timeout_ms`

- **类型**: Integer (毫秒)
- **必需**: 否
- **默认值**: `3000`
- **说明**: 连接超时时间，通过 volo `connect_timeout` 设置

#### `retry_count`

- **类型**: Integer
- **必需**: 否
- **默认值**: `0`
- **说明**: 重试次数，仅对可重试错误重试（`UNAVAILABLE`/`DEADLINE_EXCEEDED`/`RESOURCE_EXHAUSTED`/`ABORTED`），业务错误不会重试。重试带有指数退避（50ms → 800ms）和总时间预算控制

#### `default_group`

- **类型**: String
- **必需**: 否
- **默认值**: `None`
- **说明**: 默认服务分组，用于 `query_instances` 过滤。`None` 表示不按分组过滤。多机房/多 region 部署时使用

#### `default_clusters`

- **类型**: Array\<String\>
- **必需**: 否
- **默认值**: `[]`
- **说明**: 默认集群列表，用于 `query_instances` 过滤。空表示不过滤。多机房/多 region 部署时使用

#### `discover_channel_capacity`

- **类型**: Integer
- **必需**: 否
- **默认值**: `1024`
- **说明**: 服务发现变更通知通道容量。用于 `RegistryAwareDiscover` 内部 broadcast 通道的缓冲区大小。值越大越能缓冲高频服务变更（如 k8s 滚动更新 100+ Pod 同时上下线），但内存占用略增。设为 0 时使用默认值 1024
- **示例**: `2048`

---

## 注册中心 Metadata 配置

### `[registry.metadata]`

注册到注册中心的服务实例附加元数据，所有注册中心类型（Nacos/Consul/etcd/ZooKeeper）通用。

metadata 中的键值对会随服务实例一起注册到注册中心，服务消费者可通过服务发现获取这些信息。

**注意**：`grpc_port` 由 `[rpc.grpc].port` 自动注入，无需手动配置。RPC 自动注入的 key 优先级高于配置文件中的值。

#### 自定义 metadata 示例

- **类型**: HashMap\<String, String\>
- **必需**: 否
- **默认值**: `{}`
- **说明**: 用户自定义的服务实例元数据键值对，可用于服务路由、灰度发布、区域标识等场景
- **示例**:

```toml
[registry.metadata]
version = "1.0.0"
region = "cn-east"
env = "production"
```

#### 标准 Metadata Key

| Key | 说明 | 来源 |
|-----|------|------|
| `grpc_port` | gRPC 服务端口 | 由 `[rpc.grpc].port` 自动注入 |
| `version` | 服务版本号（预留） | 用户自定义 |
| `protocol` | 支持的协议列表（预留） | 用户自定义 |

---

## 配置优先级

配置优先级从高到低：

1. **环境变量** - 最高优先级，不可被覆盖（详见 [ENV_MANUAL.md](ENV_MANUAL.md#配置优先级)）
2. **远程配置中心** - 从配置中心拉取的配置
3. **本地 TOML 文件** - 配置文件中的配置
4. **代码默认值** - 代码中定义的默认值

---

## 配置文件位置

### 开发环境

配置文件通常位于项目根目录：

- `dev.toml` - 开发环境配置
- `config.toml` - 通用配置文件

### Docker 环境

```bash
./config/
├── docker.toml          # Docker 环境配置文件
└── ...
```

容器内默认配置文件路径：`/app/config/docker.toml`

### 指定配置文件

通过 `CONFIG_FILE` 环境变量指定：

```bash
CONFIG_FILE=/path/to/config.toml ./cmx-server
```

---

## 完整配置示例

- TOML 配置示例详见 [config_template.toml](config_template.toml)
- 环境变量参考详见 [.env.template](.env.template) 和 [ENV_MANUAL.md](ENV_MANUAL.md)
