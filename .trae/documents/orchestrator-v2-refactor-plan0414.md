# Orchestrator 编排器重构方案

## 一、现状分析

### 1.1 当前文件结构

```
cmx-service/src/
├── orchestrator_v2.rs   (857行，单文件承载所有编排逻辑)
├── error.rs             (ServiceError 错误类型)
├── request.rs           (InvokeRequest/InvokeResponse)
├── handler.rs           (ServiceHandler)
├── service.rs           (CmxService 核心服务)
├── lib.rs               (模块导出)
└── ...其他模块
```

### 1.2 orchestrator_v2.rs 内部分布（857行）

| 区域 | 行数 | 职责 |
|------|------|------|
| 结果结构体（OrchestrationResultV2, ExecutionStep, ExecutionContext） | ~45行 | 类型定义 |
| OrchestratorV2 结构体 + 构造函数 | ~40行 | 编排器主体 |
| `execute_service()` 主循环 | ~330行 | 核心执行循环（含事务管理+节点分发） |
| `execute_switch_node()` | ~120行 | switch 节点执行 |
| `execute_func_node()` | ~100行 | func 节点执行 |
| `execute_transaction_node()` | ~80行 | 事务框执行 |

### 1.3 核心问题

1. **代码重复**：`execute_func_node` 和 `execute_switch_node` 有 ~90% 重复代码（插件检查→WASM加载→构建输入→调用→解析输出→更新上下文→记录步骤），仅日志前缀 `[func]`/`[switch]` 不同
2. **错误处理粗糙**：执行失败时只 `break` 退出循环，最终只返回一个简单的 `success: false`，无法知道：
   - 哪个步骤失败了
   - 失败前的步骤结果是什么
   - 失败的具体原因和上下文
3. **无法控制 steps 返回**：始终返回所有步骤数据，无法按需控制（调试时需要详细信息，生产环境可能只需要最终结果）
4. **单文件过长**：857行代码包含类型定义、编排器逻辑、节点执行、事务管理、流程导航等多个关注点
5. **命名带 V2 后缀**：编排器只有一个版本，不应有 V2 后缀

---

## 二、重构目标

从**企业级编排框架**的角度出发，实现：

1. **模块化**：将 orchestrator_v2.rs 拆分为目录模块，每个子模块职责单一
2. **去除 V2 命名**：统一命名为 `Orchestrator`、`OrchestrationResult`、`orchestrator/`
3. **错误可追溯**：失败时携带完整的步骤上下文信息（哪个步骤失败、前面步骤的结果、失败原因）
4. **按需返回 steps**：通过参数控制是否返回 steps 数据
5. **消除重复代码**：提取共享的 WASM 调用逻辑
6. **性能无损失**：重构不引入额外运行时开销

---

## 三、模块拆分方案

### 3.1 目标目录结构

```
cmx-service/src/
├── orchestrator/              # 编排器模块目录（原 orchestrator_v2.rs → orchestrator/）
│   ├── mod.rs                 # 模块入口，导出公共类型
│   ├── types.rs               # 结果和上下文类型定义
│   ├── executor.rs            # Orchestrator 主执行器
│   ├── node_handler.rs        # 节点执行逻辑（统一 invoke + func/switch 分发）
│   ├── flow_navigator.rs      # 流程图导航（边的查找、下一节点定位）
│   └── transaction_manager.rs # 事务状态管理（开启/提交/回滚）
├── orchestrator_v2.rs         # 删除（替换为 orchestrator/ 目录）
├── error.rs                   # 扩展错误类型
├── ...其他文件不变
```

### 3.2 各子模块职责

#### `types.rs` — 类型定义（~80行）

从 orchestrator_v2.rs 提取所有结构体定义，并新增：

