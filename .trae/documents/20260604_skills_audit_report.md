# 技能描述审查报告

> 审查范围：`wasm-plugin-developer`、`service-orchestration-generator`、`plugin-metadata-generator`
> 对比源码：`cmx-plugin`、`cmx-service`、`cmx-metadata`、`cmx-core`
> 审查日期：2026-06-04

---

## 一、总体评估

三个技能整体质量较高，基本覆盖了插件开发的核心流程。通过源码对比共发现 23 个问题，经用户确认后：

| 状态 | 数量 | 说明 |
|------|------|------|
| 需修复 | 10 | 需要修改技能文件或源码 |
| 待讨论 | 1 | 后续再讨论 |
| 忽略 | 12 | 不处理 |

---

## 二、wasm-plugin-developer 技能审查

### 2.1 事实性错误

#### 问题 1：`plugin.type` 源码注释错误（P0 - 必须修复）

**位置**：`cmx-core/src/model/meta/plugin.rs` 第 84-85 行

**源码现状**：
```rust
//插件类型 wasm或者rhai
pub r#type: String,
```

**技能描述**（正确值）：
```json
"plugin": {
    "type": "wasm-plugin",
    ...
}
```

**问题**：源码注释写的 `"wasm"` 或 `"rhai"` 不准确，实际合法值应为 `"wasm-plugin"` 等（以技能描述为准）。

**修复方案**：修改源码注释，将 `//插件类型 wasm或者rhai` 改为准确的说明。

---

#### 问题 2：manifest.json 包含源码不解析的幽灵字段（~忽略~）

> 用户决定：忽略不处理。

---

#### 问题 3：`dependencies` 字段格式描述不准确（P1 - 应该修复）

**位置**：三、manifest.json 规范 → 3.1 完整格式

**技能描述**：
```json
"dependencies": []
```
字段说明为"依赖的其他插件ID列表"。

**源码事实**：`PluginDefinition` 第 121 行：
```rust
pub dependencies: Vec<PluginDependency>,
```
其中 `PluginDependency` 是结构体：
```rust
pub struct PluginDependency {
    pub plugin_id: String,
    pub version_constraint: Option<String>,
    pub optional: bool,
}
```

源码注释 `// 插件依赖列表 todo 现在不支持解析json文件中的依赖，因为格式不一致0320` 表明存在已知格式兼容问题。

**修复方案**：在技能中补充 `PluginDependency` 结构化对象数组格式说明。

---

### 2.2 字段遗漏

#### 问题 4：缺少 `services` 字段（~忽略~）

> 用户决定：忽略不处理。

---

#### 问题 5：缺少签名相关字段（~忽略~）

> 用户决定：忽略不处理。

---

#### 问题 6：`entries` 列表不完整（~忽略~）

> 用户决定：忽略不处理。

---

### 2.3 格式/约定不一致

#### 问题 7：`main_file` 路径格式不一致（~忽略~）

> 用户决定：忽略不处理。

---

#### 问题 8：`table_config_files` 路径格式（~无需修改~）

> 用户决定：以技能描述为准，不用改。

---

### 2.4 描述不完整

#### 问题 9：缺少 cmx-plugin-sdk 核心类型说明（P2 - 建议修复）

**位置**：四、代码架构

**技能描述**：提到了 `cmx-plugin-sdk` 依赖，但没有说明其提供的核心类型。

**实际内容**：cmx-plugin-sdk 提供了 `FunctionInput`、`FunctionOutput`、`HostCaller`、`Msgpack` 等核心类型，这些类型是 extism_layer.rs 中 `#[plugin_fn]` 函数签名的基础。

**修复方案**：在代码架构部分补充一个简表，列出 cmx-plugin-sdk 的核心类型及其用途。

---

## 三、service-orchestration-generator 技能审查

### 3.1 字段遗漏

#### 问题 10：缺少 `inputs`/`outputs` 的详细格式定义（~待讨论~）

> 用户决定：后续再讨论。

---

#### 问题 11：缺少 `databaseId` 在 func 节点中的使用说明（P2 - 建议修复）

**位置**：节点模板

**技能描述**：仅事务框节点模板中展示了 `databaseId` 字段，func 节点模板中没有。

**源码事实**：`NodeNodeMeta` 的 `database_id` 是 `Option<String>`，适用于所有有 `nodeMeta` 的节点类型。

**修复方案**：在 func 节点模板中补充说明 `databaseId` 为可选字段。

---

### 3.2 描述不完整

#### 问题 12：缺少上下文传递机制说明（P1 - 应该修复）

**位置**：核心概念 → 节点类型

**技能描述**：未描述节点之间的数据传递机制。

**源码事实**：`ExecutionContext`（`cmx-service/src/orchestrator/types.rs`）中有两个关键机制：
- `current_output`：链式传递，上一个节点的输出自动成为下一个节点的输入
- `svr_context.step_outputs`：每个节点的输出按 `node_id` 缓存，后续节点可通过 `step_outputs[node_id]` 访问任意前序节点的输出

