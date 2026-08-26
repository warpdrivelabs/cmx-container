# Service 模式与 list / page 最佳实践

> 本文件是 `axum-handler-generator` 技能的 Service 层细则。与 modql 技能的分工：
> 本文管「Service 签名契约与 handler 协同」，modql 技能管「Filter / Fields / OpVals /
> ListOptions 的类型设计与 SQL 生成」。

---

## 一、两种 Service 模式

| 维度 | 静态 Service 模式 | 注入式 Service 模式 |
|------|------------------|--------------------|
| Service 调用 | `XxxService::create(mm, &db_id, ...)`（静态方法） | `cmx_state.<业务>()?.<子>_service.create_<子>(&svr_ctx, ...)`（trait 对象） |
| 典型 crate | cmx-biz、cmx-plugin | cmx-iam、cmx-auth |
| mm / db_id | handler 内 `get_default_db_manager()` + `get_db_id_from_header(&headers)` 逐次获取 | Service 内部持有，handler 不碰 |
| Service 定义 | `impl XxxService { pub async fn create(...) }` | `#[async_trait] impl XxxService for XxxServiceImpl { async fn create(&self, ...) }`，经 `Arc<dyn Trait>` 注入 `CmxAppState` |
| Entity 来源 | 模块 mod.rs `pub use cmx_biz::xxx::...` re-export（供宏引用） | handler 直接 `use cmx_iam::xxx::...` |
| CRUD 生成 | 可用 `declare_crud_handlers!` 宏 | 不用宏，全部手写 |
| Service 实现 | `cmx-biz/src/<module>/service.rs` | `cmx-iam/src/user/service.rs` |

两种模式的 `list` / `page` **签名契约完全相同**（filters + list_options），仅 `&self`
与 mm / db_id 参数不同。

---

## 二、list / page 最佳实践（核心契约）

### 2.1 一句话原则

> Service 的 `list` / `page` 方法必须接收 `filters: Option<Vec<F>>` 和
> `list_options: ListOptions` 两个结构化参数，handler 端只做「提取 + 透传」，
> 不重新组装 page / page_size / order_bys。

### 2.2 标准参数签名

| Service 方法 | filters 类型 | list_options 类型 | 返回值 | 场景 |
|------------|-------------|------------------|--------|------|
| `list` | `Option<Vec<XxxFilter>>` | `Option<ListOptions>` | `DataSet` | 列表（不带 total） |
| `page` | `Option<Vec<XxxFilter>>` | `ListOptions` | `(DataSet, i64)` | 分页（带 total） |
| `page_custom` | `Option<Vec<XxxFilter>>` | `ListOptions` | `(DataSet, i64)` | 多表 JOIN 分页 |
| `list_custom` | `Option<Vec<XxxFilter>>` | `Option<ListOptions>` | `DataSet` | 多表 JOIN 列表 |

**禁止** `(page, page_size, keyword, ...)` 平铺签名。

### 2.3 Handler 端三步提取（顺序固定）

```rust
// 1. 提取 ListOptions（含 limit/offset/order_bys）
let list_options = params.to_list_options();
// 2. 提取分页元信息（仅 page 需要，用于响应）
let page_number = params.get_page() as u64;
let page_size = params.get_size() as u64;
// 3. 提取 filters；空数组规范化为 None，便于 Service 走「无过滤」分支
let filters = params.filters.clone().filter(|v| !v.is_empty());
```

随后直接透传：

```rust
// list
let dataset = XxxService::list(mm, &db_id, filters, Some(list_options)).await?;
Ok(Json(ApiResp::ok(dataset)))

// page
let (dataset, total) = XxxService::page(mm, &db_id, filters, list_options).await?;
Ok(Json(ApiResp::ok_with_pagination(dataset, page_number, page_size, total as u64)))
```

### 2.4 为什么这是最佳实践（反模式对照）

