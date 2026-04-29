---
name: doc-generator
description: 根据 Rust crate 代码生成或更新 README 文档。适用于用户要求"生成文档"、"更新 README"、"创建 crate 文档"等场景。
---

# Rust Crate README 文档生成器

基于 Rust crate 代码自动生成或更新 README.md 文档。

---

## 一、使用场景

用户要求以下任一操作时触发本技能：
- "为 xxx crate 生成 README"
- "更新当前 crate 的文档"
- "生成库文档"
- "创建 crate 文档"
- "请完善 xxx 的 README"

---

## 二、输入参数

| 参数 | 来源 | 说明 |
|------|------|------|
| `crate_path` | 用户指定或当前工作目录 | 要生成文档的 crate 路径 |

---

## 三、README 文档结构规范

生成的 README 必须包含以下章节：

### 3.1 项目名称与简介 (Project Name & Introduction)

```markdown
# {crate_name}

> {description}

[![Version](https://img.shields.io/badge/version-{version}-blue.svg)]
[![License](https://img.shields.io/badge/license-{license}-green.svg)]
```

**要求**：
- 项目名称：清晰、有辨识度
- 简介：一两句话精准传达 crate 的核心价值，解答"这个库解决了什么问题"

### 3.2 快速开始 (Quick Start)

#### 安装指南

```toml
[dependencies]
{crate_name} = "{version}"
```

如果有多 features：
```toml
[dependencies]
{crate_name} = "{version}"
# 或仅启用需要的 features
{crate_name} = { version = "{version}", features = ["feature_a"] }
```

#### 核心示例

提供自包含、可运行的最简代码示例，展示最常见的使用模式：

```rust
use {crate_name}::{核心类型};

// 最简示例代码
fn main() {
    let instance = {核心类型}::new();
    // 展示核心用法
}
```

**要求**：
- 示例必须可运行
- 展示最常见的使用模式
- 让用户快速评估 crate 是否满足需求

### 3.3 核心功能与特性 (Features)

```markdown
## 核心功能与特性

| 功能 | 说明 |
|------|------|
| 特性A | {描述} |
| 特性B | {描述} |

### 可选 Features

| Feature | 默认启用 | 说明 |
|---------|---------|------|
| `default` | ✅ | 基础功能 |
| `feature_a` | ❌ | {描述} |
| `feature_b` | ❌ | {描述} |
```

**要求**：
- 遵循最小默认原则
- 明确说明哪些 feature 默认启用
- 说明如何按需启用高级功能

### 3.4 模块结构 (Module Structure)

```markdown
## 模块结构

```{text}
{crate_name}
├── module_a          # 模块A功能说明
│   ├── sub_module_a1  # 子模块A1
│   └── sub_module_a2  # 子模块A2
├── module_b          # 模块B功能说明
├── module_c          # 模块C功能说明
└── error             # 错误类型定义
```

### 主要模块说明

#### `module_a`

{自动从源码注释提取模块功能描述}
```

**要求**：
- 使用树形结构展示模块层级
- 每个模块标注简短功能说明
- 对主要模块进行详细说明

### 3.5 详细使用文档 (Usage / Documentation)

**⚠️ 重要：这是最核心的部分，必须详细编写！**

使用指南必须包含**至少 5-10 个场景**，每个场景都要有：
- 场景名称（小标题）
- 场景说明
- 完整可运行的代码示例
- 代码中的中文注释
- 运行结果说明（如果有）

```markdown
## 使用指南

### 一、{主要功能大类}

#### 1.1 {子场景名称}

{场景说明：描述这个场景要解决什么问题}

```rust
// 完整的代码示例，包含中文注释
use crate_name::{Type1, Type2};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 步骤1：初始化
    let service = Type1::new();

    // 步骤2：配置
    service.configure("option", value);

    // 步骤3：执行
    let result = service.execute().await?;

    // 步骤4：处理结果
    println!("Result: {:?}", result);

    Ok(())
}
```

#### 1.2 {另一个子场景}

{场景说明}

```rust
// 代码示例
```

### 二、{另一个主要功能大类}

#### 2.1 {子场景名称}

{场景说明}

```rust
// 代码示例
```

...（继续编写所有主要功能的场景）
```

**⚠️ 必须覆盖的功能场景类型**：

| 场景类型 | 说明 | 必须包含 |
|----------|------|----------|
| 基础配置/初始化 | 如何创建和配置核心对象 | ✅ 必须 |
| CRUD 操作 | 增删改查的基本用法 | 如果有 CRUD 必须包含 |
| 高级配置 | 自定义配置项 | 如果有复杂配置必须包含 |
| 错误处理 | 各种错误类型和处理方式 | ✅ 必须 |
| 并发/异步 | 异步操作、并发控制 | 如果支持 async 必须包含 |
| 事务/批量操作 | 批量处理、事务管理 | 如果支持事务必须包含 |
| 生命周期 | 初始化、关闭、清理 | ✅ 必须 |
| 监控/指标 | 获取状态、统计信息 | 如果有监控必须包含 |
| 完整示例 | 从头到尾的完整使用流程 | ✅ 必须 |

### 3.6 常见问题解答 (FAQ)

```markdown
## 常见问题

### Q: {问题1}

**A**: {解答1}

### Q: {问题2}

**A**: {解答2}
```

**要求**：
- 针对用户可能遇到的典型问题提前解答
- 涵盖边界情况和易错点
- 如果没有 FAQ 内容，可省略此章节

---

## 四、代码分析要求