```rust
/// 执行步骤状态
pub enum StepStatus {
    /// 执行成功
    Success,
    /// 执行失败
    Failed,
    /// 跳过
    Skipped,
}

/// 执行步骤记录（增强版）
pub struct ExecutionStep {
    pub node_id: String,
    pub node_name: String,
    pub node_type: String,          // 新增：节点类型
    pub status: StepStatus,         // 新增：步骤状态
    pub output: Option<String>,
    pub elapsed_us: u64,
    pub error: Option<String>,      // 新增：步骤级错误信息
}

/// 编排执行结果（原 OrchestrationResultV2）
pub struct OrchestrationResult {
    pub success: bool,
    pub output: Option<String>,
    pub steps: Vec<ExecutionStep>,
    pub total_elapsed_us: u64,
    pub error: Option<OrchestrationError>,  // 新增：结构化错误信息
}

/// 编排错误信息（新增）
pub struct OrchestrationError {
    pub failed_step: Option<FailedStepInfo>,  // 失败步骤详情
    pub message: String,                       // 错误摘要信息
}

/// 失败步骤详情（新增）
pub struct FailedStepInfo {
    pub node_id: String,
    pub node_name: String,
    pub node_type: String,
    pub step_index: usize,           // 失败步骤的序号（从0开始）
    pub error: String,               // 具体错误信息
    pub previous_output: Option<String>,  // 上一步的输出（失败前的数据）
}

/// 执行选项（新增）
pub struct ExecuteOptions {
    /// 是否返回 steps 数据
    /// - false: 仅返回最终结果，steps 为空数组（生产环境推荐，减少数据传输）
    /// - true: 返回所有步骤数据（调试/排错时使用）
    pub include_steps: bool,
}

impl Default for ExecuteOptions {
    fn default() -> Self {
        Self { include_steps: false }
    }
}
```

#### `flow_navigator.rs` — 流程导航（~60行）

提取所有边查找和节点定位逻辑：

```rust
/// 流程导航器
///
/// 负责在 ServiceFlow 中查找节点和边
pub struct FlowNavigator<'a> {
    flow: &'a ServiceFlow,
}

impl<'a> FlowNavigator<'a> {
    pub fn new(flow: &'a ServiceFlow) -> Self;

    /// 查找节点 by ID
    pub fn find_node(&self, node_id: &str) -> Option<&ServiceNode>;

    /// 查找开始节点
    pub fn find_start_node(&self) -> Option<&ServiceNode>;

    /// 查找从指定节点出发、匹配源端口的下一条边
    pub fn find_next_edge(&self, source_node_id: &str, source_port: &str) -> Option<&ServiceEdge>;

    /// 查找事务框节点的数据库ID
    pub fn resolve_transaction_db_id(&self, txn_node_id: &str, default_db_id: &str) -> String;
}
```

#### `transaction_manager.rs` — 事务管理（~120行）

提取 `execute_service()` 中散布的事务状态管理逻辑：

```rust
/// 事务状态管理器
///
/// 跟踪当前活跃事务，负责事务的开启、提交和回滚
pub struct TransactionManager {
    /// 当前活跃的事务守卫
    active_guard: Option<TransactionGuard>,
    /// 当前活跃事务所属的事务框节点ID
    active_parent_id: Option<String>,
    /// 默认数据库ID
    default_db_id: String,
}

impl TransactionManager {
    pub fn new(default_db_id: String) -> Self;

    /// 根据当前节点的 parent 属性管理事务状态
    /// 返回可能的事务操作结果
    pub async fn ensure_transaction(
        &mut self,
        node: &ServiceNode,
        flow: &ServiceFlow,
        svr_context: &mut SVRContext,
    ) -> Result<(), ServiceError>;

    /// 提交当前活跃事务（正常结束时）
    pub async fn commit_active(&mut self, svr_context: &mut SVRContext) -> Result<(), ServiceError>;

    /// 回滚当前活跃事务（异常结束时）
    pub async fn rollback_active(&mut self);

    /// 检查是否有活跃事务
    pub fn has_active(&self) -> bool;
}
```

#### `node_handler.rs` — 节点执行（~150行）

合并 `execute_func_node` 和 `execute_switch_node`，提取共享逻辑：

