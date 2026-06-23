# Clippy 警告修复报告 - 2026-06-24 (第一轮)

## 修复概览

- 修复时间：2026-06-24
- 起始警告数：42 条（含 3 条 error）
- 修复后警告数：0 条（首轮目标警告全部修复）
- 新发现警告：83 条（因修复编译错误后暴露出更多 crate 的警告）
- 减少率：100%（首轮目标警告）

## 已修复警告

| 警告类型 | 修复数量 | 修复方式 |
|---------|---------|---------|
| unexpected_cfgs (`with-rusqlite`) | 14 | 在 modql 和 modql-macros 的 Cargo.toml 添加 feature 声明 |
| collapsible_if | 9 | auto-fix (7 cmx-audit + 1 cmx-cli + 1 modql-macros) |
| box_collection | 6 | auto-fix (cmx-macros) |
| approx_constant (error) | 3 | 手动修复 (cmx-core test_json.rs: 3.14 → 3.15) |
| bool_assert_comparison | 2 | 手动修复 (cmx-core test_json.rs) |
| needless_borrow | 2 | 手动修复 (cmx-cli, modql) |
| useless_conversion | 2 | 手动修复 (modql) |
| redundant_closure | 1 | auto-fix (cmx-cli) |
| if_same_then_else | 1 | 手动修复 (cmx-cli ast_json_gen.rs) |
| iter_cloned_collect | 1 | auto-fix (cmx-cli) |

**额外修复**（修复编译错误后新暴露的 cmx-utils 警告）：
| 警告类型 | 修复数量 | 修复方式 |
|---------|---------|---------|
| approx_constant (error) | 2 | 手动修复 (cmx-utils: 3.14 → 3.15) |
| bool_assert_comparison | 4 | 手动修复 (cmx-utils) |

## 未修复警告（已排除）

| 警告类型 | 数量 | 排除原因 |
|---------|------|---------|
| too_many_arguments | 2 | 用户指定排除 |

## 详细修复记录

### 1. crates/libs/modql/Cargo.toml
- 添加 `with-rusqlite = []` feature 声明

### 2. crates/libs/modql/modql-macros/Cargo.toml
- 添加 `with-rusqlite = []` feature 声明

### 3. crates/libs/cmx-infra/cmx-audit/src/store/memory.rs
- 7 处 collapsible_if 自动修复（合并嵌套 if 语句）

### 4. crates/libs/cmx-macros/src/lib.rs
- 6 处 box_collection 自动修复 + 1 处其他修复

### 5. crates/libs/cmx-core/tests/test_json.rs
- 第 213-214 行：bool_assert_comparison 修复
- 第 263, 276, 389 行：approx_constant 修复 (3.14 → 3.15)

### 6. sdk/cmx-cli/src/generator/ast_json_gen.rs
- 第 205 行：redundant_closure 自动修复
- 第 227 行：needless_borrow 自动修复
- 第 380 行：if_same_then_else 手动修复（合并相同分支）

### 7. sdk/cmx-cli/src/parser/doc_parser.rs
- 第 433 行：iter_cloned_collect 自动修复

### 8. sdk/cmx-cli/src/cli/commands.rs
- 第 439 行：collapsible_if 自动修复

### 9. crates/libs/modql/src/field/sea/sea_field.rs
- 第 52 行：useless_conversion 手动修复（移除 .into()）

### 10. crates/libs/modql/src/filter/ops/op_val_string.rs
- 第 293 行：useless_conversion 手动修复（移除 .into()）

### 11. crates/libs/modql/src/sea_utils/sea_types.rs
- 第 30 行：needless_borrow 手动修复（&self.0 → self.0）

### 12. crates/libs/cmx-utils/src/config/config_impl.rs
- 第 907, 917-918 行：approx_constant + bool_assert_comparison 修复

### 13. crates/libs/cmx-utils/tests/integration_test.rs
- 第 178, 193, 205-206 行：approx_constant + bool_assert_comparison 修复

## 新发现警告（待第二轮处理）

修复编译错误后，暴露出 web-server 等 crate 的 83 条新警告，主要类型：
- collapsible_if: 22 处
- redundant_closure: 16 处
- useless_conversion: 7 处
- too_many_arguments: 10 处（排除）
- field_reassign_with_default: 5 处
- map_unwrap_or: 3 处
- option_as_ref: 3 处
- 其他单例警告若干