### 4.1 分析 Cargo.toml

提取以下信息：

```rust
struct CrateInfo {
    name: String,              // 包名
    version: String,           // 版本号
    description: String,      // 描述
    authors: Vec<String>,     // 作者列表
    license: String,          // 许可证
    repository: String,       // 仓库地址
    edition: String,          // Rust 版本
    features: HashMap<String, Vec<String>>,  // features 配置
    dependencies: HashMap<String, String>,   // 依赖
}
```

### 4.2 分析源代码结构

遍历 `src/` 目录，提取：

| 文件 | 提取内容 |
|------|---------|
| `lib.rs` | 模块导出结构、public 类型/函数/宏 |
| `error.rs` | 错误类型定义 |
| `*.rs` | 各模块的公共 API |

提取规则：
- 使用 `pub` 关键字识别公共 API
- 提取 `///` 文档注释
- 提取 `#[derive(...)]` 属性中的 trait 实现
- 识别 `#[cfg(feature = "xxx")]` 条件编译

### 4.3 识别关键类型和函数

优先级排序：
1. 核心类型（`struct`、`enum`）
2. 核心函数（`pub fn`、`pub async fn`）
3. 宏定义（`macro_rules!`）
4. Trait 定义（`pub trait`）

---

## 五、执行流程

```
1. 解析 Cargo.toml → 提取 crate 元信息、features、依赖
         ↓
2. 遍历 src/ → 提取模块结构树
         ↓
3. 解析各模块 → 提取 public API 和文档注释
         ↓
4. 识别入口点 → 确定 lib.rs 导出内容
         ↓
5. 识别主要功能 → 确定使用指南的场景分类
         ↓
6. 生成文档 → 按结构规范组装 markdown
         ↓
7. 输出结果 → 新建或更新 README.md
```

---

## 六、使用指南编写规则

### 6.1 基本规则

1. **每个主要功能至少有一个完整示例**
2. **代码示例必须包含中文注释**
3. **示例代码必须可直接运行或稍作修改后运行**
4. **复杂功能需要多个递进的示例**

### 6.2 示例代码结构

```rust
/// {函数/功能名称}
/// {详细说明功能作用}
use crate_name::{TypeA, TypeB};

#[tokio::main]  // 如果是异步功能
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 导入依赖或初始化
    let config = Config::default();

    // 2. 创建核心对象
    let client = Client::new(config);

    // 3. 执行操作
    let result = client.operation(param).await?;

    // 4. 处理结果
    println!("{:?}", result);

    Ok(())
}
```

### 6.3 错误处理示例规范

```rust
match result {
    Ok(value) => {
        println!("Success: {:?}", value);
    }
    Err(e) => {
        match e.downcast_ref::<CrateError>() {
            Some(CrateError::NotFound(msg)) => {
                eprintln!("Resource not found: {}", msg);
            }
            Some(CrateError::InvalidInput(msg)) => {
                eprintln!("Invalid input: {}", msg);
            }
            _ => {
                eprintln!("Unknown error: {}", e);
            }
        }
    }
}
```

### 6.4 完整示例规范

每个 README 必须包含一个"完整示例"章节，展示从初始化到实际使用的完整流程：

```markdown
### 完整示例

以下是一个完整的 {功能} 使用示例：

```rust
use crate_name::{ComponentA, ComponentB, Config};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 初始化配置
    let config = Config::builder()
        .with_option("value1")
        .with_option("value2")
        .build();

    // 2. 创建组件
    let component = ComponentA::new(config);

    // 3. 执行操作
    let result = component.process().await?;

    // 4. 输出结果
    println!("Result: {:?}", result);

    Ok(())
}
```
```

---

## 七、输出规则

1. **新建 README**：如果 crate 目录下不存在 README.md，则新建完整文档
2. **更新 README**：如果已存在，则保留现有内容的前置部分（徽章、简介），更新其余章节
3. **文档格式**：使用 Markdown，保持与项目其他文档风格一致
4. **中文注释**：代码中的注释使用中文
5. **FAQ 生成**：如果没有从代码中提取到常见问题，可省略 FAQ 章节
6. **features 说明**：必须标注哪些是默认启用，哪些是可选
7. **使用指南详细程度**：每个 crate 的使用指南必须包含至少 5 个场景，复杂功能的 crate 必须包含更多

---

## 八、生成质量检查清单

生成 README 后，自检以下项目：

- [ ] 是否有项目名称和简介？
- [ ] 是否有快速开始示例？
- [ ] 是否有核心功能列表？
- [ ] 是否有模块结构树？
- [ ] **使用指南是否包含至少 5 个场景？**
- [ ] **每个场景是否有完整可运行的代码？**
- [ ] **代码是否包含中文注释？**
- [ ] **是否包含错误处理示例？**
- [ ] **是否包含完整示例？**
- [ ] **代码示例是否覆盖主要 API？**

---

## 九、示例

### 用户输入

```
为 cmx-database 生成 README 文档
```

### 执行结果

生成 `crates/libs/cmx-infra/cmx-database/README.md`，包含：
- 项目名称与简介（从 Cargo.toml 提取）
- 快速开始（安装指南 + 核心示例）
- 核心功能与特性列表
- 模块结构树
- 使用指南（至少 5-8 个场景，包括）：
  - 数据库管理器初始化
  - 执行查询（单行、多行、参数化）
  - CRUD 操作（插入、更新、删除）
  - 事务处理
  - 连接池监控
  - 错误处理
  - 完整示例
- 常见问题解答（如果有）