```rust
/// 节点执行器
///
/// 统一处理 func 和 switch 节点的 WASM 调用逻辑
pub struct NodeHandler<'a> {
    runtime: &'a Arc<dyn RuntimeInvoker>,
    plugin_query: &'a Arc<dyn PluginQuery>,
}

impl<'a> NodeHandler<'a> {
    pub fn new(
        runtime: &'a Arc<dyn RuntimeInvoker>,
        plugin_query: &'a Arc<dyn PluginQuery>,
    ) -> Self;

    /// 统一的节点执行入口
    ///
    /// 合并了原 execute_func_node 和 execute_switch_node 的共享逻辑：
    /// 1. 解析节点元信息
    /// 2. 检查并加载 WASM 模块
    /// 3. 构建 FunctionInput
    /// 4. 调用 WASM 函数
    /// 5. 解析 FunctionOutput
    /// 6. 更新 ExecutionContext
    /// 7. 记录 ExecutionStep
    pub async fn execute_node(
        &self,
        node: &ServiceNode,
        exec_context: &mut ExecutionContext,
        steps: &mut Vec<ExecutionStep>,
        include_steps: bool,
    ) -> Result<(), ServiceError>;
}
```

**关键去重策略**：`execute_func_node` 和 `execute_switch_node` 的差异仅在于日志标签，合并为 `execute_node` 后通过 `node.node_type` 自动区分日志。

#### `executor.rs` — 主执行器（~180行）

精简后的 `execute_service()` 主循环：

```rust
/// 编排执行器
pub struct Orchestrator {
    runtime: Arc<dyn RuntimeInvoker>,
    plugin_query: Arc<dyn PluginQuery>,
    service_query: Arc<dyn ServiceQuery>,
    default_db_id: String,
}

impl Orchestrator {
    pub fn new(...) -> Self;
    pub fn with_db_id(mut self, db_id: impl Into<String>) -> Self;

    /// 执行服务编排（核心入口方法）
    ///
    /// 增强点：
    /// - 接收 ExecuteOptions 参数控制 steps 返回
    /// - 失败时构建结构化的 OrchestrationError
    /// - 委托 TransactionManager 管理事务
    /// - 委托 FlowNavigator 进行流程导航
    /// - 委托 NodeHandler 执行节点
    pub async fn execute_service(
        &self,
        service_key: &str,
        initial_input: &str,
        headers: HashMap<String, String>,
        options: ExecuteOptions,
    ) -> Result<OrchestrationResult, ServiceError>;

    /// 执行事务框节点（内部方法）
    async fn execute_transaction_node(
        &self,
        flow: &ServiceFlow,
        transaction_node: &ServiceNode,
        exec_context: &mut ExecutionContext,
        steps: &mut Vec<ExecutionStep>,
        options: &ExecuteOptions,
    ) -> Result<(), ServiceError>;
}
```

#### `mod.rs` — 模块入口（~30行）

```rust
mod types;
mod executor;
mod node_handler;
mod flow_navigator;
mod transaction_manager;

pub use types::*;
pub use executor::Orchestrator;
```

---

## 四、错误处理增强方案

### 4.1 当前问题

```rust
// 当前：失败时只 break，丢失了步骤上下文
result = self.execute_func_node(node, &mut exec_context, &mut steps).await;
if result.is_err() {
    break;  // 只知道失败了，不知道前面做了什么
}
```

最终返回：
```json
{
    "success": false,
    "output": null,
    "steps": [...],  // 有数据但不知道哪个失败了
    "total_elapsed_us": 12345
}
```

### 4.2 增强方案

**核心思路**：每个步骤都有明确的 `status`（Success/Failed），失败时构建结构化的 `OrchestrationError`，包含失败步骤详情和上一步输出。

