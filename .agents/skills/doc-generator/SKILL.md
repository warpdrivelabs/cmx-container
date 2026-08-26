---
name: doc-generator
description: 根据 Rust crate 代码生成或更新 README 文档。当用户要求"生成文档"、"更新 README"、"创建 crate 文档"、为 crate 补 README 时必用。
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
生成 README 时，`## 核心功能与特性`、`## 模块结构`、`## 使用指南`、`## 常见问题` 四段的可直接套用模板正文见 [references/readme-template.md](references/readme-template.md)（文末另附完整生成示例）。


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
- [ ] **代码示例是否覆盖主要 API？**

---
