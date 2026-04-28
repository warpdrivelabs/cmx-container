# cmx-container 项目开发规范

> AI 在本项目中开发时遵循的规范。

---

## 一、Error 处理规则

### 1.1 必须使用 thiserror 库

所有自定义 Error 使用 `#[derive(thiserror::Error)]`：

```rust
#[derive(Error, Debug)]
pub enum MyError {
    #[error("操作失败: {0}")]
    OperationFailed(String),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}
```

### 1.2 禁止手写 Error 实现

- 禁止 `impl std::error::Error` 或 `impl Display`
- 禁止 `derive_more::From`（与 thiserror 冲突）
- 使用 `#[error(...)]` 属性定义错误消息

### 1.3 Error 定义位置

- 每个 crate 有独立 error 模块 (`src/error.rs`)
- 使用 `pub type Result<T> = core::result::Result<T, Error>;`

---

## 二、日志处理规则

### 2.1 必须使用 tracing 库

```rust
// ✅ 正确
use tracing::{info, warn, error};
info!("操作成功");

// ❌ 错误
use log::info;
```

### 2.2 日志格式

使用结构化日志：`info!("message", key = value)`

---

## 三、依赖管理规则

### 3.1 Workspace 依赖集中管理

所有第三方依赖在 workspace `Cargo.toml` 统一定义，子 crate 禁止直接指定版本，`cmx-wasmdemo`除外。

### 3.2 Crate 引用方式

使用 `workspace = true` 引用，允许添加额外 features：

```toml
# ✅ 正确
uuid = { workspace = true, features = ["v4"] }

# ❌ 错误
uuid = "1.21"
```

### 3.3 依赖注释要求

每个依赖上方必须添加单独注释，禁止分组注释：

```toml
# ✅ 正确
# 序列化框架
serde = { workspace = true }

# ❌ 错误 - 禁止分组注释
# ============================================
serde = { workspace = true }
```

注释格式：`# <用途>` 或 `# <分类> - <用途>`，内部依赖：`# 内部依赖 - <模块>`

### 3.4 禁止使用 log crate

```toml
# ❌ 错误
log = "0.4"
```

### 3.5 新增依赖流程

1. 检查 workspace 是否已定义
2. 未定义则先在 workspace 添加并注释
3. 用 `workspace = true` 引用

### 3.6 未使用依赖

注释保留而非删除：

```toml
# [dependencies]
# unused_crate = "1.0"
```
