# 会计科目管理插件方案

## 概述

基于 `cmx-wasmdemo` 架构模式，**新建**一个独立的插件工程 `cmx-account`，实现具有真实业务含义的会计科目管理功能，提供科目数据保存、列表查询、详情获取等核心函数，并配套完整的表元数据、种子数据和服务编排文件。

***

## 一、当前状态分析

### 1.1 参考架构：cmx-wasmdemo

| 文件                    | 职责                            |
| --------------------- | ----------------------------- |
| `src/lib.rs`          | 模块入口，条件编译 extism\_layer       |
| `src/models.rs`       | SDK 类型重导出 + 自定义业务模型           |
| `src/host_traits.rs`  | HostFunctions trait（11 个宿主能力） |
| `src/core.rs`         | PluginCore 业务逻辑               |
| `src/extism_layer.rs` | #\[plugin\_fn] 适配层            |
| `src/tests.rs`        | MockHostFunctions 单元测试        |

### 1.2 现有约定

* 表定义 JSON 格式参考 `crates/libs/cmx-metadata/tests/domain_app_module_tables.json`

* 配置清单 JSON 格式参考 `crates/libs/cmx-metadata/tests/domain_app_module_config.json`

* 服务编排 JSON 格式参考 `docs/插件目录说明.md` §2.5

* manifest.json 格式参考 `crates/libs/cmx-dev/templates/wasm-plugin-template/manifest.json.hbs`

* wasmdemo 的 Cargo.toml 使用 `path` 引用 `cmx-plugin-sdk`（wasmdemo 例外，不强制 workspace 依赖）

***

## 二、业务设计

### 2.1 会计科目表设计（`cmx_account`）

会计科目是财务系统的基础数据，采用树形结构组织，中国会计准则分为六大类：

| 科目大类     | 编码前缀 | 说明              |
| -------- | ---- | --------------- |
| 资产类      | 1    | 库存现金、银行存款、应收账款等 |
| 负债类      | 2    | 应付账款、短期借款等      |
| 所有者权益类   | 4    | 实收资本、盈余公积等      |
| 成本类      | 5    | 生产成本、制造费用等      |
| 损益类 - 收入 | 6    | 主营业务收入等         |
| 损益类 - 费用 | 6    | 管理费用、销售费用等      |

**表结构**：

| 列名            | 类型                        | 说明                                               |
| ------------- | ------------------------- | ------------------------------------------------ |
| id            | BIGINT PK                 | 主键                                               |
| code          | VARCHAR(32)               | 科目编码（如 "1001"），唯一                                |
| name          | VARCHAR(128)              | 科目名称（如 "库存现金"）                                   |
| parent\_id    | BIGINT FK→cmx\_account.id | 父科目ID，顶级为 NULL                                   |
| level         | INT                       | 层级（1=一级科目，2=二级...）                               |
| account\_type | VARCHAR(32)               | 科目类型：asset/liability/equity/cost/revenue/expense |
| direction     | VARCHAR(16)               | 余额方向：debit(借方)/credit(贷方)                        |
| is\_leaf      | BOOLEAN                   | 是否末级科目（只有末级可记账）                                  |
| is\_enabled   | BOOLEAN                   | 是否启用                                             |
| sort\_order   | BIGINT                    | 排序号                                              |
| remark        | TEXT                      | 备注                                               |
| create\_time  | TIMESTAMP                 | 创建时间                                             |
| update\_time  | TIMESTAMP                 | 更新时间                                             |

**索引**：

* `uk_cmx_account_code` — code 唯一索引

* `idx_cmx_account_parent_id` — parent\_id 普通索引

* `idx_cmx_account_type` — account\_type 普通索引

### 2.2 种子数据

提供中国小企业会计准则的一级科目（约 30 个），覆盖六大类：

* 资产类(1xxx): 1001库存现金, 1002银行存款, 1012其他货币资金, 1122应收账款, 1221其他应收款, 1403原材料, 1405库存商品, 1601固定资产, 1602累计折旧

* 负债类(2xxx): 2001短期借款, 2202应付账款, 2211应付职工薪酬, 2221应交税费, 2241其他应付款, 2501长期借款

