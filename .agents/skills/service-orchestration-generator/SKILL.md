---
name: service-orchestration-generator
description: 生成/编辑服务编排 Flow JSON（基于 Flow JSON 规范，5 种 skylake-* 节点、事务框、switch 多分支、画布布局公式）。当用户要求创建或修改服务编排流程 / 生成编排图 / 编排 JSON，或提到 skylake-start、skylake-func、skylake-switch、skylake-transaction、服务编排、Flow JSON、编排 servicedata 时必用。
---

# Service Orchestration JSON Generator

根据 Flow JSON 规范生成服务编排 JSON。用于创建、编辑服务编排流程定义。

## 核心概念

### 节点类型

| 类型 | 说明 | 特殊字段 |
|------|------|----------|
| `skylake-start` | 开始节点 | 无（流程入口标识） |
| `skylake-end` | 结束节点 | 无（流程出口标识） |
| `skylake-transaction` | 事务框 | 包裹子节点在同一事务中执行 |
| `skylake-switch` | 多分支节点 | `options`: 函数的可能返回值列表 |
| `skylake-func` | 函数执行节点 | `nodeMeta`: 包含 `pluginId`、`functionName` 等 |

### 边 (Edge) 连接规则

| 源节点类型 | 源端口 ID | 说明 |
|------------|-----------|------|
| `skylake-start` | `out` | 固定端口，连接到下一个节点 |
| `skylake-func` | `out` | 固定端口，连接到下一个节点 |
| `skylake-switch` | `out_{option值}` | **动态端口**，根据函数返回值选择分支 |
| `skylake-transaction` | **不出现在 edges 中** | 事务框是容器节点，不参与边的连接 |
| `skylake-end` | 无 | 终止流程，不需要出边 |

### skylake-transaction 事务框与 edges 的关系

**重要**：事务框节点本身不出现在 edges 数组中！

- 事务框的入口边：外部节点 → 事务框内第一个子节点
- 事务框的出口边：事务框内最后一个子节点 → 外部节点
- 事务框内部：子节点之间的连接照常使用 edges

**示例**：
```
外部节点 branch_process → [tx_insert] → [tx_update] → [final_process]
```

对应 edges：
```json
"edges": [
  { "sourceNodeID": "branch_process", "sourcePortID": "out", "targetNodeID": "tx_insert", "targetPortID": "in" },
  { "sourceNodeID": "tx_insert", "sourcePortID": "out", "targetNodeID": "tx_update", "targetPortID": "in" },
  { "sourceNodeID": "tx_update", "sourcePortID": "out", "targetNodeID": "final_process", "targetPortID": "in" }
]
```

**注意**：不要把 `transaction_box` 作为 sourceNodeID 或 targetNodeID 出现在 edges 中！

### 事务生命周期管理

事务框的开启、提交、回滚由 `TransactionManager` 基于 `parent` 属性自动管理，无需手动控制：

| 状态转换 | 触发条件 | 行为 |
|----------|---------|------|
| 无事务 → 无事务 | 节点不在事务框内 | 正常执行，无事务管理 |
| 无事务 → 开启事务 | 节点进入事务框（`parent` 不为空） | 开启新事务 |
| 事务框内继续 | 当前节点与前一节点在同一事务框内 | 复用当前事务 |
| 切换事务框 | 当前节点与前一节点在不同事务框内 | 提交旧事务，开启新事务 |
| 离开事务框 → 提交 | 节点不再属于事务框 | 提交当前事务 |
| 执行失败 | 事务框内节点执行出错 | 回滚当前事务 |

### skylake-switch 多分支节点详解

**核心机制**：`options` 数组定义函数的所有可能返回值，每 个返回值对应一个出边端口。

#### options 与 sourcePortID 的映射关系

```
函数返回 "1"      →  选择出边 sourcePortID = "out_1"
函数返回 "2"      →  选择出边 sourcePortID = "out_2"
函数返回 "success" →  选择出边 sourcePortID = "out_success"
函数返回 "fail"   →  选择出边 sourcePortID = "out_fail"
```

**映射公式**：`sourcePortID = "out_" + options 中的值`

**重要**：switch 节点的函数返回值**必须是字符串类型**，返回值将直接用于拼接出边端口 ID。

#### 示例

假设 switch 节点的 options 配置为：

```json
"options": ["1", "2", "success", "fail"]
```

则 switch 节点需要定义 4 条出边：

