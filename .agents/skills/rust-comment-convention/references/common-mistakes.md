# 常见注释错误与纠正

> 本文件是 rust-comment-convention 技能的 references 细节层（从 SKILL.md §六 下沉，内容未改）。返回决策入口：[../SKILL.md](../SKILL.md)

## 常见错误与纠正

### 8.1 禁止的注释模式

```rust
// ❌ 禁止：在文档注释中使用 TODO/FIXME 标记
/// TODO: 需要添加错误处理
pub fn process() {}

// ✅ 正确：TODO 使用行注释
// TODO: 需要为 process 函数添加错误处理
pub fn process() {}

// ❌ 禁止：文档注释无摘要句
/// # Arguments
/// * `x` - The x coordinate
pub fn foo(x: i32) {}

// ✅ 正确：首句为摘要
/// 计算指定坐标的哈希值。
///
/// # Arguments
///
/// * `x` - x 坐标
pub fn foo(x: i32) {}

// ❌ 禁止：在文档注释中使用祈使句
/// Return the value.
pub fn get(&self) -> i32 { self.val }

// ✅ 正确：第三人称单数现在时
/// Returns the value.
pub fn get(&self) -> i32 { self.val }

// ❌ 禁止：使用 "Example" 单数
/// # Example
/// ```

// ✅ 正确：使用 "Examples" 复数
/// # Examples
/// ```
```

```rust
// ❌ 禁止：公开函数缺少参数和返回值说明
/// 执行数据库查询。
pub fn query(sql: &str, timeout: u64) -> Result<ResultSet, DbError> {
    // ...
}

// ✅ 正确：公开函数包含 # Arguments 和 # Returns
/// 执行数据库查询。
///
/// # Arguments
///
/// * `sql` - 待执行的 SQL 语句，支持参数占位符 `?`。
/// * `timeout` - 查询超时时间（毫秒），超时后取消查询。
///
/// # Returns
///
/// 成功时返回 `ResultSet`，包含查询结果的所有行数据。
///
/// # Errors
///
/// * `DbError::Timeout` - 查询超过 `timeout` 指定的时间。
/// * `DbError::Syntax` - SQL 语法错误。
pub fn query(sql: &str, timeout: u64) -> Result<ResultSet, DbError> {
    // ...
}
```

### 8.2 中文注释的标点规范

- 中文注释使用中文标点（句号用 `。`，逗号用 `，`）
- 代码引用仍使用反引号包裹
- 中英文混排时，中英文之间加空格

```rust
/// 返回当前用户的权限列表。
///
/// 如果用户未登录，返回空列表。
///
/// # Panics
///
/// 当 `user_id` 为 0 时会触发 panic。
pub fn get_permissions(&self, user_id: u64) -> Vec<Permission> {
    // ...
}
```

---
