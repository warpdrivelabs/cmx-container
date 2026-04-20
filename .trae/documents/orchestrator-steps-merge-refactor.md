# 编排器结果集合并改造计划

## 一、现状分析

### 当前数据结构

```
OrchestrationResult
├── success: bool
├── output: Option<Value>
├── steps: Vec<ExecutionStep>          ← 所有已执行步骤
├── total_elapsed_us: u64
└── error: Option<OrchestrationError>
    ├── message: String
    └── failed_step: Option<FailedStepInfo>  ← 失败步骤详情（冗余）
        ├── node_id
        ├── node_name
        ├── node_type
        ├── step_index
        ├── error
        └── previous_output
```

### 存在的问题

1. **信息冗余**：`FailedStepInfo` 中的 `node_id`、`node_name`、`node_type`、`error` 与 `ExecutionStep` 中的字段重复
2. **信息割裂**：消费者需要同时查看 `steps` 和 `error.failed_step` 才能获取完整信息
3. **`ExecutionStep` 缺少 `previous_output`**：排错关键信息只存在于 `FailedStepInfo` 中，`ExecutionStep` 自身不携带

### 改造目标

将 `failed_step` 的独有信息（`previous_output`）下沉到 `ExecutionStep` 中，使每个 step 自包含所有信息，简化 `OrchestrationError` 结构。

## 二、改造方案

### 改造后的数据结构

```
OrchestrationResult
├── success: bool
├── output: Option<Value>
├── steps: Vec<ExecutionStep>              ← 统一结果集（包含失败步骤的完整信息）
├── total_elapsed_us: u64
└── error: Option<OrchestrationError>      ← 简化：仅保留 message
    └── message: String
```

**`ExecutionStep` 新增字段：**
- `previous_output: Option<serde_json::Value>` — 上一步输出（失败时便于排错）

**`FailedStepInfo` 结构体：删除**

**`OrchestrationError` 结构体：简化**（移除 `failed_step` 字段）

## 三、影响范围

### 需要修改的文件

| 文件 | 改动内容 |
|------|---------|
| `crates/libs/cmx-service/src/orchestrator/types.rs` | 修改 `ExecutionStep`（新增 `previous_output`）、简化 `OrchestrationError`（移除 `failed_step`）、删除 `FailedStepInfo` |
| `crates/libs/cmx-service/src/orchestrator/executor.rs` | 修改 `build_error_info()`（不再构建 `FailedStepInfo`）；节点未找到/事务失败/未知节点类型的错误处理中移除 `failed_step` |
| `crates/libs/cmx-service/src/lib.rs` | 导出列表中移除 `FailedStepInfo` |
| `crates/libs/cmx-api/src/handlers/service/models.rs` | `ServiceExecutionStep` 新增 `previous_output`、`ServiceOrchestrationError` 移除 `failed_step`、删除 `ServiceFailedStepInfo` |
| `crates/libs/cmx-api/src/handlers/service/handler.rs` | 响应映射逻辑中移除 `failed_step` 相关代码，`ServiceExecutionStep` 新增 `previous_output` 映射 |

### 涉及的结构体变更汇总

#### 1. `ExecutionStep`（types.rs）— 新增字段

```rust
pub struct ExecutionStep {
    pub node_id: String,
    pub node_name: String,
    pub node_type: String,
    pub status: StepStatus,
    pub output: Option<serde_json::Value>,
    pub elapsed_us: u64,
    pub error: Option<String>,
    // ↓ 新增：上一步的输出（失败时便于排错，成功时为 None）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_output: Option<serde_json::Value>,
}
```

#### 2. `OrchestrationError`（types.rs）— 简化

```rust
pub struct OrchestrationError {
    // 移除 failed_step 字段
    pub message: String,
}
```

#### 3. `FailedStepInfo`（types.rs）— 删除整个结构体

#### 4. `ServiceExecutionStep`（models.rs）— 新增字段

```rust
pub struct ServiceExecutionStep {
    pub node_id: String,
    pub node_name: String,
    pub node_type: String,
    pub status: String,
    pub output: Option<serde_json::Value>,
    pub elapsed_us: u64,
    pub error: Option<String>,
    // ↓ 新增
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_output: Option<serde_json::Value>,
}
```

