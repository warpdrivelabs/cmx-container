# 完整示例（两个端到端流程）

> 本文件是 service-orchestration-generator 技能的 references 细节层（从 SKILL.md 下沉，内容未改）。返回决策入口：[../SKILL.md](../SKILL.md)

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
          "nodeMeta": { "pluginId": "my_plugin", "pluginName": "my_plugin", "pluginVersion": "1.0.0", "functionName": "check_type" },
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
          "nodeMeta": { "pluginId": "my_plugin", "pluginName": "my_plugin", "pluginVersion": "1.0.0", "functionName": "handle_type_a" },
          "inputs": [], "outputs": []
        }
      },
      {
        "id": "process_B",
        "type": "skylake-func",
        "meta": { "zIndex": 4, "size": { "width": 240, "height": 98 }, "position": { "x": 420, "y": -237 } },
        "data": {
          "name": "B类型处理",
          "nodeMeta": { "pluginId": "my_plugin", "pluginName": "my_plugin", "pluginVersion": "1.0.0", "functionName": "handle_type_b" },
          "inputs": [], "outputs": []
        }
      },
      {
        "id": "process_C",
        "type": "skylake-func",
        "meta": { "zIndex": 5, "size": { "width": 240, "height": 98 }, "position": { "x": 405, "y": -16 } },
        "data": {
          "name": "C类型处理",
          "nodeMeta": { "pluginId": "my_plugin", "pluginName": "my_plugin", "pluginVersion": "1.0.0", "functionName": "handle_type_c" },
          "inputs": [], "outputs": []
        }
      },
      {
        "id": "merge_result",
        "type": "skylake-func",
        "meta": { "zIndex": 6, "size": { "width": 240, "height": 98 }, "position": { "x": 825, "y": -212 } },
        "data": {
          "name": "合并结果",
          "nodeMeta": { "pluginId": "my_plugin", "pluginName": "my_plugin", "pluginVersion": "1.0.0", "functionName": "merge" },
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
          "nodeMeta": { "pluginId": "my_plugin", "pluginName": "my_plugin", "pluginVersion": "1.0.0", "functionName": "check_type" },
          "inputs": [], "outputs": [], "options": ["1", "2", "3"]
        }
      },
      {
        "id": "branch_process",
        "type": "skylake-func",
        "meta": { "zIndex": 3, "size": { "width": 240, "height": 98 }, "position": { "x": 390, "y": -455 } },
        "data": {
          "name": "分支处理",
          "nodeMeta": { "pluginId": "my_plugin", "pluginName": "my_plugin", "pluginVersion": "1.0.0", "functionName": "handle_branch" },
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
          "nodeMeta": { "pluginId": "my_plugin", "pluginName": "my_plugin", "pluginVersion": "1.0.0", "functionName": "tx_insert" },
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
          "nodeMeta": { "pluginId": "my_plugin", "pluginName": "my_plugin", "pluginVersion": "1.0.0", "functionName": "tx_update" },
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
          "nodeMeta": { "pluginId": "my_plugin", "pluginName": "my_plugin", "pluginVersion": "1.0.0", "functionName": "tx_query" },
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
          "nodeMeta": { "pluginId": "my_plugin", "pluginName": "my_plugin", "pluginVersion": "1.0.0", "functionName": "tx_delete" },
          "inputs": [], "outputs": []
        }
      },
      {
        "id": "final_process",
        "type": "skylake-func",
        "meta": { "zIndex": 11, "size": { "width": 240, "height": 98 }, "position": { "x": 1965, "y": -249 } },
        "data": {
          "name": "最终处理",
          "nodeMeta": { "pluginId": "my_plugin", "pluginName": "my_plugin", "pluginVersion": "1.0.0", "functionName": "final_process" },
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
