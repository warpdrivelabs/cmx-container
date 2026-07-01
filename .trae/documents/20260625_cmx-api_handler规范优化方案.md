# cmx-api Handler 规范优化方案

## 一、现状分析

根据 axum-handler-generator 技能规范检查，发现以下不符合规范的地方：

### 1.1 核心问题：仍在使用 `*Doc` 包装类型

**问题描述**：
- cmx-core 已经支持 utoipa（通过 `#[cfg_attr(feature = "openapi", derive(ToSchema))]`）
- 但 cmx-api 中仍有 12 个 handler 文件使用旧的 `*Doc` 包装类型
- 这违反了技能规范 2.1 节："handler 函数签名使用 `cmx_core::PageParams<F>` 等，`#[utoipa::path]` 宏的 `request_body` 直接使用同一个类型（无需 `*Doc` 包装）"

**影响范围**：
- `table_metadata/handler.rs` (2处)
- `module/handler.rs` (1处)
- `iam/user/handler.rs` (2处)
- `iam/role/handler.rs` (2处)
- `iam/role_group/handler.rs` (2处)
- `iam/permission/handler.rs` (2处)
- `application/handler.rs` (1处)
- `marketplace/handler.rs` (1处)
- `plugin/handler.rs` (2处)
- `service/handler.rs` (1处)
- `routes/macros.rs` (宏定义，影响所有使用宏的 handler)

**总计**：约 18 处需要修改

### 1.2 次要问题：变量命名不统一

**问题描述**：
- 部分 handler 使用 `current` 而非 `page_number`
- 部分 handler 使用 `size` 而非 `page_size`
- 虽然功能正确，但不符合技能规范 14.4 节的标准命名

**影响范围**：
- `iam/user/handler.rs`
- `iam/role/handler.rs`
- `iam/role_group/handler.rs`
- `iam/permission/handler.rs`
- `marketplace/handler.rs`
- `plugin/handler.rs`
- `service/handler.rs`

### 1.3 已符合规范的部分 ✓

- 所有 handler 模块都实现了 `ModuleRoutes` trait
- cmx-api 内部没有 Entity/BMC/Filter/Service 定义（职责边界清晰）
- 大部分 handler 已经使用 `to_list_options()` + `get_page()/get_size()` 三步提取模式
- Service 方法签名基本符合 `(filters: Option<Vec<F>>, list_options: ListOptions)` 规范

---

## 二、优化方案（从易到难）

### 阶段一：替换 `*Doc` 包装（P0 - 最容易）

**目标**：将所有 `*Doc` 包装替换为直接使用 cmx_core 类型

**修改范围**：
1. 10 个 handler 文件的 `#[utoipa::path]` 注解
2. `routes/macros.rs` 中的宏定义

**具体步骤**：

#### Step 1: 修改单个 handler 文件

**修改前**：
```rust
#[utoipa::path(
    post,
    path = "/api/xxx/page",
    request_body = crate::PageParamsDoc<serde_json::Value>,
    // ...
)]
pub async fn xxx_page(
    // ...
    Json(params): Json<cmx_core::PageParams<XxxFilter>>,
) -> Result<Json<ApiResp<DataSet>>> {
    // ...
}
```

**修改后**：
```rust
#[utoipa::path(
    post,
    path = "/api/xxx/page",
    request_body = cmx_core::PageParams<XxxFilter>,  // ✅ 直接使用 cmx_core 类型
    // ...
)]
pub async fn xxx_page(
    // ...
    Json(params): Json<cmx_core::PageParams<XxxFilter>>,
) -> Result<Json<ApiResp<DataSet>>> {
    // ...
}
```

**注意事项**：
- 如果 Filter 类型没有派生 `ToSchema`，需要先给 Filter 添加 `#[cfg_attr(feature = "openapi", derive(ToSchema))]`
- 对于使用 `serde_json::Value` 作为泛型参数的情况，需要改为具体的 Filter 类型

#### Step 2: 修改宏定义

**文件**：`crates/libs/cmx-api/src/routes/macros.rs`

**修改前**：
```rust
request_body = UpdatePayloadDoc<$entity_update>,
// ...
request_body = DeletePayloadDoc,
// ...
request_body = ListParamsDoc<serde_json::Value>,
// ...
request_body = PageParamsDoc<serde_json::Value>,
```

