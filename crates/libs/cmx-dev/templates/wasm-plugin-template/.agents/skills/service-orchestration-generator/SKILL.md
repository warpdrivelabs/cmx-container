---
name: "service-orchestration-generator"
description: "Generates service orchestration JSON based on Flow JSON specification. Invoke when user wants to create/edit a service orchestration flow or asks for Flow JSON generation."
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

## 节点模板

### 1. skylake-start 开始节点

```json
{
  "id": "start_1",
  "type": "skylake-start",
  "meta": {
    "zIndex": 1,
    "size": { "width": 240, "height": 74 },
    "position": { "x": -330, "y": -345 }
  },
  "data": {
    "name": "开始节点",
    "inputs": [],
    "outputs": []
  }
}
```

### 2. skylake-end 结束节点

```json
{
  "id": "end_1",
  "type": "skylake-end",
  "meta": {
    "zIndex": 12,
    "size": { "width": 240, "height": 74 },
    "position": { "x": 2280, "y": -237 }
  },
  "data": {
    "name": "结束节点",
    "inputs": [],
    "outputs": []
  }
}
```

### 3. skylake-func 函数节点

```json
{
  "id": "func_node_id",
  "type": "skylake-func",
  "meta": {
    "zIndex": 3,
    "size": { "width": 240, "height": 98 },
    "position": { "x": 390, "y": -455 }
  },
  "data": {
    "name": "函数节点名称",
    "nodeMeta": {
      "pluginId": "插件ID",
      "pluginName": "插件名称",
      "pluginVersion": "1.0.0",
      "functionName": "函数名"
    },
    "inputs": [],
    "outputs": []
  }
}
```

**注意**：`nodeMeta` 中还可以添加可选字段 `"databaseId": "数据源ID"`，用于指定该节点使用的数据库连接。

### 4. skylake-switch 多分支节点

```json
{
  "id": "switch_node_id",
  "type": "skylake-switch",
  "meta": {
    "zIndex": 2,
    "size": { "width": 240, "height": 211 },
    "position": { "x": 55, "y": -410.5 }
  },
  "data": {
    "name": "分支判断节点",
    "nodeMeta": {
      "pluginId": "插件ID",
      "pluginName": "插件名称",
      "pluginVersion": "1.0.0",
      "functionName": "判断函数名"
    },
    "inputs": [],
    "outputs": [],
    "options": ["1", "2", "3", "4"]
  }
}
```

### 5. skylake-transaction 事务框

```json
{
  "id": "transaction_box",
  "type": "skylake-transaction",
  "parent": null,
  "meta": {
    "zIndex": 1,
    "size": { "width": 810, "height": 645 },
    "position": { "x": 1095, "y": -522.5 }
  },
  "data": {
    "name": "事务处理框",
    "nodeMeta": {
      "pluginId": "",
      "pluginName": "",
      "pluginVersion": "",
      "functionName": "",
      "databaseId": "primary"
    },
    "inputs": [],
    "outputs": []
  }
}
```

**事务框内的子节点**需要设置 `parent` 字段指向事务框 ID：

```json
{
  "id": "tx_insert",
  "type": "skylake-func",
  "parent": "transaction_box",
  "meta": {
    "size": { "width": 240, "height": 98 },
    "position": { "x": 1170, "y": -430 }
  },
  ...
}
```

## 使用场景

1. **用户请求创建服务编排**：当用户说"帮我创建一个服务编排"、"生成一个 Flow JSON" 等
2. **编辑现有编排**：当用户提供修改意见时，根据修改内容更新对应节点
3. **理解编排结构**：当用户询问某个编排的执行逻辑时

## 示例：完整流程

### 需求：创建一个分支处理流程

用户需求：开始 → 判断类型(返回 "A"/"B"/"C") → 对应分支处理 → 合并 → 结束

### 生成逻辑分析

1. **开始节点** → 连接 → **类型判断 switch**
2. **switch 节点** 配置 `options: ["A", "B", "C"]`
3. **switch 节点** 需要 3 条出边：
   - `out_A` → **分支A处理**
   - `out_B` → **分支B处理**
   - `out_C` → **分支C处理**
