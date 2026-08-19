# cmx-rpcs/

> 域 gRPC 协议皮肤集中地：各域的 `cmx-{domain}-rpc` 薄 crate（client + server impl + Bundle 三件套）归集于此，与 HTTP 皮肤分组 `../cmx-apis/` 对称。

## 分组定位

本分组收纳平台所有**域 gRPC 皮肤** crate：每个域一个 thin crate，基于
volo-grpc 提供客户端访问器、服务端实现与装配 Bundle 三件套；业务实现经
`ServerDeps` 由组装层注入，皮肤层不写业务逻辑。
共享的 RPC 基础设施（服务发现桥接、重试、鉴权、Bundle 装配接口、
Server 启动器）**不在本分组**——集中在 `../cmx-infra/cmx-rpc/`。
命名遵循 `cmx-{domain}-rpc`，新增域 gRPC 皮肤应落入本分组。

## 子 crate 清单

| 子 crate | 职责 | README |
| --- | --- | --- |
| `cmx-orchestrator-rpc` | 服务编排域的 **gRPC 皮肤**（thin crate）：基于 volo-grpc 提供 `call_service` / `call_function` 的客户端访问器、服务端实现与装配 Bundle 三件套，业务实现经 `ServerDeps` 由组装层注入 | [README](./cmx-orchestrator-rpc/README.md) |
| `cmx-resource-rpc` | 资源数据管理域的 **gRPC 皮肤**（thin crate）：基于 volo-grpc 提供 `import_resource_data` / `cleanup_resource_data` / `list_resource_data` 的客户端访问器、服务端实现与装配 Bundle 三件套，承担插件 / 模块资源（menu / perm / form / flow 四类 ZIP 包）的跨服务导入 | [README](./cmx-resource-rpc/README.md) |

## 组织规则

- 皮肤 crate 只含 proto 生成物 + 薄封装 + Bundle，不承载业务实现。
- 跨服务调用的鉴权 / 重试 / 服务发现一律复用 `../cmx-infra/cmx-rpc/` 设施。
- 皮肤与设施分层：`cmx-rpcs/*`（域皮肤）与 `cmx-infra/cmx-rpc`（设施核心）
  分工写进各自 crate README，避免新增域时误将实现塞进设施库。

## 相关背景

- 共享 RPC 基础设施核心库：`../cmx-infra/cmx-rpc/`。
- HTTP 侧对称分组：`../cmx-apis/`（域 `cmx-*-api` 皮肤 + 共享骨架）。
