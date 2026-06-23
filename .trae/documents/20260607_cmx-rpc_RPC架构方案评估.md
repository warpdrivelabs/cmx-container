# RPC 架构方案（spec/implement-rpc-framework）评估报告

## 一、总体评价

方案整体设计**质量较高**，架构分层清晰，依赖隔离原则正确，缓存复用思路合理。以下从 5 个维度逐项评估，指出漏洞和需完善之处。

---

## 二、设计亮点（值得肯定）

1. **依赖隔离原则**：`cmx-rpc-gen` 隔离在 `cmx-rpc` 内部，上层只看到 `RpcClient` trait，更换 gRPC 框架影响面小。
2. **通用缓存层设计**：`ServiceInstanceCache` 与注册中心解耦，所有实现共享同一套缓存抽象，扩展新注册中心时缓存层和 RPC 层零改动。
3. **两个"服务"概念的区分**：明确区分 `service_name`（注册中心服务发现）和 `service_key`（编排标识），避免概念混淆。
4. **策略模式选型**：`RpcClient` trait + 工厂函数 + 配置驱动，扩展新协议业务方代码零改动。

---

## 三、发现的漏洞与需完善之处

### 漏洞 1：volo `Discover` trait 的动态更新机制缺失（严重）

**问题**：方案中 `RegistryAwareDiscover` 声称"实现 volo 的 `Discover` trait"，但 volo 的 `Discover` trait 是一个**同步查询接口**（`discover` 方法返回 `Vec<Instance>`），它本身**不支持 watch/推送**。

volO 的服务发现变更通知是通过 `Discover` trait 中的 `watch` 方法（或 `subscribe` 机制）实现的。方案中只写了 `impl Discover for RegistryAwareDiscover { // 实现 discover 方法 }`，**没有说明如何实现变更通知**。

**具体风险**：
- 如果 `Discover` 实现只返回快照，volo 的负载均衡器可能缓存旧实例列表，导致请求打到已下线的实例
- volo 的 `Discover` trait 的具体签名（是否包含 `watch`/`subscribe` 方法）需要确认，方案中完全没提到

**建议**：
- 查看 volo `Discover` trait 的完整定义（在 `volo` crate 的 `discovery` 模块），确认是否需要实现变更通知方法
- 如果 volo 的 `Discover` 支持返回 `watch::Watch` 类型的 channel，需要说明如何将 `ServiceInstanceCache` 的订阅回调桥接到该 channel
- 补充 `RegistryAwareDiscover` 的完整实现代码骨架，不能只写注释

---

### 漏洞 2：gRPC Server 端如何与本地 Orchestrator 集成未说明（严重）

**问题**：方案详细描述了客户端调用链路，但 **gRPC Server 端接收请求后如何执行编排**几乎没有设计。

现有 `ServiceInvokerImpl` 组合了 `RuntimeInvoker + PluginQuery + ServiceQuery` 来执行编排。gRPC Server 接收到 `ExecuteService` 请求后：
- 如何获取 `ServiceInvoker` 实例？
- 如何将 gRPC 请求转换为 `ServiceInvoker::invoke_service()` 的参数格式？
- 如何处理 `SVRContext`（gRPC 请求中没有 headers、trace_id 等上下文信息）？
- `call_function` 路径如何与现有的 `RuntimeInvoker` 对接？

**建议**：
- 补充 gRPC Server 端的详细设计，包括请求路由、上下文构建、错误映射
- 明确 gRPC Server 持有哪些依赖（`Arc<dyn ServiceInvoker>` 还是直接持有 `Orchestrator`）
- 补充 `SVRContext` 在 RPC 场景下的构建规则（headers 如何传递、trace_id 如何生成/传递）

---

### 漏洞 3：缺少连接管理和错误处理策略（中等）

**问题**：方案没有涉及以下生产环境必需的能力：

| 能力 | 现状 | 风险 |
|------|------|------|
| gRPC 连接池 | 未提及 | 每次调用创建新连接，性能灾难 |
| 超时控制 | 未提及 | 请求可能无限挂起 |
| 重试策略 | 未提及 | 网络抖动导致调用失败 |
| 熔断/降级 | 未提及 | 下游服务故障时拖垮上游 |
| gRPC 连接复用 | 未提及 | HTTP/2 多路复用需要正确配置 |

**建议**：
- 在 `RpcConfig` 中增加超时、重试、连接池配置
- 说明 volo-grpc 本身是否支持这些能力（volo 基于 tower middleware，应该可以通过 Layer 实现）
- 至少补充超时和重试的设计

---

### 漏洞 4：`ServiceRegistry` trait 扩展的向后兼容性（中等）

**问题**：方案在 `ServiceRegistry` trait 中新增了 `subscribe_instances` 和 `get_cached_instances` 两个方法。但：

1. **现有的 `NacosRegistry` 和 `MockRegistry` 必须同步实现**，否则编译失败。方案提到了这点，但没有给出 `MockRegistry` 的实现方案。
2. **`subscribe_instances` 方法签名需要 `&self` + 回调**，但 `ServiceInstanceCache` 需要在 `NacosRegistry::new()` 时就创建。这意味着**所有现有 Registry 的构造函数签名都需要修改**，影响面较大。
3. 工厂函数 `create_registry()` 也需要修改，需要接收或创建 `ServiceInstanceCache`。

