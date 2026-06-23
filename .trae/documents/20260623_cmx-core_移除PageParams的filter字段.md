# 移除 PageParams/ListParams 中的 filter 字段

## 概述

将 `PageParams`、`ListParams`（cmx-core）以及 `ListParamsDoc`、`PageParamsDoc`（cmx-api-types）中的单数 `filter: Option<F>` 字段移除，统一使用复数 `filters: Option<Vec<F>>`。所有 handler 中引用 `params.filter` 的代码改为使用 `params.filters`，并在 `filters` 为空数组时设置为 `None`。

## 当前状态分析

### 结构定义（4 处 filter 字段）

| 文件 | 结构 | filter 行号 |
|------|------|-------------|
| `crates/libs/cmx-core/src/model/data/request/params.rs` | `ListParams<F>` | 59 |
| `crates/libs/cmx-core/src/model/data/request/params.rs` | `PageParams<F>` | 84 |
| `crates/libs/cmx-api-types/src/param_doc.rs` | `ListParamsDoc<F>` | 53 |
| `crates/libs/cmx-api-types/src/param_doc.rs` | `PageParamsDoc<F>` | 81 |

### Handler 引用 `params.filter` 的地方（7 个文件，10 处）

| 文件 | 函数 | 行号 | 当前模式 |
|------|------|------|----------|
| `cmx-api/src/rest/handler.rs` | `list` | 228 | `if let Some(filter) = params.filter.clone()` → 包装为 vec |
| `cmx-api/src/rest/handler.rs` | `page` | 272 | 同上 |
| `cmx-api/src/handlers/service/handler.rs` | `page_services` | 883 | `params.filter.clone().unwrap_or_default()` |
| `cmx-api/src/handlers/application/handler.rs` | `application_custom_page` | 68 | `if let Some(filter) = params.filter.clone()` |
| `cmx-api/src/handlers/module/handler.rs` | `module_custom_page` | 76 | 同上 |
| `cmx-api/src/handlers/plugin/handler.rs` | `plugin_list` | 519 | `params.filter.unwrap_or_default().into()` |
| `cmx-api/src/handlers/plugin/handler.rs` | `plugin_page` | 600 | 同上 |
| `cmx-api/src/handlers/table_metadata/handler.rs` | `table_metadata_list` | 84 | `if let Some(filter) = params.filter.clone()` |
| `cmx-api/src/handlers/table_metadata/handler.rs` | `table_metadata_page` | 142 | 同上 |
| `cmx-api/src/handlers/marketplace/handler.rs` | `marketplace_plugin_page` | 166 | `if let Some(filter) = params.filter.clone()` |

### 已使用 `params.filters` 的地方（无需改动 filter 逻辑）

- `cmx-api/src/handlers/iam/role/handler.rs` — `params.filters.and_then(|v| v.into_iter().next()).unwrap_or_default()`
- `cmx-api/src/handlers/iam/user/handler.rs` — 同上
- `cmx-api/src/handlers/iam/permission/handler.rs` — 同上

## 修改方案

### 1. 移除结构定义中的 filter 字段

#### 1.1 `crates/libs/cmx-core/src/model/data/request/params.rs`

**ListParams**（第 57-64 行）：移除第 59 行 `pub filter: Option<F>,`

```rust
// 修改后
pub struct ListParams<F> {
    /// 多个过滤条件（用于or查询）
    pub filters: Option<Vec<F>>,
    /// 排序字段（支持多个，用逗号分隔，前缀 - 表示降序）
    pub order_bys: Option<String>,
}
```

**PageParams**（第 82-96 行）：移除第 84 行 `pub filter: Option<F>,`

```rust
// 修改后
pub struct PageParams<F> {
    /// 多个过滤条件（用于or查询）
    pub filters: Option<Vec<F>>,
    /// 页码（从 1 开始）
    #[serde(default = "default_page")]
    pub current: Option<i64>,
    /// 每页条数
    #[serde(default = "default_size")]
    pub size: Option<i64>,
    /// 排序字段（支持多个，用逗号分隔，前缀 - 表示降序）
    pub order_bys: Option<String>,
}
```