```json
"edges": [
  { "sourceNodeID": "switch_node", "sourcePortID": "out_1", "targetNodeID": "branch_1", "targetPortID": "in" },
  { "sourceNodeID": "switch_node", "sourcePortID": "out_2", "targetNodeID": "branch_2", "targetPortID": "in" },
  { "sourceNodeID": "switch_node", "sourcePortID": "out_success", "targetNodeID": "success_handler", "targetPortID": "in" },
  { "sourceNodeID": "switch_node", "sourcePortID": "out_fail", "targetNodeID": "fail_handler", "targetPortID": "in" }
]
```

**重要**：
- `options` 数组中有多少个值，就必须有多少条对应的出边
- 函数实际返回哪个值，就走对应的分支
- 如果函数返回的值不在 options 中，则没有匹配的出边，流程将终止

### 上下文传递机制

编排执行过程中，节点之间通过以下两种机制传递数据：

**1. current_output 链式传递**

上一个节点的输出自动成为下一个节点的输入（`current_output`）。这是主要的数据传递方式，适用于线性流程和分支流程。

**2. step_outputs 缓存**

每个节点执行后，其输出会按 `node_id` 缓存到 `step_outputs` 中。后续任意节点都可以通过 `step_outputs[node_id]` 访问之前任何节点的输出，而不仅仅是上一个节点。

### 数据流设计指导

生成服务编排时，需要考虑节点间的数据传递方式，确保后续业务节点能正确获取所需数据。

#### 数据获取核心原则

当前节点从哪里获取数据，取决于**前序节点的输出是否包含所需字段**：

- **前序节点输出包含所需字段** → 直接使用 `input.input`
- **前序节点输出不含所需字段** → 从 `input.context.initial_input` 或 `input.context.get_step_output("node_id")` 获取

生成代码时，必须分析前序节点的输出结构，选择正确的数据源。

#### switch 节点设计原则

1. switch 节点的函数**只返回路由标识字符串**（如 "1"、"2"、"success"）
2. switch 节点的返回值**不会**传递给下一个节点（执行器自动恢复 current_output）
3. switch 后的节点收到的 `input.input` 与 switch 执行前相同

#### 业务节点数据源选择

生成编排对应的 Rust 代码时，应根据前序节点的输出结构选择数据源：

| 前序节点输出 | 当前节点数据源 | 示例 |
|-------------|--------------|------|
| 包含当前节点所需字段 | `input.input` | validate 透传了业务数据 → save 直接用 |
| 不含当前节点所需字段 | `input.context.initial_input` | merge 只返回合并标志 → 业务操作从 initial_input 取 |
| 需要特定步骤的输出 | `input.context.get_step_output("node_id")` | 最终聚合节点获取各步骤输出 |

#### 代码生成提示

```rust
// switch 节点：只返回路由值
pub fn route_check(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
    let route = input.input.get("route").and_then(|v| v.as_str()).unwrap_or("1");
    Ok(FunctionOutput::from_json(serde_json::to_value(route)?))
}

// 前序节点输出不含业务字段：从 initial_input 获取
pub fn tx_create(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
    let request: BusinessRequest = serde_json::from_value(
        input.context.initial_input.clone()
    ).map_err(|e| e.to_string())?;
    // ...
}

// 前序节点输出包含业务字段：直接使用 input.input
pub fn save(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
    let request: SaveRequest = serde_json::from_value(input.input.clone())
        .map_err(|e| e.to_string())?;
    // ...
}

// 最终聚合节点：使用 get_step_output
pub fn final_process(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
    let step_a = input.context.get_step_output("node_a");
    let step_b = input.context.get_step_output("node_b");
    // ...
}
```

### 固定尺寸配置

| 节点类型 | width | height |
|----------|-------|---------|
| `skylake-start` | 240 | 74 |
| `skylake-end` | 240 | 74 |
| `skylake-func` | 240 | 98 |
| `skylake-switch` | 240 | 211 |
| `skylake-transaction` | **动态** | **动态** |

### 位置 (position) 布局规则

#### 普通节点 position 规则

`position` 定义节点的左上角坐标 `{x, y}`，**确保每个节点的位置不重合**。

**布局原则**：
- 节点按执行顺序从左到右排列
- x 值递增（左边节点 x 小，右边节点 x 大）
- y 值表示垂直位置（同一垂直线上 y 值相近）
- 节点之间保持适当间距，避免重叠

**示例布局**（从左到右）：

```
x: -330      x: 55        x: 390         x: 825         x: 1200
   [start] → [switch] → [func_1] → [func_2] → [end]
              (y:-410)   (y:-455)   (y:-455)     (y:-455)
```