#### 5. `ServiceOrchestrationError`（models.rs）— 简化

```rust
pub struct ServiceOrchestrationError {
    // 移除 failed_step 字段
    pub message: String,
}
```

#### 6. `ServiceFailedStepInfo`（models.rs）— 删除整个结构体

## 四、实施步骤

### 步骤 1：修改 `ExecutionStep` 类型定义（types.rs）
- 为 `ExecutionStep` 新增 `previous_output: Option<serde_json::Value>` 字段（带 `skip_serializing_if`）
- 简化 `OrchestrationError`：移除 `failed_step` 字段
- 删除 `FailedStepInfo` 结构体定义

### 步骤 2：修改 `build_error_info()` 方法（executor.rs）
- `build_error_info()` 不再构建 `FailedStepInfo`，仅返回 `OrchestrationError { message }`
- 失败步骤的 `previous_output` 信息改由 `node_handler.rs` 中记录到 `ExecutionStep` 中

### 步骤 3：修改 executor.rs 中的错误处理
- 节点未找到（L184-188）：`OrchestrationError { failed_step: None, message }` → `OrchestrationError { message }`
- 事务管理失败（L205-208）：同上
- 未知节点类型（L354-358）：同上

### 步骤 4：修改 node_handler.rs 中 ExecutionStep 的构建
- 在构建 `ExecutionStep` 时，如果节点执行失败，设置 `previous_output` 字段
- 需要将 `previous_output` 传入 `execute_node` 方法或在构建 step 时从上下文获取

### 步骤 5：修改 lib.rs 导出
- 从 `pub use` 中移除 `FailedStepInfo`

### 步骤 6：修改 API 层模型（models.rs）
- `ServiceExecutionStep` 新增 `previous_output` 字段
- `ServiceOrchestrationError` 移除 `failed_step` 字段
- 删除 `ServiceFailedStepInfo` 结构体

### 步骤 7：修改 API 层映射（handler.rs）
- `execute_service_inner` 中 `result.steps.into_iter().map(...)` 增加 `previous_output` 映射
- `result.error.map(...)` 中移除 `failed_step` 映射
- 移除 `ServiceFailedStepInfo` 的 import

### 步骤 8：编译验证
- `cargo check` 确认无编译错误
- `cargo clippy` 检查代码质量

## 五、风险评估

| 风险 | 影响 | 缓解措施 |
|------|------|---------|
| API 响应格式变更 | 前端/调用方需要适配 | `previous_output` 使用 `skip_serializing_if`，成功时不会出现该字段；`error.failed_step` 移除需通知调用方 |
| `FailedStepInfo` 删除 | 其他模块可能引用 | 全局搜索确认仅 orchestrator 和 api 层使用 |
| `node_handler.rs` 需要获取 `previous_output` | 需要传递上下文 | 在 `execute_node` 签名中传入或从 `ExecutionContext` 获取 |

## 六、改造前后 JSON 响应对比

### 改造前（失败时）

```json
{
  "success": false,
  "output": null,
  "steps": [
    { "node_id": "n1", "status": "Success", "output": {...}, "error": null },
    { "node_id": "n2", "status": "Failed", "output": null, "error": "WASM trap..." }
  ],
  "total_elapsed_us": 12345,
  "error": {
    "failed_step": {
      "node_id": "n2",
      "node_name": "处理数据",
      "node_type": "skylake-func",
      "step_index": 1,
      "error": "WASM trap...",
      "previous_output": { "result": "上一步数据" }
    },
    "message": "步骤 [处理数据(n2)] 执行失败: WASM trap..."
  }
}
```

### 改造后（失败时）

```json
{
  "success": false,
  "output": null,
  "steps": [
    { "node_id": "n1", "status": "Success", "output": {...}, "error": null },
    {
      "node_id": "n2",
      "status": "Failed",
      "output": null,
      "error": "WASM trap...",
      "previous_output": { "result": "上一步数据" }
    }
  ],
  "total_elapsed_us": 12345,
  "error": {
    "message": "步骤 [处理数据(n2)] 执行失败: WASM trap..."
  }
}
```

**关键改进**：失败步骤的 `previous_output` 直接嵌入到 `steps` 数组的对应 step 中，无需再从 `error.failed_step` 获取，信息更加内聚。
