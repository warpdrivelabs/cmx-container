# cmx-rpc gRPC 接口架构拆分方案（v3，已按复审报告修订）

> 修订记录：
> - v2：按初版评审 P0/P1/P2 checklist 返工，引入 Bundle 模式（用户决策）、超时设计债仅文档化。
> - v3：按 v2 复审（`20260625_cmx-rpc_接口架构拆分方案评估报告.md`）修订失败路径日志回归（P1）、统一 success 语义、补全 PluginDataBundle。Bundle 模式经 volo-grpc 0.12.2 源码核验编译安全。

---

## 0. 对评审报告的采纳说明

### 0.1 初版评审项（v2 已落实）

| 评估项 | 处理 |
|--------|------|
| 2.1【P0】删除 `RpcClient` deprecated super-trait | ✅ 一次性删除，无灰度别名 |
| 2.2【P0】`GrpcInfrastructure` 暴露 `timeout_ms()` | ✅ 补方法 |
| 2.3【P1】`with_retry` 补回结构化日志字段 | ✅ 成功路径用 `RetryStats`；失败路径见 0.2（v3 补） |
| 2.4【P1】文档化 `with_retry` 使用约束 | ✅ retry.rs 注释固化 |
| 3.1【P1】OCP 措辞 | ✅ 引入 Bundle 模式，OCP 名副其实（用户决策） |
| 3.2【P1】补单测 | ✅ retry 三函数单测纳入验收 |
| 3.3【P2】server use 清单 | ✅ 每个 server/*.rs 列出 |
| 3.4【P2】init_rpc 改造伪代码 | ✅ 补全 |
| 3.5【P2】超时语义债 | ✅ 本期仅文档化（用户决策），见第十一章 |
| 3.6【P2】import 不重试理由 | ✅ 文档化 |
| 4.3 step_status 统一 | ✅ 统一到 cmx-biz 单一来源 |
| 4.4 命名约定 | ✅ 固化：客户端 `XxxGrpcClient`，服务端 `CmxXxxServerImpl` |
| 4.1 RpcClients 封装 | ⚠️ 被 Bundle + 领域全局取代，不再有 RpcClients struct |

### 0.2 v2 复审新发现问题（v3 修订）

| 复审项 | 处理 |
|--------|------|
| 3.1【P1】失败/重试路径日志字段丢失，与"日志零丢失"验收矛盾 | ✅ `with_retry` 改返回 `Result<(T, RetryStats), (RpcError, RetryStats)>`，失败日志交还调用方，见 §5.2 / §6.1 |
| 3.2【P3】`call_function` success 语义变更且与 `call_service` 不一致 | ✅ 两方法统一为业务 success，显式文档化语义微调，见 §6.1 注 |
| 3.3【P3】`PluginDataBundle` 实现缺失 | ✅ 补全完整代码，见 §6.2 |
| 3.4【P3】`ServerDeps` 对所有 Bundle 过度供给 | ✅ 记录为 OCP 合理耦合代价，见 §8.1 注；本期不做关联类型 |
| 复审 2.1 核验 Bundle 编译可行性 | ✅ volo-grpc 0.12.2 `add_service` 返回 `Self`，`fold + FnOnce` 类型擦除编译通过 |

### 0.3 v3 复审新发现微调项（v3.1 修订）

| 复审项 | 处理 |
|--------|------|
| 2.1【P3】`orchestrator.rs` 导入 `RetryStats` 未使用 → clippy 警告 | ✅ 移除 `RetryStats`，仅导入 `with_retry`，见 §6.1 line 442 |
| 2.2【P3】中间重试 warn 日志相对现状丢失业务字段 | ✅ 已论证为合理取舍（§5.1），验收标准明确"零丢失"不含中间重试路径；实施 PR 需提示此行为微调 |

---

## 一、问题分析

### 1.1 现状

`cmx-rpc/src/client.rs`（563 行）将**两个不相关的 gRPC 服务领域**混合在同一个 `VoloGrpcClient`：

| 领域 | gRPC 服务 | 方法 | Proto |
|------|----------|------|-------|
| 服务编排 | `CmxServiceOrchestrator` | `call_service`, `call_function` | `cmx_service.proto` |
| 插件数据管理 | `CmxPluginDataService` | `import_plugin_data`, `cleanup_plugin_data` | `cmx_plugin_data.proto` |

### 1.2 核心问题

1. **`RpcClient` trait 违反 ISP**：4 方法混合两领域，插件数据方法靠默认实现返回 `UnsupportedProtocol` 凑数。
2. **`CachedClient` 结构耦合不相关客户端**：`orchestrator_client` 与 `plugin_data_client` 硬编码同一 struct，`get_client()` 总是同时创建两个客户端。
3. **`VoloGrpcClient` 上帝 Struct**。
4. **扩展性差（核心痛点）**：新增 gRPC 服务需改 5 处（traits trait / client.rs / server.rs / server_runner.rs / global+factory），违反 OCP。
5. **服务端命名误导**：`CmxOrchestratorServiceImpl` 同时 impl 两个 service。

---

## 二、目标架构（Bundle 模式）

### 2.1 设计原则

- **按领域拆分 Trait**：每个 gRPC 服务领域一个独立 trait
- **Bundle 模式**：每个领域封装为一个 `RpcServiceBundle`，组装点只迭代 bundle 列表
- **共享基础设施**：服务发现、Discover 管理、重试逻辑提取复用
- **真正 OCP**：新增 gRPC 服务 = 新增一个领域模块 + `default_bundles()` 加一行，`factory/global/server_runner` 零改动

### 2.2 目标目录结构

```
cmx-traits/src/rpc/
├── mod.rs               # 更新声明 + re-exports
├── error.rs             # RpcError（从 client.rs 拆出）
├── types.rs             # FunctionCallResult 等共享类型
├── orchestrator.rs      # 【新增】ServiceOrchestrationClient trait
└── plugin_data.rs       # 【新增】PluginDataClient trait

cmx-rpc/src/
├── lib.rs               # 更新 re-exports
├── config.rs            # 不变
├── discover.rs          # 不变
├── error.rs             # 不变（框架内部错误）
├── bundle.rs            # 【新增】RpcServiceBundle trait + ServerDeps + default_bundles()
├── client/
│   ├── mod.rs           # 模块声明 + safe_parse_json + 领域全局访问器
│   ├── infra.rs         # 【新增】GrpcInfrastructure（共享基础设施）
│   ├── retry.rs         # 【新增】with_retry + RetryStats + 单测
│   ├── orchestrator.rs  # OrchestratorGrpcClient + impl trait + Bundle + 领域全局
│   └── plugin_data.rs   # PluginDataGrpcClient + impl trait + Bundle + 领域全局
├── server/
│   ├── mod.rs           # 模块声明 + re-exports
│   ├── orchestrator.rs  # CmxOrchestratorServerImpl（仅 impl CmxServiceOrchestrator）
│   └── plugin_data.rs   # CmxPluginDataServerImpl（仅 impl CmxPluginDataService）
└── server_runner.rs     # 更新：接收 Vec<Box<dyn RpcServiceBundle>>
```

---

## 三、cmx-traits：拆分 trait（一次性删除 RpcClient）

### 3.1 `error.rs`（从 client.rs 拆出，保持不变）

`RpcError` 枚举原样迁移。

### 3.2 `types.rs`

`FunctionCallResult` 原样迁移。

### 3.3 `orchestrator.rs` — 新增

```rust
//! 服务编排 RPC 客户端 trait。
use async_trait::async_trait;
use serde_json::Value;
use crate::rpc::error::RpcError;
use crate::rpc::types::FunctionCallResult;
use crate::service::invoker::ServiceInvokeOptions;

/// 服务编排 RPC 客户端接口（对应 gRPC `CmxServiceOrchestrator`）。
#[async_trait]
pub trait ServiceOrchestrationClient: Send + Sync {
    async fn call_service(
        &self, service_name: &str, service_key: &str, input: Value, options: ServiceInvokeOptions,
    ) -> Result<cmx_core::CallServiceResponse, RpcError>;

    async fn call_function(
        &self, service_name: &str, plugin_id: &str, function_name: &str, input: Value,
    ) -> Result<FunctionCallResult, RpcError>;
}
```

### 3.4 `plugin_data.rs` — 新增

```rust
//! 插件数据管理 RPC 客户端 trait。
use async_trait::async_trait;
use crate::plugin::{PluginDataCleanupRequest, PluginDataImportRequest, PluginDataImportResult};
use crate::rpc::error::RpcError;

/// 插件数据管理 RPC 客户端接口（对应 gRPC `CmxPluginDataService`）。
#[async_trait]
pub trait PluginDataClient: Send + Sync {
    async fn import_plugin_data(
        &self, service_name: &str, request: PluginDataImportRequest,
    ) -> Result<PluginDataImportResult, RpcError>;

    async fn cleanup_plugin_data(
        &self, service_name: &str, request: PluginDataCleanupRequest,
    ) -> Result<PluginDataImportResult, RpcError>;
}
```

### 3.5 `mod.rs` — 更新（删除 RpcClient）

```rust
pub mod error;
pub mod types;
pub mod orchestrator;
pub mod plugin_data;

pub use error::RpcError;
pub use types::FunctionCallResult;
pub use orchestrator::ServiceOrchestrationClient;
pub use plugin_data::PluginDataClient;
```

> **P0 2.1 落实**：不保留任何 deprecated 别名。`RpcClient` 完全删除。

---

## 四、cmx-rpc：共享基础设施（P0 2.2 补 timeout_ms）

**文件**：`cmx-rpc/src/client/infra.rs`

```rust
//! gRPC 共享基础设施：服务发现订阅、Discover 缓存、超时/重试配置访问。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::instrument;

use cmx_registry_config::registry::{ServiceInstanceCache, ServiceRegistry};
use cmx_traits::rpc::RpcError;
use crate::config::GrpcConfig;
use crate::discover::RegistryAwareDiscover;

/// gRPC 共享基础设施
///
/// 被 `OrchestratorGrpcClient` / `PluginDataGrpcClient` 通过 `Arc` 共同持有。
/// 核心：按 service_name 缓存 Discover，避免重复订阅注册中心。
///
/// 共享后多个 client 复用同一 broadcast channel，
/// 默认 `discover_channel_capacity=1024` 对双客户端订阅仍充足（实例变更事件频率低）。
pub struct GrpcInfrastructure {
    cache: Arc<ServiceInstanceCache>,
    config: GrpcConfig,
    registry: Arc<dyn ServiceRegistry>,
    discovers: RwLock<HashMap<String, RegistryAwareDiscover>>,
}

impl GrpcInfrastructure {
    pub fn new(cache: Arc<ServiceInstanceCache>, config: GrpcConfig, registry: Arc<dyn ServiceRegistry>) -> Self {
        Self { cache, config, registry, discovers: RwLock::new(HashMap::new()) }
    }

    /// 单次 gRPC rpc_timeout（P0 2.2：显式暴露，供 volo rpc_timeout 设置）
    pub fn rpc_timeout(&self) -> Duration { Duration::from_millis(self.config.timeout_ms) }

    /// 连接超时
    pub fn connect_timeout(&self) -> Duration { Duration::from_millis(self.config.connect_timeout_ms) }

    /// RPC 总超时预算（毫秒）（P0 2.2：供 with_retry 计算 deadline）
    ///
    /// 注意：当前与 `rpc_timeout` 同源，见第十一章设计债。
    pub fn timeout_ms(&self) -> u64 { self.config.timeout_ms }

    /// 重试次数
    pub fn retry_count(&self) -> usize { self.config.retry_count }

    /// 获取或创建指定服务的 Discover（double-check locking + 注册中心懒订阅）
    ///
    /// 网络 IO（subscribe_instances）在写锁外完成，写锁只保护 HashMap insert。
    #[instrument(target = "cmx_rpc", skip(self), fields(service_name = %service_name))]
    pub async fn get_or_create_discover(
        &self, service_name: &str,
    ) -> Result<RegistryAwareDiscover, RpcError> {
        if let Some(d) = self.discovers.read().await.get(service_name) {
            return Ok(d.clone());
        }
        if self.cache.get(service_name).is_none_or(|v| v.is_empty()) {
            self.registry
                .subscribe_instances(service_name, Arc::new(|_, _| {}))
                .await
                .map_err(|e| RpcError::NoAvailableInstance(
                    format!("服务 '{}' 订阅失败: {}", service_name, e)))?;
            if self.cache.get(service_name).is_none_or(|v| v.is_empty()) {
                return Err(RpcError::NoAvailableInstance(service_name.to_string()));
            }
        }
        let discover = RegistryAwareDiscover::new(self.cache.clone(), self.config.discover_channel_capacity);
        discover.start_watch(service_name);
        let mut discovers = self.discovers.write().await;
        if let Some(d) = discovers.get(service_name) { return Ok(d.clone()); }
        discovers.insert(service_name.to_string(), discover.clone());
        Ok(discover)
    }
}
```

---

## 五、cmx-rpc：重试工具（P1 2.3 补回日志字段 + P1 2.4 约束）

**文件**：`cmx-rpc/src/client/retry.rs`

### 5.1 设计要点（v3 修订失败路径）

- **返回 `Result<(T, RetryStats), (RpcError, RetryStats)>`**（v3·P1 3.1）：成功和失败都携带 `RetryStats`，**失败日志交还调用方记录**，使失败路径也能补全 `service_name/service_key/elapsed_us/success=false` 等业务字段，真正做到"日志字段零丢失"。
- **中间重试 warn 保留在 `with_retry` 内**（仅记 `attempt/max_retries/error`）：重试中日志业务关联性弱，可由最终成功/失败日志聚合；避免给泛型函数引入业务上下文参数。
  - **v3·P3 2.2 行为微调提示**：相对现状，中间重试 warn 日志不再带 `service_name/service_key`。若下游有基于中间重试 warn + service_name 的告警规则，需在实施 PR 中提示此行为变更，并建议改为基于最终成功/失败日志聚合。
- **P1 2.4 使用约束**（写死在文档注释）：闭包只返回原始 `Status`；`into_inner()` / proto 转换必须在 `with_retry` 之外做一次，避免重试时重复消费 response。

### 5.2 代码

```rust
//! gRPC 重试工具：纯函数 + 泛型 with_retry。
//!
//! # 使用约束（P1 2.4）
//!
//! `with_retry` 的闭包 **只能返回原始 `volo_grpc::Status` 错误**。
//! 业务解析（`Response::into_inner()`、proto → domain 转换）必须在 `with_retry`
//! 返回后做一次。否则重试分支会重复消费 response，导致 panic 或语义错误。

use std::time::{Duration, Instant};
use cmx_traits::rpc::RpcError;
use volo_grpc::{Code, Status};

/// 重试统计（P1 2.3：供调用方补全结构化日志字段）
#[derive(Debug, Clone, Copy)]
pub struct RetryStats {
    /// 实际尝试次数（从 1 开始）
    pub attempts: usize,
    /// 总耗时
    pub elapsed: Duration,
}

pub fn is_retryable_error(status: &Status) -> bool {
    matches!(
        status.code(),
        Code::Unavailable | Code::DeadlineExceeded | Code::ResourceExhausted | Code::Aborted
    )
}

pub fn retry_backoff(attempt: usize) -> Duration {
    let backoff_ms = 50u64.saturating_mul(1u64 << attempt.min(4));
    Duration::from_millis(backoff_ms.min(800))
}

/// 执行带总时间预算的重试循环。
///
/// 成功返回 `(T, RetryStats)`，失败返回 `(RpcError, RetryStats)`。
///
/// **v3·P1 3.1**：本函数**不记最终失败日志**（仅记中间重试 warn），最终失败日志由调用方
/// 拿到 `stats` 后用业务字段（service_name/service_key/success=false 等）记录，
/// 确保失败路径结构化字段零丢失。
pub async fn with_retry<F, Fut, T>(
    timeout_ms: u64,
    max_retries: usize,
    f: F,
) -> Result<(T, RetryStats), (RpcError, RetryStats)>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, Status>>,
{
    let start = Instant::now();
    let deadline = start + Duration::from_millis(timeout_ms);

    for attempt in 0..=max_retries {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if !remaining.is_zero() && attempt > 0 {
            let backoff = std::cmp::min(retry_backoff(attempt - 1), remaining);
            tokio::time::sleep(backoff).await;
        }
        let stats_now = RetryStats { attempts: attempt + 1, elapsed: start.elapsed() };
        if remaining.is_zero() {
            // 预算耗尽：返回 stats 给调用方记录失败日志
            return Err((RpcError::Timeout(format!(
                "重试预算耗尽: 总耗时 {}ms", stats_now.elapsed.as_millis()
            )), stats_now));
        }
        if attempt > 0 {
            // 中间重试 warn：仅记重试调度信息，业务字段由调用方最终日志聚合
            tracing::debug!(target: "cmx_rpc", attempt, max_retries, remaining_ms = remaining.as_millis() as u64, "RPC 重试调度");
        }
        match f().await {
            Ok(result) => return Ok((result, stats_now)),
            Err(e) => {
                if is_retryable_error(&e) && attempt < max_retries {
                    // 中间重试：业务关联性弱，仅记 attempt/max_retries/error
                    tracing::warn!(target: "cmx_rpc", attempt = attempt + 1, max_retries, error = %e, "RPC 失败（可重试）");
                    continue;
                }
                // 最终失败：不在此记日志，交还调用方（带业务字段）
                return Err((RpcError::RpcCallFailed(e.to_string()), stats_now));
            }
        }
    }
    unreachable!("retry loop must return before exiting")
}

// ==================== 单元测试（P1 3.2）====================
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn status(code: Code) -> Status { Status::new(code, "test") }

    #[test]
    fn test_is_retryable() {
        for c in [Code::Unavailable, Code::DeadlineExceeded, Code::ResourceExhausted, Code::Aborted] {
            assert!(is_retryable_error(&status(c)), "{:?} 应可重试", c);
        }
        for c in [Code::InvalidArgument, Code::NotFound, Code::PermissionDenied, Code::Unimplemented] {
            assert!(!is_retryable_error(&status(c)), "{:?} 不应可重试", c);
        }
    }

    #[test]
    fn test_retry_backoff_sequence() {
        // 50 → 100 → 200 → 400 → 800 → 800（上限）
        let seq: Vec<u64> = (0..6).map(|i| retry_backoff(i).as_millis() as u64).collect();
        assert_eq!(seq, vec![50, 100, 200, 400, 800, 800]);
    }

    #[tokio::test]
    async fn test_with_retry_success_after_failures() {
        let counter = Arc::new(AtomicUsize::new(0));
        let c = counter.clone();
        let (result, stats) = with_retry(10_000, 3, || {
            let c = c.clone();
            async move {
                let n = c.fetch_add(1, Ordering::SeqCst);
                if n < 2 { Err(status(Code::Unavailable)) } else { Ok(42) }
            }
        }).await.unwrap();
        assert_eq!(result, 42);
        assert_eq!(stats.attempts, 3);
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_with_retry_non_retryable_fails_fast() {
        let counter = Arc::new(AtomicUsize::new(0));
        let c = counter.clone();
        let (err, stats) = with_retry::<_, _, i32>(10_000, 3, || {
            let c = c.clone();
            async move { c.fetch_add(1, Ordering::SeqCst); Err(status(Code::InvalidArgument)) }
        }).await.unwrap_err();
        assert!(matches!(err, RpcError::RpcCallFailed(_)));
        assert_eq!(stats.attempts, 1); // 失败也带 stats
        assert_eq!(counter.load(Ordering::SeqCst), 1); // 只调一次
    }

    #[tokio::test]
    async fn test_with_retry_budget_exhausted() {
        // 预算 0 → 立即超时，不执行闭包；失败带 stats
        let (err, stats) = with_retry::<_, _, i32>(0, 3, || async { Ok(1) }).await.unwrap_err();
        assert!(matches!(err, RpcError::Timeout(_)));
        assert_eq!(stats.attempts, 1);
    }
}
```

---

## 六、cmx-rpc：领域客户端 + Bundle + 领域全局

### 6.1 `client/orchestrator.rs`

```rust
//! 服务编排 gRPC 客户端 + Bundle + 领域全局访问器。
//!
//! # 领域全局
//! 访问：`cmx_rpc::orchestrator_client()` → `&'static Arc<dyn ServiceOrchestrationClient>`

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use async_trait::async_trait;
use cmx_core::CallServiceResponse;
use cmx_rpc_gen::cmx::cmx_service_orchestrator::cmx_service_orchestrator::cmx::*;
use cmx_traits::rpc::{FunctionCallResult, RpcError, ServiceOrchestrationClient};
use serde_json::Value;
use tokio::sync::RwLock;
use tracing::instrument;

use super::infra::GrpcInfrastructure;
use super::retry::with_retry;  // v3·P3 2.1：RetryStats 仅由元组解构推断，不需导入类型名
use super::safe_parse_json;
use crate::bundle::{RpcServiceBundle, ServerDeps, ServerRegistration};

// ==================== 领域全局访问器 ====================
static ORCHESTRATOR_CLIENT: OnceLock<Arc<dyn ServiceOrchestrationClient>> = OnceLock::new();

pub(crate) fn set_client(c: Arc<dyn ServiceOrchestrationClient>) -> Result<(), ()> {
    ORCHESTRATOR_CLIENT.set(c).map_err(|_| ())
}

/// 获取服务编排 RPC 客户端（须先 init）
pub fn orchestrator_client() -> &'static Arc<dyn ServiceOrchestrationClient> {
    ORCHESTRATOR_CLIENT.get().expect("orchestrator client not initialized")
}

// ==================== 客户端实现 ====================
pub struct OrchestratorGrpcClient {
    infra: Arc<GrpcInfrastructure>,
    clients: RwLock<HashMap<String, CmxServiceOrchestratorClient>>,
}

impl OrchestratorGrpcClient {
    pub fn new(infra: Arc<GrpcInfrastructure>) -> Self {
        Self { infra, clients: RwLock::new(HashMap::new()) }
    }

    async fn get_client(&self, service_name: &str) -> Result<CmxServiceOrchestratorClient, RpcError> {
        if let Some(c) = self.clients.read().await.get(service_name) {
            return Ok(c.clone());
        }
        let discover = self.infra.get_or_create_discover(service_name).await?;
        let client = CmxServiceOrchestratorClientBuilder::new(service_name)
            .discover(discover)
            .rpc_timeout(Some(self.infra.rpc_timeout()))
            .connect_timeout(self.infra.connect_timeout())
            .build();
        let mut clients = self.clients.write().await;
        if let Some(c) = clients.get(service_name) { return Ok(c.clone()); }
        clients.insert(service_name.to_string(), client.clone());
        Ok(client)
    }
}

#[async_trait]
impl ServiceOrchestrationClient for OrchestratorGrpcClient {
    #[instrument(target = "cmx_rpc", skip(self, input), fields(service_name = %service_name, service_key = %service_key))]
    async fn call_service(
        &self, service_name: &str, service_key: &str, input: Value,
        options: cmx_traits::service::ServiceInvokeOptions,
    ) -> Result<CallServiceResponse, RpcError> {
        let client = self.get_client(service_name).await?;
        let timeout_ms = self.infra.timeout_ms();       // P0 2.2
        let max_retries = self.infra.retry_count();

        let service_key_fs: pilota::FastStr = service_key.to_string().into();
        let input_fs: pilota::FastStr = input.to_string().into();
        let debug_node_id = options.debug_node_id.map(|s| -> pilota::FastStr { s.into() });
        let debug_params: pilota::AHashMap<pilota::FastStr, pilota::FastStr> = options
            .debug_params.unwrap_or_default().into_iter()
            .map(|(k, v)| (k.into(), v.into())).collect();

        // P1 2.4：闭包只返回原始 Status，into_inner 在外做一次
        match with_retry(timeout_ms, max_retries, || {
            let req = ExecuteServiceRequest {
                service_key: service_key_fs.clone(),
                input: input_fs.clone(),
                include_steps: options.include_steps,
                debug: options.debug,
                debug_node_id: debug_node_id.clone(),
                debug_params: debug_params.clone(),
            };
            let client = client.clone();
            async move { client.execute_service(req).await }
        }).await {
            Ok((resp, stats)) => {
                let resp = resp.into_inner();
                // P1 2.3 + v3·P3 3.2：成功路径业务字段 + 业务 success（与 call_function 统一）
                tracing::info!(
                    target: "cmx_rpc",
                    service_name = %service_name, service_key = %service_key,
                    elapsed_us = stats.elapsed.as_micros() as u64,
                    attempts = stats.attempts, success = resp.success,
                    "RPC call_service 完成"
                );
                Ok(proto_to_call_service_response(resp))
            }
            Err((e, stats)) => {
                // v3·P1 3.1：失败路径业务字段 + stats（零丢失）
                tracing::warn!(
                    target: "cmx_rpc",
                    service_name = %service_name, service_key = %service_key,
                    elapsed_us = stats.elapsed.as_micros() as u64,
                    attempts = stats.attempts, success = false, error = %e,
                    "RPC call_service 失败"
                );
                Err(e)
            }
        }
    }

    #[instrument(target = "cmx_rpc", skip(self, input), fields(service_name = %service_name, plugin_id = %plugin_id, function_name = %function_name))]
    async fn call_function(
        &self, service_name: &str, plugin_id: &str, function_name: &str, input: Value,
    ) -> Result<FunctionCallResult, RpcError> {
        let client = self.get_client(service_name).await?;
        let timeout_ms = self.infra.timeout_ms();
        let max_retries = self.infra.retry_count();

        let plugin_id_fs: pilota::FastStr = plugin_id.to_string().into();
        let function_name_fs: pilota::FastStr = function_name.to_string().into();
        let input_fs: pilota::FastStr = input.to_string().into();

        match with_retry(timeout_ms, max_retries, || {
            let req = CallFunctionRequest {
                plugin_id: plugin_id_fs.clone(),
                function_name: function_name_fs.clone(),
                input: input_fs.clone(),
                initial_input: None, debug: false,
            };
            let client = client.clone();
            async move { client.call_function(req).await }
        }).await {
            Ok((resp, stats)) => {
                let inner = resp.into_inner();
                tracing::info!(
                    target: "cmx_rpc",
                    service_name = %service_name, plugin_id = %plugin_id, function_name = %function_name,
                    elapsed_us = stats.elapsed.as_micros() as u64,
                    attempts = stats.attempts, success = inner.success,
                    "RPC call_function 完成"
                );
                Ok(FunctionCallResult {
                    success: inner.success,
                    result: inner.result.map(|s| safe_parse_json(&s, "call_function.result")),
                    elapsed_us: inner.elapsed_us,
                    error: inner.error.map(|s| s.to_string()),
                })
            }
            Err((e, stats)) => {
                tracing::warn!(
                    target: "cmx_rpc",
                    service_name = %service_name, plugin_id = %plugin_id, function_name = %function_name,
                    elapsed_us = stats.elapsed.as_micros() as u64,
                    attempts = stats.attempts, success = false, error = %e,
                    "RPC call_function 失败"
                );
                Err(e)
            }
        }
    }
}

// ==================== Bundle ====================
pub struct OrchestratorBundle;

impl RpcServiceBundle for OrchestratorBundle {
    fn name(&self) -> &'static str { "orchestrator" }

    fn init_client(&self, infra: Arc<GrpcInfrastructure>) {
        set_client(Arc::new(OrchestratorGrpcClient::new(infra)))
            .expect("orchestrator client already initialized");
    }

    fn build_server(&self, deps: &ServerDeps) -> ServerRegistration {
        let service_invoker = deps.service_invoker.clone();
        let runtime_invoker = deps.runtime_invoker.clone();
        let plugin_query = deps.plugin_query.clone();
        ServerRegistration::new(move |server| {
            let impl_ = crate::server::orchestrator::CmxOrchestratorServerImpl::new(
                service_invoker, runtime_invoker, plugin_query);
            let svc = volo_grpc::server::ServiceBuilder::new(
                CmxServiceOrchestratorServer::new(impl_))
                .build::<CmxServiceOrchestratorRequestRecv, CmxServiceOrchestratorResponseSend>();
            server.add_service(svc)
        })
    }
}

// ==================== proto 转换（含 P 4.3 统一 step_status）====================
fn proto_to_call_service_response(resp: ExecuteServiceResponse) -> CallServiceResponse {
    CallServiceResponse {
        success: resp.success,
        output: resp.output.map(|v| safe_parse_json(&v, "call_service.output")),
        steps: resp.steps.into_iter().map(|s| cmx_core::ExecutionStep {
            node_id: s.node_id.to_string(),
            node_name: s.node_name.to_string(),
            node_type: s.node_type.to_string(),
            // P 4.3：统一到 cmx-biz 单一来源
            status: cmx_biz::service_executor::parse_step_status(&s.status),
            output: s.output.map(|v| safe_parse_json(&v, "step.output")),
            elapsed_us: s.elapsed_us,
            error: s.error.map(|e| e.to_string()),
            previous_output: s.previous_output.map(|v| safe_parse_json(&v, "step.previous_output")),
        }).collect(),
        total_elapsed_us: Some(resp.total_elapsed_us),
        error: resp.error.map(|e| cmx_core::OrchestrationError { message: e.message.to_string() }),
    }
}
```

> **P 4.3 落实**：删除 client.rs 本地的 `parse_step_status`，新增并复用 `cmx_biz::service_executor::parse_step_status`（在 cmx-biz 暴露该函数，把客户端硬编码 match 逻辑迁过去，服务端 `step_status_to_str` 一并收拢，保证 str↔enum 双向单一来源）。

> **v3·P3 3.2 语义微调声明**：成功日志的 `success` 字段统一为**业务 success**（`resp.success` / `inner.success`）。
> - `call_service`：现状硬编码 `success = true`，改为 `success = resp.success`。
> - `call_function`：现状硬编码 `success = true`，改为 `success = inner.success`。
>
> 影响：按 `success` 字段做错误率告警的下游，行为会更准确（业务失败也会被记为 `success=false`）。需在实施 PR 描述中标注此行为变更。

### 6.2 `client/plugin_data.rs`

```rust
//! 插件数据管理 gRPC 客户端 + Bundle + 领域全局访问器。
//!
//! # 领域全局
//! 访问：`cmx_rpc::plugin_data_client()` → `&'static Arc<dyn PluginDataClient>`

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};
use async_trait::async_trait;
use cmx_rpc_gen::cmx::cmx_plugin_data_service::cmx_plugin_data_service::cmx as plugin_data_proto;
use cmx_traits::plugin::{PluginDataCleanupRequest, PluginDataImportRequest, PluginDataImportResult};
use cmx_traits::rpc::{PluginDataClient, RpcError};
use tokio::sync::RwLock;
use tracing::instrument;

use super::infra::GrpcInfrastructure;
use crate::bundle::{RpcServiceBundle, ServerDeps, ServerRegistration};

// ==================== 领域全局访问器 ====================
static PLUGIN_DATA_CLIENT: OnceLock<Arc<dyn PluginDataClient>> = OnceLock::new();

pub(crate) fn set_client(c: Arc<dyn PluginDataClient>) -> Result<(), ()> {
    PLUGIN_DATA_CLIENT.set(c).map_err(|_| ())
}

/// 获取插件数据管理 RPC 客户端（须先 init）
pub fn plugin_data_client() -> &'static Arc<dyn PluginDataClient> {
    PLUGIN_DATA_CLIENT.get().expect("plugin_data client not initialized")
}

// ==================== 客户端实现 ====================
pub struct PluginDataGrpcClient {
    infra: Arc<GrpcInfrastructure>,
    clients: RwLock<HashMap<String, plugin_data_proto::CmxPluginDataServiceClient>>,
}

impl PluginDataGrpcClient {
    pub fn new(infra: Arc<GrpcInfrastructure>) -> Self {
        Self { infra, clients: RwLock::new(HashMap::new()) }
    }

    async fn get_client(
        &self, service_name: &str,
    ) -> Result<plugin_data_proto::CmxPluginDataServiceClient, RpcError> {
        if let Some(c) = self.clients.read().await.get(service_name) {
            return Ok(c.clone());
        }
        let discover = self.infra.get_or_create_discover(service_name).await?;
        let client = plugin_data_proto::CmxPluginDataServiceClientBuilder::new(service_name)
            .discover(discover)
            .rpc_timeout(Some(self.infra.rpc_timeout()))
            .connect_timeout(self.infra.connect_timeout())
            .build();
        let mut clients = self.clients.write().await;
        if let Some(c) = clients.get(service_name) { return Ok(c.clone()); }
        clients.insert(service_name.to_string(), client.clone());
        Ok(client)
    }
}

#[async_trait]
impl PluginDataClient for PluginDataGrpcClient {
    #[instrument(target = "cmx_rpc", skip(self, request), fields(service_name = %service_name, category = ?request.category))]
    async fn import_plugin_data(
        &self, service_name: &str, request: PluginDataImportRequest,
    ) -> Result<PluginDataImportResult, RpcError> {
        // 不走 with_retry（见下方理由）；保持与现状一致
        let client = self.get_client(service_name).await?;
        let category_str = request.category.as_str();
        let proto_req = plugin_data_proto::ImportPluginDataRequest {
            category: category_str.into(),
            domain_code: request.domain_code.clone().into(),
            application_code: request.application_code.clone().into(),
            module_code: request.module_code.clone().into(),
            plugin_id: request.plugin_id.clone().into(),
            app_id: request.app_id.clone().into(),
            version: request.version.clone().into(),
            zip_data: request.zip_data.clone().into(),
        };
        match client.import_plugin_data(proto_req).await {
            Ok(resp) => {
                let resp = resp.into_inner();
                tracing::info!(
                    target: "cmx_rpc", service_name = %service_name, category = category_str,
                    success = resp.success, created = resp.created_count,
                    updated = resp.updated_count, deleted = resp.deleted_count,
                    "RPC import_plugin_data 完成"
                );
                Ok(PluginDataImportResult {
                    success: resp.success,
                    message: resp.message.to_string(),
                    created_count: resp.created_count,
                    updated_count: resp.updated_count,
                    deleted_count: resp.deleted_count,
                })
            }
            Err(e) => {
                tracing::warn!(
                    target: "cmx_rpc", service_name = %service_name, category = category_str,
                    success = false, error = %e, "RPC import_plugin_data 失败"
                );
                Err(RpcError::RpcCallFailed(e.to_string()))
            }
        }
    }

    #[instrument(target = "cmx_rpc", skip(self, request), fields(service_name = %service_name, category = ?request.category))]
    async fn cleanup_plugin_data(
        &self, service_name: &str, request: PluginDataCleanupRequest,
    ) -> Result<PluginDataImportResult, RpcError> {
        let client = self.get_client(service_name).await?;
        let category_str = request.category.as_str();
        let proto_req = plugin_data_proto::CleanupPluginDataRequest {
            category: category_str.into(),
            domain_code: request.domain_code.clone().into(),
            application_code: request.application_code.clone().into(),
            module_code: request.module_code.clone().into(),
            plugin_id: request.plugin_id.clone().into(),
            app_id: request.app_id.clone().into(),
        };
        match client.cleanup_plugin_data(proto_req).await {
            Ok(resp) => {
                let resp = resp.into_inner();
                tracing::info!(
                    target: "cmx_rpc", service_name = %service_name, category = category_str,
                    success = resp.success, deleted = resp.deleted_count,
                    "RPC cleanup_plugin_data 完成"
                );
                Ok(PluginDataImportResult {
                    success: resp.success,
                    message: resp.message.to_string(),
                    created_count: resp.created_count,
                    updated_count: resp.updated_count,
                    deleted_count: resp.deleted_count,
                })
            }
            Err(e) => {
                tracing::warn!(
                    target: "cmx_rpc", service_name = %service_name, category = category_str,
                    success = false, error = %e, "RPC cleanup_plugin_data 失败"
                );
                Err(RpcError::RpcCallFailed(e.to_string()))
            }
        }
    }
}

// ==================== Bundle ====================
pub struct PluginDataBundle;

impl RpcServiceBundle for PluginDataBundle {
    fn name(&self) -> &'static str { "plugin_data" }

    fn init_client(&self, infra: Arc<GrpcInfrastructure>) {
        set_client(Arc::new(PluginDataGrpcClient::new(infra)))
            .expect("plugin_data client already initialized");
    }

    fn build_server(&self, deps: &ServerDeps) -> ServerRegistration {
        let data_importer = deps.data_importer.clone();
        ServerRegistration::new(move |server| {
            let impl_ = crate::server::plugin_data::CmxPluginDataServerImpl::new(data_importer);
            let svc = volo_grpc::server::ServiceBuilder::new(
                plugin_data_proto::CmxPluginDataServiceServer::new(impl_))
                .build::<plugin_data_proto::CmxPluginDataServiceRequestRecv,
                        plugin_data_proto::CmxPluginDataServiceResponseSend>();
            server.add_service(svc)
        })
    }
}
```

> **P 3.6 落实（import 不重试理由）**：`import_plugin_data` 传输 ZIP 二进制大包（默认上限 4MB），重试需保证下游导入幂等。当前 `ImportPluginData` 服务端按 upsert 语义实现，理论上幂等，但：(1) 大包重试放大带宽与下游负载；(2) 4MB 上限下网络抖动概率高，盲目重试易雪崩；(3) import 由插件安装流程驱动，失败可由上层重试整个安装任务。故本期**不启用 RPC 层重试**，失败立即返回，且失败日志已带 `service_name/category/success=false/error`。路线：未来若引入幂等 token + 分片上传，可启用有限重试。

### 6.3 `client/mod.rs`

```rust
pub mod infra;
pub mod retry;
pub mod orchestrator;
pub mod plugin_data;

pub use orchestrator::orchestrator_client;
pub use plugin_data::plugin_data_client;

/// 安全解析 JSON（从原 client.rs 迁入）
pub(crate) fn safe_parse_json(raw: &str, context: &str) -> serde_json::Value {
    serde_json::from_str(raw).unwrap_or_else(|e| {
        tracing::warn!(target: "cmx_rpc", error = %e, raw = %raw, context, "RPC 返回 JSON 解析失败，降级为 Null");
        serde_json::Value::Null
    })
}
```

---

## 七、cmx-rpc：服务端拆分（P2 3.3 列 use 清单）

### 7.1 `server/orchestrator.rs`

```rust
//! 服务编排 gRPC 服务端实现。

use std::sync::Arc;
use cmx_core::model::service::SVRContext;
use cmx_rpc_gen::cmx::cmx_service_orchestrator::cmx_service_orchestrator::cmx::*;
use cmx_traits::runtime::RuntimeInvoker;
use cmx_traits::service::ServiceInvoker;
use cmx_traits::plugin::PluginQuery;
use tracing::instrument;

#[derive(Clone)]
pub struct CmxOrchestratorServerImpl {
    service_invoker: Arc<dyn ServiceInvoker>,
    runtime_invoker: Arc<dyn RuntimeInvoker>,
    plugin_query: Arc<dyn PluginQuery>,
}

impl CmxOrchestratorServerImpl {
    pub fn new(
        service_invoker: Arc<dyn ServiceInvoker>,
        runtime_invoker: Arc<dyn RuntimeInvoker>,
        plugin_query: Arc<dyn PluginQuery>,
    ) -> Self { Self { service_invoker, runtime_invoker, plugin_query } }
}

impl CmxServiceOrchestrator for CmxOrchestratorServerImpl {
    // execute_service / call_function 从原 server.rs:51-198 原样迁入
    // 依赖：serde_json, cmx_traits::service::ServiceInvokeOptions, cmx_biz::function_invoker, cmx_biz::service_executor::step_status_to_str, uuid, chrono
}

fn execution_step_to_proto(step: cmx_core::ExecutionStep) -> ExecutionStep {
    ExecutionStep {
        node_id: step.node_id.into(),
        node_name: step.node_name.into(),
        node_type: step.node_type.into(),
        status: cmx_biz::service_executor::step_status_to_str(&step.status).into(),
        output: step.output.map(|v| v.to_string().into()),
        elapsed_us: step.elapsed_us,
        error: step.error.map(|s| s.into()),
        previous_output: step.previous_output.map(|v| v.to_string().into()),
    }
}
```

### 7.2 `server/plugin_data.rs`

```rust
//! 插件数据管理 gRPC 服务端实现。

use std::sync::Arc;
use cmx_rpc_gen::cmx::cmx_plugin_data_service::cmx_plugin_data_service::cmx as plugin_data_proto;
use cmx_traits::plugin::{
    PluginDataCategory, PluginDataCleanupRequest, PluginDataImportRequest, PluginDataImporter,
};
use tracing::instrument;

#[derive(Clone)]
pub struct CmxPluginDataServerImpl {
    data_importer: Option<Arc<dyn PluginDataImporter>>,
}

impl CmxPluginDataServerImpl {
    pub fn new(data_importer: Option<Arc<dyn PluginDataImporter>>) -> Self { Self { data_importer } }
}

impl plugin_data_proto::CmxPluginDataService for CmxPluginDataServerImpl {
    // import_plugin_data / cleanup_plugin_data 从原 server.rs:200-390 原样迁入
}
```

> **P2 3.3 落实**：上述 `use` 清单已列全；`cmx_biz` 依赖随 `call_function` 迁入 orchestrator.rs。

---

## 八、cmx-rpc：Bundle 模式（核心，实现 OCP）

### 8.1 `bundle.rs`

```rust
//! RpcServiceBundle：每个 gRPC 服务领域的客户端+服务端注册封装。
//!
//! 新增领域 = 新增一个领域模块（含 Bundle 实现）+ 在 default_bundles() 加一行。
//! factory / global / server_runner 零改动（OCP）。

use std::sync::Arc;
use crate::client::infra::GrpcInfrastructure;

/// 领域依赖（服务端组装用）。各 Bundle 按需取用。
pub struct ServerDeps {
    pub service_invoker: Arc<dyn cmx_traits::service::ServiceInvoker>,
    pub runtime_invoker: Arc<dyn cmx_traits::runtime::RuntimeInvoker>,
    pub plugin_query: Arc<dyn cmx_traits::plugin::PluginQuery>,
    pub data_importer: Option<Arc<dyn cmx_traits::plugin::PluginDataImporter>>,
}

/// "把 service 加到 server 上"的类型擦除闭包。
/// 因 volo `add_service` 是泛型方法，用闭包在 Bundle 内部 monomorphize。
pub struct ServerRegistration {
    inner: Box<dyn FnOnce(volo_grpc::server::Server) -> volo_grpc::server::Server + Send>,
}
impl ServerRegistration {
    pub fn new<F>(f: F) -> Self
    where F: FnOnce(volo_grpc::server::Server) -> volo_grpc::server::Server + Send {
        Self { inner: Box::new(f) }
    }
    pub fn apply(self, server: volo_grpc::server::Server) -> volo_grpc::server::Server {
        (self.inner)(server)
    }
}

/// 领域 Bundle 接口
pub trait RpcServiceBundle: Send + Sync {
    /// 领域名（日志/诊断）
    fn name(&self) -> &'static str;
    /// 初始化客户端：构建并注册到该领域全局单例
    fn init_client(&self, infra: Arc<GrpcInfrastructure>);
    /// 构建服务端注册闭包
    fn build_server(&self, deps: &ServerDeps) -> ServerRegistration;
}

/// 内置 Bundle 清单（新增领域时此处加一行）
pub fn default_bundles() -> Vec<Box<dyn RpcServiceBundle>> {
    vec![
        Box::new(crate::client::orchestrator::OrchestratorBundle),
        Box::new(crate::client::plugin_data::PluginDataBundle),
    ]
}
```

> **v3·P3 3.4 耦合代价说明**：`ServerDeps` 含 4 字段，每个 Bundle 都收到全量，但 `OrchestratorBundle` 忽略 `data_importer`、`PluginDataBundle` 忽略前 3 个。这是为换取 OCP（factory/server_runner 零改动）付出的合理耦合代价。当前仅 2 领域，引入 `type Deps` 关联类型属过度设计，本期不做；若未来 Bundle 数量增长，可再考虑每 Bundle 自带关联类型。

### 8.2 OCP 验证（扩展成本）

| 操作 | 现状 | 本方案 |
|------|------|--------|
| 新增 gRPC 服务改动文件数 | 5 处（散落修改） | **1 处**（`default_bundles()` 加一行 + 新增领域模块文件） |

新增 `CmxAuditService` 的完整步骤：
1. `cmx-rpc-gen/idl/cmx_audit.proto` + 更新 `volo.yml` / `lib.rs`
2. `cmx-traits/src/rpc/audit.rs` 新增 `AuditClient` trait + `mod.rs` 声明
3. `cmx-rpc/src/client/audit.rs` 新增 `AuditGrpcClient` + `AuditBundle` + 领域全局
4. `cmx-rpc/src/server/audit.rs` 新增 `CmxAuditServerImpl`
5. `cmx-rpc/src/bundle.rs` 的 `default_bundles()` **加一行** `Box::new(AuditBundle)`

`factory/global/server_runner` **零改动**。

---

## 九、cmx-rpc：global / factory / server_runner

### 9.1 `lib.rs`

```rust
pub mod bundle;
pub mod client;
pub mod config;
pub mod discover;
pub mod error;
pub mod factory;
pub mod global;
pub mod server;
pub mod server_runner;

// 领域客户端访问器（调用方入口）
pub use client::orchestrator_client;
pub use client::plugin_data_client;
// 共享类型
pub use config::{GrpcConfig, HttpRestConfig, RpcConfig};
pub use discover::RegistryAwareDiscover;
pub use error::RpcFrameworkError;
pub use cmx_traits::plugin::PluginDataImporter;
pub use factory::{init_rpc_clients, ClientInitError};
pub use global::{GlobalRpcClient, GlobalRpcClientAlreadySetError};
pub use server_runner::start_grpc_server;
```

### 9.2 `global.rs`（简化为初始化状态守卫）

```rust
//! 全局 RPC 初始化状态守卫。
//!
//! 领域客户端各自维护 OnceLock 全局（见各领域模块）。
//! GlobalRpcClient 仅跟踪整体初始化状态，提供 is_initialized 守卫。

use std::sync::OnceLock;

#[derive(thiserror::Error, Debug)]
#[error("GlobalRpcClient 已初始化")]
pub struct GlobalRpcClientAlreadySetError;

pub struct GlobalRpcClient;
static INITIALIZED: OnceLock<()> = OnceLock::new();

impl GlobalRpcClient {
    /// 标记已初始化（由 init_rpc_clients 调用）
    pub(crate) fn mark_initialized() -> Result<(), GlobalRpcClientAlreadySetError> {
        INITIALIZED.set(()).map_err(|_| GlobalRpcClientAlreadySetError)
    }

    pub fn is_initialized() -> bool { INITIALIZED.get().is_some() }
}
```

### 9.3 `factory.rs`

```rust
//! RPC 客户端工厂：迭代 default_bundles() 初始化各领域客户端。

use std::sync::Arc;
use cmx_registry_config::registry::{ServiceInstanceCache, ServiceRegistry};
use cmx_traits::rpc::RpcError;
use crate::bundle::default_bundles;
use crate::client::infra::GrpcInfrastructure;
use crate::config::RpcConfig;
use crate::global::{GlobalRpcClient, GlobalRpcClientAlreadySetError};

#[derive(thiserror::Error, Debug)]
pub enum ClientInitError {
    #[error(transparent)] Rpc(#[from] RpcError),
    #[error(transparent)] AlreadySet(#[from] GlobalRpcClientAlreadySetError),
}

/// 初始化全部内置领域客户端（OCP：不关心具体领域）
pub fn init_rpc_clients(
    config: &RpcConfig,
    cache: Arc<ServiceInstanceCache>,
    registry: Arc<dyn ServiceRegistry>,
) -> Result<Vec<Box<dyn crate::bundle::RpcServiceBundle>>, ClientInitError> {
    if config.protocol != "grpc" {
        return Err(ClientInitError::Rpc(RpcError::UnsupportedProtocol(config.protocol.clone())));
    }
    let infra = Arc::new(GrpcInfrastructure::new(cache, config.grpc.clone(), registry));
    let bundles = default_bundles();
    for b in &bundles {
        b.init_client(infra.clone());
    }
    GlobalRpcClient::mark_initialized()?;
    Ok(bundles)
}
```

### 9.4 `server_runner.rs`

```rust
//! gRPC 服务启动器：迭代 bundles 注册服务端。

use std::sync::Arc;
use volo::net::incoming::DefaultIncoming;
use crate::bundle::{RpcServiceBundle, ServerDeps};
use crate::error::RpcFrameworkError;
use tracing::instrument;

#[instrument(target = "cmx_rpc", skip(bundles, deps, ready_tx), fields(port = port))]
pub async fn start_grpc_server(
    port: u16,
    bundles: Vec<Box<dyn RpcServiceBundle>>,
    deps: ServerDeps,
    ready_tx: tokio::sync::oneshot::Sender<()>,
) -> Result<(), RpcFrameworkError> {
    let addr: std::net::SocketAddr = format!("[::]:{port}").parse()
        .map_err(|e: std::net::AddrParseError| RpcFrameworkError::ServerStartFailed(e.to_string()))?;

    // OCP：fold 迭代 bundles，每个 bundle 把自己的 service 加到 server
    let server = bundles.into_iter().fold(
        volo_grpc::server::Server::new(),
        |server, bundle| bundle.build_server(&deps).apply(server),
    );

    let listener = tokio::net::TcpListener::bind(addr).await
        .map_err(|e| RpcFrameworkError::ServerStartFailed(format!("端口绑定失败: {}", e)))?;
    tracing::info!(target: "cmx_rpc", port, local_addr = ?listener.local_addr(), "gRPC 端口绑定成功");

    let _ = ready_tx.send(()); // 失败已 warn，保持原行为
    let incoming = DefaultIncoming::from(listener);
    server.run(incoming).await
        .map_err(|e| RpcFrameworkError::ServerStartFailed(e.to_string()))?;
    Ok(())
}
```

---

## 十、调用方改造（P2 3.4 补 init_rpc 伪代码）

### 10.1 `web/web-server/src/config/rpc.rs`

```rust
pub async fn init_rpc(
    service_invoker: Arc<dyn ServiceInvoker>,
    runtime_invoker: Arc<dyn RuntimeInvoker>,
    plugin_query: Arc<dyn PluginQuery>,
    data_importer: Option<Arc<dyn PluginDataImporter>>,
) -> crate::Result<Option<u16>> {
    let rpc = match load_rpc_config() {
        Some(cfg) if cfg.enabled && cfg.protocol == "grpc" => cfg,
        Some(cfg) if cfg.enabled => { warn!(...); return Ok(None); }
        _ => { info!("RPC 未启用"); return Ok(None); }
    };

    let cache = GlobalServiceInstanceCache::get().clone();
    let registry = GlobalServiceRegistry::get().clone();

    // 1. 初始化客户端（迭代 bundles）
    let bundles = cmx_rpc::init_rpc_clients(&rpc, cache, registry.clone())?;
    let grpc_port = rpc.grpc.port;

    // 2. 组装 ServerDeps + 启动 server（迭代 bundles）
    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
    let deps = cmx_rpc::bundle::ServerDeps {
        service_invoker, runtime_invoker, plugin_query, data_importer,
    };
    let handle = tokio::spawn(async move {
        match cmx_rpc::start_grpc_server(grpc_port, bundles, deps, ready_tx).await {
            Ok(()) => info!("gRPC Server 已正常退出"),
            Err(e) => warn!("gRPC Server 运行失败: {}", e),
        }
    });

    // 3. 等待就绪（保持原 3s 超时逻辑）
    match tokio::time::timeout(Duration::from_secs(3), ready_rx).await {
        Ok(Ok(())) => info!("gRPC Server 启动成功"),
        Ok(Err(_)) => { handle.abort(); return Err(Error::ServerSetup("gRPC Server 启动失败".into())); }
        Err(_) => { handle.abort(); return Err(Error::ServerSetup("gRPC Server 启动超时".into())); }
    }

    // 4. 缓存预热（保持原逻辑）
    if !rpc.warmup_services.is_empty() { /* ... 不变 ... */ }

    Ok(Some(grpc_port))
}
```

### 10.2 其他调用方（访问路径变更）

| 文件 | 旧 | 新 |
|------|----|----|
| `cmx-api/.../service/handler.rs` | `GlobalRpcClient::get().call_service(...)` | `cmx_rpc::orchestrator_client().call_service(...)` |
| 同上 | `...call_function(...)` | `cmx_rpc::orchestrator_client().call_function(...)` |
| `cmx-plugin/.../host_functions.rs` | `GlobalRpcClient::get().call_function(...)` | `cmx_rpc::orchestrator_client().call_function(...)` |
| `cmx-plugin/.../grpc_sender.rs` | `rpc_client.import_plugin_data(...)` | `cmx_rpc::plugin_data_client().import_plugin_data(...)` |
| 同上 | `rpc_client.cleanup_plugin_data(...)` | `cmx_rpc::plugin_data_client().cleanup_plugin_data(...)` |

`is_initialized` 守卫不变：`GlobalRpcClient::is_initialized()`。

---

## 十一、设计债（仅文档化，本期不处理）

### 11.1 超时语义二义性（评估 3.5）

`config.timeout_ms` 同时用作：
- 单次 gRPC `rpc_timeout`（volo 层，单次调用超时）
- `with_retry` 的总重试预算（deadline）

**后果**：首次慢调用一旦耗时接近 `timeout_ms`，deadline 立即耗尽，**重试永不触发**。重试实际仅对"快速失败的可重试错误（如 UNAVAILABLE 立即返回）"有效。

**本期处理**：保持现状，已在 `GrpcInfrastructure::timeout_ms()` 文档注明。

**未来路线**：拆分为 `rpc_timeout_ms`（单次，默认 5000）与 `retry_total_budget_ms`（总预算，默认 = rpc_timeout_ms × (retry_count + 1)）。需同步更新 `config_template.toml` / `CONFIG_MANUAL.md` 及加载逻辑，并补充集成测试验证慢调用重试生效。作为独立任务推进。

### 11.2 全局状态生命周期

`OnceLock` 只能 set 不能 replace；`start_grpc_server` 无 graceful shutdown。本期不引入运行时注册/反注册/健康检查。

---

## 十二、实施步骤（推荐顺序）

1. **cmx-traits**：拆 `error.rs` / `types.rs` / `orchestrator.rs` / `plugin_data.rs`，更新 `mod.rs`，**删除 RpcClient**
2. **cmx-biz**：暴露 `parse_step_status`（P 4.3），与 `step_status_to_str` 收拢
3. **cmx-rpc/client**：新建 `client/` 目录，`infra.rs` / `retry.rs`(含单测) / `orchestrator.rs` / `plugin_data.rs` / `mod.rs`；删除旧 `client.rs`
4. **cmx-rpc/server**：新建 `server/` 目录，`orchestrator.rs` / `plugin_data.rs` / `mod.rs`；删除旧 `server.rs`
5. **cmx-rpc/bundle.rs** + `global.rs` + `factory.rs` + `server_runner.rs` + `lib.rs`
6. **调用方**：web-server/config/rpc.rs + cmx-api/handler + cmx-plugin(2 处)
7. **验证**：
   - `rtk cargo check` 编译通过
   - `rtk cargo clippy` 无新增警告
   - `rtk cargo test -p cmx-rpc` 含 retry 单测通过
   - **验收标准（P1 2.3 + v3·P1 3.1）**：对比新旧日志，`call_service`/`call_function` 的 `service_name/service_key/elapsed_us/attempts/success` 字段在**成功路径与失败路径均零丢失**（不仅对比成功路径）；失败路径的 warn 日志由调用方带业务字段记录。
   - **验收标准（v3·P3 3.2）**：确认 `call_service` 与 `call_function` 成功日志的 `success` 字段均为业务 success（`resp.success`/`inner.success`），口径一致；PR 描述标注此行为变更。
   - **验收标准（v3·P3 2.1）**：`rtk cargo clippy` 确认 `orchestrator.rs` 无 `unused_imports` 警告（`RetryStats` 不导入）。
   - **行为微调提示（v3·P3 2.2）**：PR 描述标注中间重试 warn 日志不再带 `service_name/service_key`，建议下游告警改基于最终成功/失败日志聚合。

---

## 十三、假设与决策

| # | 决策点 | 选择 | 理由 |
|---|--------|------|------|
| 1 | RpcClient trait 处理 | 一次性删除 | 评估 2.1：访问路径必改，deprecated 别名零收益 |
| 2 | 领域客户端访问方式 | 领域全局访问器（`orchestrator_client()` / `plugin_data_client()`） | 配合 Bundle 实现 OCP，调用方零耦合复合 struct |
| 3 | OCP 实现方式 | Bundle 模式（`RpcServiceBundle` + `default_bundles()`） | 用户决策；扩展成本从 5 处降到 1 处 |
| 4 | Discover 缓存层级 | `GrpcInfrastructure` 共享 | 避免重复订阅；网络 IO 在写锁外 |
| 5 | 重试逻辑 | `with_retry` 泛型 + `RetryStats` | 消除重复；日志字段零丢失 |
| 6 | 超时配置 | 本期不拆分，仅文档化 | 用户决策；拆分会改变运行时行为，独立任务 |
| 7 | import 重试 | 不启用，文档化理由 | 大包重试副作用 + 上层任务级重试兜底 |
| 8 | step_status 序列化 | 统一到 cmx-biz | 消除客户端/服务端不一致 |
| 9 | 命名约定 | 客户端 `XxxGrpcClient`，服务端 `CmxXxxServerImpl`，Bundle `XxxBundle` | 固化一致性 |
| 10 | 失败日志归属（v3·P1 3.1） | `with_retry` 返回 `(RpcError, RetryStats)`，失败日志交还调用方 | 泛型函数拿不到业务上下文；失败路径业务字段零丢失 |
| 11 | success 字段语义（v3·P3 3.2） | 两方法统一为业务 success | 更利于按错误率告警；PR 标注行为变更 |
| 12 | ServerDeps 过度供给（v3·P3 3.4） | 本期保持全量供给 | OCP 的合理耦合代价；2 领域下关联类型属过度设计 |
| 13 | RetryStats 导入（v3·P3 2.1） | 仅导入 `with_retry`，不导入 `RetryStats` 类型名 | 元组解构自动推断类型；避免 `unused_imports` 警告 |
| 14 | 中间重试日志字段（v3·P3 2.2） | 保留取舍：仅记 `attempt/max_retries/error` | 业务关联性弱，最终日志聚合；PR 标注行为微调 |