4. **三个分支处理** → 分别连接 → **合并节点**
5. **合并节点** → 连接 → **结束节点**

### 布局规划

```
y:-455 ┌──────────────────────────────────────────────────────────────┐
       │                                                              │
y:-455 │ [start] → [switch] → [process_A] ─┐                        │
       │                  ↓                  │                        │
y:-237 │                  └── [process_B] ──┼──→ [merge] → [end]     │
       │                                      │                        │
y:-16  │                  └── [process_C] ───┘                        │
       │                                                              │
y:-455 └──────────────────────────────────────────────────────────────┘
       x:-330    x:55        x:390            x:825         x:1200
```

### 生成的 JSON

```json
{
  "name": "分支处理流程",
  "code": "branch_process",
  "description": "根据类型选择分支处理",
  "flow": {
    "nodes": [
      {
        "id": "start_1",
        "type": "skylake-start",
        "meta": { "zIndex": 1, "size": { "width": 240, "height": 74 }, "position": { "x": -330, "y": -345 } },
        "data": { "name": "开始节点", "inputs": [], "outputs": [] }
      },
      {
        "id": "type_check",
        "type": "skylake-switch",
        "meta": { "zIndex": 2, "size": { "width": 240, "height": 211 }, "position": { "x": 55, "y": -410.5 } },
        "data": {
          "name": "类型判断",
          "nodeMeta": { "pluginId": "my-plugin", "pluginName": "my-plugin", "pluginVersion": "1.0.0", "functionName": "check_type" },
          "inputs": [], "outputs": [],
          "options": ["A", "B", "C"]
        }
      },
      {
        "id": "process_A",
        "type": "skylake-func",
        "meta": { "zIndex": 3, "size": { "width": 240, "height": 98 }, "position": { "x": 390, "y": -455 } },
        "data": {
          "name": "A类型处理",
          "nodeMeta": { "pluginId": "my-plugin", "pluginName": "my-plugin", "pluginVersion": "1.0.0", "functionName": "handle_type_a" },
          "inputs": [], "outputs": []
        }
      },
      {
        "id": "process_B",
        "type": "skylake-func",
        "meta": { "zIndex": 4, "size": { "width": 240, "height": 98 }, "position": { "x": 420, "y": -237 } },
        "data": {
          "name": "B类型处理",
          "nodeMeta": { "pluginId": "my-plugin", "pluginName": "my-plugin", "pluginVersion": "1.0.0", "functionName": "handle_type_b" },
          "inputs": [], "outputs": []
        }
      },
      {
        "id": "process_C",
        "type": "skylake-func",
        "meta": { "zIndex": 5, "size": { "width": 240, "height": 98 }, "position": { "x": 405, "y": -16 } },
        "data": {
          "name": "C类型处理",
          "nodeMeta": { "pluginId": "my-plugin", "pluginName": "my-plugin", "pluginVersion": "1.0.0", "functionName": "handle_type_c" },
          "inputs": [], "outputs": []
        }
      },
      {
        "id": "merge_result",
        "type": "skylake-func",
        "meta": { "zIndex": 6, "size": { "width": 240, "height": 98 }, "position": { "x": 825, "y": -212 } },
        "data": {
          "name": "合并结果",
          "nodeMeta": { "pluginId": "my-plugin", "pluginName": "my-plugin", "pluginVersion": "1.0.0", "functionName": "merge" },
          "inputs": [], "outputs": []
        }
      },
      {
        "id": "end_1",
        "type": "skylake-end",
        "meta": { "zIndex": 7, "size": { "width": 240, "height": 74 }, "position": { "x": 1200, "y": -237 } },
        "data": { "name": "结束节点", "inputs": [], "outputs": [] }
      }
    ],
    "edges": [
      { "sourceNodeID": "start_1", "sourcePortID": "out", "targetNodeID": "type_check", "targetPortID": "in" },
      { "sourceNodeID": "type_check", "sourcePortID": "out_A", "targetNodeID": "process_A", "targetPortID": "in" },
      { "sourceNodeID": "type_check", "sourcePortID": "out_B", "targetNodeID": "process_B", "targetPortID": "in" },
      { "sourceNodeID": "type_check", "sourcePortID": "out_C", "targetNodeID": "process_C", "targetPortID": "in" },
      { "sourceNodeID": "process_A", "sourcePortID": "out", "targetNodeID": "merge_result", "targetPortID": "in" },
      { "sourceNodeID": "process_B", "sourcePortID": "out", "targetNodeID": "merge_result", "targetPortID": "in" },
      { "sourceNodeID": "process_C", "sourcePortID": "out", "targetNodeID": "merge_result", "targetPortID": "in" },
      { "sourceNodeID": "merge_result", "sourcePortID": "out", "targetNodeID": "end_1", "targetPortID": "in" }
    ]
  }
}
```