| 反模式 | 问题 | 正确做法 |
|--------|------|---------|
| Service 接收 `page, page_size, keyword, ...` 平铺 | 加条件要改签名，调用方全改 | `filters + list_options`，加条件只改 Filter 结构体 |
| handler 组装分页/排序调 `page(mm, id, 1, 20, kw)` | handler 越长越像业务层 | handler 三步提取后透传 |
| Service 内硬编码默认过滤（如 app_id） | 复用 Service 要改源码 | handler 注入默认 filter（见 handler-templates.md §3.7） |
| 手搓 `{ limit, offset, order_bys }` | 字段名易错 | 统一 `modql::filter::ListOptions` |
| `filters: XxxFilter`（单组） | 无法表达 `(A AND B) OR C` | `Option<Vec<XxxFilter>>`（多组 OR） |

### 2.5 前端 JSON 传参约定（ListParams / PageParams body）

```json
{
    "filters": [
        { "name": { "$contains": "财务" }, "status": { "$eq": "published" } },
        { "type": { "$eq": "platform" } }
    ],
    "page": 1,
    "size": 20,
    "order_bys": "!create_time,code"
}
```

| 字段 | 类型 | 说明 |
|------|------|------|
| `filters` | `Vec<Filter>` 或 `null` | 多组过滤器，**组间 OR、组内 AND** |
| `page` | 整数 | 页码（从 1 开始），仅 PageParams |
| `size` | 整数 | 每页条数 |
| `order_bys` | 字符串 | 逗号分隔排序字段，前缀 `!` 降序，如 `!create_time,code` |

操作符（`$eq` / `$contains` / `$in` / ...）与字段类型映射查 **modql 技能**。

---

## 三、Service 层实现模板

### 3.1 静态 Service 模式（cmx-biz，真实骨架）

```rust
// crates/libs/cmx-biz/src/xxx/service.rs
use cmx_core::model::data::dataset::DataSet;
use cmx_database::crud::{CustomQueryService, GenericCrudService};
use cmx_database::DatabaseManager;
use modql::filter::ListOptions;
use sea_query::Value;

use crate::xxx::{XxxBmc, XxxFilter, XxxForCreate, XxxForUpdate};
use crate::error::Result;

pub struct XxxService;

impl XxxService {
    pub async fn create(mm: &DatabaseManager, db_id: &str, data: XxxForCreate) -> Result<DataSet> {
        GenericCrudService::<XxxBmc>::create(mm, db_id, None, data).await
    }

    pub async fn get(mm: &DatabaseManager, db_id: &str, id: &str) -> Result<DataSet> {
        GenericCrudService::<XxxBmc>::get(mm, db_id, None, id.into()).await
    }

    pub async fn update(mm: &DatabaseManager, db_id: &str, id: Value, data: XxxForUpdate) -> Result<DataSet> {
        GenericCrudService::<XxxBmc>::update(mm, db_id, None, id, data).await
    }

    pub async fn delete(mm: &DatabaseManager, db_id: &str, ids: Vec<Value>) -> Result<DataSet> {
        GenericCrudService::<XxxBmc>::delete(mm, db_id, None, ids).await
    }

    /// 列表查询。
    ///
    /// - `filters`：多组过滤器，组间 OR、组内 AND
    /// - `list_options`：分页与排序（None 用默认 limit）
    pub async fn list(
        mm: &DatabaseManager,
        db_id: &str,
        filters: Option<Vec<XxxFilter>>,
        list_options: Option<ListOptions>,
    ) -> Result<DataSet> {
        GenericCrudService::<XxxBmc, XxxFilter>::list(mm, db_id, None, filters, list_options).await
    }

    /// 分页查询。返回 `(DataSet, total)`，total 供前端分页器。
    pub async fn page(
        mm: &DatabaseManager,
        db_id: &str,
        filters: Option<Vec<XxxFilter>>,
        list_options: ListOptions,
    ) -> Result<(DataSet, i64)> {
        GenericCrudService::<XxxBmc, XxxFilter>::page(mm, db_id, None, filters, list_options).await
    }

    /// 多表 JOIN 自定义分页。Filter 字段需 `#[modql(rel = "主表别名")]`。
    pub async fn page_custom(
        mm: &DatabaseManager,
        db_id: &str,
        filters: Option<Vec<XxxFilter>>,
        list_options: ListOptions,
    ) -> Result<(DataSet, i64)> {
        let sql = r#"
            SELECT a.*, d.name as rel_name
            FROM cmx_xxx a
            LEFT JOIN cmx_xxx_rel d ON a.rel_code = d.code
        "#;
        CustomQueryService::page_custom(mm, db_id, None, filters, list_options, sql, "cmx-xxx").await
    }
}
```

`GenericCrudService` 的第三个参数是 `txn_id`：非事务传 `None`，事务内传 `Some(&txn_id)`。

### 3.2 注入式 Service 模式（cmx-iam 风格）

```rust
// crates/libs/cmx-iam/src/user/service.rs
#[async_trait::async_trait]
pub trait UserService: Send + Sync {
    async fn create_user(&self, ctx: &CmxSvrContext, data: UserForCreate) -> Result<User>;
    async fn page_users(
        &self,
        filters: Option<Vec<UserFilter>>,
        list_options: ListOptions,
    ) -> Result<(Vec<User>, i64)>;
    async fn list_users(
        &self,
        filters: Option<Vec<UserFilter>>,
        list_options: Option<ListOptions>,
    ) -> Result<Vec<User>>;
    // ...
}