**返回示例（失败时）**：
```json
{
    "success": false,
    "output": null,
    "steps": [
        {
            "node_id": "node_1",
            "node_name": "数据校验",
            "node_type": "skylake-func",
            "status": "Success",
            "output": "{\"valid\": true}",
            "elapsed_us": 1200,
            "error": null
        },
        {
            "node_id": "node_2",
            "node_name": "数据保存",
            "node_type": "skylake-func",
            "status": "Failed",
            "output": null,
            "elapsed_us": 3500,
            "error": "运行时调用失败: wasm trap: out of bounds memory access"
        }
    ],
    "total_elapsed_us": 5800,
    "error": {
        "failed_step": {
            "node_id": "node_2",
            "node_name": "数据保存",
            "node_type": "skylake-func",
            "step_index": 1,
            "error": "运行时调用失败: wasm trap: out of bounds memory access",
            "previous_output": "{\"valid\": true}"
        },
        "message": "步骤 [数据保存(node_2)] 执行失败: 运行时调用失败: wasm trap: out of bounds memory access"
    }
}
```

### 4.3 实现要点

在 `executor.rs` 的主循环中：

```rust
// 失败时的处理逻辑（伪代码）
let step_index = steps.len();
let previous_output = if step_index > 0 {
    steps[step_index - 1].output.clone()
} else {
    None
};

// 将失败的步骤也记录到 steps 中
steps.push(ExecutionStep {
    node_id: node.id.clone(),
    node_name: node_data.name.clone(),
    node_type: node.node_type.clone(),
    status: StepStatus::Failed,
    output: None,
    elapsed_us: 0,
    error: Some(err.to_string()),
});

// 构建结构化错误
let orch_error = OrchestrationError {
    failed_step: Some(FailedStepInfo {
        node_id: node.id.clone(),
        node_name: node_data.name.clone(),
        node_type: node.node_type.clone(),
        step_index,
        error: err.to_string(),
        previous_output,
    }),
    message: format!("步骤 [{}({})] 执行失败: {}", node_data.name, node.id, err),
};
```

---

## 五、Steps 按需返回方案

### 5.1 设计

新增 `ExecuteOptions` 结构体，通过 `include_steps` 参数控制：

| 场景 | include_steps | 返回 steps |
|------|--------------|------------|
| 生产环境调用 | `false`（默认） | 空数组 `[]` |
| 调试/前端调试 | `true` | 完整步骤数据 |
| 执行失败 | 不管设置如何 | 始终返回（包含失败步骤） |

**核心原则**：
- **失败时始终返回 steps**：无论 `include_steps` 如何设置，失败时都返回完整的步骤数据（因为排错需要）
- **成功时按需返回**：成功时根据 `include_steps` 决定是否返回 steps
- **零开销**：`include_steps=false` 时，steps 内部仍然记录（用于错误场景），但最终构建结果时清空

### 5.2 API 层变更

**ServiceExecuteRequest 增加字段**：

```rust
// cmx-api/src/handlers/service/models.rs
pub struct ServiceExecuteRequest {
    pub service_key: String,
    pub input: serde_json::Value,
    /// 是否返回步骤数据（可选，默认 false）
    /// - true: 调试模式，返回每个步骤的详细数据
    /// - false: 生产模式，仅返回最终结果
    /// - 注意：执行失败时无论此参数设置如何，都会返回步骤数据
    #[serde(default)]
    pub include_steps: Option<bool>,
}
```

**ServiceExecuteResponse 增加字段**：

```rust
pub struct ServiceExecuteResponse {
    pub success: bool,
    pub output: Option<String>,
    pub steps: Vec<ServiceExecutionStep>,
    pub total_elapsed_us: u64,
    /// 错误详情（失败时包含结构化错误信息）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ServiceOrchestrationError>,
}

/// 服务编排错误信息
pub struct ServiceOrchestrationError {
    pub failed_step: Option<ServiceFailedStepInfo>,
    pub message: String,
}

/// 服务失败步骤详情
pub struct ServiceFailedStepInfo {
    pub node_id: String,
    pub node_name: String,
    pub node_type: String,
    pub step_index: usize,
    pub error: String,
    pub previous_output: Option<String>,
}
```

### 5.3 调用链传递

