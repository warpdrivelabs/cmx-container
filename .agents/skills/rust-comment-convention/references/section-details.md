# 文档注释章节详解（Arguments / Returns / Examples / Panics / Errors / Safety）

> 本文件是 rust-comment-convention 技能的 references 细节层（从 SKILL.md §3.2-3.7 下沉，内容未改）。返回决策入口：[../SKILL.md](../SKILL.md)

### 3.2 `# Arguments` 章节

逐个说明函数的每个参数。使用无序列表格式，参数名用反引号包裹：

- 每个参数占一行，格式为 `* \`name\` - 描述`
- 参数描述应说明用途、取值范围、约束条件
- 当参数语义不够直观时（如单位、格式、特殊值含义），必须说明
- **句号要求**：完整句子以句号结尾，短描述（短语形式）可省略句号

```rust
/// 将数据写入指定偏移量的位置。
///
/// # Arguments
///
/// * `buf` - 待写入的数据缓冲区，不可为空。
/// * `offset` - 写入起始偏移量（字节），必须小于文件大小。
/// * `flush` - 是否在写入后立即刷盘
pub fn write_at(&self, buf: &[u8], offset: u64, flush: bool) -> io::Result<()> {
    // ...
}
```

### 3.3 `# Returns` 章节

说明返回值的含义、可能的取值及特殊情况：

- 描述正常返回值的语义
- 如有特殊返回值（如 `None`、空集合、默认值），说明触发条件
- 返回 `Result` 时，此处可简要说明 `Ok` 变体的含义，详细错误信息放在 `# Errors` 章节

```rust
/// 查找指定 key 对应的缓存条目。
///
/// # Returns
///
/// * `Some(&Entry)` - 当 key 存在且未过期时返回对应条目的引用。
/// * `None` - 当 key 不存在或条目已过期时返回。
pub fn get(&self, key: &str) -> Option<&Entry> {
    // ...
}
```

`# Arguments` 和 `# Returns` 的联合使用示例：

```rust
/// 根据用户 ID 获取用户信息。
///
/// # Arguments
///
/// * `user_id` - 用户唯一标识，必须为正整数。
/// * `fields` - 需要返回的字段列表。为空时返回所有字段。
///
/// # Returns
///
/// 成功时返回 `User` 实例。当用户不存在时返回 `UserError::NotFound`。
///
/// # Errors
///
/// * `UserError::NotFound` - 指定 `user_id` 的用户不存在。
/// * `UserError::Database` - 数据库查询失败。
pub fn get_user(user_id: u64, fields: &[&str]) -> Result<User, UserError> {
    // ...
}
```

### 3.4 `# Examples` 章节

- 标题使用**复数**形式（即使只有一个示例）
- 代码块标注 `rust` 语言，使 rustdoc 能运行测试
- 使用 `# ` 前缀隐藏不希望出现在文档中但编译需要的代码

```rust
/// 计算两个整数的和。
///
/// # Examples
///
/// ```
/// let result = my_crate::add(2, 3);
/// assert_eq!(result, 5);
/// ```
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}
```

隐藏辅助代码：

```rust
/// # Examples
///
/// ```
/// # use my_crate::Foo;  // 这行不会出现在文档中
/// let foo = Foo::new();
/// assert!(foo.is_valid());
/// ```
```

### 3.5 `# Panics` 章节

记录函数可能发生 panic 的所有场景：

```rust
/// 将切片在指定位置分割为两部分。
///
/// # Panics
///
/// 当 `mid > len` 时会触发 panic。
pub fn split_at(&self, mid: usize) -> (&[T], &[T]) {
    // ...
}
```

### 3.6 `# Errors` 章节

当函数返回 `Result` 时，说明可能的错误类型及触发条件：

```rust
/// 从文件中读取配置。
///
/// # Errors
///
/// 当文件不存在或权限不足时返回 `std::io::Error`。
pub fn read_config(path: &Path) -> io::Result<Config> {
    // ...
}
```

### 3.7 `# Safety` 章节

`unsafe` 函数**必须**包含 Safety 章节，说明调用者需满足的安全条件：

```rust
/// 将字节切片重新解释为 UTF-8 字符串切片。
///
/// # Safety
///
/// 调用者必须确保 `bytes` 包含有效的 UTF-8 字节序列，
/// 且在 `'a` 生命周期内不会被修改。
pub unsafe fn from_utf8_unchecked<'a>(bytes: &[u8]) -> &'a str {
    // ...
}
```

---
