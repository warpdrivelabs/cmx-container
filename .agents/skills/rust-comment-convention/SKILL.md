---
name: rust-comment-convention
description: Rust 代码注释规范：文档注释格式、章节结构（Arguments/Returns/Examples）、语气规则和行内注释标准。当用户编写、审查或重构 Rust 代码的文档注释、为函数/类型补注释、或检查注释规范符合性时必用。
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

### 3.2 - 3.7 六大章节详解

`# Arguments` / `# Returns` / `# Examples` / `# Panics` / `# Errors` / `# Safety` 各章节的格式细则、正误对照示例见 [references/section-details.md](references/section-details.md)。


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

## 五、模块/Crate 级、struct/enum、trait 的文档规范

`//!` 的正确使用与模块文档内容、结构体/枚举文档、trait 文档（含实现者义务）的完整规范见 [references/advanced-types-and-templates.md](references/advanced-types-and-templates.md)。**速记**：`//!` 只能在 mod 块顶部/crate 根；struct 文档首句用第三人称描述其代表的集合；trait 文档写实现者义务与调用方契约。

---

## 六、常见错误与纠正

高频注释错误的正误对照全集见 [references/common-mistakes.md](references/common-mistakes.md)。

---

## 七、完整的函数文档模板

可直接套用的完整函数文档模板见 [references/advanced-types-and-templates.md](references/advanced-types-and-templates.md)（同文件末章）。

---

## 八、检查清单


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