```
API Handler (ServiceExecuteRequest.include_steps)
    ↓
execute_service_inner (传递 ExecuteOptions)
    ↓
Orchestrator::execute_service (接收 ExecuteOptions)
    ↓
NodeHandler::execute_node (传递 include_steps)
```

---

## 六、error.rs 扩展方案

### 6.1 新增错误变体

```rust
pub enum ServiceError {
    // ...保留现有变体...

    /// 节点执行失败（新增，携带步骤上下文）
    #[error("节点执行失败 [{node_type}] {node_name}({node_id}): {source}")]
    NodeExecutionFailed {
        node_id: String,
        node_name: String,
        node_type: String,
        source: String,
    },

    /// 事务回滚（新增，携带回滚原因）
    #[error("事务回滚: txn_id={txn_id}, reason={reason}")]
    TransactionRolledBack {
        txn_id: String,
        reason: String,
    },
}
```

---

## 七、命名重映射（V2 → 统一命名）

| 原名称 | 新名称 | 说明 |
|--------|--------|------|
| `orchestrator_v2.rs` | `orchestrator/` 目录 | 模块化拆分 |
| `OrchestratorV2` | `Orchestrator` | 去掉 V2 后缀 |
| `OrchestrationResultV2` | `OrchestrationResult` | 去掉 V2 后缀 |
| `ExecutionContext` | `ExecutionContext`（不变） | 名称已经合适 |
| `ExecutionStep` | `ExecutionStep`（不变） | 名称已经合适 |

---

## 八、实施步骤（按顺序执行）

### 步骤 1：创建模块目录和类型定义

1. 删除 `orchestrator_v2.rs` 文件
2. 创建 `orchestrator/` 目录
3. 创建 `orchestrator/types.rs`：
   - 迁移 `OrchestrationResult`（原 OrchestrationResultV2）、`ExecutionStep`、`ExecutionContext`
   - 新增 `StepStatus` 枚举
   - 新增 `OrchestrationError`、`FailedStepInfo` 结构体
   - 新增 `ExecuteOptions` 结构体（Default impl）
   - `ExecutionStep` 增加 `node_type`、`status`、`error` 字段
   - `OrchestrationResult` 增加 `error` 字段
4. 创建 `orchestrator/mod.rs`：导出子模块和公共类型

### 步骤 2：创建流程导航模块

5. 创建 `orchestrator/flow_navigator.rs`：
   - 实现 `FlowNavigator` 结构体
   - 提取 `find_node`、`find_start_node`、`find_next_edge`、`resolve_transaction_db_id` 方法

### 步骤 3：创建事务管理模块

6. 创建 `orchestrator/transaction_manager.rs`：
   - 实现 `TransactionManager` 结构体
   - 提取事务开启、提交、回滚、状态转换逻辑
   - 将 `execute_service()` 中的 match (&active_txn_guard, &node_parent_id) 逻辑封装

### 步骤 4：创建节点执行模块

7. 创建 `orchestrator/node_handler.rs`：
   - 实现 `NodeHandler` 结构体
   - 合并 `execute_func_node` 和 `execute_switch_node` 为 `execute_node`
   - 统一 WASM 调用逻辑（检查→加载→构建输入→调用→解析→更新上下文→记录步骤）

### 步骤 5：创建主执行器模块

8. 创建 `orchestrator/executor.rs`：
   - 迁移 `Orchestrator` 结构体和构造函数
   - 重写 `execute_service()` 主循环，使用委托模式：
     - `FlowNavigator` 负责导航
     - `TransactionManager` 负责事务
     - `NodeHandler` 负责节点执行
   - 增强错误处理：失败时构建 `OrchestrationError`
   - 实现 `include_steps` 控制逻辑
   - 迁移 `execute_transaction_node()` 内部方法

### 步骤 6：更新错误类型

9. 修改 `error.rs`：新增 `NodeExecutionFailed`、`TransactionRolledBack` 变体

### 步骤 7：更新 API 层