## 示例：包含事务框的完整流程

### 需求：开始 → 判断 → 分支处理 → 事务框(插入+更新+查询+删除) → 最终处理 → 结束

### 布局规划

```
y:-522 ┌────────────────────────────────────────────────────────────────────────────┐
       │                         [transaction_box]                                  │
y:-455 │  ┌──────────────────────────────────────────────────────────────────────┐ │
       │  │  [tx_insert] ──→ [tx_update]                                         │ │
y:-357 │  │        ↓                │                                              │ │
       │  │  [tx_query] ──→ [tx_delete]                                          │ │
y:-114 │  └──────────────────────────────────────────────────────────────────────┘ │
y:-212 │                    ↑                                                      │
y:-455 │  [start] → [switch] → [branch] → ┌─────────────────────────────────────┐ │
       │                                 │           [final] → [end]           │ │
       │                                 └─────────────────────────────────────┘ │
       └────────────────────────────────────────────────────────────────────────────┘
       x:-330  x:55     x:390    x:825    x:1095         x:1965         x:2280
```

### 生成的 JSON

```json
{
  "name": "事务处理完整流程",
  "code": "transaction_process",
  "description": "包含分支路由和事务处理的完整测试流程",
  "flow": {
    "nodes": [
      {
        "id": "start_1",
        "type": "skylake-start",
        "meta": { "zIndex": 1, "size": { "width": 240, "height": 74 }, "position": { "x": -330, "y": -345 } },
        "data": { "name": "开始节点", "inputs": [], "outputs": [] }
      },
      {
        "id": "type_check",
        "type": "skylake-switch",
        "meta": { "zIndex": 2, "size": { "width": 240, "height": 211 }, "position": { "x": 55, "y": -410.5 } },
        "data": {
          "name": "类型判断",
          "nodeMeta": { "pluginId": "my-plugin", "pluginName": "my-plugin", "pluginVersion": "1.0.0", "functionName": "check_type" },
          "inputs": [], "outputs": [], "options": ["1", "2", "3"]
        }
      },
      {
        "id": "branch_process",
        "type": "skylake-func",
        "meta": { "zIndex": 3, "size": { "width": 240, "height": 98 }, "position": { "x": 390, "y": -455 } },
        "data": {
          "name": "分支处理",
          "nodeMeta": { "pluginId": "my-plugin", "pluginName": "my-plugin", "pluginVersion": "1.0.0", "functionName": "handle_branch" },
          "inputs": [], "outputs": []
        }
      },
      {
        "id": "transaction_box",
        "type": "skylake-transaction",
        "parent": null,
        "meta": { "zIndex": 1, "size": { "width": 810, "height": 645 }, "position": { "x": 1095, "y": -522.5 } },
        "data": {
          "name": "事务处理框",
          "nodeMeta": { "pluginId": "", "pluginName": "", "pluginVersion": "", "functionName": "", "databaseId": "primary" },
          "inputs": [], "outputs": []
        }
      },
      {
        "id": "tx_insert",
        "type": "skylake-func",
        "parent": "transaction_box",
        "meta": { "zIndex": 7, "size": { "width": 240, "height": 98 }, "position": { "x": 1170, "y": -430 } },
        "data": {
          "name": "事务插入",
          "nodeMeta": { "pluginId": "my-plugin", "pluginName": "my-plugin", "pluginVersion": "1.0.0", "functionName": "tx_insert" },
          "inputs": [], "outputs": []
        }
      },
      {
        "id": "tx_update",
        "type": "skylake-func",
        "parent": "transaction_box",
        "meta": { "zIndex": 8, "size": { "width": 240, "height": 98 }, "position": { "x": 1590, "y": -357.5 } },
        "data": {
          "name": "事务更新",
          "nodeMeta": { "pluginId": "my-plugin", "pluginName": "my-plugin", "pluginVersion": "1.0.0", "functionName": "tx_update" },
          "inputs": [], "outputs": []
        }
      },
      {
        "id": "tx_query",
        "type": "skylake-func",
        "parent": "transaction_box",
        "meta": { "zIndex": 9, "size": { "width": 240, "height": 98 }, "position": { "x": 1185, "y": -114 } },
        "data": {
          "name": "事务查询",
          "nodeMeta": { "pluginId": "my-plugin", "pluginName": "my-plugin", "pluginVersion": "1.0.0", "functionName": "tx_query" },
          "inputs": [], "outputs": []
        }
      },
      {
        "id": "tx_delete",
        "type": "skylake-func",
        "parent": "transaction_box",
        "meta": { "zIndex": 10, "size": { "width": 240, "height": 98 }, "position": { "x": 1605, "y": -114 } },
        "data": {
          "name": "事务删除",
          "nodeMeta": { "pluginId": "my-plugin", "pluginName": "my-plugin", "pluginVersion": "1.0.0", "functionName": "tx_delete" },
          "inputs": [], "outputs": []
        }
      },
      {
        "id": "final_process",
        "type": "skylake-func",
        "meta": { "zIndex": 11, "size": { "width": 240, "height": 98 }, "position": { "x": 1965, "y": -249 } },
        "data": {
          "name": "最终处理",
          "nodeMeta": { "pluginId": "my-plugin", "pluginName": "my-plugin", "pluginVersion": "1.0.0", "functionName": "final_process" },
          "inputs": [], "outputs": []
        }
      },
      {
        "id": "end_1",
        "type": "skylake-end",
        "meta": { "zIndex": 12, "size": { "width": 240, "height": 74 }, "position": { "x": 2280, "y": -237 } },
        "data": { "name": "结束节点", "inputs": [], "outputs": [] }
      }
    ],
    "edges": [
      { "sourceNodeID": "start_1", "sourcePortID": "out", "targetNodeID": "type_check", "targetPortID": "in" },
      { "sourceNodeID": "type_check", "sourcePortID": "out_1", "targetNodeID": "branch_process", "targetPortID": "in" },
      { "sourceNodeID": "type_check", "sourcePortID": "out_2", "targetNodeID": "branch_process", "targetPortID": "in" },
      { "sourceNodeID": "type_check", "sourcePortID": "out_3", "targetNodeID": "branch_process", "targetPortID": "in" },
      { "sourceNodeID": "branch_process", "sourcePortID": "out", "targetNodeID": "tx_insert", "targetPortID": "in" },
      { "sourceNodeID": "tx_insert", "sourcePortID": "out", "targetNodeID": "tx_update", "targetPortID": "in" },
      { "sourceNodeID": "tx_update", "sourcePortID": "out", "targetNodeID": "tx_query", "targetPortID": "in" },
      { "sourceNodeID": "tx_query", "sourcePortID": "out", "targetNodeID": "tx_delete", "targetPortID": "in" },
      { "sourceNodeID": "tx_delete", "sourcePortID": "out", "targetNodeID": "final_process", "targetPortID": "in" },
      { "sourceNodeID": "final_process", "sourcePortID": "out", "targetNodeID": "end_1", "targetPortID": "in" }
    ]
  }
}
```

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