* 权益类(4xxx): 4001实收资本, 4002资本公积, 4101盈余公积, 4103本年利润, 4104利润分配

* 成本类(5xxx): 5001生产成本, 5101制造费用

* 损益类(6xxx): 6001主营业务收入, 6051其他业务收入, 6401管理费用, 6402销售费用, 6403财务费用, 6801所得税费用

### 2.3 插件函数设计

#### 核心业务函数（3 个）

| 函数名                  | 类型   | 说明           | 输入                   | 输出                    |
| -------------------- | ---- | ------------ | -------------------- | --------------------- |
| `save_account`       | func | 保存科目（新增/更新）  | AccountSaveRequest   | AccountSaveResponse   |
| `list_accounts`      | func | 科目列表查询       | AccountListRequest   | AccountListResponse   |
| `get_account_detail` | func | 获取科目详情（含子科目） | AccountDetailRequest | AccountDetailResponse |

#### 服务编排函数（2 个）

| 函数名                | 类型         | 说明                | 输入                 | 输出                |
| ------------------ | ---------- | ----------------- | ------------------ | ----------------- |
| `save_route_check` | branch\_fn | 保存路由判断（新增 vs 更新）  | 含 id 字段的 JSON      | "1"(新增) 或 "2"(更新) |
| `validate_account` | func       | 科目校验（编码规则、父科目存在性） | AccountSaveRequest | 校验结果              |

### 2.4 数据模型定义

```rust
// 科目保存请求
pub struct AccountSaveRequest {
    pub id: Option<i64>,
    pub code: String,
    pub name: String,
    pub parent_id: Option<i64>,
    pub level: i32,
    pub account_type: String,
    pub direction: String,
    pub is_leaf: bool,
    pub is_enabled: Option<bool>,
    pub sort_order: Option<i64>,
    pub remark: Option<String>,
}

// 科目列表查询请求
pub struct AccountListRequest {
    pub account_type: Option<String>,
    pub parent_id: Option<i64>,
    pub keyword: Option<String>,
    pub is_leaf: Option<bool>,
    pub is_enabled: Option<bool>,
    pub page: Option<i32>,
    pub page_size: Option<i32>,
}

// 科目详情请求
pub struct AccountDetailRequest {
    pub id: Option<i64>,
    pub code: Option<String>,
    pub include_children: bool,
}
```

### 2.5 服务编排流程

每个服务编排文件对应一个接口，共三个：

#### 流程一：科目保存 (`save_account_flow`)

```
开始 → 校验科目 → 路由判断(新增/更新) → 新增处理 / 更新处理 → 结束
```

| 节点ID | 类型 | 说明 |
|--------|------|------|
| start_1 | skylake-start | 开始 |
| validate_account | skylake-func | 校验科目数据 |
| save_route_check | skylake-switch | 路由判断：有id→更新(2)，无id→新增(1) |
| create_account | skylake-func | 新增科目 |
| update_account | skylake-func | 更新科目 |
| end_1 | skylake-end | 结束 |

边：
- start_1[out] → validate_account[in]
- validate_account[out] → save_route_check[in]
- save_route_check[out_1] → create_account[in]
- save_route_check[out_2] → update_account[in]
- create_account[out] → end_1[in]
- update_account[out] → end_1[in]

#### 流程二：科目列表查询 (`list_accounts_flow`)

```
开始 → 查询科目列表 → 结束
```

| 节点ID | 类型 | 说明 |
|--------|------|------|
| start_1 | skylake-start | 开始 |
| list_accounts | skylake-func | 查询科目列表 |
| end_1 | skylake-end | 结束 |

边：
- start_1[out] → list_accounts[in]
- list_accounts[out] → end_1[in]

#### 流程三：科目详情查询 (`get_account_detail_flow`)

```
开始 → 查询科目详情 → 结束
```

| 节点ID | 类型 | 说明 |
|--------|------|------|
| start_1 | skylake-start | 开始 |
| get_account_detail | skylake-func | 查询科目详情（含子科目） |
| end_1 | skylake-end | 结束 |

