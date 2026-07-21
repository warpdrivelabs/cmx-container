# CMX Container 配置手册

本文档详细介绍 CMX Container 应用的所有配置项，包括配置含义、默认值、使用建议等信息。

---

## 目录

- [服务器配置](#服务器配置)
- [Web 服务配置](#web-服务配置)
- [部署模式配置](#部署模式配置)
- [应用标识配置](#应用标识配置仅-micro-模式生效)
- [数据库配置](#数据库配置)
- [Redis 配置](#redis-配置)
- [WASM 运行时配置](#wasm-运行时配置)
- [插件配置](#插件配置)
- [数据库迁移配置](#数据库迁移配置)
- [文件存储配置](#文件存储配置)
- [模板配置](#模板配置)
- [Code Server 配置](#code-server-配置)
- [基础服务中心配置](#基础服务中心配置)
- [RPC 配置](#rpc-配置)
- [服务对外身份配置](#服务对外身份配置)
- [注册中心 Metadata 配置](#注册中心-metadata-配置)
- [认证配置](#认证配置)
- [IAM 权限管理配置](#iam-权限管理配置)
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

## 部署模式配置

### `[deploy]`

启动期契约，决定数据源加载策略、`app_id` 取值、模块导入守卫。把"部署意图"从"运行时副作用"提升为"启动期契约"，让服务既支持单体（资源全局共享）又支持分体（按模块隔离）。

支持环境变量覆盖：`DEPLOY__MODE`（详见 [ENV_MANUAL.md](ENV_MANUAL.md)）。

#### `mode`

- **类型**: string
- **必需**: 否
- **默认值**: `"mono"`
- **可选值**: `"mono"` / `"micro"`
- **说明**: 部署模式
  - `mono`（单体，默认）：一个进程服务所有域/应用/模块
    - **数据源**：加载 `cmx_sys_datasource` 中所有 `status=1 AND archived=0` 的记录（忽略 D-A-M 过滤）
    - **app_id**：`get_app_id()` 固定返回 `"default"`（不读 `[app].module_code`）
    - **模块导入守卫**：放宽（允许导入任意 `module_code` 的模块包）
    - **启动期校验**：无（mono 允许主库 + 业务库分库部署，是常见用法）
    - **`[app]` 块**：整体不生效（可省略或保留作 micro 切换预留）
  - `micro`（微服务）：一个进程只服务 `[app]` 三元组指定的模块
    - **数据源**：按 `[app]` 三元组精确过滤
    - **app_id**：返回 `[app].module_code`（维持现状）
    - **模块导入守卫**：保留（`module_code != app_id` 则拒绝）
    - **`[app]` 块**：三元组必需，缺省值 `default` 会被拒绝启动
- **示例**: `mode = "mono"`

> **mono 切换的数据迁移**：从 micro 切到 mono 时，需执行迁移脚本 `docs/sql/migrations/20260721_001_deploy_mode_mono_app_id_unification.up.sql` 把历史 `app_id` 统一为 `'default'`，否则历史数据在 mono 模式下不可见。

---

## 应用标识配置（仅 micro 模式生效）

### `[app]`

当前实例所属的域/应用/模块标识。

> **mono 模式下本块整体不生效**（`get_app_id` 固定返回 `"default"`，数据源不按此过滤）。
> **micro 模式下**：用于数据源过滤、插件/服务隔离（`app_id = module_code`）、模块导入守卫。

支持环境变量覆盖：`APP__DOMAIN_CODE` / `APP__APPLICATION_CODE` / `APP__MODULE_CODE`（双下划线分隔层级，详见 [ENV_MANUAL.md](ENV_MANUAL.md)）。

#### `domain_code`

- **类型**: string
- **必需**: micro 模式下必需
- **默认值**: `"default"`
- **说明**: 当前实例所属域编码，micro 模式下用于过滤该实例应加载的数据源
- **示例**: `"default"`、`"finance"`、`"logistics"`

#### `application_code`

- **类型**: string
- **必需**: micro 模式下必需
- **默认值**: `"default"`
- **说明**: 当前实例所属应用编码
- **示例**: `"default"`、`"erp"`、`"wms"`

#### `module_code`

- **类型**: string
- **必需**: micro 模式下必需
- **默认值**: `"default"`
- **说明**: 当前实例所属模块编码；micro 模式下决定 `app_id`
- **示例**: `"default"`、`"order"`、`"inventory"`

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

#### `db_name`

- **类型**: String
- **必需**: 否
- **默认值**: 未配置时从 `db_url` 的 path 部分解析数据库名作为显示名称
- **说明**: 数据源显示名称，便于运维识别和后台展示。仅用于展示，不参与连接逻辑
- **示例**: `"cmx"`, `"cmx-biz"`

#### `default`

- **类型**: Boolean
- **必需**: 否
- **默认值**: `false`
- **说明**: 是否为默认数据库。多个数据源时只能有一个为 `true`

#### `source_type`

- **类型**: String (enum)
- **必需**: 否
- **默认值**: 未配置时按 `default` 标志判定（`default=true` → `"default"`，否则 → `"other"`）
- **说明**: 数据源类型，标识库的用途分类，与 `default` 正交共存
- **可选值**:
    - `default` - 默认库（系统核心库）
    - `biz` - 业务库（业务数据）
    - `other` - 其他
- **示例**: `"biz"`

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
- **说明**: TCP 连接建立超时时间（保留字段）。从连接池获取连接的超时请使用 `acquire_timeout`

#### `acquire_timeout`

- **类型**: Integer (秒)
- **必需**: 否
- **默认值**: `30`
- **说明**: 从连接池获取连接的超时时间。连接池耗尽时，等待可用连接超过该时间将返回错误，而非无限期等待。适用于 PostgreSQL、MySQL、SQLite 三种数据库的连接池

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
- **默认值**: `"Standalone"`
- **说明**: Redis 运行模式
- **可选值**:
    - `Standalone` - 单机模式
    - `Cluster` - 集群模式

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

## OpenCode AI 中继配置

### `[opencode]`

cmx-ai 薄代理连接 OpenCode（:4096）的配置。优先级：环境变量 > 本段 > 默认值。
一期不持久化会话、不保存生成产物；二期演进为胖代理时新增 `session_store` 子段。

#### `base_url`

- **类型**: String
- **必需**: 否
- **默认值**: `http://127.0.0.1:4096`
- **说明**: OpenCode 服务地址（含协议与端口），对应 `opencode serve --host 0.0.0.0 --port 4096`
- **环境变量**: `OPENCODE_BASE_URL`（优先级更高）

#### `password`

- **类型**: String
- **必需**: 否
- **默认值**: 空
- **说明**: OpenCode 访问凭证（`OPENCODE_SERVER_PASSWORD`）。开发环境可留空（OpenCode 不启用鉴权）；
  生产部署必须配置强密码，cmx-ai 所有请求（含 SSE）会以 `Authorization: Bearer` 携带
- **环境变量**: `OPENCODE_SERVER_PASSWORD`（优先级更高）

#### `request_timeout_ms`

- **类型**: Integer (毫秒)
- **必需**: 否
- **默认值**: `30000`
- **说明**: 普通 HTTP 请求（创建会话/发消息/abort 等）超时时间，最小 1000ms

#### `sse_heartbeat_secs`

- **类型**: Integer (秒)
- **必需**: 否
- **默认值**: `30`
- **说明**: SSE 长连接的心跳/健康检查周期（仅作日志参考，OpenCode 实际每 10 秒推送 `server.heartbeat`）

> **注意**：AI SSE 端点 `GET /api/ai/events` 因 EventSource 无法发 Authorization header，需在
> `[auth].whitelist` 中加入 `"/api/ai/events"`，由 handler 内部校验 query `access_token`。
> 其他 `/api/ai/*` 接口（创建会话/发消息/审批等）**不要**加入白名单，需正常 Bearer 认证。

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


## 基础服务中心配置

插件生命周期与外部基础服务中心（门户中心、权限中心、表单中心、流程中心）之间的数据交互配置。

### `[center_client]`

#### `mode`

- **类型**: String (enum)
- **必需**: 否
- **默认值**: `"local"`
- **说明**: 模块资源（表单/菜单/元数据/权限）导入导出的部署模式
- **可选值**:
    - `local`（默认，或任意非远程值）- **本地模式**，定义导入器直调本地 Service（FormService/MenuService/TableMetadataService/PermissionServiceImpl），无网络开销，适用于单体部署
    - `grpc` - **远程模式（gRPC）**，经 gRPC 调用专门中心（CmxPluginDataService），需启用 `[rpc]` 并配置 `[center_client.discovery]` 的各中心服务名
    - `http_url` - **远程模式（HTTP 直连）**，经 HTTP multipart form-data POST 到 `[center_client.urls]` 配置的各中心 URL
    - `http_discovery` - **远程模式（HTTP 服务发现）**，经服务发现解析实例地址后走 HTTP，需配置 `[center_client.discovery]` 的各中心服务名
    - 远程模式下本节点仍可作为接收端（PluginDataImporterImpl 已注入 form/menu 本地导入器）

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

### `[rpc.grpc]`

#### `port`

- **类型**: Integer
- **必需**: 是
- **说明**: gRPC Server 监听端口

#### `timeout_ms`

- **类型**: Integer (毫秒)
- **必需**: 否
- **默认值**: `30000`
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

## 服务对外身份配置

### `[service_auth]`

本服务作为调用方时携带的服务级凭证，用于 gRPC / HTTP 跨服务调用的 M2M 鉴权。
凭证须同时在 `[[auth.static_api_keys]]` 注册（接收端 mw_auth 据此验证）。

#### `outgoing_api_key`

- **类型**: String
- **必需**: 否
- **默认值**: `""`（空字符串，表示不配置服务身份）

本服务对外服务级凭证，格式 `cmx_sk_xxx`。配置后：

- **gRPC 出站**：所有跨服务 gRPC 调用自动携带 `X-API-Key: <cmx_sk_xxx>` +
  委托用户 token（若有）+ 请求 ID（见 `docs/20260707_跨服务认证与认证上下文传播设计.md`）。
- **HTTP 出站**：`RemoteImporterContext` 的跨服务 HTTP 调用携带同样三层 header。

单体无跨服务调用场景可留空（默认）。微服务模式必填，否则跨服务调用会被接收端
`mw_auth` 以 401 拒绝。

#### 凭证生成与注册

1. 在 `[[auth.static_api_keys]]` 段声明一个服务专用 key（`user_id` 留空即为 M2M key）。
2. 将该 key 明文填入 `[service_auth].outgoing_api_key`。
3. 生产环境优先用环境变量注入（见 ENV_MANUAL.md）。

---

## 认证配置

### `[auth.jwt]`

JWT 编解码配置，支持 HS256（HMAC）和 RS256（RSA 非对称）两种算法。

#### `algorithm`

- **类型**: String
- **必需**: 否
- **默认值**: `"HS256"`
- **说明**: JWT 签名算法。`HS256` 使用 HMAC 对称加密，性能更好；`RS256` 使用 RSA 非对称加密，安全性更高，适合微服务场景（公钥可公开分发验签）
- **可选值**: `"HS256"`, `"RS256"`
- **示例**: `"HS256"`

#### `secret`

- **类型**: String
- **必需**: HS256 模式下必需
- **默认值**: `"a7k9m2p4x8q1w5e3r6t0y7u2i9o4p1"`
- **说明**: HMAC 签名密钥。HS256 模式下用于 JWT 编解码，生产环境**务必修改**为随机长字符串
- **示例**: `"your-random-256-bit-secret-key"`

#### `issuer`

- **类型**: String
- **必需**: 否
- **默认值**: `"cmx-auth"`
- **说明**: JWT 签发者标识，写入 Token 的 `iss` 声明，验签时校验
- **示例**: `"cmx-auth"`

#### `audience`

- **类型**: String
- **必需**: 否
- **默认值**: `"cmx-platform"`
- **说明**: JWT 受众标识，写入 Token 的 `aud` 声明，验签时校验
- **示例**: `"cmx-platform"`

#### `private_key`

- **类型**: String
- **必需**: RS256 模式下必需
- **默认值**: 无
- **说明**: RS256 模式的 RSA 私钥。支持文件路径（如 `/path/to/private.pem`）或 PEM 内容（以 `-----BEGIN` 开头）。生产环境建议使用文件路径 + `chmod 600` 权限控制
- **示例**: `"/etc/cmx/keys/jwt-private.pem"`

#### `public_key`

- **类型**: String
- **必需**: RS256 模式下必需
- **默认值**: 无
- **说明**: RS256 模式的 RSA 公钥。支持文件路径或 PEM 内容。公钥可安全分发，用于各微服务验签
- **示例**: `"/etc/cmx/keys/jwt-public.pem"`

#### `current_kid`

- **类型**: String
- **必需**: 否
- **默认值**: 无
- **说明**: 当前签发使用的密钥 ID（kid），写入 JWT Header 的 `kid` 字段。用于密钥轮换场景，验签方根据 kid 选择对应的公钥验证。不设置时 JWT Header 中不包含 kid 字段
- **示例**: `"key-2026-01"`

#### `legacy_public_keys`

- **类型**: Array of Tables（`[[auth.jwt.legacy_public_keys]]`）
- **必需**: 否
- **默认值**: `[]`（空列表）
- **说明**: 密钥轮换时的旧公钥列表。每个条目包含 `kid`（密钥 ID）和 `pem`（PEM 内容或文件路径）。宽限期内旧 kid 签发的 Token 仍可通过对应的旧公钥验签，实现无缝密钥轮换。最多支持 5 个旧密钥对
- **示例**:
  ```toml
  [[auth.jwt.legacy_public_keys]]
  kid = "key-2025-12"
  pem = "/path/to/old-public.pem"
  ```

> 环境变量覆盖格式：`AUTH__JWT__LEGACY_PUBLIC_KEYS_0_KID` / `AUTH__JWT__LEGACY_PUBLIC_KEYS_0_PEM`（索引 0-4）

### `[auth.argon2]`

Argon2 密码哈希算法配置，影响用户密码的哈希强度和计算资源消耗。

#### `memory_cost`

- **类型**: Integer
- **必需**: 否
- **默认值**: `65536`
- **说明**: 内存开销（KB），即哈希计算使用的内存量。值越大抗 GPU/ASIC 破解能力越强，但消耗更多内存。默认 65536（64MB）为 OWASP 推荐值
- **示例**: `65536`（64MB）

#### `time_cost`

- **类型**: Integer
- **必需**: 否
- **默认值**: `3`
- **说明**: 时间开销（迭代次数），即哈希计算的遍历次数。值越大哈希越慢但越安全
- **示例**: `3`

#### `parallelism`

- **类型**: Integer
- **必需**: 否
- **默认值**: `4`
- **说明**: 并行线程数，即哈希计算使用的并行度。需与服务器 CPU 核心数匹配
- **示例**: `4`

### `[auth.token]`

Token 有效期配置。

#### `access_ttl_secs`

- **类型**: Integer
- **必需**: 否
- **默认值**: `1800`
- **说明**: Access Token 有效期（秒）。值越小安全性越高但刷新频率越频繁，建议 900~3600
- **示例**: `1800`（30 分钟）

#### `refresh_ttl_secs`

- **类型**: Integer
- **必需**: 否
- **默认值**: `604800`
- **说明**: Refresh Token 有效期（秒）。应远大于 access_ttl_secs，用户在此期间内可无感刷新
- **示例**: `604800`（7 天）

### `[auth.session]`

会话管理配置。

#### `single_session_per_device_type`

- **类型**: Boolean
- **必需**: 否
- **默认值**: `false`
- **说明**: 同一设备类型是否只允许一个活跃会话。设为 `true` 后，同设备类型新登录会踢掉旧会话（SSO 互踢）
- **示例**: `false`

#### `max_sessions`

- **类型**: Integer
- **必需**: 否
- **默认值**: `0`
- **说明**: 单用户最大并发会话数。`0` 表示不限制
- **示例**: `0`

#### `idle_timeout_secs`

- **类型**: Integer
- **必需**: 否
- **默认值**: `86400`
- **说明**: 会话空闲超时时间（秒）。超过此时间无心跳的会话将被定时清理任务标记为过期
- **示例**: `86400`（24 小时）

#### `heartbeat_interval_secs`

- **类型**: Integer
- **必需**: 否
- **默认值**: `300`
- **说明**: 客户端心跳间隔（秒）。客户端需定时调用心跳接口刷新 `last_active_at`，防止会话过期
- **示例**: `300`（5 分钟）

### `[auth.cache]`

认证缓存配置。

#### `enable_local_cache`

- **类型**: Boolean
- **必需**: 否
- **默认值**: `true`
- **说明**: 是否启用本地缓存（moka）。启用后 Token 黑名单和会话存活检查优先走本地缓存，减少 Redis 往返
- **示例**: `true`

#### `local_ttl_secs`

- **类型**: Integer
- **必需**: 否
- **默认值**: `30`
- **说明**: 本地缓存 TTL（秒）。值越小实时性越好但 Redis 压力越大。安全敏感场景建议设为 5 秒以下
- **示例**: `30`

#### `local_cache_max_entries`

- **类型**: Integer
- **必需**: 否
- **默认值**: `10000`
- **说明**: 本地缓存最大容量（条目数）
- **示例**: `10000`

#### `max_login_attempts`

- **类型**: Integer
- **必需**: 否
- **默认值**: `5`
- **说明**: 登录失败锁定阈值。连续失败达到此次数后账号将被锁定
- **示例**: `5`

#### `lock_duration_secs`

- **类型**: Integer
- **必需**: 否
- **默认值**: `900`
- **说明**: 账号锁定时长（秒）。锁定期间拒绝所有登录请求
- **示例**: `900`（15 分钟）

### `[auth]`

认证主配置节（顶层），用于存放与子节无关的认证级配置。

#### `whitelist`

- **类型**: Array of String
- **必需**: 否
- **默认值**: `[]`（仅使用内置白名单）
- **说明**: 认证白名单（无需认证的路径规则列表）。请求路径匹配列表中任一规则即跳过认证中间件。**用户配置为追加模式**，启动时与内置白名单合并去重，不会覆盖内置项。内置白名单始终生效，包括：
  - `/api/auth/login`、`/api/auth/refresh`、`/api/auth/validate`、`/api/auth/logout`、`/api/auth/health`
  - `/api/auth/oauth2/authorize`、`/api/auth/oauth2/login`、`/api/auth/oauth2/token`、`/api/auth/oauth2/providers`、`/api/auth/oauth2/provider`
  - `/swagger`、`/api-docs`、`/health`
- **匹配规则**（支持通配符）：
  - **普通规则**（不含通配符）：前缀匹配。如 `/api/public` 匹配 `/api/public`、`/api/public/anything`、`/api/public/a/b/c`
  - **`*` 通配符**：匹配单层路径段（不含 `/`）。如 `/api/biz/*` 匹配 `/api/biz/users`、`/api/biz/orders`，但不匹配 `/api/biz/users/123`
  - **`**` 通配符**：匹配多层路径（含 `/`）。如 `/api/auth/**` 匹配 `/api/auth/`、`/api/auth/oauth2/token`、`/api/auth/a/b/c`
  - **`?` 通配符**：匹配单个非 `/` 字符。如 `/api/v?/users` 匹配 `/api/v1/users`、`/api/v2/users`，但不匹配 `/api/v12/users`
- **示例**:
  ```toml
  [auth]
  whitelist = ["/api/public", "/api/v1/webhook", "/api/biz/*", "/api/docs/**"]
  ```
- **使用场景**: Webhook 回调、公开 API、静态资源、监控探针、版本化 API 前缀等无需认证的接口
- **安全提示**:
  - 仅添加完全公开的端点，避免误将内部 API 加入白名单导致未授权访问
  - `**` 通配符匹配范围广，请谨慎使用（如 `/api/**` 会使整个 `/api/` 命名空间免认证）
  - 建议优先使用更精确的 `*` 单层匹配或普通前缀规则

### `[auth.oauth2]`

OAuth2 授权码模式配置。OAuth2 功能为可选模块，不配置时使用代码默认值。

包含两部分功能：
1. **Authorization Server**（自建授权服务）— 授权码 + PKCE
2. **OAuth2 Client**（第三方 Provider 对接）— Google/GitHub/通用 Provider 登录

#### `auth_code_ttl_secs`

- **类型**: Integer
- **必需**: 否
- **默认值**: `600`
- **说明**: 授权码有效期（秒）。授权码在此时间内可换取 Token，过期后需重新发起 authorize 请求。建议不超过 10 分钟
- **示例**: `600`（10 分钟）

#### `pkce_required`

- **类型**: Boolean
- **必需**: 否
- **默认值**: `true`
- **说明**: 是否强制 PKCE（Proof Key for Code Exchange）。开启后客户端必须在 authorize 请求中提供 `code_challenge`，token 交换时提供 `code_verifier`，防止授权码拦截攻击。生产环境强烈建议开启
- **示例**: `true`

#### `state_ttl_secs`

- **类型**: Integer
- **必需**: 否
- **默认值**: `600`
- **说明**: 第三方 OAuth2 Provider 的 state 参数有效期（秒）。state 用于防止 CSRF 攻击，用户在 Provider 授权页停留超时后需重新发起授权
- **示例**: `600`（10 分钟）

#### `callback_code_ttl_secs`

- **类型**: Integer
- **必需**: 否
- **默认值**: `30`
- **说明**: 第三方 OAuth2 回调授权码有效期（秒）。回调成功后服务端签发一次性授权码，前端需在有效期内调用 `/api/auth/oauth2/provider/exchange` 换取 Token。建议设置较短（30 秒），防止重放
- **示例**: `30`

#### `frontend_callback_url`

- **类型**: String
- **必需**: 是（启用第三方 Provider 时）
- **默认值**: 无
- **说明**: 第三方 OAuth2 登录成功后重定向到前端的 URL。回调时拼接为 `{frontend_callback_url}?code={one_time_code}&state={original_state}`。配置缺失时回调将返回错误
- **示例**: `"https://app.example.com/auth/callback"`

> OAuth2 配置支持环境变量覆盖，详见 [ENV_MANUAL.md](ENV_MANUAL.md#认证配置环境变量覆盖)

### `[auth.oauth2.account_link]`

第三方账号关联策略配置。控制 Provider 用户与本地用户的关联行为。

#### `auto_link_by_email`

- **类型**: Boolean
- **必需**: 否
- **默认值**: `false`
- **说明**: 是否根据邮箱自动关联已有本地用户。启用后，当 Provider 返回的邮箱（需已验证）与本地用户邮箱匹配时，自动创建关联记录。邮箱未验证时跳过关联，继续尝试后续策略
- **示例**: `true`

#### `auto_link_by_username`

- **类型**: Boolean
- **必需**: 否
- **默认值**: `true`
- **说明**: 是否根据用户名自动关联已有本地用户（企业场景常用）。启用后，当 Provider 返回的 username 与本地用户名匹配时，自动创建关联记录。关联优先级：已关联 > 邮箱关联（要求已验证）> username 关联 > 自动注册 > BindingRequired
- **安全提示**: 仅应对可信 Provider 启用。与 `auto_link_by_email`（要求邮箱已验证）不同，username 无"已验证"概念，恶意 Provider 可通过返回目标用户名关联到任意本地账号
- **示例**: `true`

#### `auto_register`

- **类型**: Boolean
- **必需**: 否
- **默认值**: `false`
- **说明**: 是否自动注册新用户。启用后，当无匹配的本地用户时自动创建用户并关联。禁用时返回"账号未注册"错误
- **示例**: `false`

#### `default_role`

- **类型**: String
- **必需**: 否
- **默认值**: 无
- **说明**: 自动注册时的默认角色编码（role_code）。自动注册的用户将自动关联此角色。需为 `cmx_role` 表中已存在且启用的角色编码
- **示例**: `"user"`

#### `username_strategy`

- **类型**: String
- **必需**: 否
- **默认值**: `"username"`
- **说明**: 自动注册时的用户名生成策略
- **可选值**:
  - `"provider_prefix"` — `{provider}_{provider_user_id}`，如 `google_1234567890`
  - `"provider_user_id"` — 直接使用 Provider 返回的 `provider_user_id`，如 `1234567890`
  - `"username"` — 直接使用 Provider 返回的 `username` 字段；字段缺失时回退为 `{provider}_{provider_user_id}`
  - `"email_prefix"` — 邮箱 @ 前部分，如 `user` from `user@gmail.com`
  - `"display_name"` — 使用 Provider 返回的昵称，冲突时追加 4 位随机后缀
- **示例**: `"provider_prefix"`

### `[[auth.oauth2.providers]]`

第三方 OAuth2 Provider 列表配置（数组表格式）。每个元素定义一个 Provider。

#### `name`

- **类型**: String
- **必需**: 是
- **默认值**: 无
- **说明**: Provider 唯一标识，用于 API 路由和关联记录
- **示例**: `"google"`

#### `display_name`

- **类型**: String
- **必需**: 否
- **默认值**: 同 `name`
- **说明**: Provider 显示名称，供前端展示登录按钮
- **示例**: `"Google"`

#### `provider_type`

- **类型**: String
- **必需**: 否
- **默认值**: `"generic"`
- **说明**: Provider 实现类型。内置实现提供端点 URL 默认值和特殊处理逻辑
- **可选值**:
  - `"google"` — Google OAuth2（含 JWKS 签名验证 ID Token）
  - `"github"` — GitHub OAuth2（含 /user/emails API）
  - `"generic"` — 通用标准 OAuth2（需配置端点 URL）
- **示例**: `"google"`

#### `client_id`

- **类型**: String
- **必需**: 是
- **默认值**: 无
- **说明**: OAuth2 Client ID，在 Provider 开发者后台获取
- **示例**: `"your-client-id.apps.googleusercontent.com"`

#### `client_secret`

- **类型**: String
- **必需**: 是
- **默认值**: 无
- **说明**: OAuth2 Client Secret，仅服务端使用，不暴露给前端
- **示例**: `"your-client-secret"`

#### `redirect_uri`

- **类型**: String
- **必需**: 是
- **默认值**: 无
- **说明**: 回调地址。服务端配置，前端不可覆盖，防止 Open Redirect。格式为 `{your-domain}/api/auth/oauth2/{provider}/callback`
- **示例**: `"https://your-domain.com/api/auth/oauth2/provider/google/callback"`

#### `authorize_url`

- **类型**: String
- **必需**: `provider_type = "generic"` 时必需
- **默认值**: 无
- **说明**: 授权端点 URL。内置类型（google/github）有默认值，generic 类型必须配置
- **示例**: `"https://gitlab.com/oauth/authorize"`

#### `token_url`

- **类型**: String
- **必需**: `provider_type = "generic"` 时必需
- **默认值**: 无
- **说明**: Token 端点 URL。内置类型有默认值，generic 类型必须配置
- **示例**: `"https://gitlab.com/oauth/token"`

#### `userinfo_url`

- **类型**: String
- **必需**: `provider_type = "generic"` 时必需
- **默认值**: 无
- **说明**: 用户信息端点 URL。内置类型有默认值，generic 类型必须配置
- **示例**: `"https://gitlab.com/api/v4/user"`

#### `scopes`

- **类型**: String（逗号分隔）
- **必需**: 否
- **默认值**: 内置类型有默认值（google: `["openid", "email", "profile"]`，github: `["user:email", "read:user"]`）
- **说明**: 请求的 scope 列表。配置文件中使用逗号分隔的字符串格式
- **示例**: `"openid,email,profile"`

#### `token_endpoint_auth_method`

- **类型**: String
- **必需**: 否
- **默认值**: `"client_secret_post"`
- **说明**: Token 端点认证方式
- **可选值**:
  - `"client_secret_post"` — client_id 和 client_secret 放在 POST body 中
  - `"client_secret_basic"` — client_id 和 client_secret 使用 HTTP Basic Auth
- **示例**: `"client_secret_post"`

#### `field_mapping`

- **类型**: 内联表（TOML inline table）
- **必需**: 否
- **默认值**: 空
- **说明**: 用户信息字段映射，仅 generic 类型使用。将 Provider 返回的 JSON 字段名映射到标准字段名，支持 number 类型自动转 string
- **标准字段**: `provider_user_id`, `email`, `email_verified`, `username`, `display_name`, `avatar_url`
- **示例**: `{ provider_user_id = "id", email = "email", username = "username", display_name = "name", avatar_url = "avatar_url" }`

#### `token_response_path`

- **类型**: String
- **必需**: 否
- **默认值**: `""`（无包装）
- **说明**: Token 响应嵌套路径（点分 JSON 路径）。部分厂商（企业 CAS）将 Token 响应包装在 `{"code":0,"data":{...}}` 中，配置此字段后先导航到指定路径再提取 Token 字段。空字符串表示直接从根对象提取
- **示例**: `"data"` 或 `"result.data"`

#### `token_field_mapping`

- **类型**: 内联表（TOML inline table）
- **必需**: 否
- **默认值**: 空
- **说明**: Token 响应字段映射，仅 generic 类型使用。将标准字段名映射到厂商实际字段名
- **标准字段**: `access_token`, `token_type`, `expires_in`, `refresh_token`, `scope`, `id_token`
- **示例**: `{ access_token = "accessToken", expires_in = "expire" }`

#### `userinfo_method`

- **类型**: String
- **必需**: 否
- **默认值**: `"GET"`
- **说明**: 用户信息端点请求方法。部分厂商要求使用 POST 请求获取用户信息
- **可选值**: `"GET"`、`"POST"`
- **示例**: `"GET"`

#### `userinfo_token_param`

- **类型**: String
- **必需**: 否
- **默认值**: `"bearer"`
- **说明**: 用户信息端点 access_token 传递方式
- **可选值**:
  - `"bearer"` — 通过 `Authorization: Bearer {token}` 请求头传递（默认）
  - `"query"` — 通过 query string `access_token={token}` 传递
  - `"form"` — 通过 form body `access_token={token}` 传递（仅 POST 有效）
- **边界处理**: `userinfo_method = "GET"` + `userinfo_token_param = "form"` 不合理（GET 无 body），自动降级为 `query` 并记录 warn 日志
- **示例**: `"bearer"`

#### `userinfo_extra_params`

- **类型**: 表（TOML table）
- **必需**: 否
- **默认值**: 空
- **说明**: 用户信息端点额外请求参数。始终作为 query 参数附加（GET/POST 均同，与 Java 实现一致）
- **示例**:

  ```toml
  [auth.oauth2.providers.userinfo_extra_params]
  client_id = "xxx"
  ```

#### `userinfo_response_path`

- **类型**: String
- **必需**: 否
- **默认值**: `""`（无包装）
- **说明**: 用户信息响应嵌套路径（点分 JSON 路径）。部分厂商将用户信息包装在 `{"code":0,"data":{...}}` 中。空字符串表示直接从根对象提取
- **示例**: `"data"`

#### `authorize_extra_params`

- **类型**: 表（TOML table）
- **必需**: 否
- **默认值**: 空
- **说明**: 授权 URL 额外参数。部分厂商授权端点需要额外参数（如 Azure AD 的 `resource` 参数）
- **示例**:

  ```toml
  [auth.oauth2.providers.authorize_extra_params]
  resource = "https://graph.microsoft.com"
  ```

#### `skip_ssl_verification`

- **类型**: Boolean
- **必需**: 否
- **默认值**: `false`
- **说明**: 是否跳过 SSL 证书验证。仅内网自签名证书场景使用，生产环境慎用。启用后使用 `danger_accept_invalid_certs` 构建客户端，构建失败时回退到默认 Client
- **示例**: `false`

#### `icon_url`

- **类型**: String
- **必需**: 否
- **默认值**: 内置类型有默认值
- **说明**: Provider 图标 URL，供前端展示登录按钮图标
- **示例**: `"https://www.gstatic.com/firebasejs/ui/identity/google.svg"`

#### `brand_color`

- **类型**: String
- **必需**: 否
- **默认值**: 内置类型有默认值（google: `#4285F4`，github: `#24292e`）
- **说明**: 品牌色（十六进制颜色码），供前端按钮样式使用
- **示例**: `"#4285F4"`

#### `enabled`

- **类型**: Boolean
- **必需**: 否
- **默认值**: `true`
- **说明**: 是否启用此 Provider。禁用的 Provider 不会注册到 Registry，`list_providers` 不返回
- **示例**: `true`

### `[auth.super_admin]`

超管初始化配置。可选，未配置时使用默认账号（`admin` / `cmxadmin`）。系统启动时自动创建超管账号；若账号已存在，则每次启动同步密码（配置为密码唯一真源）。

#### `username`

- **类型**: String
- **必需**: 否
- **默认值**: `"admin"`
- **说明**: 超管用户名。未配置时使用默认值
- **示例**: `"admin"`

#### `password`

- **类型**: String
- **必需**: 否
- **默认值**: `"cmxadmin"`
- **说明**: 超管密码。每次启动同步到数据库（覆盖通过 UI 修改的密码）。生产环境强烈建议通过环境变量注入，不要写在配置文件中
- **示例**: `"a7k9m2p4x8q1w5e3r6t0y7u2i9o4p1"`

#### `email`

- **类型**: String
- **必需**: 否
- **默认值**: 无
- **说明**: 超管邮箱地址
- **示例**: `"admin@example.com"`

#### `roles`

- **类型**: String（逗号分隔）
- **必需**: 否
- **默认值**: `"admin"`
- **说明**: 超管角色编码列表，逗号分隔
- **示例**: `"super_admin,system_admin"`

> 超管配置支持环境变量覆盖，详见 [ENV_MANUAL.md](ENV_MANUAL.md#认证配置环境变量覆盖)

### `[[auth.static_api_keys]]`

静态 API Key 配置（数组表格式）。可选，配置后系统启动时自动导入到 `cmx_auth_api_key` 表。

**简化用法（推荐）**：只需配置 `key` 字段，`key_prefix` 会自动从 `key` 的前 8 位提取。启动时会在日志中打印每个导入的 Key（明文）和 prefix，便于复制使用。将日志中的 `key` 值作为请求头 `X-API-Key` 的值即可。

**高级用法**：显式指定 `key_prefix`（用于迁移旧数据或自定义前缀）。

#### `key`

- **类型**: String
- **必需**: 是
- **默认值**: 无
- **说明**: API Key 明文。启动时自动 SHA256 哈希后存储到数据库，明文不保留。建议使用带前缀的随机字符串（如 `cmx_sk_xxxxxxxx...`，长度 ≥ 32 字符）。启动日志会打印此值，便于管理员复制使用作为 `X-API-Key` 请求头
- **示例**: `"cmx_sk_Ab3dEf9hJkLmN2pQrStUvWxYz123456"`

#### `key_prefix`

- **类型**: String
- **必需**: 否
- **默认值**: 从 `key` 的前 8 位自动提取
- **说明**: API Key 前缀（唯一标识）。未配置时自动从 `key` 前 8 位提取；显式配置用于迁移旧数据或自定义前缀场景
- **示例**: `"cmx_sk_"`

#### `user_id`

- **类型**: String
- **必需**: 否
- **默认值**: 无
- **说明**: 关联的用户 ID
- **示例**: `"system"`

#### `service_name`

- **类型**: String
- **必需**: 否
- **默认值**: 无
- **说明**: 关联的服务名称
- **示例**: `"internal-service"`

#### `scopes`

- **类型**: String（逗号分隔）
- **必需**: 否
- **默认值**: 无
- **说明**: 允许的 scope 列表，逗号分隔
- **示例**: `"read,write"`

#### `description`

- **类型**: String
- **必需**: 否
- **默认值**: 无
- **说明**: API Key 描述
- **示例**: `"内部服务 API Key"`

---

## IAM 权限管理配置

### `[iam]`

IAM（Identity and Access Management）权限管理配置，控制用户、角色、权限等 RBAC 功能的行为。

#### `auth_db_id`

- **类型**: String
- **必需**: 否
- **默认值**: 使用 `default_db_id`（即 `[[databases]]` 中 `default = true` 的数据源）
- **说明**: IAM 表所在的数据源标识。指定后，cmx_user/cmx_role/cmx_permission 等 IAM 表将使用此 db_id 对应的数据库
- **示例**: `"primary"`

#### `password_min_length`

- **类型**: Integer
- **必需**: 否
- **默认值**: `8`
- **说明**: 用户密码最小长度。创建用户和修改密码时校验，短于此长度的密码将被拒绝
- **示例**: `8`

#### `builtin_role_codes`

- **类型**: String（逗号分隔）
- **必需**: 否
- **默认值**: `"admin"`
- **说明**: 内置角色编码列表，逗号分隔。内置角色不可删除（Service 层会拦截删除请求），确保系统管理角色始终可用
- **示例**: `"admin"`, `"admin,system_admin"`

#### `permission_cache_ttl_secs`

- **类型**: Integer (秒)
- **必需**: 否
- **默认值**: `300`
- **说明**: 权限缓存 TTL（预留配置）。当前权限检查依赖 AuthContext 内存查询，未来若引入 IamChecker 本地缓存（moka），此配置控制缓存过期时间
- **示例**: `300`（5 分钟）

#### `permission_consistency_mode`

- **类型**: String
- **必需**: 否
- **默认值**: `"warn"`
- **说明**: 权限一致性校验模式。启动时比对代码声明的权限码（通过 `#[has_permission]` 等宏注册到 inventory）与 DB `cmx_permission` 表中的记录。如果代码声明的权限在 DB 中不存在，会导致 `IamChecker` 的 SQL JOIN 查不到该权限，宏注入的 `require_permission()` 将拒绝所有非超级用户。可选值：
  - `"panic"`: 缺失时启动 panic，强制开发者手动创建 DB 记录
  - `"warn"`: 仅告警并输出建议执行的 INSERT DDL，不阻断启动
  - `"off"`: 不校验
- **示例**: `"warn"`

#### `enable_sod_check`

- **类型**: Boolean
- **必需**: 否
- **默认值**: `true`
- **说明**: 是否启用 SoD（职责分离互斥）规则校验（Separation of Duties）。开启后，在分配角色/权限时会校验互斥规则（功能权限互斥 + 角色互斥）。规则数据存储于 `cmx_exclusion_rule` 表；当规则表为空时校验直接通过（无规则=不拦截），可安全开启。关闭时所有互斥校验跳过。如需停用可显式设为 `false`
- **示例**: `true`

#### `assignment_cleanup_interval_secs`

- **类型**: Integer (秒)
- **必需**: 否
- **默认值**: `3600`
- **说明**: 临时授权清理任务执行间隔。定时将过期的 `cmx_user_role_assignment` 记录（`effective_until < NOW()`）标记为已撤销（`status = 0`）
- **示例**: `3600`（1 小时）

#### `audit_batch_size`

- **类型**: Integer
- **必需**: 否
- **默认值**: `100`
- **说明**: 审计日志批量阈值。临时授权批量过期时，超过此阈值则聚合为统计记录（`{expired_count, sample_ids}`）而非逐条写审计日志，避免审计日志爆炸
- **示例**: `100`

#### `failure_mode`

- **类型**: String
- **必需**: 否
- **默认值**: `"FailClose"`
- **说明**: 故障降级策略。DB/缓存故障时的权限检查降级行为。可选值：
  - `"FailClose"`: 全部拒绝（更安全，推荐生产环境使用）
  - `"FailOpen"`: 仅放行 `system:all` 用户（DB 也故障时实际返回错误）
- **示例**: `"FailClose"`

#### `circuit_breaker_threshold`

- **类型**: Integer
- **必需**: 否
- **默认值**: `5`
- **说明**: 熔断阈值。权限检查连续失败达到此次数后触发熔断，按 `failure_mode` 策略降级
- **示例**: `5`

#### `circuit_breaker_reset_secs`

- **类型**: Integer (秒)
- **必需**: 否
- **默认值**: `60`
- **说明**: 熔断恢复时间。熔断器打开后经过此时间自动尝试恢复（半开状态）
- **示例**: `60`

### `[iam_permissions]`

IAM 路由权限映射配置（可选）。配置 API 路由到权限码的映射，`mw_permission` 中间件据此进行权限校验。

**行为规则**：
- 未配置映射的路由默认放行（白名单模式）
- 拥有 `system:all` 权限的用户自动放行所有路由
- 路由路径支持最长前缀匹配（如 `/api/iam/users` 会匹配 `/api/iam/users/123`）

**配置格式**：

```toml
[iam_permissions]
"/api/iam/users" = "user:read"
"/api/iam/roles" = "role:read"
"/api/iam/permissions" = "permission:read"
```

每个条目为键值对：
- **键**: API 路由路径前缀
- **值**: 所需权限码（格式 `resource:action`，如 `user:read`、`role:write`、`system:all`）

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