**修改后**：
```rust
request_body = cmx_core::UpdatePayload<$entity_update>,
// ...
request_body = cmx_core::DeletePayload,
// ...
request_body = cmx_core::ListParams<$filter>,  // 需要传入 $filter 参数
// ...
request_body = cmx_core::PageParams<$filter>,  // 需要传入 $filter 参数
```

**影响评估**：
- 需要更新所有调用 `declare_crud_handlers!` 宏的地方，传入 Filter 类型参数
- 影响文件：`crates/libs/cmx-api/src/routes/crud_handlers.rs`

#### Step 3: 清理 param_doc.rs

**文件**：`crates/libs/cmx-api-types/src/param_doc.rs`

**操作**：
- 确认所有引用都已替换后，删除整个文件
- 从 `crates/libs/cmx-api-types/src/lib.rs` 中移除 `pub mod param_doc;` 和 `pub use param_doc::*;`
- 从 `crates/libs/cmx-api/src/rest/mod.rs` 中移除相关 re-export
- 从 `crates/libs/cmx-api/src/lib.rs` 中移除相关 re-export

**验证方式**：
```bash
cargo build --package cmx-api
cargo test --package cmx-api
```

---

### 阶段二：统一变量命名（P1 - 中等难度）

**目标**：统一所有 handler 中的分页变量命名

**修改范围**：
- 7 个 handler 文件

**具体步骤**：

#### Step 1: 统一命名规范

**修改前**：
```rust
let current = params.get_page() as u64;
let size = params.get_size() as u64;
```

**修改后**：
```rust
let page_number = params.get_page() as u64;
let page_size = params.get_size() as u64;
```

**影响文件**：
1. `iam/user/handler.rs`
2. `iam/role/handler.rs`
3. `iam/role_group/handler.rs`
4. `iam/permission/handler.rs`
5. `marketplace/handler.rs`
6. `plugin/handler.rs`
7. `service/handler.rs`

#### Step 2: 更新响应调用

**修改前**：
```rust
Ok(Json(ApiResp::ok_with_pagination(dataset, current, size, total as u64)))
```

**修改后**：
```rust
Ok(Json(ApiResp::ok_with_pagination(dataset, page_number, page_size, total as u64)))
```

**验证方式**：
```bash
cargo build --package cmx-api
cargo test --package cmx-api
```

---

### 阶段三：优化宏系统（P2 - 较难）

**目标**：让宏系统支持更灵活的 Filter 类型传递

**当前问题**：
- 宏定义中使用 `serde_json::Value` 作为 Filter 类型的占位符
- 这导致 OpenAPI 文档不够精确

**优化方案**：

#### Step 1: 扩展宏参数

**修改前**：
```rust
declare_crud_handlers!(
    domain_crud,
    crate::handlers::domain::Domain,
    crate::handlers::domain::DomainBmc,
    crate::handlers::domain::DomainForCreate,
    crate::handlers::domain::DomainForUpdate,
    crate::handlers::domain::DomainFilter,  // 已有 Filter 参数
    "Domain",
    "/domains"
);
```

**修改后**：
```rust
// 宏定义已经接收 Filter 参数，只需在宏内部正确使用
macro_rules! declare_crud_handlers {
    (
        $module:ident,
        $entity:ty,
        $bmc:ty,
        $create_dto:ty,
        $update_dto:ty,
        $filter:ty,  // ✅ 使用这个参数
        $tag:expr,
        $prefix:expr
    ) => {
        // ...
        request_body = cmx_core::ListParams<$filter>,
        request_body = cmx_core::PageParams<$filter>,
        // ...
    }
}
```

**验证方式**：
```bash
cargo build --package cmx-api
cargo test --package cmx-api
# 检查 OpenAPI 文档是否正确生成
curl http://localhost:8080/api-doc/openapi.json | jq .
```

---

## 三、实施顺序与风险评估

### 3.1 实施顺序

```
阶段一（P0）→ 阶段二（P1）→ 阶段三（P2）
```

### 3.2 风险评估

| 阶段 | 风险等级 | 影响范围 | 回滚难度 |
|------|---------|---------|---------|
| 阶段一 | 低 | 仅修改注解，不影响运行时逻辑 | 简单（git revert） |
| 阶段二 | 低 | 仅修改变量名，不影响逻辑 | 简单（git revert） |
| 阶段三 | 中 | 修改宏系统，可能影响所有使用宏的 handler | 中等（需要测试所有 CRUD handler） |