pub struct UserServiceImpl { /* 持有 mm / db_id */ }

#[async_trait::async_trait]
impl UserService for UserServiceImpl {
    async fn page_users(
        &self,
        filters: Option<Vec<UserFilter>>,
        list_options: ListOptions,
    ) -> Result<(Vec<User>, i64)> {
        GenericCrudService::<UserBmc, UserFilter>::page(
            &self.mm, &self.db_id, None, filters, list_options,
        ).await
    }
    // ...
}
```

handler 端调用见 `handler-templates.md` §3.8（`cmx_state.iam()?.user_service.page_users(...)`）。

### 3.3 GenericCrudService vs 自定义 SQL 选择

| 场景 | 方式 |
|------|------|
| 单表 CRUD | `GenericCrudService` |
| 单表 list / page | `GenericCrudService::list/page` + FilterNodes |
| 多表 JOIN | `CustomQueryService::page_custom` + FilterNodes（`#[modql(rel)]`） |
| UPSERT / 聚合（GROUP BY / SUM）/ 跨表事务 | 手写 SQL（先调 **cmx-sql-execution** 技能） |

### 3.4 Service 方法参数规范

多参数时**必须**用结构体，禁止平铺：

```rust
// ❌ 错误：参数平铺
pub async fn publish(&self, id: String, name: Option<String>, /* 20+ 个 */) -> Result<Xxx>

// ✅ 正确：结构体参数
pub async fn publish(&self, req: PublishRequest) -> Result<Xxx>
```

例外：`filters + list_options` 本身是 modql 结构化参数，符合要求。

---

## 四、业务类型定义要点（Entity / BMC / Filter）

> 详细规则（OpVals 全表、操作符、表别名、HasSeaFields）以 **modql 技能** 为准，
> 此处仅列 handler 生成时必须知道的最小集合。真实目录：`crates/libs/cmx-biz/src/<module>/`。

```
<业务 crate>/src/xxx/
  ├── mod.rs       # pub use entity::...; pub use bmc::...; ...
  ├── entity.rs    # Entity / ForCreate / ForUpdate
  ├── bmc.rs       # Bmc（impl DbBmc）
  ├── filter.rs    # Filter（derive FilterNodes）
  └── service.rs   # Service（见上节）
```

### 4.1 Entity（entity.rs）

```rust
use modql::field::Fields;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;   // 需要出现在 OpenAPI 文档中的类型必须派生

/// 完整实体（查询返回用）。
#[derive(Debug, Clone, Serialize, Deserialize, Fields, ToSchema)]
pub struct Xxx {
    pub id: String,
    pub code: String,
    pub name: Option<String>,
    /* 审计字段：create_time / update_time / create_by / ... */
}

/// 创建 DTO。不含 id / create_time / update_time（GenericCrudService 自动生成）。
#[derive(Debug, Clone, Serialize, Deserialize, Fields, ToSchema)]
pub struct XxxForCreate {
    pub code: String,
    pub name: Option<String>,
}

/// 更新 DTO。全字段 Option，仅更新非 None 字段。
#[derive(Debug, Clone, Serialize, Deserialize, Fields, ToSchema)]
pub struct XxxForUpdate {
    pub name: Option<String>,
    pub status: Option<String>,
}
```

