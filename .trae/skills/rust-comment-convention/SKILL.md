---
name: "rust-comment-convention"
description: "Rust 代码注释规范技能。在编写、审查或重构 Rust 代码时自动应用注释规范，包括文档注释格式、章节结构、语气规则和行内注释标准。Invoke when writing or reviewing Rust doc comments, adding function/type documentation, or ensuring comment conventions compliance."
---

# Rust 代码注释规范

本技能定义了 Rust 项目中代码注释的完整规范，基于以下权威来源：

- [RFC 0505 — API Comment Conventions](https://rust-lang.github.io/rfcs/0505-api-comment-conventions.html)
- [RFC 1574 — More API Documentation Conventions](https://rust-lang.github.io/rfcs/1574-more-api-documentation-conventions.html)
- [Rust Style Guide — Comments](https://rust-lang.github.io/style-guide/#comments)
- [The rustdoc Book](https://doc.rust-lang.org/rustdoc/how-to-write-documentation.html)
- [Canonical Rust Best Practices — Comment Discipline](https://canonical.github.io/rust-best-practices/comment-discipline.html)

---

## 一、注释类型与使用场景

### 1.1 三种注释语法

| 语法 | 类型 | 用途 |
|------|------|------|
| `//` | 行注释 | 函数体内实现细节、私有辅助逻辑说明 |
| `///` | 外层文档注释 | 为紧跟其后的项（fn/struct/enum/trait/const/static/type/impl/mod）生成文档 |
| `//!` | 内层文档注释 | 描述所在模块或 crate 本身，仅用于模块/crate 级别文档 |

### 1.2 核心规则：注释类型必须匹配被标注的对象

- `///` 用于所有项（item）的文档：`fn`、`struct`、`enum`、`trait`、`const`、`static`、`type`、`impl` 块、`mod` 声明
- `//!` 仅用于模块/crate 内部文档，放在 `mod.rs` / `lib.rs` / `main.rs` 文件顶部，在所有 `use` 或项之前
- `//` 用于函数体内的行内说明和私有辅助逻辑
- **禁止** `////`（四个斜杠不是文档注释，只是普通注释）

### 1.3 优先使用行注释

行注释 (`//`, `///`, `//!`) 优先于块注释 (`/* */`, `/** */`, `/*! */`)：

```rust
// ✅ 正确：使用行注释
// 等待主任务返回，并设置进程错误码
```

```rust
// ❌ 错误：避免块注释
/*
 * 等待主任务返回，并设置进程错误码
 */
```

### 1.4 mod 块中使用外层文档注释

```rust
// ✅ 正确：在 mod 块外部使用 ///
/// 测试模块
mod tests {
    // ...
}
```

```rust
// ❌ 错误：避免在 mod 块内部使用 //!
mod tests {
    //! 测试模块
    // ...
}
```

---

## 二、文档注释的结构规范

### 2.1 首句是摘要（G-2）

文档注释的第一行必须是**一个简短的完整句子**，**必须以句号结尾**。这是 rustdoc 在索引和搜索结果中显示的内容，句号帮助 rustdoc 正确识别句子边界。

后续详细说明用空 `///` 行与摘要分隔。

```rust
/// 返回当前计数器的值。
///
/// 该方法不会修改计数器状态，可以安全地在多线程环境中调用。
fn get(&self) -> u64 {
    self.count
}
```

> **注意**：文档注释（`///`、`//!`）中的正文段落也应使用完整句子并以句号结尾（RFC 0505 要求 "properly punctuated"）。但对于 `# Arguments` 等章节中的短描述列表项，不加句号也是可接受的。详见 [4.4 标点符号使用指引](#44-标点符号使用指引)。

### 2.2 第三人称单数现在时（G-3）

函数/方法的文档摘要使用第三人称单数现在时：

```rust
// ✅ 正确
/// Returns the current counter value.
/// 返回当前计数器的值。

// ❌ 错误
/// Return the current counter value.  （祈使句）
/// This function returns the current counter value.  （冗余主语）
/// Gets the value.  （缺少句号）
```

模块/crate 的 `//!` 摘要可以使用名词短语，因为主语是模块本身：

```rust
//! 嵌入式模板树解析器。
//! Embedded template tree parser.
```

### 2.3 参数引用使用定冠词

引用参数时，按名称引用，优先使用定冠词（the），避免不定冠词（a/an）造成的歧义：

```rust
// ✅ 正确
/// Increments this counter by the given `delta`.
/// 将此计数器增加给定的 `delta`。

// ❌ 错误
/// Increments a counter by a given amount.
```

---

## 三、文档注释的章节结构

### 3.1 标准章节及顺序

文档注释中使用的章节标题必须是 rustdoc 识别的标准标题，按以下顺序排列：

| 顺序 | 标题 | 必要性 | 说明 |
|------|------|--------|------|
| 1 | `# Arguments` | 推荐 | 逐个说明函数参数的用途和约束 |
| 2 | `# Returns` | 推荐 | 说明返回值的含义和可能的取值 |
| 3 | `# Examples` | 强烈推荐 | 即使只有一个示例也用复数形式 |
| 4 | `# Panics` | 按需 | 函数可能 panic 的场景 |
| 5 | `# Errors` | 按需 | 返回 `Result` 时说明可能的错误 |
| 6 | `# Safety` | 按需 | `unsafe` 函数必须说明安全使用条件 |

> **何时使用 `# Arguments` 和 `# Returns`**：当参数名或返回值类型**不足以完全表达语义**时，应使用这两个章节。对于自描述性极强的函数（如 `fn len(&self) -> usize`），可以省略。对于**公开 API（`pub fn`）**，强烈建议始终包含这两个章节。

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

## 四、行注释规范

### 4.1 格式要求

- 行注释 `//` 后留一个空格
- 注释应独立成行；若跟随代码，注释前留一个空格
- 行注释**不强制**以句号结尾：完整句子推荐加句号，短语、标签、简单说明可省略
- 纯注释行长度限制为 80 字符（含注释符号，不含缩进），或行最大宽度（含缩进和符号），取较小者

```rust
// ✅ 正确：完整句子加句号
// 等待异步任务完成并处理返回结果。
let result = task.await;

// ✅ 正确：短语不加句号也可以
// 递归终止条件
if n == 0 { return 1; }

// ✅ 正确：行尾简短注释
let x = 42; // 初始值

// ❌ 错误
//等待任务完成（缺少空格）
let x = 42;//初始化（缺少空格）
```

### 4.2 行注释的使用场景

- 函数体内实现细节说明
- 复杂逻辑、非直观代码的解释
- 优化技巧或算法选择的说明
- `TODO`/`FIXME`/`HACK`/`SAFETY` 等标记（仅用于行注释，禁止出现在文档注释中）

```rust
fn sort_items(items: &mut [Item]) {
    // 使用 TimSort 算法，对部分有序数据具有 O(n) 最佳时间复杂度。
    items.sort_by(|a, b| a.priority.cmp(&b.priority));

    // SAFETY: 此处解引用安全，因为我们在上方已验证指针非空。
    unsafe {
        (*ptr).update();
    }
}
```

### 4.3 行注释标记约定

| 标记 | 含义 | 格式 |
|------|------|------|
| `// SAFETY:` | unsafe 代码的安全保证说明 | 必须紧跟 unsafe 块 |
| `// TODO:` | 待完成的功能或改进 | 后跟具体描述 |
| `// FIXME:` | 已知需要修复的问题 | 后跟问题描述 |
| `// HACK:` | 临时方案，需要后续优化 | 后跟原因说明 |
| `// NOTE:` | 需要特别关注的说明 | 后跟注意内容 |

### 4.4 标点符号使用指引

不同类型的注释对标点符号（句号）的要求不同：

| 注释类型 | 句号要求 | 依据 |
|----------|---------|------|
| `///` 摘要行 | **必须** | rustdoc 用句号识别句子边界，用于搜索摘要 |
| `///` 段落正文 | **推荐** | RFC 0505 要求 "properly punctuated" |
| `///` 列表项（`* \`arg\` - ...`） | **灵活** | 完整句子加句号，短描述可省略 |
| `//!` 模块文档 | **推荐** | 与 `///` 正文一致 |
| `//` 行注释 | **不强制** | RFC 0505 和 Rust Book 示例均不要求 |
| `// SAFETY:` 等标记 | **推荐** | 完整句子推荐加，短语可省略 |

```rust
// ✅ 文档注释摘要：必须以句号结尾
/// 返回当前计数器的值。
fn get(&self) -> u64 { self.count }

// ✅ 文档注释列表项：完整句子加句号，短描述可省略
/// # Arguments
///
/// * `buf` - 待写入的数据缓冲区，不可为空。
/// * `flush` - 是否立即刷盘

// ✅ 行注释：不强制句号
// 递归终止条件
if n == 0 { return 1; }

// ✅ 行注释：完整句子推荐加句号
// 使用 TimSort 算法，对部分有序数据具有 O(n) 最佳时间复杂度。

// ✅ SAFETY 标记：完整句子加句号
// SAFETY: 此处解引用安全，因为我们在上方已验证指针非空。
unsafe { (*ptr).update(); }
```

---

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

## 八、常见错误与纠正

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

## 九、完整的函数文档模板

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

## 十、检查清单

在编写或审查 Rust 代码注释时，逐项检查：

- [ ] `///` 用于项文档，`//!` 仅用于模块/crate 文档
- [ ] 文档注释首句为简短摘要，**必须以句号结尾**
- [ ] 函数/方法摘要使用第三人称单数现在时（"Returns" 而非 "Return"）
- [ ] 章节标题使用标准 rustdoc 格式：`# Arguments`、`# Returns`、`# Examples`、`# Panics`、`# Errors`、`# Safety`
- [ ] 公开函数（`pub fn`）包含 `# Arguments` 章节，逐个说明参数用途和约束
- [ ] 公开函数（`pub fn`）包含 `# Returns` 章节，说明返回值含义和特殊情况
- [ ] `Examples` 使用复数形式
- [ ] 代码示例标注 `rust` 语言
- [ ] `unsafe` 函数包含 `# Safety` 章节
- [ ] 返回 `Result` 的函数包含 `# Errors` 章节
- [ ] 行注释 `//` 后留一个空格
- [ ] 行注释句号**不强制**：完整句子推荐加，短语/标签可省略
- [ ] TODO/FIXME/HACK 标记使用行注释而非文档注释
- [ ] 中文注释使用中文标点
- [ ] 参数引用使用名称和定冠词
