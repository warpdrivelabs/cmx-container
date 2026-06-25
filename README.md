# cmx-container

> 插件化容器运行时：WebAssembly 插件热插拔、可视化服务编排、统一认证授权（JWT + IAM）、分布式存储、注册/配置中心、gRPC 通信与全链路审计。

[![Version](https://img.shields.io/badge/version-0.1.9-blue.svg)]()
[![Edition](https://img.shields.io/badge/rust--edition-2024-orange.svg)]()
[![Workspace](https://img.shields.io/badge/cargo-workspace-lightgrey.svg)]()

## 项目简介

`cmx-container` 是基于 Rust 构建的 **插件化容器运行时**，以 Cargo Workspace 组织，由 20+ 个 crate 协同组成。核心目标是在单一二进制中提供：

- **WASM 插件热插拔**（Extism + 自研 SDK，集群级 Redis Pub/Sub 同步）
- **可视化服务编排**（声明式 JSON 流程 + 事务框 + 分支路由）
- **统一认证**（JWT 双令牌、Refresh Rotation、OAuth2 + PKCE）
- **统一授权**（RBAC/权限/角色组/SoD 互斥规则 + 缓存与熔断）
- **分布式基础设施**（PostgreSQL、Redis、S3/本地存储、Nacos 注册&配置、gRPC）
- **全链路审计**（统一 `AuditLogger` trait，数据库落地）

## 核心功能

| 领域 | 能力 |
|------|------|
| 插件系统 | 基于 Extism 的 WebAssembly 插件运行时；安装/升级/降级/卸载/覆盖安装；集群同步；签名验证；插件市场；审计 |
| 服务编排 | 声明式 JSON 流程、事务框、switch 多分支、SVRContext 上下文、调试模式 |
| 认证 | `cmx-auth`：JWT 双令牌 + Refresh Rotation、Argon2id、OAuth2 授权码 + PKCE、API Key、登录失败锁定、Prometheus 指标 |
| 授权 | `cmx-iam`：用户/角色/权限/角色组 CRUD、临时授权、SoD 互斥规则、缓存+熔断的 `IamChecker` |
| 审计 | `cmx-audit`：通用 `AuditLogger` trait + `DatabaseAuditStore`，供各模块统一落库 |
| Web 框架 | 基于 Axum 0.8 + tower-http，统一 `ApiResp`、OpenAPI/Swagger UI、axum 中间件 |
| RPC | `cmx-rpc`：基于 volo-grpc 的服务端 + 客户端，集成注册中心服务发现 + 负载均衡 |
| 注册/配置 | `cmx-registry-config`：抽象 `ServiceRegistry` / `ConfigCenter` trait，内置 Mock + Nacos 实现 |
| 文件存储 | `cmx-storage`：本地存储 + S3 兼容对象存储（opendal）；缩略图自动生成 |
| 分布式缓存 | `cmx-buffer`：Redis 客户端 + moka 本地缓存 + 分布式锁 |
| 数据库 | `cmx-database`：基于 sqlx 0.9（PostgreSQL/MySQL/SQLite），统一事务上下文 |
| 配置中心 | 支持 Nacos 远程配置覆盖（兼容旧 `NACOS_*` 环境变量） |
| SDK | `sdk/cmx-cli`：插件工程脚手架与 `api.json` 生成工具 |

## 整体架构

```text
                      ┌────────────────────────────────────────────┐
                      │            web-server (Axum 0.8)           │
                      │  ┌────────────────────────────────────┐   │
                      │  │ cmx-api │ cmx-biz │ cmx-service   │   │
                      │  └────────────────────────────────────┘   │
                      └─────┬───────────┬──────────────┬──────────┘
                            │           │              │
                ┌───────────▼─┐   ┌─────▼─────┐   ┌────▼─────┐
                │  cmx-auth   │   │  cmx-iam  │   │ cmx-     │
                │  (JWT/OAuth)│   │ (RBAC/SoD)│   │ plugin   │
                └─────┬───────┘   └─────┬─────┘   └────┬─────┘
                      │ 注入 UserAuthQuery    │         │ Extism WASM
                      │     PermissionChecker │         │
                ┌─────▼─────────────────────▼───────────▼─────┐
                │              cmx-audit (统一审计)             │
                └─────────────────────┬───────────────────────┘
                                      │
        ┌──────────────┬──────────────┼──────────────┬──────────────┐
        │              │              │              │              │
   ┌────▼─────┐  ┌─────▼─────┐  ┌─────▼─────┐  ┌─────▼─────┐  ┌─────▼─────┐
   │cmx-      │  │cmx-       │  │cmx-       │  │cmx-       │  │cmx-       │
   │database  │  │buffer     │  │storage    │  │rpc (gRPC) │  │registry-  │
   │(sqlx)    │  │(Redis)    │  │(opendal)  │  │(volo)     │  │config     │
   └──────────┘  └───────────┘  └───────────┘  └───────────┘  │(Nacos/Mock)│
                                                              └───────────┘
```

### 关键设计原则

- **单一写入原则**：分布式插件管理中，仅 API 接收节点写 DB，其他节点通过 Redis Pub/Sub 同步运行时。
- **Trait 解耦**：`cmx-auth` 不直接依赖 `cmx-iam`，通过 `cmx-traits::auth::UserAuthQuery` + `cmx-traits::iam::PermissionChecker` 在 `cmx-biz` 注入。
- **集中错误处理**：所有自定义 Error 派生 `thiserror::Error`，使用 `#[error(transparent)]` + `#[from]` 做转换。
- **统一日志**：全工作区使用 `tracing`（禁止 `log` crate）。
- **依赖集中管理**：所有第三方依赖在 workspace `Cargo.toml` 统一定义，sub-crate 通过 `workspace = true` 引用并单独加注释。

## 目录结构

```text
cmx-container/
├── crates/
│   ├── web/
│   │   └── web-server/              # 主二进制：Axum HTTP 服务
│   └── libs/
│       ├── cmx-core/                # 核心类型：SVRContext、DataValue、雪花 ID
│       ├── cmx-utils/               # 通用工具：加密、雪花 ID、文件、Hash
│       ├── cmx-traits/              # 跨模块 trait：AuthService、PermissionChecker、PluginQuery、RuntimeInvoker
│       ├── cmx-macros/              # 过程宏
│       ├── modql/                   # MongoDB 风格查询过滤器（本地副本）
│       │
│       ├── cmx-api/                 # HTTP API 层：路由、ApiResp、OpenAPI
│       ├── cmx-api-types/           # API 通用类型
│       ├── cmx-biz/                 # 业务逻辑层：UserAuthQuery / PermissionChecker 注入
│       │
│       ├── cmx-iam/                 # IAM：用户/角色/权限/角色组/临时授权/SoD
│       │
│       ├── cmx-plugin/              # 插件管理：注册表、ZIP 加载、签名、生命周期、集群同步
│       ├── cmx-plugin-sdk/          # WASM 插件 SDK（cdylib + rlib）
│       ├── cmx-plugin-demo/         # 插件示例工程
│       ├── cmx-runtime/             # Extism WASM 运行时
│       ├── cmx-debug/               # 运行时调试支持
│       │
│       ├── cmx-service/             # 服务编排：Orchestrator、事务框、switch 节点
│       │
│       └── cmx-infra/               # 基础设施层
│           ├── cmx-database/        # sqlx 封装 + 事务上下文
│           ├── cmx-buffer/          # Redis 缓存 + 分布式锁
│           ├── cmx-storage/         # opendal 本地/S3 存储
│           ├── cmx-rpc/             # volo gRPC 客户端/服务端
│           ├── cmx-registry-config/ # 注册中心/配置中心抽象（Nacos/Mock）
│           ├── cmx-auth/            # 统一认证基础设施
│           ├── cmx-audit/           # 统一审计基础设施
│           ├── cmx-nacos/           # （已停用，由 cmx-registry-config 替代）
│           └── cmx-rpc-gen/         # volo-build gRPC 代码生成
├── sdk/
│   └── cmx-cli/                     # 插件开发 CLI：脚手架 + api.json 生成
├── config/                          # 配置模板与配置手册
│   ├── config_template.toml
│   ├── docker.toml
│   ├── .env.template
│   ├── CONFIG_MANUAL.md
│   └── ENV_MANUAL.md
├── docker/                          # Docker 镜像、Compose、K8s 部署
│   ├── Dockerfile
│   ├── docker-compose*.yml
│   ├── k8s-deployment.yml
│   └── scripts/
├── docs/                            # 设计文档与方案
│   ├── 插件目录说明.md
│   ├── cmx-docker-build-guide.md
│   ├── multi-instance-deployment-analysis.md
│   ├── WASM 插件调用安全防护机制设计.md
│   ├── cargo-workspace使用指南.md
│   ├── 表定义JSON解析与DDL生成开发文档.md
│   ├── api.json
│   └── sql/
├── e2e_tests/                       # 端到端测试
├── example/                         # 参考实现与迁移样例
│   ├── cmxold/
│   ├── javaoauth2/
│   ├── redis-rs/
│   └── rust-modql/
├── plugins/                         # 插件运行目录（root/backup/temp/uploads）
├── storage/                         # 本地文件存储根目录
├── logs/                            # 日志输出目录
├── Cargo.toml                       # Workspace 配置（version 0.1.9）
├── Cross.toml                       # 跨平台构建配置
├── dev.toml                         # 开发环境配置
├── dev-vpn.toml                     # VPN 环境配置
├── Cargo.lock
└── README.md
```

## 快速开始

### 环境要求

- **Rust 1.85+**（Edition 2024）
- **PostgreSQL 12+** / MySQL / SQLite 任选其一
- **Redis 6+**
- **Nacos 2.x**（可选，用于服务注册与远程配置）
- **WASM 工具链**（仅插件开发需要）

### 1. 克隆与构建

```bash
git clone https://git.openserver.cn:8089/CPPSPACE/cmxspace/cmxcontainerservice/cmx-container.git
cd cmx-container

# Debug 构建
cargo build

# Release 构建
cargo build --release
```

> **注意**：项目使用 `volo-build` 在编译时自动生成 gRPC 代码（`cmx-rpc-gen`）。
> 若遇到 `cmx_service_orchestrator.rs not found` 之类的错误，请清理构建缓存：
> ```bash
> rm -rf target/debug/build/cmx-rpc-gen-* && cargo build
> ```

### 2. 配置

复制模板并按需修改：

```bash
cp config/config_template.toml config.toml
cp config/.env.template .env
```

最小化 `config.toml` 示例：

```toml
[server]
host = "0.0.0.0"
port = 8080

[[databases]]
db_id = "primary"
db_type = "postgres"
db_url = "postgresql://postgres:postgres@localhost:5432/cmx"
default = true

[databases.pool_config]
max_connections = 20

[redis]
url = "redis://localhost:6379/13"

[plugin]
install_root = "plugins/root"
backup_root  = "plugins/backup"
temp_root    = "plugins/temp"
upload_root  = "plugins/uploads"

[[storage.instances]]
platform      = "local-1"
storage_type  = "local"
storage_path  = "./storage"
domain        = "http://localhost:8080/files/"
enable_access = true
path_patterns = ["/file/**"]
```

完整配置项说明参见 [CONFIG_MANUAL.md](config/CONFIG_MANUAL.md) 与 [ENV_MANUAL.md](config/ENV_MANUAL.md)。

### 3. 启动 web-server

```bash
export CONFIG_FILE=config.toml
export RUST_LOG=info,cmx_plugin=debug,cmx_auth=debug

# 调试启动
cargo run --bin web-server

# Release 启动
cargo run --release --bin web-server
```

后台运行：

```bash
nohup ./target/release/web-server > server.log 2>&1 &
```

### 4. Docker 部署

```bash
# 构建镜像（多架构）
./docker/scripts/build-docker.sh --image-name cmx-container --push

# 本地 compose 启动
docker-compose -f docker/docker-compose.local.yml up -d
```

### 5. 验证服务

```bash
# 健康检查
curl http://localhost:8080/api/health

# 交互式 API 文档
open http://localhost:8080/swagger
```

## 关键能力速览

### 1. WASM 插件（`cmx-plugin` + `cmx-runtime`）

- **三种来源**：`Local`（本地 ZIP）、`Remote`（URL 下载）、`Marketplace`（插件市场）
- **智能部署**：`GlobalPluginManager::deploy()` 自动判断安装/升级/覆盖安装
- **集群同步**：单写原则 + Redis Pub/Sub + 定时一致性校验（DB vs Registry vs 本地文件）
- **签名验证**：Ed25519 签名
- **插件市场**：发布、搜索、下载、评分统计

详见 [crates/libs/cmx-plugin/README.md](crates/libs/cmx-plugin/README.md)。

### 2. 服务编排（`cmx-service`）

```rust
use cmx_service::{Orchestrator, ServiceRegistry};
use cmx_core::model::service::ServiceOrchestration;

let orchestrator = Orchestrator::new(runtime_invoker, plugin_query, service_storage, config);
let result = orchestrator.execute(&orchestration, input).await?;
```

支持 start → func → func → end 线性流程、事务框、switch 多分支、SVRContext 上下文传递与调试模式。详见 [crates/libs/cmx-service/README.md](crates/libs/cmx-service/README.md)。

### 3. 统一认证（`cmx-auth`）

- JWT 双令牌（Access 30min / Refresh 7d）+ Refresh Rotation
- Argon2id 密码哈希（可调内存/时间/并行度）
- OAuth2 授权码 + PKCE，内置 Google / GitHub Provider
- API Key M2M 认证（两层缓存）
- Token 黑名单、登录失败锁定、Prometheus 指标

```rust
use cmx_auth::{AuthServiceImpl, AuthConfig};
use cmx_traits::auth::{AuthService, Credentials, DeviceInfo};

let auth = AuthServiceImpl::new(cache, config, user_query)?;
let tokens = auth.authenticate(
    Credentials::Password { username: "admin".into(), password: "...".into() },
    Some(DeviceInfo { device_type: "web".into(), device_id: "browser-001".into(),
                      ip: None, user_agent: None }),
).await?;
```

详见 [crates/libs/cmx-infra/cmx-auth/README.md](crates/libs/cmx-infra/cmx-auth/README.md)。

### 4. 统一授权（`cmx-iam`）

- 用户/角色/权限/角色组 CRUD
- 临时角色授权（带有效期 + 原因 + 来源）
- SoD 互斥规则（功能权限互斥 + 角色互斥）
- `IamChecker`（实现 `PermissionChecker`）：缓存 + 熔断（FailOpen/FailClose）
- 权限一致性校验（代码声明 vs DB）

`cmx-iam` 与 `cmx-auth` 通过 `cmx-biz` 注入的 `UserAuthQuery` / `PermissionChecker` trait 解耦。详见 [crates/libs/cmx-iam/README.md](crates/libs/cmx-iam/README.md)。

### 5. gRPC 通信（`cmx-rpc`）

- volo-grpc 客户端 + 服务端
- 桥接 `cmx-registry-config` 的服务发现缓存 → volo `Discover` trait
- 全链路 tracing，结构化日志
- `GlobalRpcClient` 全局单例

详见 [crates/libs/cmx-infra/cmx-rpc/README.md](crates/libs/cmx-infra/cmx-rpc/README.md)。

### 6. 注册中心 + 配置中心（`cmx-registry-config`）

- 两个独立 trait：`ServiceRegistry` / `ConfigCenter`
- 内置实现：`Mock`（开发/测试）+ `Nacos`（生产）
- 通过环境变量配置：`SERVICE_REGISTRY_TYPE`、`CONFIG_CENTER_TYPE`、`NACOS_*` 等

详见 [crates/libs/cmx-infra/cmx-registry-config/README.md](crates/libs/cmx-infra/cmx-registry-config/README.md)。

### 7. 审计日志（`cmx-audit`）

- 统一 `AuditLogger` trait + `DatabaseAuditStore`（PostgreSQL）
- `AuditRecord`：领域、对象、操作、结果、上下文、扩展字段
- `cmx-iam` / `cmx-auth` / `cmx-plugin` 均通过 trait 注入使用

### 8. 插件开发（`sdk/cmx-cli`）

```bash
# 创建新插件工程
cmx-cli new my-plugin

# 构建 WASM
cd my-plugin
cargo build --release --target wasm32-unknown-unknown

# 打包为 cmx 插件 ZIP
cmx-cli pack
```

插件包结构详见 [docs/插件目录说明.md](docs/插件目录说明.md)。

## VSCode 调试

`.vscode/` 目录已预置：

- **Debug Rust Program** — F5 启动调试会话（需 CodeLLDB 扩展）
- **cargo build web-server** / **cargo build web-server --release** — 构建任务
- 默认环境变量：`RUST_LOG=debug`、`RUST_BACKTRACE=1`

## 构建与发布

### 跨平台构建

```bash
# Linux musl 静态构建
cross build --release --target x86_64-unknown-linux-musl

# Windows
cross build --release --target x86_64-pc-windows-gnu
```

### Docker 多架构镜像

```bash
./docker/scripts/build-docker.sh --image-name cmx-container --push
```

## 常见问题

### 端口被占用

```bash
lsof -i :8080
fuser 8080/tcp
```

### 数据库/Redis 连接失败

检查 `config.toml` 中对应 URL，确保服务已启动。

### 插件加载失败

```bash
RUST_LOG=debug,cmx_plugin=trace cargo run --bin web-server 2>&1 | grep plugin
ls -la plugins/   # 检查插件目录权限
```

### 插件调用出现 `array had incorrect length, expected N`

宿主端 `SVRContext` / `FunctionInput` / `FunctionOutput` 结构变更时，**所有依赖 `cmx-plugin-sdk` 的 WASM 插件必须使用最新 SDK 重新编译**。

### `cmx_service_orchestrator.rs not found`

`volo-build` 生成的 gRPC 代码未产出。清理后重新构建：

```bash
rm -rf target/debug/build/cmx-rpc-gen-* && cargo build
```

## 开发规范

本项目遵循以下硬性约束（详见 [`.trae/rules/project_rules.md`](.trae/rules/project_rules.md)）：

1. **错误处理**：统一使用 `thiserror` 派生，禁止手写 `impl Display/Error`，禁止 `derive_more::From`。
2. **日志**：统一使用 `tracing`，禁止 `log` crate；结构化字段。
3. **依赖管理**：所有第三方依赖在 workspace `Cargo.toml` 集中定义，sub-crate 用 `workspace = true` 引用并加单行注释。
4. **数据库表名**：必须以 `cmx_` 前缀。
5. **文件上传**：使用 `multipart/form-data`，字段名为 `file`。
6. **CRUD create**：不手动设置 `id`，由 `GenericCrudService` 自动生成。
7. **插件来源**：`registry` 已重命名为 `marketplace`（向后兼容）；`url` 已重命名为 `remote`（向后兼容）。
8. **多租户**：12 张插件相关表必须带 `app_id` 字段。
9. **初始化函数**：必须返回 `Result<()>`，禁止 `panic!` / `expect` / `unwrap`。
10. **缩略图**：图片上传自动生成 `{filename}.min.jpg`，失败不阻塞主流程。
11. **本地存储路径**：使用 `yyyyMM` 格式目录。

## 文档导航

| 文档 | 说明 |
|------|------|
| [docs/插件目录说明.md](docs/插件目录说明.md) | 插件包结构与 manifest 字段详解 |
| [docs/cmx-docker-build-guide.md](docs/cmx-docker-build-guide.md) | Docker 镜像构建指南 |
| [docs/multi-instance-deployment-analysis.md](docs/multi-instance-deployment-analysis.md) | 多实例部署分析 |
| [docs/WASM 插件调用安全防护机制设计.md](docs/WASM%20插件调用安全防护机制设计.md) | WASM 安全防护设计 |
| [docs/cargo-workspace使用指南.md](docs/cargo-workspace使用指南.md) | Workspace 使用指南 |
| [config/CONFIG_MANUAL.md](config/CONFIG_MANUAL.md) | 配置文件手册 |
| [config/ENV_MANUAL.md](config/ENV_MANUAL.md) | 环境变量手册 |

各 crate 子模块文档：

| Crate | 文档 |
|-------|------|
| `cmx-plugin` | [crates/libs/cmx-plugin/README.md](crates/libs/cmx-plugin/README.md) · [cmx-plugin-api-reference.md](crates/libs/cmx-plugin/cmx-plugin-api-reference.md) |
| `cmx-plugin-sdk` | [crates/libs/cmx-plugin-sdk/README.md](crates/libs/cmx-plugin-sdk/README.md) |
| `cmx-service` | [crates/libs/cmx-service/README.md](crates/libs/cmx-service/README.md) |
| `cmx-iam` | [crates/libs/cmx-iam/README.md](crates/libs/cmx-iam/README.md) |
| `cmx-auth` | [crates/libs/cmx-infra/cmx-auth/README.md](crates/libs/cmx-infra/cmx-auth/README.md) |
| `cmx-rpc` | [crates/libs/cmx-infra/cmx-rpc/README.md](crates/libs/cmx-infra/cmx-rpc/README.md) |
| `cmx-registry-config` | [crates/libs/cmx-infra/cmx-registry-config/README.md](crates/libs/cmx-infra/cmx-registry-config/README.md) |
| `cmx-api` | [crates/libs/cmx-api/README.md](crates/libs/cmx-api/README.md) |
| `web-server` | [crates/web/web-server/README.md](crates/web/web-server/README.md) |

## 许可证

MIT
