# cmx-flow-sdk

> 流程微服务（cmx-flow-server，:8091）**REST 契约 SDK**：`/api/flow/v1` 契约的两端同源封装——路径常量 + wire DTO + `FlowClient` trait + HTTP 实现。跨服务起实例 / 办结任务 / 查待办的消费方（cmx-mdm 送审、cmx-report 关账编排等）依赖本 SDK，不再各自手拼 URL 与解析信封。

[![Version](https://img.shields.io/badge/version-0.1.12-blue.svg)]()
[![Edition](https://img.shields.io/badge/rust--edition-2024-orange.svg)]()
[![Authors](https://img.shields.io/badge/authors-skylake%40pansoft.com-lightgrey.svg)]()

## 快速开始

```toml
[dependencies]
cmx-flow-sdk = { workspace = true }
```

服务目录配置（消费方 toml；无 url 时走 Nacos 服务发现选例）：

```toml
[service_rpc.services]
flow = { url = "http://127.0.0.1:8091", timeout_ms = 10000 }
```

调用方代码：

```rust
use cmx_flow_sdk::{client, paths};

// 全局便捷形态（服务目录键 flow 已配置）
let flow = cmx_flow_sdk::client()?;                       // Arc<dyn FlowClient>
let inst = flow.start_instance(
    "mdm_cr_v1",                                          // 流程定义 key
    &variables_json,                                      // 表单/上下文变量（Value 进出）
    Some(&biz_link),                                      // 单据绑定坐标（bizTable/bizId）
).await?;

// 带委托用户（OBO：基座注入 X-Delegated-User-Token，flow 侧按真实办理人建待办）
let flow = cmx_flow_sdk::client_with_token(user_jwt)?;

// 路径常量自持拼装（自管 HTTP 的场景，与 SDK 同源）
let path = paths::biz_instances("cv_mdm_apply", cr_id);   // /api/flow/v1/biz/{table}/{id}/instances
```

`client()` 前置要求：`init_infra()` 已装配全局基座（各服务 main 正常启动即满足），且 `[service_rpc.services]` 配了 `flow` 键——未配置返回 `ServiceRpcError`（键未配置）。

## 覆盖端点（paths）

| 常量 / 函数 | 端点 |
|-----|------|
| `INSTANCES` | `POST/GET /api/flow/v1/instances`（起实例 / 实例列表） |
| `instance(id)` | `GET /api/flow/v1/instances/{id}` |
| `instance_variables(id)` | `GET …/variables` |
| `instance_comments(id)` | `GET …/comments`（审批意见） |
| `instance_biz(id)` | `GET …/biz`（绑定单据坐标） |
| `instance_cancel(id)` | `POST …/cancel` |
| `biz_instances(table, id)` | `GET /api/flow/v1/biz/{table}/{id}/instances`（倒序，第一条 = 当前实例） |
| `task_complete(task_id)` | `POST /api/flow/v1/tasks/{taskId}/complete` |
| `task_reject(task_id)` | `POST /api/flow/v1/tasks/{taskId}/reject` |
| `TASKS_MY` | `GET /api/flow/v1/tasks/my`（kind=todo\|claimable\|all） |

## DTO 约定

- **请求 DTO 严格类型化**（`StartInstanceReq` / `CompleteTaskReq` / `RejectTaskReq` / `CancelReq` / `BizLink`），camelCase 线格式；变量与上下文保持 `serde_json::Value` 进出（流程变量的自由度留给定义方）。
- **响应 DTO 稳定字段 + `#[serde(flatten)] extra`**：`InstanceView` / `BizInstanceSummary` / `TaskSummary` 等只声明消费方依赖的字段，flow 侧后续加字段**不破编译**（落进 `extra`）。
- **`flex_string` 兼容 id**：流程 id / 任务 id 的线格式在 string 与 number 间摇摆过，反序列化两者皆收。

## 错误语义

统一 `cmx_service_rpc::ServiceRpcError`：定位失败 / 连接失败 → `Unavailable`；超时 → `Timeout`；flow 侧 401/403 → `AuthRejected`；业务拒绝（信封 code != 0，如任务已被办结）→ `Remote`（`msg` 带 flow 侧业务消息）。韧性策略（熔断 / 幂等重试 / 打点）由基座统一执行，SDK 零策略代码。

## 设计约束

- **完全自包含**：仅依赖 `cmx-service-rpc`（默认 http feature）+ serde 家族，不依赖 flow 引擎 crate——消费方（mdm / report）编译图不引入流程引擎。
- **单一真源**：路径与 wire 契约以本 SDK 为准；flow 侧 handler 变更契约时同步改这里，两端一起编译发现漂移。
- 变量 / 审批意见等自由结构走 `Value`，不强行类型化（避免 SDK 长尾膨胀）。
