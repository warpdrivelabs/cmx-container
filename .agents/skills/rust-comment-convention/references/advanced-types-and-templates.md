# 模块级 / struct / enum / trait 文档规范 + 完整函数文档模板

> 本文件是 rust-comment-convention 技能的 references 细节层（从 SKILL.md 下沉，内容未改）。返回决策入口：[../SKILL.md](../SKILL.md)

模块级 / struct / enum / trait 文档规范 + 完整函数文档模板

## 五、模块/Crate 级文档规范

### 5.1 `//!` 的正确使用

`//!` 仅用于描述其所在的模块或 crate，放在文件顶部：

```rust
//! 插件市场 HTTP Handler。
//!
//! 该模块提供插件市场的 REST API 接口，包括插件的
//! 查询、安装、卸载和更新等功能。

use axum::{Json, Router};
```

### 5.2 模块文档的内容

- 第一行：简明描述模块用途
- 后续段落：详细说明模块的职责、设计决策、与其他模块的关系
- 可包含 `# Examples` 展示模块级使用方式

---

## 六、结构体与枚举的文档规范

### 6.1 结构体文档

```rust
/// 用户认证信息。
///
/// 包含用户的认证令牌和刷新令牌，用于 API 请求的身份验证。
/// 令牌在创建后具有有限的有效期，过期后需使用刷新令牌重新获取。
pub struct AuthToken {
    /// 访问令牌，用于 API 请求的身份验证。
    access_token: String,

    /// 刷新令牌，用于获取新的访问令牌。
    refresh_token: String,

    /// 令牌过期时间（UNIX 时间戳）。
    expires_at: u64,
}
```

### 6.2 枚举文档

枚举整体文档 + 每个变体的文档：

```rust
/// 数据库连接错误类型。
#[derive(Error, Debug)]
pub enum DbError {
    /// 连接池已耗尽，无可用连接。
    PoolExhausted,

    /// 连接超时。
    Timeout {
        /// 超时持续时间（毫秒）。
        millis: u64,
    },

    /// 查询执行失败。
    #[error("查询执行失败: {0}")]
    QueryFailed(String),
}
```

---

## 七、trait 的文档规范

```rust
/// 插件生命周期管理 trait。
///
/// 定义了插件从安装到卸载的完整生命周期，
/// 所有插件实现者必须提供这些方法。
pub trait Plugin: Send + Sync {
    /// 初始化插件。
    ///
    /// 在插件首次加载时调用，用于执行一次性初始化操作。
    ///
    /// # Errors
    ///
    /// 当插件初始化失败时返回错误。
    fn initialize(&mut self) -> Result<()>;

    /// 销毁插件，释放资源。
    ///
    /// 在插件卸载前调用，确保所有资源被正确释放。
    fn destroy(&mut self);
}
```

---

---

## 完整的函数文档模板

```rust
/// 一句话摘要，第三人称单数现在时。
///
/// 更详细的描述段落，解释函数的用途、行为和设计决策。
/// 可以包含多行。
///
/// # Examples
///
/// ```
/// use my_crate::my_function;
///
/// let result = my_function(42);
/// assert_eq!(result, 84);
/// ```
///
/// # Arguments
///
/// * `param` - 参数说明，使用定冠词引用。
///
/// # Returns
///
/// 返回值说明，描述正常返回和特殊情况。
///
/// # Errors
///
/// （返回 Result 时必须）说明可能返回的错误及触发条件。
///
/// # Panics
///
/// （可能 panic 时必须）说明会触发 panic 的条件。
///
/// # Safety
///
/// （unsafe 函数必须）说明调用者必须满足的安全条件。
pub fn my_function(param: i32) -> Result<i32, MyError> {
    // 实现细节行注释
    Ok(param * 2)
}
```

---
