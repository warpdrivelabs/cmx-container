# 微服务间 RESTful 接口调用方案评估

> 日期：2026-06-12 | 模块：cmx-rpc, cmx-api | 状态：方案评估中

***

## 一、背景与现状

### 1.1 当前架构

```
┌─────────────┐     gRPC (volo)     ┌─────────────┐
│  服务实例 A  │ ◄──────────────────► │  服务实例 B  │
│             │                      │             │
│  axum HTTP  │                      │  axum HTTP  │
│  :8080      │                      │  :8080      │
│             │                      │             │
│  gRPC volo  │                      │  gRPC volo  │
│  :50051     │                      │  :50051     │
└─────────────┘                      └─────────────┘
```

* **gRPC 层**：基于 volo 框架，已有完整的服务注册/发现（Nacos）、负载均衡、重试机制

* **HTTP 层**：基于 axum，提供 30+ RESTful API 端点，当前 **无任何认证机制**

* **RPC 通信**：`cmx-rpc` 仅实现了 `CmxServiceOrchestrator` 的两个方法（`execute_service` 和 `call_function`）

* **预留扩展**：`factory.rs` 中已预留 `"http_rest"` 协议分支但未实现

### 1.2 问题

目前 axum 开发的 RESTful 接口（如 domain、application、module、datasource、table-metadata、plugin、storage 等）在多实例部署时，无法被其他服务实例直接调用。需要一种方案实现微服务间对这些 RESTful 接口的调用。

### 1.3 涉及的 RESTful API 模块

| 模块            | 路由前缀                  | 典型用途        |
| ------------- | --------------------- | ----------- |
| Domain        | `/api/domains`        | 域/租户管理 CRUD |
| Application   | `/api/applications`   | 应用管理 CRUD   |
| Module        | `/api/module`         | 模块管理 CRUD   |
| SysDatasource | `/api/sys-datasource` | 数据源管理       |
| TableMetadata | `/api/table-metadata` | 表元数据查询      |
| Plugin        | `/api/plugin`         | 插件部署/安装/卸载  |
| Service       | `/api/service`        | 服务编排/调用     |
| Storage       | `/api/storage`        | 文件上传/下载     |
| Marketplace   | `/api/marketplace`    | 插件市场        |

***

## 二、三种方案对比

### 方案 A：改造为 gRPC 调用

为需要跨服务调用的 RESTful 接口新增对应的 gRPC 服务定义和实现。

#### 架构图

```
┌─────────────┐     gRPC (volo)     ┌─────────────┐
│  服务实例 A  │ ◄──────────────────► │  服务实例 B  │
│             │                      │             │
│  axum HTTP  │                      │  axum HTTP  │
│  :8080      │                      │  :8080      │
│             │                      │             │
│  gRPC volo  │  新增多个 gRPC 服务   │  gRPC volo  │
│  :50051     │ ◄──────────────────► │  :50051     │
└─────────────┘                      └─────────────┘
```

#### 实现要点

1. **新增 Proto 定义**：在 `cmx-rpc-gen/idl/` 下新增 proto 文件，定义各业务模块的 gRPC 服务
2. **实现 gRPC Server**：每个业务模块实现对应的 volo gRPC Service trait，内部复用现有业务逻辑
3. **扩展 RPC 客户端**：在 `RpcClient` trait 中新增各业务模块的调用方法
4. **复用现有基础设施**：服务发现、负载均衡、重试机制全部复用 `cmx-rpc` 现有能力

#### 优点

* **性能最优**：Protobuf 二进制序列化，比 JSON 更紧凑高效

* **类型安全**：编译期类型检查，IDL 即契约

* **复用成熟基础设施**：服务发现、负载均衡、重试、全局客户端管理全部现成

* **统一通信协议**：服务间通信统一走 gRPC，架构一致性强

* **代码生成**：volo-build 自动生成客户端/服务端代码，减少手写

* **双向流支持**：未来可扩展流式调用

#### 缺点

* **开发成本高**：每个业务模块需要定义 proto、实现 server、扩展 client trait

* **维护双份接口**：RESTful（面向前端）+ gRPC（面向内部）两套 API 需同步维护

* **Proto 学习成本**：团队需要熟悉 Protobuf IDL

* **调试不便**：gRPC 比 HTTP REST 更难用 curl/浏览器调试

* **axum 生态割裂**：业务逻辑需要同时支持 axum handler 和 volo service 两种调用方式

***

### 方案 B：改造 HTTP 内部调用（推荐）

基于现有 axum 接口，实现内部 HTTP 客户端框架，通过服务发现实现免认证的内部调用。

#### 架构图

```
┌─────────────┐                      ┌─────────────┐
│  服务实例 A  │                      │  服务实例 B  │
│             │   HTTP (内部免认证)    │             │
│  axum HTTP  │ ◄──────────────────► │  axum HTTP  │
│  :8080      │   reqwest + 发现      │  :8080      │
│             │                      │             │
│  gRPC volo  │     gRPC (保持不变)   │  gRPC volo  │
│  :50051     │ ◄──────────────────► │  :50051     │
└─────────────┘                      └─────────────┘
```