### 3.3 依赖关系

- 阶段一和阶段二可以并行实施
- 阶段三依赖阶段一完成（需要先替换完所有 `*Doc` 才能清理 param_doc.rs）

---

## 四、验证清单

### 4.1 编译验证

```bash
# 编译整个 workspace
cargo build

# 编译 cmx-api
cargo build --package cmx-api

# 编译 cmx-api-types
cargo build --package cmx-api-types
```

### 4.2 测试验证

```bash
# 运行所有测试
cargo test

# 运行 cmx-api 测试
cargo test --package cmx-api

# 运行集成测试
cargo test --package cmx-api --test '*'
```

### 4.3 功能验证

```bash
# 启动服务
cargo run --bin cmx-server

# 检查 OpenAPI 文档
curl http://localhost:8080/api-doc/openapi.json | jq .

# 测试分页查询
curl -X POST http://localhost:8080/api/domains/page \
  -H "Content-Type: application/json" \
  -d '{"page": 1, "size": 20, "filters": []}'
```

### 4.4 代码质量验证

```bash
# 代码格式化检查
cargo fmt -- --check

# Clippy 检查
cargo clippy --package cmx-api -- -D warnings
```

---

## 五、时间估算

| 阶段 | 工作量 | 预计耗时 |
|------|--------|---------|
| 阶段一 | 修改 10 个 handler 文件 + 1 个宏文件 | 2-3 小时 |
| 阶段二 | 修改 7 个 handler 文件 | 1 小时 |
| 阶段三 | 修改宏定义 + 测试 | 2-3 小时 |
| **总计** | - | **5-7 小时** |

---

## 六、后续优化建议

1. **自动化检查**：添加 CI 检查，禁止使用 `*Doc` 包装类型
2. **文档更新**：更新 axum-handler-generator 技能文档，明确标注 `*Doc` 已废弃
3. **迁移指南**：编写迁移指南，帮助其他开发者理解变更原因
4. **性能优化**：考虑移除 `param_doc.rs` 后，减少编译时间

---

## 七、附录

### 7.1 相关文件清单

**需要修改的文件**：
- `crates/libs/cmx-api-types/src/param_doc.rs`（待删除）
- `crates/libs/cmx-api-types/src/lib.rs`
- `crates/libs/cmx-api/src/rest/mod.rs`
- `crates/libs/cmx-api/src/lib.rs`
- `crates/libs/cmx-api/src/routes/macros.rs`
- `crates/libs/cmx-api/src/handlers/table_metadata/handler.rs`
- `crates/libs/cmx-api/src/handlers/module/handler.rs`
- `crates/libs/cmx-api/src/handlers/iam/user/handler.rs`
- `crates/libs/cmx-api/src/handlers/iam/role/handler.rs`
- `crates/libs/cmx-api/src/handlers/iam/role_group/handler.rs`
- `crates/libs/cmx-api/src/handlers/iam/permission/handler.rs`
- `crates/libs/cmx-api/src/handlers/application/handler.rs`
- `crates/libs/cmx-api/src/handlers/marketplace/handler.rs`
- `crates/libs/cmx-api/src/handlers/plugin/handler.rs`
- `crates/libs/cmx-api/src/handlers/service/handler.rs`

**参考文件**：
- `/media/yqs/工作/rustspace/cmx/cmx-container/.trae/skills/axum-handler-generator/SKILL.md`

### 7.2 关键代码片段

**cmx-core 的 utoipa 支持示例**：
```rust
// crates/libs/cmx-core/src/model/data/request/params.rs
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct PageParams<F> {
    pub page: u64,
    pub size: u64,
    pub filters: Option<Vec<F>>,
    pub order_bys: Option<String>,
}
```

**正确的 handler 注解示例**：
```rust
#[utoipa::path(
    post,
    path = "/api/domains/page",
    request_body = cmx_core::PageParams<DomainFilter>,  // ✅ 正确
    responses((status = 200, description = "分页查询", body = ApiResp<DataSet>)),
    tag = "Domain"
)]
pub async fn page_domains(
    // ...
) -> Result<Json<ApiResp<DataSet>>> {
    // ...
}
```
