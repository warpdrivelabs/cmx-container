# Clippy 警告修复计划 - 2026-06-24 (第一轮)

## 检查结果概览

- 检查时间：2026-06-24
- 检查命令：`cargo clippy --all-targets`
- 警告总数：42 条（去重后）
- 错误总数：3 条（approx_constant）
- 待修复数：37 条（排除 too_many_arguments 2 条）
- 已排除数：2 条（too_many_arguments）

## 待修复警告列表

### 警告类型统计

| 警告类型 | 数量 | 是否修复 |
|---------|------|---------|
| unexpected_cfgs (`with-rusqlite`) | 14 | 修复（在 Cargo.toml 添加 feature 或使用 check-cfg） |
| collapsible_if | 9 | 修复 |
| box_collection (creating a new box) | 6 | 修复 |
| approx_constant (error) | 3 | 修复 |
| bool_assert_comparison | 2 | 修复 |
| needless_borrow | 2 | 修复 |
| useless_conversion | 2 | 修复 |
| redundant_closure | 1 | 修复 |
| if_same_then_else | 1 | 修复 |
| iter_cloned_collect | 1 | 修复 |
| too_many_arguments | 2 | **排除** |

### 详细警告列表

#### 1. unexpected_cfgs (`with-rusqlite`) - 14 处

modql crate 中使用了 `#[cfg(feature = "with-rusqlite")]` 但 Cargo.toml 未声明该 feature：

- `crates/libs/modql/modql-macros/src/lib.rs`: 79, 82, 132, 139, 150, 157, 164
- `crates/libs/modql/src/lib.rs`: 4, 20
- `crates/libs/modql/src/field/mod.rs`: 9, 26
- `crates/libs/modql/src/sea_utils/mod.rs`: 3, 7

#### 2. collapsible_if - 9 处

- `crates/libs/cmx-infra/cmx-audit/src/store/memory.rs`: 46, 49, 52, 55, 58, 61, 64 (7 处)
- `sdk/cmx-cli/src/cli/commands.rs`: 439
- `crates/libs/modql/modql-macros/src/derives_field/derive_fields.rs`: 36

#### 3. box_collection (creating a new box) - 6 处

`crates/libs/cmx-macros/src/lib.rs`: 144, 205, 252, 292, 339, 386

#### 4. approx_constant (error) - 3 处

`crates/libs/cmx-core/tests/test_json.rs`: 263, 276, 389 - 使用了 3.14（接近 PI）

#### 5. bool_assert_comparison - 2 处

`crates/libs/cmx-core/tests/test_json.rs`: 213, 214

#### 6. needless_borrow - 2 处

- `sdk/cmx-cli/src/generator/ast_json_gen.rs`: 227
- `crates/libs/modql/src/sea_utils/sea_types.rs`: 30

#### 7. useless_conversion - 2 处

- `crates/libs/modql/src/field/sea/sea_field.rs`: 52
- `crates/libs/modql/src/filter/ops/op_val_string.rs`: 293

#### 8. redundant_closure - 1 处

`sdk/cmx-cli/src/generator/ast_json_gen.rs`: 205

#### 9. if_same_then_else - 1 处

`sdk/cmx-cli/src/generator/ast_json_gen.rs`: 380

#### 10. iter_cloned_collect - 1 处

`sdk/cmx-cli/src/parser/doc_parser.rs`: 433

## 修复方案

### 阶段一：自动修复（cargo clippy --fix）

对以下 crate 执行自动修复：
- `cmx-audit` - 7 处 collapsible_if
- `cmx-cli` - 4 处（redundant_closure, needless_borrow, iter_cloned_collect, collapsible_if）
- `cmx-macros` - 6 处 box_collection
- `modql` - 3 处（useless_conversion, needless_borrow）

### 阶段二：手动修复

#### 2.1 修复 approx_constant 错误（test_json.rs）

将 `3.14` 改为 `3.15` 或其他不接近常数的值：

```rust
// 修复前
"float": 3.14,
// 修复后
"float": 3.15,
```

#### 2.2 修复 bool_assert_comparison（test_json.rs）

```rust
// 修复前
assert_eq!(value["enabled"].as_bool().unwrap(), true);
assert_eq!(value["disabled"].as_bool().unwrap(), false);
// 修复后
assert!(value["enabled"].as_bool().unwrap());
assert!(!value["disabled"].as_bool().unwrap());
```

#### 2.3 修复 if_same_then_else（ast_json_gen.rs:380）

检查并合并相同的分支。

#### 2.4 修复 unexpected_cfgs（modql）

在 modql 和 modql-macros 的 Cargo.toml 中添加 `with-rusqlite` feature 声明，或使用 `cargo-features = ["check-cfg"]` 声明该 cfg 为合法值。

### 阶段三：验证

运行 `cargo clippy --all-targets` 验证修复结果。