**测试代码**（第 145-230 行）：移除所有 `filter: None,` 行（共 6 处：ListParams 测试 1 处，PageParams 测试 5 处）。

#### 1.2 `crates/libs/cmx-api-types/src/param_doc.rs`

**ListParamsDoc**（第 51-58 行）：移除第 53 行 `pub filter: Option<F>,`

**PageParamsDoc**（第 79-90 行）：移除第 81 行 `pub filter: Option<F>,`

### 2. 修改 Handler 代码

统一规则：移除 `params.filter` 相关逻辑，使用 `params.filters` 并在空数组时设为 `None`。空数组检查使用等价于用户给出的 `is_none() || unwrap().is_empty()` 的安全写法 `filter(|v| !v.is_empty())`。

#### 2.1 `cmx-api/src/rest/handler.rs` — `list` 函数（第 222-233 行）

```rust
// 修改前
let mut filters = params.filters.clone();
if params.filters.is_none() || params.filters.unwrap().is_empty() {
  filters = None;
}
//都存在时优先使用filter
if let Some(filter) = params.filter.clone() {
  filters = Some(vec![filter]);
}

let dataset = GenericCrudService::<MC, F>::list(mm, &db_id, None, filters, Some(list_options)).await?;

// 修改后
let filters = params.filters.clone().filter(|v| !v.is_empty());

let dataset = GenericCrudService::<MC, F>::list(mm, &db_id, None, filters, Some(list_options)).await?;
```

#### 2.2 `cmx-api/src/rest/handler.rs` — `page` 函数（第 267-276 行）

```rust
// 修改前
let mut filters = params.filters.clone();
if params.filters.is_none() || params.filters.unwrap().is_empty() {
    filters = None;
}
//都存在时优先使用filter
if let Some(filter) = params.filter.clone() {
    filters = Some(vec![filter]);
}

let (dataset, total) = GenericCrudService::<MC, F>::page(mm, &db_id, None, filters, list_options).await?;

// 修改后
let filters = params.filters.clone().filter(|v| !v.is_empty());

let (dataset, total) = GenericCrudService::<MC, F>::page(mm, &db_id, None, filters, list_options).await?;
```

#### 2.3 `cmx-api/src/handlers/service/handler.rs` — `page_services`（第 883 行）

改为从 `filters` 取第一个元素（与 IAM handler 模式一致）：

```rust
// 修改前
let filter = params.filter.clone().unwrap_or_default();

// 修改后
let filter = params.filters
    .and_then(|v| v.into_iter().next())
    .unwrap_or_default();
```

#### 2.4 `cmx-api/src/handlers/application/handler.rs` — `application_custom_page`（第 67-73 行）

```rust
// 修改前
let mut filters = params.filters.clone();
if let Some(filter) = params.filter.clone() {
    filters = Some(vec![filter]);
}
if filters.is_none() || filters.as_ref().unwrap().is_empty() {
    filters = None;
}

// 修改后
let mut filters = params.filters.clone().filter(|v| !v.is_empty());
```

#### 2.5 `cmx-api/src/handlers/module/handler.rs` — `module_custom_page`（第 75-81 行）

```rust
// 修改前
let mut filters = params.filters.clone();
if let Some(filter) = params.filter.clone() {
    filters = Some(vec![filter]);
}
if filters.is_none() || filters.as_ref().unwrap().is_empty() {
    filters = None;
}

// 修改后
let mut filters = params.filters.clone().filter(|v| !v.is_empty());
```

#### 2.6 `cmx-api/src/handlers/plugin/handler.rs` — `plugin_list`（第 519-521 行）

```rust
// 修改前
let mut filter: cmx_plugin::domain::plugin::PluginFilter = params.filter
    .unwrap_or_default()
    .into();

// 修改后
let mut filter: cmx_plugin::domain::plugin::PluginFilter = params.filters
    .and_then(|v| v.into_iter().next())
    .unwrap_or_default()
    .into();
```