10. 修改 `cmx-api/src/handlers/service/models.rs`：
    - `ServiceExecuteRequest` 增加 `include_steps` 字段
    - `ServiceExecuteResponse` 增加 `error` 字段
    - 新增 `ServiceOrchestrationError`、`ServiceFailedStepInfo` 结构体
    - `ServiceExecutionStep` 增加 `node_type`、`status`、`error` 字段

11. 修改 `cmx-api/src/handlers/service/handler.rs`：
    - `execute_service_inner()` 传递 `ExecuteOptions`
    - 响应构建时映射新增字段
    - 将 `OrchestratorV2` 引用改为 `Orchestrator`

### 步骤 8：更新模块导出

12. 修改 `cmx-service/src/lib.rs`：
    - `pub mod orchestrator_v2;` → `pub mod orchestrator;`
    - 更新 `pub use` 导出：`OrchestratorV2` → `Orchestrator`，`OrchestrationResultV2` → `OrchestrationResult`
    - 新增导出 `ExecuteOptions`、`OrchestrationError`、`FailedStepInfo`、`StepStatus`

### 步骤 9：更新所有外部引用

13. 全局搜索并替换所有 `OrchestratorV2` → `Orchestrator`、`OrchestrationResultV2` → `OrchestrationResult` 引用
    - `cmx-api/src/handlers/service/handler.rs`
    - 其他可能引用的文件

### 步骤 10：编译验证

14. `cargo build` 确保所有修改编译通过
15. 检查无编译警告

---

## 九、影响范围

### 9.1 需要修改的文件

| 文件 | 修改内容 |
|------|---------|
| `cmx-service/src/orchestrator_v2.rs` | 删除，替换为 `orchestrator/` 目录 |
| `cmx-service/src/orchestrator/mod.rs` | 新建 |
| `cmx-service/src/orchestrator/types.rs` | 新建 |
| `cmx-service/src/orchestrator/executor.rs` | 新建 |
| `cmx-service/src/orchestrator/node_handler.rs` | 新建 |
| `cmx-service/src/orchestrator/flow_navigator.rs` | 新建 |
| `cmx-service/src/orchestrator/transaction_manager.rs` | 新建 |
| `cmx-service/src/error.rs` | 新增错误变体 |
| `cmx-service/src/lib.rs` | 更新导出，V2→统一命名 |
| `cmx-api/src/handlers/service/models.rs` | 新增字段和结构体 |
| `cmx-api/src/handlers/service/handler.rs` | 传递 ExecuteOptions，V2→统一命名 |

### 9.2 不需要修改的文件

| 文件 | 原因 |
|------|------|
| `cmx-core` 模型层 | 不修改 ServiceNode、SVRContext 等核心模型 |
| `cmx-plugin-sdk` | 不涉及 |
| `cmx-traits` | RuntimeInvoker/PluginQuery/ServiceQuery 接口不变 |
| `cmx-database` | 事务相关接口不变 |

### 9.3 向后兼容性

- **API 兼容**：`ServiceExecuteRequest.include_steps` 为 `Option<bool>`，默认 `None` 等同 `false`，现有调用方无需修改
- **响应兼容**：`ServiceExecuteResponse.error` 使用 `skip_serializing_if = "Option::is_none"`，成功时 JSON 中不出现该字段
- **新增字段**：`ServiceExecutionStep` 中的 `node_type`、`status`、`error` 都是新增字段，不破坏现有 JSON 解析
- **Rust API 兼容**：`OrchestratorV2` → `Orchestrator` 是 breaking change，需更新所有引用处

---

## 十、设计原则总结

1. **单一职责**：每个子模块只负责一个关注点（导航/事务/执行/类型）
2. **委托优于继承**：通过组合 FlowNavigator、TransactionManager、NodeHandler 实现功能
3. **错误即上下文**：失败信息携带完整的执行历史，便于排查
4. **按需暴露**：通过 ExecuteOptions 控制数据暴露粒度
5. **零运行时开销**：include_steps 只影响最终结果构建，不影响执行过程
6. **向后兼容**：所有新增字段都有默认值，现有调用方无感知
7. **命名统一**：去掉 V2 后缀，编排器只有一个版本
