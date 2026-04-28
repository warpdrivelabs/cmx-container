# cmx-container 项目开发规范

> 本文档用于指导 AI 在 cmx-container 项目中开发时遵循的规范。
> 请在进行 Rust 代码开发前，仔细阅读并严格遵守。

---

## 一、Error 处理规则

### 1.1 必须使用 thiserror 库

所有自定义 Error 类型必须使用 `#[derive(thiserror::Error)]`：

```rust
use thiserror::Error;

/// 错误类型定义
#[derive(Error, Debug)]
pub enum MyError {
    #[error("操作失败: {0}")]
    OperationFailed(String),
    #[error("资源未找到: {id}")]
    NotFound { id: String },
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}
```

### 1.2 禁止手写 Error 实现

- **禁止**使用手写的 `impl std::error::Error` 或 `impl Display`
- **禁止**使用 `derive_more::From`（与 thiserror 冲突）
- 使用 `#[error(...)]` 属性定义错误消息格式

### 1.3 Error 定义位置

- 每个 crate 应有独立的 error 模块 (`src/error.rs`)
- Error 枚举命名为 `{CrateName}Error` 或 `{模块}Error`
- 使用 `pub type Result<T> = core::result::Result<T, Error>;` 定义结果类型

---

## 二、日志处理规则

### 2.1 必须使用 tracing 库

所有日志输出必须使用 `tracing` crate 的宏，禁止使用 `log` crate：

```rust
// ✅ 正确
use tracing::{info, warn, error, debug};
info!("操作成功");
warn!("资源即将过期");
error!("操作失败: {}", err);

// ❌ 错误
use log::{info, warn, error};
log::info!("操作成功");
```

### 2.2 第三方库日志桥接

项目使用 `tracing-log` 将第三方库（如 `log` crate）的日志桥接到 tracing：

```rust
// 在应用入口初始化
tracing_log::LogTracer::init().expect("Failed to set logger");
```

### 2.3 日志消息格式

- 使用结构化日志：`info!("message", key = value)`
- 避免使用 `format!` 风格的字符串插值

```rust
// ✅ 推荐
info!("用户 {} 登录成功", user_id);

// ✅ 结构化日志
info!(user_id = %user_id, "用户登录成功");

```

### 2.4 Workspace 依赖

确保 `Cargo.toml` 中有正确的依赖配置：

```toml
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
tracing-log = "0.2"
```

---

## 三、依赖管理规则

### 3.1 Workspace 依赖集中管理

所有 crate 的第三方依赖必须在 workspace 的 `Cargo.toml` 中统一定义，crate 级别的 `Cargo.toml` 禁止直接指定第三方依赖版本（除非该依赖未在 workspace 中定义且确实需要新增）。

```toml
# workspace Cargo.toml
[workspace.dependencies]
# 依赖版本集中定义
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
```

### 3.2 Crate 引用 Workspace 依赖

在 crate 的 `Cargo.toml` 中使用 `workspace = true` 引用，版本必须与 workspace 保持一致：

```toml
# crate Cargo.toml
[dependencies]
serde = { workspace = true }
serde_json = { workspace = true }
```

允许在 workspace 版本基础上添加额外 features：

```toml
# ✅ 正确 - 引用workspace版本并添加额外features
uuid = { workspace = true, features = ["v4", "fast-rng"] }
serde = { workspace = true, features = ["derive"] }
```

禁止在子crate中直接指定版本号：

```toml
# ❌ 错误 - 版本必须来自workspace
uuid = "1.21"
serde = "1.0"
```

### 3.3 依赖注释要求

每个依赖项前必须添加简单注释说明用途：

```toml
[dependencies]
# 序列化框架
serde = { workspace = true }
# JSON 序列化/反序列化
serde_json = { workspace = true }
```

### 3.4 禁止使用 log crate

项目要求使用 `tracing` 进行日志记录，禁止在 crate 中添加 `log` 依赖：

```toml
# ❌ 错误
log = "0.4"

# ✅ 正确 - 使用 tracing-log 桥接第三方库的 log 输出
tracing-log = { workspace = true }
```

### 3.5 新增依赖流程

1. 首先检查 workspace `Cargo.toml` 是否已定义该依赖
2. 如未定义，先在 workspace 中添加并注明用途
3. 再在 crate 中使用 `workspace = true` 引用
4. 确保依赖版本与已定义的版本一致

### 3.6 未使用依赖处理

如发现某个依赖未被使用，应将其注释掉而非直接删除，以便后续追溯：

```toml
# [dependencies]
# 未使用的依赖注释保留
# unused_crate = "1.0"
```