边：
- start_1[out] → get_account_detail[in]
- get_account_detail[out] → end_1[in]

***

## 三、文件变更清单

### 3.1 新建插件工程 `crates/libs/cmx-account/`

**Rust 源码**（参照 wasmdemo 架构）：

| 文件路径                  | 说明                                       |
| --------------------- | ---------------------------------------- |
| `Cargo.toml`          | 项目配置，参照 wasmdemo（path 引用 cmx-plugin-sdk） |
| `src/lib.rs`          | 模块入口，条件编译 extism\_layer                  |
| `src/models.rs`       | SDK 类型重导出 + 会计科目业务模型                     |
| `src/host_traits.rs`  | HostFunctions trait（与 wasmdemo 完全一致）     |
| `src/core.rs`         | PluginCore，实现 5 个会计科目业务函数                |
| `src/extism_layer.rs` | #\[plugin\_fn] 适配层，5 个导出函数               |
| `src/tests.rs`        | MockHostFunctions 单元测试                   |

**元数据文件**（参照 wasm-plugin-template 目录结构）：

| 文件路径 | 说明 |
|---------|------|
| `config/account_config.json` | 表定义配置清单 |
| `config/account_tables.json` | cmx_account 表结构定义 |
| `seeddata/account_seed.json` | 会计科目种子数据（一级科目） |
| `servicedata/save_account_flow.json` | 科目保存服务编排流程 |
| `servicedata/list_accounts_flow.json` | 科目列表查询服务编排流程 |
| `servicedata/get_account_detail_flow.json` | 科目详情查询服务编排流程 |
| `menudata/account_menu.json` | 会计科目菜单配置 |
| `formdata/account_form.json` | 会计科目表单配置 |

### 3.2 修改 Workspace 配置

| 文件                        | 变更内容                                    |
| ------------------------- | --------------------------------------- |
| `Cargo.toml`（workspace 根） | members 中添加 `"crates/libs/cmx-account"` |

**不修改** cmx-wasmdemo 的任何文件。

***

## 四、详细实现方案

### 4.1 Cargo.toml

```toml
[package]
name = "cmx-account"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
# Extism PDK
extism-pdk = { version = "1.4.1", optional = true }
# CMX 插件 SDK
cmx-plugin-sdk = { path = "../cmx-plugin-sdk", version = "0.1.8", registry = "nora", default-features = false }
# 序列化框架
serde = { version = "1.0", features = ["derive"] }
# JSON 序列化/反序列化
serde_json = "1.0"
# 时间处理
chrono = { version = "0.4", features = ["serde"] }

[dev-dependencies]
# Mock 测试框架
mockall = "0.11"

[features]
# Extism 特性 — 编译 Wasm 时开启
extism = ["extism-pdk", "cmx-plugin-sdk/extism"]

[profile.release]
lto = true
opt-level = "s"
```

### 4.2 src/lib.rs

```rust
//! 会计科目管理插件模块。
//!
//! 提供会计科目的增删改查功能，支持树形层级结构，
//! 涵盖资产/负债/权益/成本/损益六大类科目。

pub mod models;
pub mod host_traits;
pub mod core;

#[cfg(test)]
pub mod tests;

#[cfg(feature = "extism")]
pub mod extism_layer;
```

### 4.3 src/models.rs

重导出 SDK 类型 + 定义会计科目业务模型（AccountSaveRequest/Response、AccountListRequest/Response、AccountDetailRequest/Response）。

### 4.4 src/host\_traits.rs

与 wasmdemo 完全一致（HostFunctions trait 的 11 个方法）。

### 4.5 src/core.rs — 5 个业务函数

#### save\_account

```rust
pub fn save_account(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
    let request: AccountSaveRequest = serde_json::from_value(input.input.clone()).map_err(|e| e.to_string())?;
    if let Some(id) = request.id {
        // UPDATE cmx_account SET name=..., direction=..., is_leaf=..., is_enabled=..., remark=... WHERE id = {id}
    } else {
        // INSERT INTO cmx_account (code, name, parent_id, level, account_type, direction, is_leaf, is_enabled, sort_order, remark) VALUES (...)
    }
    // 通过 host.db_execute 执行 SQL
}
```