这是理解服务编排数据流的关键信息。

**修复方案**：在"核心概念"部分新增"上下文传递机制"小节，说明 `current_output` 链式传递和 `step_outputs` 缓存机制。

---

#### 问题 13：缺少调试功能说明（~忽略~）

> 用户决定：忽略不处理。

---

#### 问题 14：switch 节点返回值类型要求不够明确（P2 - 建议修复）

**位置**：skylake-switch 多分支节点详解

**技能描述**：展示了返回值映射关系，但没有明确说明函数返回值**必须是字符串类型**。

**源码事实**：executor 中 switch 节点处理逻辑会将函数返回值作为字符串拼接 `"out_{value}"` 来查找出边。

**修复方案**：在"skylake-switch 多分支节点详解"部分添加明确说明："switch 节点的函数返回值必须是字符串类型"。

---

#### 问题 15：事务状态转换规则未描述（P2 - 建议修复）

**位置**：skylake-transaction 相关部分

**技能描述**：说明了事务框不出现在 edges 中，子节点需设置 parent，但未描述运行时的事务管理行为。

**源码事实**：`TransactionManager` 基于 `parent` 属性自动管理事务生命周期，有五种状态转换：
1. 无事务 → 无事务（节点不在事务框内）
2. 无事务 → 开启事务（节点进入事务框）
3. 同一事务框内继续
4. 切换事务框（提交旧事务，开启新事务）
5. 离开事务框 → 提交事务

**修复方案**：在事务框说明部分补充事务生命周期管理的简要描述。

---

## 四、plugin-metadata-generator 技能审查

### 4.1 字段遗漏

#### 问题 16：ColumnDefine 缺少 `precision` 和 `scale` 字段（P0 - 必须修复）

**位置**：三、metadata 表结构定义规范 → 3.3 ColumnDefine 完整字段

**技能描述**：ColumnDefine 字段表中未列出 `precision` 和 `scale`。

**源码事实**：`cmx-core/src/model/meta/table.rs` 第 106-110 行：
```rust
pub precision: Option<u32>,  // 数值类型总精度（总位数）
pub scale: Option<u32>,      // 数值类型小数位数
```

**影响**：没有这两个字段，开发者无法正确生成 `Decimal` 类型的列定义（如 `NUMERIC(18,2)`）。

**修复方案**：
1. 在 ColumnDefine 完整字段表中添加 `precision`（条件必填，Decimal 类型）和 `scale`（条件必填，Decimal 类型）
2. 在 §3.4 FieldType 表中 Decimal 行的"需要额外字段"列标注"**precision + scale**"
3. 在 §3.5 db_type 生成规则中，Decimal 行补充说明
4. 在 §3.8 常用列模板中补充 Decimal 列模板

---

#### 问题 17：ColumnDefine 缺少 `create_time` 和 `update_time` 字段（P2 - 建议修复）

**位置**：三、metadata 表结构定义规范 → 3.3 ColumnDefine 完整字段

**源码事实**：`ColumnDefine` 第 118-122 行有 `create_time: Option<DateTime<Utc>>` 和 `update_time: Option<DateTime<Utc>>`。

**修复方案**：在 ColumnDefine 字段表中补充这两个字段，标注为"系统自动维护，生成时通常设为 `null`"。

---

#### 问题 18：TableDefine 缺少 `tablespace`、`create_time`、`update_time` 字段（P2 - 建议修复）

**位置**：三、metadata 表结构定义规范 → 3.2 TableDefine 完整字段

**源码事实**：`TableDefine` 中有 `tablespace: Option<String>`、`create_time: Option<DateTime<Utc>>`、`update_time: Option<DateTime<Utc>>` 三个字段。

**修复方案**：在 TableDefine 字段表中补充这三个字段。

---

### 4.2 描述不完整

#### 问题 19：i18n 伴生表生成规则缺失（~忽略~）

> 用户决定：忽略不处理。

---

#### 问题 20：DDL 增量升级机制描述不完整（~忽略~）

> 用户决定：忽略不处理。

---

#### 问题 21：种子数据批次执行策略未说明（P2 - 建议修复）

**位置**：四、seeddata 种子数据规范

**技能描述**：未提及种子数据的执行策略。

**源码事实**：`PgSeedDataExecutor` 默认批次大小 100 行，批量执行失败时自动降级为逐行执行，最终校验实际行数。

**修复方案**：在种子数据规范部分补充执行策略说明。

---

## 五、跨技能一致性问题

### 问题 22：三个技能之间的 config 文件路径引用不一致（P3 - 可选改进）

**wasm-plugin-developer** 中 manifest.json 示例：
```json
"table_config_files": ["config/account_config.json"]
```

**manifest_example.json**（cmx-plugin 测试文件）中：
```json
"table_config_files": ["meta/sys_tables_config.json"]
```

**修复方案**：统一说明 `table_config_files` 中的路径是相对于 ZIP 根目录的，`config/` 和 `meta/` 目录名都可以使用，但需与实际目录结构一致。

---

### 问题 23：FieldType 枚举顺序与源码不一致（P3 - 可选改进）

