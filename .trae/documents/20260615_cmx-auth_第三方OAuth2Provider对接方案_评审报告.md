# 第三方 OAuth2 Provider 对接方案 — 评审报告（v5）

> 评审对象：`20260615_cmx-auth_第三方OAuth2Provider对接方案.md`（v5）
> 评审日期：2026-06-15
> 评审结论：**可通过，有 1 项需补充**

---

## 评审摘要

v4 评审 2 项问题已**全部修复**。新发现 1 项问题（依赖遗漏），不影响架构设计，补充即可。

| v4 编号 | 问题 | v5 处理 | 状态 |
|---------|------|---------|------|
| 新-09 | `create_account` INSERT 缺少 `id` 列 | 改用 `GenericCrudService::create`，id 由 `#[derive(Fields)]` 自动生成 | 已修复 |
| 新-10 | 数据访问方法 API 与 `DatabaseManager` 不匹配 | 全部改用 `GenericCrudService`（`list`/`create`/`count`/`delete`），补充 `OAuth2AccountBmc`/`OAuth2AccountForCreate`/`OAuth2AccountFilter` | 已修复 |

---

## 新发现问题

### 新-11：`modql` 依赖未列入 §8 新增依赖 [重要]

**位置**：方案 §3.6 `AccountLinker`、§8 新增依赖

**问题**：`AccountLinker` 代码使用了 `modql` 的多个类型和 derive 宏：

```rust
use modql::filter::{OpValsString, OpValsInt64, OpValString};
use modql::field::Fields;

#[derive(FilterNodes, Deserialize, Default)]
pub struct OAuth2AccountFilter { ... }

#[derive(Fields)]
pub struct OAuth2AccountForCreate { ... }
```

但 §8.2 `cmx-auth Cargo.toml 新增` 中未列出 `modql` 依赖。`modql` 是 workspace 内部 crate（`path = "crates/libs/modql"`），当前 cmx-auth 的 Cargo.toml 中也没有此依赖。

**建议**：在 §8.2 补充：

```toml
# 内部依赖 - 查询过滤/字段映射（GenericCrudService 的 FilterNodes + Fields）
modql = { workspace = true }
```

---

## 评审结论

方案经过 5 轮评审，累计 32 项问题已修复 31 项，仅剩 1 项依赖遗漏。

**结论：方案可通过。** 实施时补充 `modql` 依赖即可。
