# cmx-apis/

> 域 HTTP 协议皮肤集中地：各业务域的 `cmx-*-api` 薄 crate 统一归集于此，与 gRPC 皮肤分组 `../cmx-rpcs/` 对称。

## 分组定位

本分组集中收纳平台所有**域 HTTP 层** crate。每个业务域一个薄皮肤 crate，
只做参数提取、响应封装（`ApiResp` / msgpack 信封）与路由装配，业务实现一律
委托对应域的 Service / Store crate，皮肤层不写业务逻辑。

组织上分三类：`cmx-api-core` 提供共享骨架（应用状态、`ModuleRoutes` 路由契约、
通用 CRUD 与中间件）；`cmx-api-types` 收敛通用 HTTP 类型；`cmx-common-api`
作为装配中枢 re-export 骨架并聚合路由。其余为各域皮肤，命名遵循 `cmx-{domain}-api`。

## 子 crate 清单

| 子 crate | 职责 | README |
| --- | --- | --- |
| `cmx-api-core` | Web API 共享骨架：`CmxAppState` 应用状态、`ModuleRoutes` 路由契约、通用 CRUD handler、CRUD 声明宏与全套 HTTP 中间件 | [README](./cmx-api-core/README.md) |
| `cmx-api-types` | 通用 HTTP 类型库：统一 REST 响应格式、错误处理、OpenAPI 文档参数与树形结构 | [README](./cmx-api-types/README.md) |
| `cmx-common-api` | 通用 API 层与装配中枢（原 `cmx-api` 重命名）：re-export 共享骨架，保留 service / debug / portal / dev 四组 handler 与路由聚合、OpenAPI 文档 | [README](./cmx-common-api/README.md) |
| `cmx-ai-api` | AI 生成能力中继模块的 HTTP 皮肤（一期薄代理）：会话 / 消息 / 询问 / 审批端点转发 OpenCode 服务，SSE 事件流按 sessionID 分发 | [README](./cmx-ai-api/README.md) |
| `cmx-biz-api` | 业务基础模型域（domain / application / module / menu / sys_datasource / form）的 HTTP 皮肤：写操作手写委托 cmx-biz Service（带 DAM 资产钩子），读操作复用通用 CRUD | [README](./cmx-biz-api/README.md) |
| `cmx-iam-api` | IAM（用户 / 角色 / 角色组 / 权限 / 互斥规则）与认证（登录 / OAuth2 / API Key）域的 HTTP 皮肤：薄 handler 委托 cmx-iam / cmx-auth 服务 | [README](./cmx-iam-api/README.md) |
| `cmx-plugin-api` | 插件域的 HTTP 皮肤：插件本地运行时（安装 / 部署 / 升降级）、插件市场、模块迁移包导入导出与表元数据查询，委托 cmx-plugin 服务 | [README](./cmx-plugin-api/README.md) |
| `cmx-storage-api` | 文件存储域的 HTTP 皮肤（纯路由胶水）：把 cmx-storage::handler 的 13 个 HTTP 函数装配成 axum Router，本 crate 不写任何 handler | [README](./cmx-storage-api/README.md) |

## 相关背景

- gRPC 侧对称分组：`../cmx-rpcs/`（域 gRPC 皮肤），共享设施在 `../cmx-infra/cmx-service-rpc/`（src/grpc/）。
- 域三件套（api + model + store-pg）分组：`../cmx-dct/`、`../cmx-doc/`、`../cmx-mdm/`，任务中心见 `../cmx-job/`。
- 反代薄壳分组（引擎已外迁独立 workspace）：`../cmx-flow/`、`../cmx-rpt/`、`../cmx-rule/`。