- `#[derive(Fields)]` 不可省略——GenericCrudService 依赖它构建 INSERT/UPDATE SQL。
- ForCreate 不含自动生成字段；ForUpdate 全 Option（AGENTS.md §八硬约束）。

### 4.2 Filter（filter.rs）

```rust
use modql::filter::{FilterNodes, OpValsString, OpValsInt64};
use serde::Deserialize;

/// 过滤器。
#[derive(Debug, Clone, FilterNodes, Deserialize, Default)]
pub struct XxxFilter {
    pub code: Option<OpValsString>,
    pub name: Option<OpValsString>,
    pub archived: Option<OpValsInt64>,
    // 多表 JOIN 时加表别名：#[modql(rel = "a")]
}
```

字段必须用 `Option<OpValsXxx>`，**禁止**原始 `String` / `i64`。

### 4.3 BMC（bmc.rs）

```rust
use cmx_database::crud::DbBmc;

pub struct XxxBmc;

impl DbBmc for XxxBmc {
    const TABLE: &'static str = "cmx_xxx";
    const PK_COLUMN: &'static str = "id";
    // 可覆盖：has_timestamps()(默认 true) / has_owner_id()(默认 false) / encrypted_fields()
}
```

---

## 五、DTO 归属决策

```
一个 Request / Response / DTO 给谁用？
  ├─ 仅本 handler 模块 ──────────────> <域 crate>/src/handlers/<module>/request.rs
  ├─ 本域多个 handler ───────────────> 提到域 crate 内公共位置
  ├─ api crate + 业务 crate + WASM/RPC（≥2 类消费方）
  │     ───────────────────────────> cmx-core/src/model/<子域>/（真源，经 lib.rs re-export）
  └─ 纯 axum 通用参数（PageParams 等）
        ───────────────────────────> cmx-core/src/model/data/request/params.rs（已有，勿重复定义）
```

cmx-core 依赖约束（AGENTS.md §十四，基于真实 Cargo.toml）：

- ✅ 允许且已用：`serde` / `serde_json` / `utoipa`(optional) / `chrono` / `uuid` /
  `thiserror` / **`modql`**（`ListOptions` 来源；workspace 用本地 path 依赖）。
- ❌ 禁止：`sea-query` / `axum` / `cmx-database` / 业务 crate / 重量级二进制依赖。
- 修改 cmx-core 依赖前必须核对真实 `Cargo.toml`。

新增共享 DTO 步骤：

1. 在 `crates/libs/cmx-core/src/model/<子域>/` 新建或追加类型（派生
   `Serialize/Deserialize/ToSchema`）。
2. 在对应 `model/<子域>/mod.rs` 与需要处 re-export。
3. 各使用方 `use cmx_core::model::<子域>::Xxx;` 引用。
4. `cargo check -p cmx-core` 确认依赖方向无违反。

---

## 六、检查清单（Service + list/page）

- [ ] Service `list` / `page` 用 `(filters: Option<Vec<F>>, list_options)` 签名？
- [ ] 无 `(page, page_size, keyword, ...)` 平铺签名？
- [ ] handler 只做「提取 + 透传」，未重组分页/排序？
- [ ] `filters.filter(|v| !v.is_empty())` 把空数组规范化为 None？
- [ ] 多表 JOIN 时 Filter 字段带 `#[modql(rel = "表别名")]`？
- [ ] 多租户 app_id 默认值在 handler 注入，不在 Service 硬编码？
- [ ] `page` 返回 `(DataSet, total)` 且经 `ApiResp::ok_with_pagination` 响应？
- [ ] 排序经前端 `order_bys` 传入，Service 未硬编码？
- [ ] ForCreate 不含 id / 时间戳；ForUpdate 全 Option？
- [ ] Entity 派生 `Fields`、Filter 字段用 `Option<OpValsXxx>`？
- [ ] 事务内调用时 txn_id 传 `Some(&txn_id)`？
