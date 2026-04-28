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