**建议**：
- 补充 `MockRegistry` 的缓存实现（简单的内存 HashMap 即可）
- 补充 `create_registry()` 工厂函数的修改方案
- 考虑是否将 `ServiceInstanceCache` 作为工厂函数的共享参数（多个 Registry 共享同一 cache 实例）

---

### 漏洞 5：Nacos `subscribe` API 的兼容性风险（中等）

**问题**：方案中 `NacosRegistry` 使用 `naming.subscribe()` 注册 `NamingEventListener`。但：

1. 当前项目使用的 `nacos-sdk` 版本是 **0.8**（见 workspace Cargo.toml），需要确认 0.8 版本的 `NamingEventListener` trait 是否存在且签名一致。
2. `NamingChangeEvent` 的 `instances` 字段结构需要确认（方案中写 `event.instances`，但实际可能是 `event.service_instances` 或其他名称）。
3. Nacos SDK 0.8 的 `subscribe` 方法可能返回的是 `JoinHandle` 或需要特定的生命周期管理。

**建议**：
- 在实施前先用 `cargo doc --open -p nacos-sdk` 确认 API 签名
- 补充对 nacos-sdk 0.8 的 `subscribe` API 的实际调研结果

---

### 漏洞 6：多服务实例场景下的缓存预热（低）

**问题**：`ServiceInstanceCache` 采用懒加载（首次查询时拉取）。但在 RPC 调用场景下：

- 首次调用 `call_service("cmx-order-service", ...)` 时需要先拉取服务实例列表，增加首次调用延迟
- 如果 Nacos Server 此时不可用，首次调用直接失败，即使缓存中可能有其他可用实例

**建议**：
- 考虑在应用启动时提供可选的缓存预热机制（配置需要预热的 service_name 列表）
- 或在 `subscribe_instances` 被调用时就触发首次拉取（方案中已部分覆盖，但应明确为启动时行为）

---

### 漏洞 7：缺少监控与可观测性设计（低）

**问题**：方案没有涉及 RPC 调用的监控指标：

- 调用延迟（P50/P95/P99）
- 调用成功率/失败率
- 服务实例健康状态
- 负载均衡分布

**建议**：
- 至少在 `RpcClient` trait 层面预留 metrics 埋点位置
- 利用 `tracing` 记录关键调用链路的 span（项目已使用 tracing）

---

### 漏洞 8：`cmx-rpc-gen` 的构建流程未说明（低）

**问题**：
- `volo.yml` 的配置内容未给出
- `build.rs` 的构建脚本如何配置未说明
- protobuf 文件放在 `idl/` 顶级目录，但 `cmx-rpc-gen` crate 如何引用它（相对路径？）
- 是否需要安装 `protoc` 编译器？volo-build 是否自带 protoc？

**建议**：
- 补充 `volo.yml` 配置示例
- 补充 `build.rs` 的核心逻辑
- 说明 protobuf 文件的引用方式和构建依赖

---

## 四、方案中需要明确的设计决策

| # | 决策点 | 方案现状 | 建议 |
|---|--------|----------|------|
| 1 | gRPC Server 端口配置 | 未提及 | 应在 `RpcConfig` 中配置，与 HTTP 端口分离 |
| 2 | 是否同时启动 HTTP + gRPC | 未明确 | 应支持双协议共存，gRPC 用于服务间，HTTP 用于外部 |
| 3 | `GlobalRpcClient` 放在哪个 crate | 提到 cmx-traits 或 cmx-rpc | 建议放在 `cmx-rpc`（与 volo 依赖绑定），cmx-traits 只定义 trait |
| 4 | `HttpRestClient` 是否在本期实现 | 架构图中有但无详细设计 | 建议本期只实现 gRPC，HttpRest 作为占位即可 |
| 5 | 服务间调用的认证/鉴权 | 未提及 | 微服务间调用需要考虑 mTLS 或 token 验证 |

---

## 五、与现有代码的兼容性总结

| 现有组件 | 影响程度 | 说明 |
|----------|----------|------|
| `cmx-registry-config` | **修改** | trait 扩展 + 新增缓存模块，改动较大 |
| `cmx-traits` | **修改** | 新增 `RpcClient` trait + 相关类型 |
| `cmx-service` | **无修改** | gRPC Server 通过 trait 调用，不直接依赖 |
| `cmx-api` | **无修改** | HTTP API 不受影响 |
| `web-server` | **修改** | 集成 gRPC Server 启动 + RPC Client 初始化 |
| workspace Cargo.toml | **修改** | 新增 volo-grpc、volo-build 等依赖 |

---

## 六、总结

方案的核心架构思路正确，但在**实施细节层面有多处关键遗漏**，按优先级排序：

1. **P0（必须修复）**：volo `Discover` trait 的完整实现设计，特别是动态更新机制
2. **P0（必须修复）**：gRPC Server 端与本地 Orchestrator 的集成设计
3. **P1（强烈建议）**：连接管理、超时、重试策略
4. **P1（强烈建议）**：`ServiceRegistry` 扩展的向后兼容性方案
5. **P2（建议补充）**：nacos-sdk 0.8 API 兼容性确认、构建流程、监控埋点