#### list\_accounts

```rust
pub fn list_accounts(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
    let request: AccountListRequest = serde_json::from_value(input.input.clone()).map_err(|e| e.to_string())?;
    // 动态拼接 WHERE 条件：
    // - account_type → AND account_type = '{value}'
    // - parent_id → AND parent_id = {value}  (None 时 AND parent_id IS NULL 查一级科目)
    // - keyword → AND (code LIKE '%{value}%' OR name LIKE '%{value}%')
    // - is_leaf → AND is_leaf = {value}
    // - is_enabled → AND is_enabled = {value}
    // 分页：ORDER BY code LIMIT {page_size} OFFSET {(page-1)*page_size}
    // 通过 host.db_query 执行查询
}
```

#### get\_account\_detail

```rust
pub fn get_account_detail(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
    let request: AccountDetailRequest = serde_json::from_value(input.input.clone()).map_err(|e| e.to_string())?;
    // 1. 按 id 或 code 查询科目本身
    // 2. 如果 include_children=true，查询子科目列表：SELECT * FROM cmx_account WHERE parent_id = {id}
    // 通过 host.db_query 执行查询
}
```

#### save\_route\_check（branch\_fn）

```rust
pub fn save_route_check(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
    // 从 input.input 中判断是否有 id 字段且非 null
    // 有 id → 返回 "2"（更新分支）
    // 无 id → 返回 "1"（新增分支）
}
```

#### validate\_account

```rust
pub fn validate_account(&self, input: &FunctionInput) -> Result<FunctionOutput, String> {
    // 1. 校验编码不为空且格式正确（纯数字）
    // 2. 校验科目类型合法（asset/liability/equity/cost/revenue/expense）
    // 3. 校验余额方向合法（debit/credit）
    // 4. 如果有 parent_id，查询父科目是否存在（host.db_query）
    // 5. 返回校验结果 { valid: bool, errors: [...] }
}
```

### 4.6 src/extism\_layer.rs

5 个 `#[plugin_fn]` 导出函数，保持 ExtismHost + PluginCore 调用模式：

* `save_account` — func

* `list_accounts` — func

* `get_account_detail` — func

* `save_route_check` — branch\_fn（`#[doc_type = "branch_fn"]`）

* `validate_account` — func

### 4.7 src/tests.rs

使用 MockHostFunctions 编写单元测试：

* `test_save_account_insert` — 测试新增科目

* `test_save_account_update` — 测试更新科目

* `test_list_accounts` — 测试列表查询

* `test_get_account_detail` — 测试详情获取

* `test_save_route_check_insert` — 测试新增路由

* `test_save_route_check_update` — 测试更新路由

* `test_validate_account_valid` — 测试校验通过

* `test_validate_account_invalid_type` — 测试校验失败

### 4.8 config/account\_config.json

```json
{
  "name": "account",
  "description": "会计科目表定义，支持资产/负债/权益/成本/损益六大类科目的树形管理",
  "depends_on": [],
  "priority": 0,
  "files": ["account_tables.json"],
  "seed_data": [
    {
      "table_name": "cmx_account",
      "file": "seeddata/account_seed.json",
      "conflict_columns": ["code"],
      "enabled": true
    }
  ]
}
```

### 4.9 config/account\_tables.json

定义 `cmx_account` 表，包含 13 个列和 3 个索引，格式严格参照 `domain_app_module_tables.json`。

### 4.10 seeddata/account\_seed.json

约 30 个一级科目种子数据，每条记录包含：id, code, code\_version, name, parent\_id(null), level(1), account\_type, direction, is\_leaf(true), is\_enabled(true), sort\_order, remark(null)。

### 4.11 servicedata/save_account_flow.json

科目保存服务编排流程，6 个节点 + 6 条边（含 switch 分支路由）：

```
start_1 --[out]--> validate_account --[out]--> save_route_check
save_route_check --[out_1]--> create_account --[out]--> end_1
save_route_check --[out_2]--> update_account --[out]--> end_1
```