#### skylake-transaction 事务框 position 规则

**事务框是容器节点**，需要将所有子节点包含在内。

**计算公式**：
```
事务框 width = 最右子节点 position.x + 最右子节点 width - 最左子节点 position.x + 边距
事务框 height = 最下子节点 position.y + 最下子节点 height - 最上子节点 position.y + 边距
事务框 position.x = 最左子节点 position.x - 左边距
事务框 position.y = 最上子节点 position.y - 上边距
```

**示例**：
假设事务框内有 4 个子节点：
- tx_insert: `{x: 1170, y: -430, width: 240, height: 98}`
- tx_update: `{x: 1590, y: -357, width: 240, height: 98}`
- tx_query: `{x: 1185, y: -114, width: 240, height: 98}`
- tx_delete: `{x: 1605, y: -114, width: 240, height: 98}`

计算：
```
最左 x = 1170
最右 = 1170 + 240 = 1410
最上 y = -430
最下 = -114 + 98 = -16

事务框 width = 1590 + 240 - 1170 + 80 = 740  (约810)
事务框 height = -16 - (-430) + 80 = 494 (约645)

事务框 position.x = 1170 - 30 = 1140  (约1095)
事务框 position.y = -430 - 30 = -460  (约-522)
```

## JSON 结构

```json
{
  "name": "编排名称",
  "code": "服务唯一标识key",
  "description": "编排描述",
  "flow": {
    "nodes": [
      {
        "id": "节点唯一ID",
        "type": "skylake-start|skylake-end|skylake-transaction|skylake-switch|skylake-func",
        "parent": "父节点ID（仅事务框内的子节点需要）",
        "meta": {
          "zIndex": 1,
          "size": { "width": 240, "height": 74 },
          "position": { "x": 0, "y": 0 }
        },
        "data": {
          "name": "节点显示名称",
          "inputs": [],
          "outputs": [],
          "nodeMeta": { },
          "options": ["返回值1", "返回值2"]
        }
      }
    ],
    "edges": [
      {
        "sourceNodeID": "源节点ID",
        "sourcePortID": "out|out_1|out_xxx",
        "targetNodeID": "目标节点ID",
        "targetPortID": "in"
      }
    ]
  }
}
```

## 节点模板与画布布局

5 种 skylake-* 节点（start/end/func/switch/transaction）的完整 JSON 模板、固定尺寸配置、position 布局公式见 [references/node-templates.md](references/node-templates.md)。

## 完整示例

两个端到端示例（分支处理流程 / 含事务框的完整流程，含需求→生成逻辑→布局规划→完整 JSON）见 [references/full-examples.md](references/full-examples.md)。


## 重要提示

1. **节点 ID 必须唯一**，建议使用有意义的命名如 `start_1`, `end_1`, `func_check`, `branch_1` 等
2. **边连接要完整**，确保每个节点的出边都正确连接到下一个节点
3. **switch 节点的 options 与 sourcePortID 必须一一对应**：
   - `options: ["A", "B"]` → 必须有 `out_A` 和 `out_B` 两条出边
   - 映射公式：`sourcePortID = "out_" + options中的值`
4. **position 仅供可视化参考**，确保节点不重叠即可
5. **zIndex 用于图层顺序**，数字大的在上层
6. **事务框内的子节点必须设置 `parent` 字段**
7. **事务框的 size 是动态的**，必须包含所有子节点
8. **事务框的 position** 需根据子节点位置计算，确保完整包裹
9. **事务框节点不出现在 edges 中**：
   - 外部节点直接连接到事务框内的第一个子节点
   - 事务框内最后一个子节点直接连接到外部节点
   - 不要使用 `transaction_box` 作为 sourceNodeID 或 targetNodeID
10. **pluginId 命名约束**：`nodeMeta.pluginId` 只能使用下划线 `_` 分隔，禁止使用连字符 `-`（如 `my_plugin`，不能写成 `my-plugin`）
11. **databaseId 规范**：事务框（`skylake-transaction`）的 `nodeMeta.databaseId` 必须使用插件 `manifest.json` 中 `plugin.datasource_id` 的值，确保事务操作使用插件关联的数据源
12. **数据流设计**：生成代码时必须分析前序节点的输出结构，判断 `input.input` 是否包含当前节点所需字段。包含则直接使用 `input.input`，不包含则从 `input.context.initial_input` 或 `input.context.get_step_output("node_id")` 获取
