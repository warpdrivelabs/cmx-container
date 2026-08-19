# cmx-container

> CMX 平台「后端公用库 + 插件平台」Rust Cargo Workspace：WebAssembly 插件运行时、可视化服务编排、统一认证授权、字典/单据/主数据元数据数据服务，以及门户微服务的全部领域库。本仓库**不含可执行 server bin**——主应用在下游 [cmx-portalservice](../cmx-portalservice)（`cmx-portal-server`，:8080），经 path 依赖跨 workspace 引用本仓库的公用库。

[![Version](https://img.shields.io/badge/version-0.1.12-blue.svg)]()
[![Edition](https://img.shields.io/badge/rust--edition-2024-orange.svg)]()
[![Workspace](https://img.shields.io/badge/cargo-workspace-lightgrey.svg)]()
[![License](https://img.shields.io/badge/license-Apache--2.0-green.svg)]()

---

## 项目简介

`cmx-container` 是 CMX 企业级平台的**后端公用库与插件平台**，以 Cargo Workspace 组织，当前含 **65 个活跃成员 crate**（另有 `cmx-wasmdemo`、`cmx-nacos` 两个已停用 crate 源码保留、member 注释）。定位演进历程：早期是单体「插件化容器运行时」（含 web-server bin），后经架构重构——**server bin 迁至下游 `cmx-portalservice`（薄 bin + 业务层），流程引擎迁 `cmx-flowengine`、报表迁 `cmx-report`、规则引擎迁 `cmx-rulesengine`（各自独立 workspace），本仓库沉淀为被各微服务跨 workspace 共享的公用库 + 插件平台**。

核心能力：

- **WASM 插件平台**：Extism 运行时（`cmx-runtime`）+ 插件管理/市场/集群同步（`cmx-plugin`）+ 插件 SDK 与示范（`cmx-plugin-sdk` / `cmx-plugin-demo`）
- **可视化服务编排**：声明式 JSON 流程 + 事务框 + 分支路由（`cmx-service`，gRPC 皮肤在 `cmx-rpcs/cmx-orchestrator-rpc`）
- **元数据数据服务**：数据字典 DCT / 业务单据 DOC / 主数据 MDM 三域「api + model + store-pg」三件套，定义 JSON 驱动的强类型装载/回存/铸号
- **模型中心**：表定义 → DDL 生成 → 数据库初始化 → 模块部署台账（`cmx-model/*`、`cmx-metadata`）
- **统一认证授权**：JWT 双令牌 + Refresh Rotation + OAuth2/PKCE（`cmx-auth`）；RBAC/角色组/SoD 互斥 + 缓存熔断（`cmx-iam`）
- **通用业务编码引擎**：单据号/流水号规则铸号（`cmx-code/*`）
- **定时任务中心**：常驻消费者作业、集群可重入（`cmx-job/*`）
- **基础设施**：sqlx 与 tokio-postgres 双数据库链路、Redis 缓存与分布式锁、S3/本地对象存储、volo gRPC、Nacos 注册/配置中心抽象、统一审计

## 整体架构

```text
                        下游微服务 workspace（各自独立仓库、薄 bin）
    ┌───────────────────────────┬────────────────────┬────────────────────┐
    │ cmx-portalservice         │ cmx-flowengine     │ cmx-report /       │
    │ cmx-portal-server :8080   │ 流程微服务          │ cmx-rulesengine    │
    └─────────────┬─────────────┴─────────┬──────────┴─────────┬──────────┘
                  │  path 依赖（跨 workspace 引用本仓库 crates）
    ══════════════╪═══════════════════════╪════════════════════╪══════════
                  ▼
    ┌──────────────────────────────────────────────────────────────────┐
    │ 装配层：cmx-platform-app（聚合路由/配置） · cmx-service-base        │
    │         cmx-web-chassis（HTTP 底盘） · cmx-web-monitor（监控）      │
    ├──────────────────────────────────────────────────────────────────┤
    │ 协议皮肤：cmx-apis/（HTTP：api-core + 各域 *-api）                 │
    │           cmx-rpcs/（gRPC：orchestrator-rpc · resource-rpc）       │
    │           反代薄壳：flow-api / rpt-api / rule-api（proxy-only）    │
    ├──────────────────────────────────────────────────────────────────┤
    │ 域层：cmx-biz · cmx-iam · cmx-plugin · cmx-ai · cmx-form          │
    │       cmx-doc/* · cmx-dct/* · cmx-mdm/* · cmx-job/*               │
    │       cmx-model/* · cmx-code/* · cmx-master-slave · cmx-metadata  │
    ├──────────────────────────────────────────────────────────────────┤
    │ 运行时/服务：cmx-runtime（Extism WASM） · cmx-service（编排）      │
    │              cmx-debug · cmx-jsonstore                            │
    ├──────────────────────────────────────────────────────────────────┤
    │ 基础设施 cmx-infra/：cmx-database(sqlx) · cmx-database-pg          │
    │   cmx-rowsource · cmx-buffer(Redis) · cmx-storage(opendal)        │
    │   cmx-rpc(volo gRPC) · cmx-registry-config(Nacos/Mock)            │
    │   cmx-auth(认证) · cmx-audit(审计) · cmx-nacos(已停用)             │
    ├──────────────────────────────────────────────────────────────────┤
    │ 基础层：cmx-core · cmx-traits · cmx-utils · cmx-macros · modql    │
    └──────────────────────────────────────────────────────────────────┘
```

### 关键设计原则

- **薄 bin + 公用库下沉**：各微服务 bin 只做启动装配，业务与基建全部经 `path = "../cmx-container/crates/..."` 复用本仓库。
- **协议皮肤与领域分离**（AGENTS.md 第八章）：HTTP handler 集中在 `cmx-apis/*-api` 薄皮肤 crate（参数提取/响应封装），gRPC 皮肤集中在 `cmx-rpcs/*-rpc`；领域逻辑留在域 crate。
- **独立微服务反代**：流程/报表/规则三个引擎已在独立 workspace，本仓库仅留 proxy-only 薄壳（`cmx-flow-api` 等），门户编译期不触碰引擎源码。
- **Trait 解耦**：`cmx-auth` 不直接依赖 `cmx-iam`，经 `cmx-traits` 的 `UserAuthQuery` / `PermissionChecker` 注入。
- **无状态集群约束**：进程无状态、会话与缓存外置（Redis/DB）、定时任务 `SELECT ... FOR UPDATE SKIP LOCKED` 可重入、插件集群同步走 Redis Pub/Sub 单写原则。
- **依赖集中管理**：第三方依赖在 workspace `Cargo.toml` 统一定义，成员 crate 以 `workspace = true` 引用。

## 目录结构

```text
cmx-container/
├── AGENTS.md                    # 开发规范（18 章，权威）
├── Cargo.toml                   # Workspace 配置（version 0.1.12 · 65 成员）
├── crates/
│   ├── libs/                    # 全部公用库 crate（见下节导航）
│   ├── tests/cmx-database-test/ # 数据库层基准/E2E 测试 crate
│   └── web/web-folder/          # 门户前端静态资源（历史遗留位置）
├── sdk/cmx-cli/                 # 插件开发 CLI：脚手架 + api.json 生成
├── data/                        # 平台数据蓝本：meta/ menu-pages/ html-pages/
│                                #   native-pages/ form-pages/ dictbak/ factbak/
│                                #   flow/ help/ service-catalog/ modules/ 等
├── config/                      # 配置模板 + CONFIG_MANUAL.md + ENV_MANUAL.md
├── docs/                        # 设计文档与方案（含 sql/migrations）
├── docker/                      # Dockerfile / compose / k8s 部署
├── e2e_tests/                   # Python pytest 端到端测试（认证/IAM）
├── example/                     # 参考实现（cmxold/javaoauth2/redis-rs/rust-modql）
├── plugins/                     # 插件运行目录（root/uploads）
├── bash/                        # 运维脚本（appctl/deploy/update-webserver）
├── dev.toml                     # 开发配置蓝本（本仓库直读；下游 portalservice 亦以此为蓝本）
├── dev-vpn.toml                 # VPN 环境配置
├── Cross.toml                   # 跨平台构建配置
└── wasmtime_config.toml         # WASM 运行时配置
```

## Workspace 导航（每 crate 均有 README）

### 基础层

| Crate | 职责 |
|-------|------|
| [cmx-core](crates/libs/cmx-core/README.md) | 核心类型：SVRContext、DataValue/Cell、表定义、模型数据集 |
| [cmx-traits](crates/libs/cmx-traits/README.md) | 跨模块 trait 契约：auth/runtime/plugin/event_bus/rpc |
| [cmx-utils](crates/libs/cmx-utils/README.md) | 通用工具：配置加载、加密、雪花 ID、zip、时间 |
| [cmx-macros](crates/libs/cmx-macros/README.md) | 过程宏：handler 鉴权/上下文注入、inventory 注册 |
| [modql](crates/libs/modql/README.md) · [modql-macros](crates/libs/modql/modql-macros/README.md) | Model Query Language（Filter/ListOptions → sea-query，内部 fork） |

### 基础设施层（cmx-infra/）

| Crate | 职责 |
|-------|------|
| [cmx-database](crates/libs/cmx-infra/cmx-database/README.md) | sqlx 数据库层（PostgreSQL/MySQL/SQLite）+ 事务上下文 |
| [cmx-database-pg](crates/libs/cmx-infra/cmx-database-pg/README.md) | tokio-postgres + deadpool 的 PG-only 并行链路（列式 msgpack） |
| [cmx-rowsource](crates/libs/cmx-infra/cmx-rowsource/README.md) | 驱动无关行来源抽象 + 零拷贝列式编码 |
| [cmx-buffer](crates/libs/cmx-infra/cmx-buffer/README.md) | Redis 缓存 + moka 本地缓存 + 分布式锁 |
| [cmx-storage](crates/libs/cmx-infra/cmx-storage/README.md) | opendal 本地/S3 对象存储 + 秒传 + 缩略图 |
| [cmx-rpc](crates/libs/cmx-infra/cmx-rpc/README.md) | volo gRPC 客户端/服务端设施 + 服务发现 + 重试 |
| [cmx-registry-config](crates/libs/cmx-infra/cmx-registry-config/README.md) | 注册中心/配置中心抽象（Nacos/Mock） |
| [cmx-auth](crates/libs/cmx-infra/cmx-auth/README.md) | 统一认证：JWT 双令牌、OAuth2+PKCE、API Key |
| [cmx-audit](crates/libs/cmx-infra/cmx-audit/README.md) | 统一审计：AuditLogger trait + PG/内存实现 |
| [cmx-nacos](crates/libs/cmx-infra/cmx-nacos/README.md) | nacos-sdk 集成（**已停用**，由 cmx-registry-config 替代） |

### 运行时与服务

| Crate | 职责 |
|-------|------|
| [cmx-runtime](crates/libs/cmx-runtime/README.md) | Extism WASM 运行时引擎与配置 |
| [cmx-service](crates/libs/cmx-service/README.md) | 服务注册 + Orchestrator 编排（事务框/分支/调试） |
| [cmx-debug](crates/libs/cmx-debug/README.md) | 插件调试会话管理（code-server 联动） |
| [cmx-jsonstore](crates/libs/cmx-jsonstore/README.md) | JSON 文件存储基础设施（原 cmx-portal-base 下沉） |

### 域层（单 crate）

| Crate | 职责 |
|-------|------|
| [cmx-biz](crates/libs/cmx-biz/README.md) | 平台业务实体（Domain/Application/Module/Menu/Datasource）+ 协议无关执行核心 |
| [cmx-iam](crates/libs/cmx-iam/README.md) | 用户/角色/权限/角色组/SoD 规则 + IamChecker |
| [cmx-plugin](crates/libs/cmx-plugin/README.md) | 插件全生命周期、市场、集群同步、签名验证 |
| [cmx-plugin-sdk](crates/libs/cmx-plugin-sdk/README.md) · [cmx-plugin-demo](crates/libs/cmx-plugin-demo/README.md) | 插件 SDK（cdylib+rlib）与官方示范 |
| [cmx-ai](crates/libs/cmx-ai/README.md) | OpenCode AI 中继：SSE 会话/消息/审批 |
| [cmx-form](crates/libs/cmx-form/README.md) | form/html/native 三类页面资源 JSON 存储 |
| [cmx-master-slave](crates/libs/cmx-master-slave/README.md) | 主从结构引擎：schema 路径树/树装配/汇总上卷/变更集 |
| [cmx-metadata](crates/libs/cmx-metadata/README.md) | 表定义 JSON → DDL 生成/迁移/seed 执行器 |

### 域三件套（api + model + store-pg）

| 域 | api（HTTP 皮肤） | model（领域） | store-pg（持久化） |
|----|------------------|---------------|--------------------|
| 单据 DOC | [cmx-doc-api](crates/libs/cmx-doc/cmx-doc-api/README.md) | [cmx-doc-model](crates/libs/cmx-doc/cmx-doc-model/README.md) | [cmx-doc-store-pg](crates/libs/cmx-doc/cmx-doc-store-pg/README.md) |
| 字典 DCT | [cmx-dct-api](crates/libs/cmx-dct/cmx-dct-api/README.md) | [cmx-dct-model](crates/libs/cmx-dct/cmx-dct-model/README.md) | [cmx-dct-store-pg](crates/libs/cmx-dct/cmx-dct-store-pg/README.md) |
| 主数据 MDM | [cmx-mdm-api](crates/libs/cmx-mdm/cmx-mdm-api/README.md) | [cmx-mdm-model](crates/libs/cmx-mdm/cmx-mdm-model/README.md) | [cmx-mdm-store-pg](crates/libs/cmx-mdm/cmx-mdm-store-pg/README.md) |
| 任务 JOB | [cmx-job-api](crates/libs/cmx-job/cmx-job-api/README.md) | [cmx-job-core](crates/libs/cmx-job/cmx-job-core/README.md) | [cmx-job-store-pg](crates/libs/cmx-job/cmx-job-store-pg/README.md) |
| 模型 MODEL | [cmx-model-api](crates/libs/cmx-model/cmx-model-api/README.md) | [cmx-model-meta](crates/libs/cmx-model/cmx-model-meta/README.md) | [cmx-model-deploy](crates/libs/cmx-model/cmx-model-deploy/README.md)（部署） |
| 编码 CODE | [cmx-code-api](crates/libs/cmx-code/cmx-code-api/README.md) | [cmx-code-model](crates/libs/cmx-code/cmx-code-model/README.md) | —（复用通用 store） |

### 协议皮肤与装配

| Crate | 职责 |
|-------|------|
| [cmx-api-core](crates/libs/cmx-apis/cmx-api-core/README.md) | 全部 `cmx-*-api` 共享基建：CmxAppState、ModuleRoutes、CRUD 宏、中间件 |
| [cmx-api-types](crates/libs/cmx-apis/cmx-api-types/README.md) | ApiResp/Pagination/ErrCode 等通用 API 类型 |
| [cmx-common-api](crates/libs/cmx-apis/cmx-common-api/README.md) | 平台公共 HTTP 端点（health/portal/data/fact/help/debug 等） |
| [cmx-biz-api](crates/libs/cmx-apis/cmx-biz-api/README.md) · [cmx-iam-api](crates/libs/cmx-apis/cmx-iam-api/README.md) · [cmx-plugin-api](crates/libs/cmx-apis/cmx-plugin-api/README.md) · [cmx-ai-api](crates/libs/cmx-apis/cmx-ai-api/README.md) · [cmx-storage-api](crates/libs/cmx-apis/cmx-storage-api/README.md) | 各域 HTTP 皮肤 |
| [cmx-orchestrator-rpc](crates/libs/cmx-rpcs/cmx-orchestrator-rpc/README.md) · [cmx-resource-rpc](crates/libs/cmx-rpcs/cmx-resource-rpc/README.md) | gRPC 皮肤（编排调用 / 资源包跨服务导入） |
| [cmx-rpc-gen](crates/libs/cmx-rpc-gen/README.md) | volo-build 编译期 gRPC 代码生成 |
| [cmx-flow-api](crates/libs/cmx-flow/cmx-flow-api/README.md) · [cmx-rpt-api](crates/libs/cmx-rpt/cmx-rpt-api/README.md) · [cmx-rule-api](crates/libs/cmx-rule/cmx-rule-api/README.md) | 反代薄壳 → 独立微服务（proxy-only） |
| [cmx-platform-app](crates/libs/cmx-platform-app/README.md) | 平台总装配：聚合全域路由 + 有序初始化 |
| [cmx-service-base](crates/libs/cmx-service-base/README.md) | 微服务起服基础设施（feature 门控 init_* 原语） |
| [cmx-web-chassis](crates/libs/cmx-web-chassis/README.md) · [cmx-web-monitor](crates/libs/cmx-web-monitor/README.md) | HTTP 服务底盘 / 技术监控 |
| [cmx-database-test](crates/tests/cmx-database-test/README.md) · [cmx-cli](sdk/cmx-cli/README.md) | 数据库基准测试 / 插件开发 CLI |

> 已停用保留源码：[cmx-wasmdemo](crates/libs/cmx-wasmdemo/README.md)（示范已由 cmx-plugin-demo 接替）。

## 快速开始

### 环境要求

- **Rust**（见 [rust-toolchain.toml](rust-toolchain.toml)，Edition 2024）
- **PostgreSQL 12+**（必需）、**Redis 6+**（缓存/锁/集群同步）
- Nacos 2.x（可选：服务注册与远程配置）

### 1. 构建与检查

本仓库无可执行 bin，日常验证一律用 check（禁止 `cargo build` 做编译检查）：

```bash
cargo check              # 全 workspace 快速类型/借用检查
cargo clippy             # 静态质量检查
cargo test -p cmx-core   # 指定 crate 跑测试
```

> `cmx-rpc-gen` 使用 volo-build 在编译期生成 gRPC 代码。若遇 `cmx_service_orchestrator.rs not found`，清理构建缓存后重试：
> `rm -rf target/debug/build/cmx-rpc-gen-* && cargo check`

### 2. 启动主服务（在下游仓库）

主应用 `cmx-portal-server` 在 **cmx-portalservice** 仓库，读取本仓库的 `dev.toml` 为配置蓝本：

```bash
cd ../cmx-portalservice
./portal.sh               # 开发模式，等价 cargo run -p cmx-portal-server
# 启动后监听 0.0.0.0:8080，访问 http://127.0.0.1:8080/portal/
```

流程（:8091+）、报表、规则引擎分别见 `../cmx-flowengine`、`../cmx-report`、`../cmx-rulesengine` 仓库各自 README。

### 3. 配置

- 配置入口：环境变量 `CONFIG_FILE` 指定 toml（本仓库开发默认 `./dev.toml`）。
- `[[databases]]` 中 `default = true` 为**平台库**（菜单/治理表），`source_type = "biz"` 为**业务库**。
- 完整字段说明：[config/CONFIG_MANUAL.md](config/CONFIG_MANUAL.md) 与 [config/ENV_MANUAL.md](config/ENV_MANUAL.md)。

### 4. Docker 部署

```bash
./docker/scripts/build-docker.sh --image-name cmx-container --push   # 多架构镜像
docker-compose -f docker/docker-compose.local.yml up -d              # 本地 compose
```

跨平台静态构建：`cross build --release --target x86_64-unknown-linux-musl`（配置见 [Cross.toml](Cross.toml)）。

## 开发规范（摘要）

完整规范见 [AGENTS.md](AGENTS.md)（18 章）。硬性约束摘要：

1. **错误处理**：统一 `thiserror` 派生，禁止手写 `impl Display/Error`、禁止裸 `unwrap()`。
2. **日志**：统一 `tracing`，禁止 `log` crate。
3. **依赖管理**：第三方依赖在 workspace `Cargo.toml` 集中定义，成员以 `workspace = true` 引用并加单行注释。
4. **数据库表名**：必须 `cmx_` 前缀；SQL 迁移与 `init_ddl.sql` 维护规则见 AGENTS.md 第五章。
5. **新接口规范**：禁用 Path Variable（`/api/foo/{id}`）与 `PUT/PATCH/DELETE`；列表/写操作一律 `POST + JSON body`，仅"取一条详情"可用 `GET + query`（只对新增接口生效）。
6. **cmx-\*-api 皮肤边界**：handler 只做协议适配，业务逻辑在域 crate（第八章）。
7. **Service 列表/分页契约**：见 AGENTS.md 第七章。
8. **集群无状态**：禁止进程内存业务状态、本地磁盘持久化；定时任务可重入；详见各下游部署约束。

## 常见问题

### Q: 仓库里为什么没有可执行程序？

架构重构后 server bin 迁至 `cmx-portalservice`（薄 bin + 业务层），本仓库专注公用库与插件平台；`.vscode/` 下遗留的 web-server 任务/调试配置已不适用，忽略即可。

### Q: `cmx-nacos` / `cmx-wasmdemo` 还能用吗？

两者已从 workspace members 注释（源码保留）。Nacos 能力由 `cmx-registry-config` 抽象层承载；插件示范由 `cmx-plugin-demo` 接替。

### Q: 插件调用出现 `array had incorrect length, expected N`

宿主端 `SVRContext` / `FunctionInput` / `FunctionOutput` 结构变更后，所有依赖 `cmx-plugin-sdk` 的 WASM 插件必须用最新 SDK 重新编译。

### Q: 如何给平台新增菜单/页面/字典数据？

数据真源在本仓库 `data/`（menu-pages / html-pages / native-pages / meta 等），经模型中心 deploy 或同步脚本落地数据库；详见 `data/20260716_data_目录结构与内容总结.md`。

## 文档导航

| 文档 | 说明 |
|------|------|
| [AGENTS.md](AGENTS.md) | 开发规范（18 章，权威） |
| [config/CONFIG_MANUAL.md](config/CONFIG_MANUAL.md) · [ENV_MANUAL.md](config/ENV_MANUAL.md) | 配置/环境变量手册 |
| [docs/插件目录说明.md](docs/插件目录说明.md) | 插件包结构与 manifest 字段 |
| [docs/sql/](docs/sql/) | SQL 迁移与 DDL |
| [docs/cmx-docker-build-guide.md](docs/cmx-docker-build-guide.md) | Docker 镜像构建 |
| [docs/multi-instance-deployment-analysis.md](docs/multi-instance-deployment-analysis.md) | 多实例/集群部署分析 |
| [data/20260716_data_目录结构与内容总结.md](data/20260716_data_目录结构与内容总结.md) | 平台数据蓝本目录说明 |
| [crates/libs/](crates/libs/) | 各 crate README（见上文导航表） |

## 许可证 / 贡献 / 安全

- 许可证：[Apache-2.0](LICENSE)（modql 为 `MIT OR Apache-2.0`，见其 Cargo.toml）。
- 贡献指南：[CONTRIBUTING.md](CONTRIBUTING.md)。
- 安全漏洞请**勿开公开 Issue**：[SECURITY.md](SECURITY.md)。
- 配置凭据只走本地未跟踪文件（复制 `config/config_template.toml` 填真值）；仓库内无真实凭据。