### 4.12 servicedata/list_accounts_flow.json

科目列表查询服务编排流程，3 个节点 + 2 条边（线性流程）：

```
start_1 --[out]--> list_accounts --[out]--> end_1
```

### 4.13 servicedata/get_account_detail_flow.json

科目详情查询服务编排流程，3 个节点 + 2 条边（线性流程）：

```
start_1 --[out]--> get_account_detail --[out]--> end_1
```

### 4.14 menudata/account_menu.json

会计科目菜单配置，参照模板 `sample-menu.json` 格式：

```json
{
  "name": "会计科目管理",
  "version": "1.0.0",
  "description": "会计科目管理菜单配置",
  "items": [
    {
      "id": "account_manage",
      "label": "科目管理",
      "icon": "book",
      "path": "/finance/account",
      "children": [
        { "id": "account_list", "label": "科目列表", "path": "/finance/account/list" },
        { "id": "account_detail", "label": "科目详情", "path": "/finance/account/detail" }
      ]
    }
  ]
}
```

### 4.15 formdata/account_form.json

会计科目表单配置，参照模板 `sample-form.json` 格式：

```json
{
  "name": "会计科目表单",
  "version": "1.0.0",
  "description": "会计科目新增/编辑表单配置",
  "fields": [
    { "id": "code", "type": "input", "label": "科目编码", "placeholder": "请输入科目编码", "required": true },
    { "id": "name", "type": "input", "label": "科目名称", "placeholder": "请输入科目名称", "required": true },
    { "id": "account_type", "type": "select", "label": "科目类型", "required": true, "options": [
      { "label": "资产类", "value": "asset" },
      { "label": "负债类", "value": "liability" },
      { "label": "权益类", "value": "equity" },
      { "label": "成本类", "value": "cost" },
      { "label": "收入类", "value": "revenue" },
      { "label": "费用类", "value": "expense" }
    ]},
    { "id": "direction", "type": "select", "label": "余额方向", "required": true, "options": [
      { "label": "借方", "value": "debit" },
      { "label": "贷方", "value": "credit" }
    ]},
    { "id": "is_leaf", "type": "select", "label": "是否末级科目", "options": [
      { "label": "是", "value": "true" },
      { "label": "否", "value": "false" }
    ]},
    { "id": "remark", "type": "textarea", "label": "备注", "placeholder": "请输入备注" }
  ]
}
```

***

## 五、假设与决策

| 项目            | 决策                         | 原因                                 |
| ------------- | -------------------------- | ---------------------------------- |
| 新 crate 名称    | `cmx-account`              | 遵循 cmx- 前缀命名约定                     |
| 新 crate 路径    | `crates/libs/cmx-account/` | 与 wasmdemo 同级                      |
| 是否修改 wasmdemo | 否                          | 完全独立的新工程                           |
| 域/应用/模块编码     | FIN/FI/GL                  | 财务域/会计核算应用/总账模块                    |
| 插件ID          | `cmx-account`              | 与 crate 名称一致                       |
| SDK 引用方式      | path 引用（与 wasmdemo 一致）     | wasmdemo 例外，不强制 workspace 依赖       |
| SQL 拼接方式      | 字符串拼接（与 wasmdemo 现有方式一致）   | 当前 SDK 不支持参数化查询                    |
| 种子数据规模        | 约 30 个一级科目                 | 覆盖六大类，数据量适中                        |
| 服务编排流程 | 三个流程（保存/列表/详情） | 每个服务编排文件对应一个接口 |

***

## 六、验证步骤

1. **编译验证**：`cargo build -p cmx-account --release --target wasm32-wasip1 --features extism` 编译通过
2. **单元测试**：`cargo test -p cmx-account` 通过（使用 MockHostFunctions）
3. **元数据校验**：config JSON 和 tables JSON 格式与现有 `domain_app_module_tables.json` 一致
4. **种子数据校验**：seed JSON 中每条记录的 code 唯一，account\_type 值在合法范围内
5. **服务编排校验**：flow JSON 包含 skylake-start 和 skylake-end 节点，edges 连接完整

