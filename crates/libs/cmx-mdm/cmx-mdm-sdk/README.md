# cmx-mdm-sdk

> 主数据微服务（cmx-mdm-server，:8095）**webhook 契约 SDK**：flow → mdm 生命周期回调的**两端同源**封装——事件 wire DTO、HMAC-SHA256 签名 / 验签（常量时间比较）、投递客户端。flow 侧（WebhookSender）与 mdm 侧（flow_cb 验签入口）共同依赖本 crate，杜绝两端签名口径漂移。

[![Version](https://img.shields.io/badge/version-0.1.12-blue.svg)]()
[![Edition](https://img.shields.io/badge/rust--edition-2024-orange.svg)]()
[![Authors](https://img.shields.io/badge/authors-skylake%40pansoft.com-lightgrey.svg)]()

## 快速开始

```toml
[dependencies]
cmx-mdm-sdk = { workspace = true }
```

flow 侧配置（服务目录键 + 共享密钥）：

```toml
[service_rpc.services]
mdm = { url = "http://127.0.0.1:8095", discovery = "cmx-mdm-server" }
```

发送方（flow，重试 / 退避策略归发送方）：

```rust
use cmx_mdm_sdk::{deliver_flow_event, FlowEvent};

// 便捷形态：全局基座 + 服务键 mdm，单次投递
deliver_flow_event(&event, &secret).await?;

// 或注入句柄（测试用）
let client = cmx_mdm_sdk::MdmFlowCallbackClient::new(rpc_handle);
client.deliver(&event, &secret).await?;
```

接收方（mdm，验签后反序列化）：

```rust
use cmx_mdm_sdk::{verify_signature, FlowEvent};

let body = axum_bytes;                                       // 原始请求体字节（不能重序列化）
if !verify_signature(&secret, &body, headers.get(SIGNATURE_HEADER)) {
    return unauthorized;                                     // 密钥未配 / 头缺失 / 前缀不符 / 比较不等
}
let event: FlowEvent = serde_json::from_slice(&body)?;
```

## 契约细节

- **端点**：`POST /api/mdm/flow/callback`（`paths::FLOW_CALLBACK`）。
- **三头协议**：
  - `x-cmx-flow-signature`（`SIGNATURE_HEADER`）：`sha256=<hex(HMAC-SHA256(body, secret))>`——签名对象是**实际发送的 raw body 字节**（`deliver` 内部序列化一次、对同一份字节签名与发送）。
  - `x-cmx-flow-event`（`EVENT_HEADER`）：载荷 `event` 字段的冗余副本，便于接收方路由 / 过滤。
  - `x-cmx-flow-delivery`（`DELIVERY_HEADER`）：`{instanceId}-{occurredAt}` 投递幂等键，接收方可据此去重。
- **验签**：`verify_signature` 常量时间比较（`verify_slice` + hex 解码），大小写不敏感；**密钥为空直接拒绝**（签名即凭证，无密钥等于裸奔）。
- **密钥分发**：`secret` 不进 SDK / 不进服务目录——由 flow 侧 webhook 目标配置与 mdm 侧 `[mdm.flow].webhook_secret` 各自持有，两端一致即可。

## 设计约束

- 完全自包含：仅依赖 `cmx-service-rpc`（默认 http feature）+ hmac / sha2 / hex / serde，不依赖 mdm 引擎 crate——flow 侧编译图不引入主数据域。
- `FlowEvent` 是 wire DTO（camelCase）：新增事件类型只加枚举值 / 字段，两端同编译即发现漂移。
- 单次投递不内置重试——重试 / 指数退避归发送方（flow 的 WebhookSender），SDK 保持纯契约。
