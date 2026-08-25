# 节点模板与画布布局

> 本文件是 service-orchestration-generator 技能的 references 细节层（从 SKILL.md 下沉，内容未改）。返回决策入口：[../SKILL.md](../SKILL.md)

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

**databaseId 规范**：事务框（`skylake-transaction`）的 `databaseId` 必须使用插件 `manifest.json` 中 `plugin.datasource_id` 的值，确保事务操作使用插件关联的数据源。

**pluginId 命名约束**：`pluginId` 只能使用下划线 `_` 分隔，禁止使用连字符 `-`。正确：`my_plugin`，错误：`my-plugin`。

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
