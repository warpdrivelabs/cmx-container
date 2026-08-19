# cmx-infra/

> 基础设施层 crate 分组：数据库 / 认证 / 缓存 / 审计 / 对象存储 / 注册配置 / RPC 共享设施等领域无关的底座能力集中地。

## 分组定位

本分组收纳**领域无关**的基础设施 crate：上承各业务域（dct / doc / mdm /
job 等），下接 PostgreSQL、Redis、S3、Nacos、volo-grpc 等外部系统。
分组内再分两档：`cmx-rowsource` 是被两个数据库门面共同依赖的近叶子基础
crate；`cmx-rpc` 是 RPC 基础设施核心库（纯共享设施层），具体域的 gRPC
皮肤不在本分组，见 `../cmx-rpcs/`。

## 子 crate 清单

| 子 crate | 职责 | README |
| --- | --- | --- |
| `cmx-rowsource` | 驱动无关的行来源抽象（`ZmcRowSource` trait + 中立列类型 `ZmcColType`）与零拷贝列式二进制（msgpack）编码器，是两个数据库门面共同依赖的近叶子基础 crate | [README](./cmx-rowsource/README.md) |
| `cmx-database` | 数据库操作模块（sqlx 抽象层），支持 WebAssembly 调用 host 实现数据库操作 | [README](./cmx-database/README.md) |
| `cmx-database-pg` | 基于 tokio-postgres + deadpool-postgres 的 PostgreSQL-only 门面：与 `cmx-database` 并行存在的独立实现，提供多数据源管理、声明式事务、零拷贝列式查询与通用 CRUD | [README](./cmx-database-pg/README.md) |
| `cmx-buffer` | 基于 Redis 实现的缓存和分布式锁管理模块，提供高效、安全、易用的缓存访问接口与分布式锁机制 | [README](./cmx-buffer/README.md) |
| `cmx-storage` | 统一对象存储抽象层：本地文件系统、S3 兼容存储等多平台（基于 OpenDAL），为上层提供一致的文件操作接口 | [README](./cmx-storage/README.md) |
| `cmx-audit` | 通用审计日志基础设施：领域无关的 `AuditRecord` + 记录 / 查询双层 trait（`AuditLogger` / `AuditStore`），内置 PostgreSQL 与内存两套存储实现 | [README](./cmx-audit/README.md) |
| `cmx-auth` | 企业级统一认证基础设施：JWT 双令牌、Refresh Token Rotation、Argon2id 密码哈希、OAuth2 授权码 + PKCE、会话管理、API Key 两层缓存、密钥轮换 | [README](./cmx-auth/README.md) |
| `cmx-registry-config` | 注册中心与配置中心可扩展抽象层，支持 Nacos、Mock 及后续扩展（Consul、Etcd、Apollo 等） | [README](./cmx-registry-config/README.md) |
| `cmx-rpc` | 基于 volo-grpc 的 RPC **基础设施核心库**（纯共享设施层）：服务发现桥接、重试、出 / 入站鉴权、客户端共享基础设施、Bundle 装配接口、gRPC Server 启动器 | [README](./cmx-rpc/README.md) |
| `cmx-nacos` | ⚠️ **已停用**（member 注释）：基于 nacos-sdk-rust 封装的 Nacos 服务注册 / 发现 + 配置中心集成库 | [README](./cmx-nacos/README.md) |

## 特殊状态

- **`cmx-nacos` 已停用**：根 `Cargo.toml` 的 `members` 与
  `workspace.dependencies` 中相关条目均已注释，未被编译，也无可用下游
  依赖方；活跃替代者为抽象层 crate `cmx-registry-config`。源码保留仅作
  参考，勿新增依赖。

## 相关背景

- 域 gRPC 皮肤（基于 `cmx-rpc` 设施）：`../cmx-rpcs/`。
- 域 HTTP 皮肤与共享骨架：`../cmx-apis/`。
