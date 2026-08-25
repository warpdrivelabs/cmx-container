---
name: clippy-fix
description: 执行 cargo clippy 检查并修复警告（排除 too_many_arguments、unused_variables、unused_functions 三类）。当用户要求检查/运行/修复 clippy 警告、跑"项目警告"检查，或要求生成修复计划/修复报告文档时必用。修复前产出计划文档、修复后产出报告文档（归档工作区根 documents/plans/）。
---

# Clippy 警告检查与修复

本技能用于执行 Clippy 警告检查和修复，排除特定类型的警告。

## 使用场景

当用户要求执行以下操作时调用本技能：
- "检查 clippy 警告"
- "运行 clippy"
- "修复 clippy 警告"
- "检查项目警告"

## 警告排除规则

以下类型的警告**被排除**，不会修复：
- `too_many_arguments` - 函数参数过多
- `unused_variables` - 未使用的变量（函数参数）
- `unused_functions` - 未使用的函数

## 执行流程

### 1. 运行 Clippy 检查

```bash
cargo clippy -- -W clippy::all | grep -vE "too_many_arguments|unused_variables|unused_functions"
```

### 2. 生成修复计划文档

在工作区根 `documents/plans/` 目录下创建（plan-naming 规范：`yyyyMMdd_clippy_警告修复计划.md`，如 `20260825_clippy_警告修复计划.md`）：

```markdown
# Clippy 警告修复计划 - YYYY-MM-DD

## 检查结果概览

- 检查时间：YYYY-MM-DD HH:MM:SS
- 警告总数：N
- 待修复数：M（排除 too_many_arguments, unused_variables, unused_functions）
- 已排除数：X

## 待修复警告列表

### 警告类型统计

| 警告类型 | 数量 |
|---------|------|
| collapsible_if | N |
| redundant_field_names | N |
| ... | ... |

### 详细警告列表

按警告类型分组，列出文件路径和行号。

## 修复方案

按警告类型分组制定修复方案。
```

### 3. 执行修复

按复杂度从低到高执行修复：

#### 阶段一：自动修复（cargo clippy --fix）
- collapsible_if
- redundant_field_names
- unnecessary_borrow
- unused_imports
- empty_line_after_doc_comments

#### 阶段二：简单手动修复
- dead_code（未使用的代码块）
- doc_lazy_continuation
- redundant_locals
- cloned_ref_to_slice_refs

#### 阶段三：中等复杂度修复
- unnecessary_unwrap
- field_reassign_with_default
- crate_in_macro_def

#### 阶段四：需要代码重构的修复
- unused_import
- doc_list_item_without_indentation
- this_returns_result（返回 Result 但总是返回 Ok/Err）
- type_complexity（类型过于复杂）

### 4. 生成修复报告文档

在工作区根 `documents/plans/` 目录下创建（plan-naming 规范：`yyyyMMdd_clippy_警告修复报告.md`）：

```markdown
# Clippy 警告修复报告 - YYYY-MM-DD

## 修复概览

- 修复时间：YYYY-MM-DD HH:MM:SS
- 起始警告数：N
- 修复后警告数：M
- 减少率：X%

## 已修复警告

| 警告类型 | 修复数量 | 修复方式 |
|---------|---------|---------|
| collapsible_if | N | auto-fix |
| ... | ... | ... |

## 未修复警告（已排除）

| 警告类型 | 数量 | 排除原因 |
|---------|------|---------|
| too_many_arguments | N | 用户指定排除 |
| unused_variables | N | 用户指定排除 |
| unused_functions | N | 用户指定排除 |

## 详细修复记录

按文件列出所有修改。
```

## 修复指南

### collapsible_if（可折叠的 if）

将嵌套的 if 语句合并：

```rust
// 修复前
if a {
    if b {
        do_something();
    }
}

// 修复后
if a && b {
    do_something();
}
```

### redundant_field_names（冗余字段名）

```rust
// 修复前
Point { x: x, y: y }

// 修复后
Point { x, y }
```

### unnecessary_borrow（不必要的借用）

```rust
// 修复前
let v = &vec![1, 2, 3];

// 修复后
let v = vec![1, 2, 3];
```

### empty_line_after_doc_comments（文档注释后空行）

```rust
// 修复前
/// Doc comment
// 空行
fn foo() {}

// 修复后
/// Doc comment
fn foo() {}
```

### unnecessary_unwrap（不必要的 unwrap）

```rust
// 修复前
let x = Some(1);
if x.is_some() {
    let v = x.unwrap();
}

// 修复后
if let Some(v) = x {
    // 使用 v
}
```

### this_returns_result（返回 Result 但总是成功/失败）

创建自定义错误类型替代 `Result<(), ()>`：

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GlobalRuntimeError(&'static str);

impl GlobalRuntimeError {
    pub const ALREADY_SET: Self = GlobalRuntimeError("运行时已初始化，无法重复设置");
}

pub fn set(runtime: std::sync::Arc<dyn RuntimeInvoker>) -> Result<(), GlobalRuntimeError> {
    RUNTIME.set(runtime).map_err(|_| GlobalRuntimeError::ALREADY_SET)
}
```

### type_complexity（类型过于复杂）

使用类型别名简化：

```rust
type TxnHolderMutex = Arc<Mutex<Option<TxnHolder>>>;
type TxnHolderMap = HashMap<String, TxnHolderMutex>;
type TxnHolderRegistry = Arc<RwLock<TxnHolderMap>>;
```

### module_inception（模块内嵌套模块）

重命名内部模块：

```rust
// 修复前
mod config {
    mod config { ... }
}

// 修复后
mod config_impl;
```

### dead_code（未使用的代码）

删除或添加 `#[allow(dead_code)]`（如果代码有意保留）：

```rust
#[allow(dead_code)]
fn unused_but_kept() {}
```

## 关键文件路径

| 文件 | 用途 |
|------|------|
| `<工作区根>/documents/plans/` | 存放修复计划/报告文档（cmx-container 仓内无 documents/，统一归档到工作区根） |
| `Cargo.toml` | 项目配置 |

## 注意事项

1. 修复前先备份或使用版本控制
2. 每次修复后运行 `cargo check` 确保编译通过（全局规则：禁止用 `cargo build` 做编译检查）
3. 如果修复导致编译错误，立即回滚并调整方案
4. 复杂重构建议分步骤进行
5. 修复完成后运行 `cargo clippy` 验证