#### 实现要点

1. **实现** **`factory.rs`** **中预留的** **`"http_rest"`** **分支**，创建 `HttpRestClient`
2. **服务发现集成**：从 `ServiceInstanceCache` 获取目标实例的 HTTP 地址（`ip:port`）
3. **内部调用标识**：通过特殊 Header（如 `X-Internal-Call` + 共享密钥）标识内部请求
4. **负载均衡**：在 HTTP 客户端层实现简单的轮询/随机负载均衡
5. **重试机制**：参考 `VoloGrpcClient` 的重试策略
6. **axum 中间件**：新增 `mw_internal_auth.rs` 中间件，验证内部调用标识

#### 核心设计

```rust
// cmx-rpc/src/http_client.rs (新增)
pub struct HttpRestClient {
    cache: Arc<ServiceInstanceCache>,
    http: reqwest::Client,
    config: HttpRestConfig,
    internal_secret: String,  // 内部调用共享密钥
}

impl RpcClient for HttpRestClient {
    async fn call_service(...) -> Result<...>;
    async fn call_function(...) -> Result<...>;
}

// 通用内部 HTTP 调用方法
impl HttpRestClient {
    pub async fn call_http<P: Serialize, R: DeserializeOwned>(
        &self,
        service_name: &str,
        method: Method,
        path: &str,
        body: Option<&P>,
    ) -> Result<R>;
}
```

```rust
// cmx-api/src/middleware/mw_internal_auth.rs (新增)
// 验证 X-Internal-Call + X-Internal-Secret Header
// 通过验证的请求跳过认证（未来添加认证时）
```

#### 优点

* **开发成本低**：无需定义 proto，直接复用现有 axum handler 和 JSON 序列化

* **零接口重复**：内部调用和外部调用走同一套 axum 路由，无维护双份接口的问题

* **与 axum 生态一致**：请求/响应类型、错误处理完全一致

* **调试友好**：HTTP + JSON 可用 curl、Postman、浏览器直接调试

* **渐进式改造**：可在现有框架上逐步增强，不影响已有功能

* **复用大部分基础设施**：服务发现、配置管理可复用

#### 缺点

* **性能略低**：JSON 文本序列化比 Protobuf 二进制更大更慢

* **无编译期契约**：接口变更只能在运行时发现

* **需新增负载均衡**：HTTP 客户端需自行实现负载均衡（volo 已内置）

* **两套通信协议**：gRPC 用于服务编排，HTTP REST 用于业务接口，架构不完全统一

***

### 方案 C：不改造，仅实现免认证调用

保持现有接口不变，仅通过内部网络策略 + 端口隔离实现免认证。

#### 架构图

```
┌─────────────┐                      ┌─────────────┐
│  服务实例 A  │   HTTP (直连免认证)   │  服务实例 B  │
│             │ ──────────────────► │             │
│  axum HTTP  │   知道对方 IP:Port   │  axum HTTP  │
│  :8080      │                      │  :8080      │
│             │                      │             │
│  gRPC volo  │     gRPC (保持不变)   │  gRPC volo  │
│  :50051     │ ◄──────────────────► │  :50051     │
└─────────────┘                      └─────────────┘
```

#### 实现要点

1. **配置化目标地址**：通过环境变量或配置中心指定其他服务的 HTTP 地址
2. **简单 reqwest 调用**：直接用 reqwest 调用目标服务的 HTTP API
3. **网络层隔离**：依赖 Docker/K8s 网络策略限制外部访问

#### 优点

* **最简单**：几乎不需要改代码，只需配置

* **零侵入**：不影响现有架构

#### 缺点

* **无服务发现**：硬编码或配置目标地址，不支持自动实例发现

* **无负载均衡**：单点调用或需要外部负载均衡器

* **无重试/容错**：调用失败无自动重试

* **安全隐患大**：依赖网络层隔离，一旦被突破则无任何防护

* **不可扩展**：实例增减需要手动更新配置

* **不是企业级方案**：仅适合快速验证，不适合生产环境

***

## 三、综合评估对比

| 维度            | 方案 A (gRPC) | 方案 B (HTTP 内部调用) | 方案 C (免认证直连) |
| ------------- | :---------: | :--------------: | :----------: |
| **开发成本**      |      高      |         中        |       低      |
| **维护成本**      |   高（双份接口）   |     低（复用现有接口）    |       低      |
| **性能**        |    ★★★★★    |       ★★★★       |      ★★★     |
| **类型安全**      |    ★★★★★    |        ★★★       |      ★★      |
| **调试便利性**     |      ★★     |       ★★★★★      |     ★★★★★    |
| **架构一致性**     |    ★★★★★    |       ★★★★       |      ★★      |
| **可扩展性**      |    ★★★★★    |       ★★★★       |       ★      |
| **与现有代码兼容**   |     ★★★     |       ★★★★★      |     ★★★★     |
| **服务发现/负载均衡** |      内置     |        需实现       |       无      |
| **企业级成熟度**    |    ★★★★★    |       ★★★★       |       ★      |