**技能描述**（plugin-metadata-generator §3.4）的顺序：Int, Float, Decimal, String, Text, Bool, Date, DateTime, Json, Binary, Array, Uuid, Unknown

**源码事实**（`cmx-core/src/model/meta/table.rs` 第 26-57 行）的顺序：String, Int, Float, Decimal, DateTime, Date, Bool, Text, Binary, Array, Json, Uuid, Unknown

**修复方案**：按源码顺序重新排列 FieldType 表格，保持与代码定义一致。

---

## 六、修复优先级排序（修订版）

### P0 - 必须修复（会导致生成错误的代码或源码注释错误）

| # | 技能 | 问题 | 修复对象 | 概要 |
|---|------|------|---------|------|
| 1 | wasm-plugin-developer | 问题 1 | **源码** | 修改 `plugin.rs` 中 `type` 字段注释，以技能描述为准 |
| 2 | plugin-metadata-generator | 问题 16 | **技能** | ColumnDefine 缺少 `precision`/`scale`，无法正确生成 Decimal 列 |

### P1 - 应该修复（信息不准确或重要遗漏）

| # | 技能 | 问题 | 修复对象 | 概要 |
|---|------|------|---------|------|
| 3 | wasm-plugin-developer | 问题 3 | **技能** | `dependencies` 格式应为结构化对象数组 |
| 4 | service-orchestration-generator | 问题 12 | **技能** | 缺少上下文传递机制说明 |

### P2 - 建议修复（完善性改进）

| # | 技能 | 问题 | 修复对象 | 概要 |
|---|------|------|---------|------|
| 5 | wasm-plugin-developer | 问题 9 | **技能** | 缺少 cmx-plugin-sdk 核心类型说明 |
| 6 | service-orchestration-generator | 问题 11 | **技能** | databaseId 在 func 节点中的使用说明 |
| 7 | service-orchestration-generator | 问题 14 | **技能** | switch 返回值类型要求不明确 |
| 8 | service-orchestration-generator | 问题 15 | **技能** | 事务状态转换规则未描述 |
| 9 | plugin-metadata-generator | 问题 17 | **技能** | ColumnDefine 缺少 create_time/update_time |
| 10 | plugin-metadata-generator | 问题 18 | **技能** | TableDefine 缺少 tablespace/create_time/update_time |
| 11 | plugin-metadata-generator | 问题 21 | **技能** | 种子数据批次执行策略未说明 |

### P3 - 可选改进（一致性优化）

| # | 技能 | 问题 | 修复对象 | 概要 |
|---|------|------|---------|------|
| 12 | 跨技能 | 问题 22 | **技能** | config 文件路径引用不一致 |
| 13 | 跨技能 | 问题 23 | **技能** | FieldType 枚举顺序与源码不一致 |

---

## 七、修复实施计划

### 7.1 源码修复

| 文件 | 修改内容 |
|------|---------|
| `crates/libs/cmx-core/src/model/meta/plugin.rs` 第 84 行 | 将注释 `//插件类型 wasm或者rhai` 修改为准确的类型说明，与技能描述一致 |

### 7.2 wasm-plugin-developer 技能修复

| 优先级 | 修改内容 |
|--------|---------|
| P1 | 补充 `dependencies` 字段的 `PluginDependency` 结构化对象数组格式说明 |
| P2 | 在代码架构部分新增 cmx-plugin-sdk 核心类型简表（FunctionInput, FunctionOutput, HostCaller, Msgpack 等） |

### 7.3 service-orchestration-generator 技能修复

| 优先级 | 修改内容 |
|--------|---------|
| P1 | 新增"上下文传递机制"小节：说明 current_output 链式传递和 step_outputs 缓存机制 |
| P2 | 在 func 节点模板中补充 `databaseId` 可选字段说明 |
| P2 | 在 switch 多分支节点详解中明确"函数返回值必须是字符串类型" |
| P2 | 在事务框说明部分补充事务生命周期管理的简要描述（五种状态转换） |

### 7.4 plugin-metadata-generator 技能修复

| 优先级 | 修改内容 |
|--------|---------|
| P0 | ColumnDefine 字段表添加 `precision`（条件必填）和 `scale`（条件必填）字段 |
| P0 | §3.4 FieldType 表 Decimal 行补充"需要 precision + scale" |
| P0 | §3.5 db_type 生成规则 Decimal 行补充 precision/scale 说明 |
| P0 | §3.8 常用列模板补充 Decimal 列模板 |
| P2 | ColumnDefine 字段表补充 `create_time` 和 `update_time` |
| P2 | TableDefine 字段表补充 `tablespace`、`create_time`、`update_time` |
| P2 | 种子数据规范部分补充执行策略说明（100 行批次，失败降级逐行） |
| P3 | §3.4 FieldType 表按源码定义顺序重排 |

### 7.5 待讨论项

| 问题 | 说明 |
|------|------|
| 问题 10 | service-orchestration-generator 中 inputs/outputs 的详细格式定义，后续讨论 |
