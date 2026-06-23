# Clippy 警告修复计划 - 2026-06-24 (第二轮)

## 检查结果概览

- 检查时间：2026-06-24
- 警告总数：83 条（第一轮修复后新暴露）
- 待修复数：73 条（排除 too_many_arguments 10 条）
- 已排除数：10 条（too_many_arguments）

## 待修复警告列表

### 警告类型统计

| 警告类型 | 数量 | 是否修复 |
|---------|------|---------|
| collapsible_if | 22 | 修复 |
| redundant_closure | 16 | 修复 |
| useless_conversion | 7 | 修复 |
| too_many_arguments | 10 | **排除** |
| field_reassign_with_default | 5 | 修复 |
| map_unwrap_or | 3 | 修复 |
| option_as_ref (called .as_ref().map(|s| s.as_str())) | 3 | 修复 |
| unused_imports | 1 | 修复 |
| unused_variables | 1 | **排除** |
| dead_code (fields/methods/struct/functions) | 6 | 修复 |
| needless_borrow | 1 | 修复 |
| explicit_auto_deref | 1 | 修复 |
| doc_list_item_without_indentation | 1 | 修复 |
| unnecessary_closure (Option::None) | 1 | 修复 |
| let_and_return | 1 | 修复 |
| unused_unit | 1 | 修复 |
| idx never read | 1 | 修复 |
| should_implement_trait (from_str) | 1 | 修复 |
| unnecessary_unwrap | 1 | 修复 |
| borrowed_expression | 1 | 修复 |

### 按 crate 分布

| Crate | 警告数 | 可自动修复 |
|-------|-------|-----------|
| web-server | 10 | 9 |
| cmx-iam | 19 | 15 |
| cmx-auth | 11 | 8 |
| cmx-api | 4 (lib) + 8 (test) | 4 + 部分 |
| cmx-plugin | 7 | 5 |
| cmx-database | 7 | 7 |
| cmx-rpc | 7 | 2 |
| cmx-service | 1 | 0 (排除) |
| cmx-biz | 1 | 0 (排除) |
| cmx-utils | 1 | 1 |
| cmx-buffer | 5 (test) | 部分 |

## 修复方案

### 阶段一：自动修复（cargo clippy --fix）

按 crate 执行自动修复：
- `cmx-database` - 7 处
- `cmx-auth` - 8 处
- `cmx-rpc` - 2 处
- `cmx-plugin` - 5 处
- `cmx-iam` - 15 处
- `cmx-api` - 4 处
- `web-server` - 9 处

### 阶段二：手动修复

- dead_code（cmx-api/tests/common/mod.rs 中的未使用字段/方法/结构体/函数）
- doc_list_item_without_indentation（cmx-iam/src/rule/enforcer.rs:63）
- should_implement_trait（cmx-iam/src/rule/entity.rs:26 - from_str 方法）
- idx never read（cmx-iam/src/rule/service.rs:488）
- unused_imports（cmx-iam/src/permission/consistency_check.rs:12）
- unnecessary_unwrap（cmx-auth/src/auth_service_impl.rs）
- 其他自动修复未覆盖的警告

### 阶段三：验证

运行 `cargo clippy --all-targets` 验证修复结果。