#### 2.7 `cmx-api/src/handlers/plugin/handler.rs` — `plugin_page`（第 600-602 行）

```rust
// 修改前
let mut filter: cmx_plugin::domain::plugin::PluginFilter = params.filter
    .unwrap_or_default()
    .into();

// 修改后
let mut filter: cmx_plugin::domain::plugin::PluginFilter = params.filters
    .and_then(|v| v.into_iter().next())
    .unwrap_or_default()
    .into();
```

#### 2.8 `cmx-api/src/handlers/table_metadata/handler.rs` — `table_metadata_list`（第 82-89 行）

```rust
// 修改前
let mut filters = params.filters.clone();
//都存在时优先使用filter
if let Some(filter) = params.filter.clone() {
    filters = Some(vec![filter]);
}
if params.filters.is_none() || params.filters.unwrap().is_empty() {
    filters = None;
}

// 修改后
let mut filters = params.filters.clone().filter(|v| !v.is_empty());
```

#### 2.9 `cmx-api/src/handlers/table_metadata/handler.rs` — `table_metadata_page`（第 137-144 行）

```rust
// 修改前
let mut filters = params.filters.clone();
if params.filters.is_none() || params.filters.unwrap().is_empty() {
    filters = None;
}
//都存在时优先使用filter
if let Some(filter) = params.filter.clone() {
    filters = Some(vec![filter]);
}

// 修改后
let mut filters = params.filters.clone().filter(|v| !v.is_empty());
```

#### 2.10 `cmx-api/src/handlers/marketplace/handler.rs` — `marketplace_plugin_page`（第 166-176 行）

```rust
// 修改前
let filters: Option<Vec<MarketplacePluginFilter>> = if let Some(filter) = params.filter.clone() {
    Some(vec![filter.into()])
} else if let Some(fs) = params.filters.clone() {
    if !fs.is_empty() {
        Some(fs.into_iter().map(Into::into).collect())
    } else {
        None
    }
} else {
    None
};

// 修改后
let filters: Option<Vec<MarketplacePluginFilter>> = params.filters
    .clone()
    .filter(|v| !v.is_empty())
    .map(|fs| fs.into_iter().map(Into::into).collect());
```

## 假设与决策

1. **空数组检查写法**：用户给出的 `if params.filters.is_none() || params.filters.unwrap().is_empty()` 虽然因短路求值不会 panic，但调用 `.unwrap()` 不够 idiomatic。统一使用 `Option::filter(|v| !v.is_empty())` 实现等价逻辑（None 或空数组 → None，非空数组 → Some）。

2. **单 filter → filters 转换模式**：对于 `service`、`plugin` 等需要单个 filter 对象（而非 Vec）的 handler，采用与 IAM handler 一致的模式 `params.filters.and_then(|v| v.into_iter().next()).unwrap_or_default()`，取第一个元素。

3. **IAM handlers 无需修改**：role/user/permission handler 已只使用 `params.filters`，不引用 `params.filter`，无需改动。

4. **domain handler 无需修改**：用户当前打开的 `domain/handler.rs` 不使用 PageParams/ListParams，无需改动。

5. **行为变更说明**：原先部分 handler "filter 优先于 filters"，移除后统一只使用 `filters`。前端调用方需确保将单个过滤条件放入 `filters` 数组中传递。

## 验证步骤

1. `rtk cargo check -p cmx-core` — 验证 cmx-core 编译通过
2. `rtk cargo check -p cmx-api-types` — 验证 cmx-api-types 编译通过
3. `rtk cargo check -p cmx-api` — 验证 cmx-api 编译通过
4. `rtk cargo build` — 全量编译验证
5. `rtk cargo test -p cmx-core` — 运行 cmx-core 单元测试
6. `rtk cargo clippy` — 检查无警告