***

## 四、推荐方案：方案 B（HTTP 内部调用框架）

### 推荐理由

1. **性价比最高**：中等开发成本，获得完整的服务间调用能力
2. **与现有架构最契合**：axum 是项目核心框架，HTTP 内部调用完全复用现有 handler
3. **无接口重复**：不像方案 A 需要维护 gRPC + RESTful 两套接口
4. **渐进式演进**：未来如需对特定高频接口改为 gRPC，可以单独迁移（方案 B → 方案 A 的演进路径清晰）
5. **预留基础设施已有**：`factory.rs` 已预留 `"http_rest"` 分支，`HttpRestConfig` 已定义

### 企业级架构最佳实践依据

在微服务架构中，业界普遍采用的内部调用模式：

* **Spring Cloud 生态**：OpenFeign（HTTP 内部调用 + 声明式客户端）是最主流的方案

* **Kubernetes 生态**：Service（HTTP）+ Istio（Service Mesh）的组合比纯 gRPC 更常见

* **混合协议模式**：高频/低延迟接口走 gRPC，普通业务接口走 HTTP REST，是大规模系统的标准做法

**方案 B 正是这一最佳实践的具体体现**：保留 gRPC 用于已有的服务编排场景，新增 HTTP 内部调用用于业务接口，两者互补。

### 方案 B 的实施计划

#### 阶段 1：HTTP 内部客户端核心框架

| 步骤  | 文件                             | 内容                                                  |
| --- | ------------------------------ | --------------------------------------------------- |
| 1.1 | `cmx-rpc/src/http_client.rs`   | 新增 `HttpRestClient`，实现服务发现 + 负载均衡 + reqwest 调用      |
| 1.2 | `cmx-rpc/src/factory.rs`       | 实现 `"http_rest"` 分支，返回 `HttpRestClient`             |
| 1.3 | `cmx-rpc/src/config.rs`        | 确认 `HttpRestConfig` 配置项完整（已存在，需补充 internal\_secret） |
| 1.4 | `cmx-rpc/src/lib.rs`           | 导出 `HttpRestClient`                                 |
| 1.5 | `cmx-rpc/src/load_balancer.rs` | 新增简单轮询负载均衡器                                         |

#### 阶段 2：内部调用认证中间件

| 步骤  | 文件                                           | 内容              |
| --- | -------------------------------------------- | --------------- |
| 2.1 | `cmx-api/src/middleware/mw_internal_auth.rs` | 新增内部调用验证中间件     |
| 2.2 | `cmx-api/src/middleware/mod.rs`              | 注册新中间件          |
| 2.3 | `web-server/src/main.rs`                     | 在中间件栈中插入内部认证中间件 |

#### 阶段 3：业务层集成

| 步骤  | 文件                             | 内容                                                  |
| --- | ------------------------------ | --------------------------------------------------- |
| 3.1 | `cmx-traits/src/rpc_client.rs` | 扩展 `RpcClient` trait 或新增 `InternalHttpClient` trait |
| 3.2 | 业务模块按需调用                       | 通过 `GlobalRpcClient` 或新的全局 HTTP 客户端发起内部调用           |

***

## 五、长期演进路径

```
当前状态                     短期目标                      长期目标
─────────                   ─────────                    ─────────
gRPC: 服务编排    ──►    gRPC: 服务编排（不变）    ──►   gRPC: 服务编排 + 高频接口
                            │                              │
HTTP: 仅对外 API  ──►    HTTP: 对外 + 内部调用    ──►   HTTP: 对外 + 内部 + Service Mesh
                            │                              │
无认证            ──►    内部密钥认证              ──►   mTLS / JWT 完整认证体系
```

***

## 六、假设与决策

| 编号 | 假设/决策                         | 说明                     |
| -- | ----------------------------- | ---------------------- |
| A1 | 所有服务实例共享相同的 axum 路由定义         | 内部调用可以直接按路径调用          |
| A2 | 内部调用密钥通过环境变量注入                | `INTERNAL_CALL_SECRET` |
| A3 | HTTP 负载均衡采用简单轮询策略             | 足以满足当前规模               |
| A4 | 未来会引入完整认证体系                   | 内部密钥是过渡方案，不作为长期安全方案    |
| D1 | reqwest 作为 HTTP 客户端           | 已在项目中使用，无需引入新依赖        |
| D2 | 服务发现复用 `ServiceInstanceCache` | 与 gRPC 层共享同一套缓存        |

***

## 七、验证步骤

1. **单元测试**：`HttpRestClient` 的服务发现、负载均衡、重试逻辑
2. **集成测试**：启动两个 web-server 实例，通过 HTTP 内部客户端互相调用
3. **中间件测试**：验证内部调用 Header 能正确通过认证中间件
4. **端到端测试**：模拟多实例部署，验证完整的跨服务调用链路

