# cmx-cli

> CMX 插件文档生成器 CLI，用于扫描 Rust 代码中的 `#[plugin_fn]` 属性函数并生成 JSON 格式的 API 文档。

## 项目简介

cmx-cli 是一个命令行工具，用于扫描 Rust 代码，识别 `#[plugin_fn]` 属性函数，解析文档注释并生成结构化的 JSON API 文档。

## 快速开始

### 安装

```bash
cargo install cmx-cli
```

### 核心示例

```bash
# 扫描代码生成文档
cmx-cli doc scan src/ --output docs/api.json

# 美化输出
cmx-cli doc scan src/ --output docs/api.json --pretty

# 初始化插件项目
cmx-cli plugin new my-plugin
```

## 核心功能与特性

| 功能 | 说明 |
|------|------|
| 文档扫描 | 自动识别 `#[plugin_fn]` 属性函数 |
| 文档解析 | 提取函数的文档注释 |
| JSON 生成 | 生成结构化 JSON 文档 |
| 插件初始化 | 创建新的插件项目 |
| 自定义输出 | 支持指定输出文件和美化 JSON |

## 模块结构

```
cmx-cli
├── src/
│   ├── lib.rs              # 库入口
│   ├── main.rs             # 主程序入口
│   ├── cli/
│   │   ├── commands.rs     # 命令解析与执行
│   │   └── mod.rs
│   ├── generator/
│   │   ├── json_gen.rs     # JSON 文档生成
│   │   └── mod.rs
│   ├── models/
│   │   ├── doc_types.rs    # 数据模型定义
│   │   └── mod.rs
│   └── parser/
│       ├── ast_parser.rs    # AST 解析器
│       ├── doc_parser.rs    # 文档注释解析器
│       └── mod.rs
└── Cargo.toml
```

## 使用指南

### 一、命令概述

```bash
cmx-cli <COMMAND> [OPTIONS]

可用命令：
  doc       文档相关操作
  plugin    插件相关操作
  version   显示版本信息
  help      显示帮助信息
```

### 二、文档扫描命令 (doc scan)

#### 2.1 基础用法

```bash
# 扫描单个目录
cmx-cli doc scan src/

# 扫描多个路径
cmx-cli doc scan src/ lib/

# 指定输出文件
cmx-cli doc scan src/ --output docs/api.json
```

#### 2.2 完整选项

```bash
cmx-cli doc scan <PATHS>... [OPTIONS]

位置参数：
  <PATHS>              要扫描的目录或文件路径

选项：
  -o, --output <FILE>     输出文件路径（默认为 stdout）
  -p, --pretty            美化 JSON 输出
  -e, --exclude <PATTERN> 排除匹配的文件模式（可重复）
  -n, --plugin-name <NAME> 指定插件名称
  --include-hidden        包含隐藏文件
  -v, --verbose           详细输出模式
  -h, --help              显示帮助信息
```

#### 2.3 使用示例

```bash
# 美化输出的完整示例
cmx-cli doc scan src/ \
  --output docs/plugin-api.json \
  --pretty \
  --plugin-name "my-plugin" \
  --exclude "**/tests/**" \
  --exclude "**/target/**" \
  --verbose
```

### 三、插件初始化命令 (plugin new)

#### 3.1 基础用法

```bash
# 在当前目录创建插件项目
cmx-cli plugin new my-plugin

# 指定目录创建
cmx-cli plugin new my-plugin --path ./plugins
```

#### 3.2 完整选项

```bash
cmx-cli plugin new <NAME> [OPTIONS]

位置参数：
  <NAME>                  插件名称

选项：
  -p, --path <PATH>       目标路径（默认为当前目录）
  -t, --template <TEMPLATE>  使用模板（basic, service, data-processor）
  --no-git                 跳过 git 初始化
  -h, --help              显示帮助信息
```

#### 3.3 模板类型

```bash
# 创建基础插件模板
cmx-cli plugin new my-plugin -t basic

# 创建服务类型插件
cmx-cli plugin new my-service-plugin -t service

# 创建数据处理插件
cmx-cli plugin new my-data-plugin -t data-processor
```

### 四、插件构建命令 (plugin build)

#### 4.1 构建插件

```bash
# 构建为 WASM
cmx-cli plugin build

# 指定目标目录
cmx-cli plugin build --target ./dist

# 发布模式构建
cmx-cli plugin build --release
```

#### 4.2 完整选项

```bash
cmx-cli plugin build [OPTIONS]

选项：
  -t, --target <DIR>       输出目录
  -r, --release           发布模式构建
  -d, --debug             调试模式构建
  --no-verify            跳过签名验证
  -h, --help             显示帮助信息
```

### 五、插件打包命令 (plugin package)

#### 5.1 打包为 ZIP

```bash
# 打包当前插件
cmx-cli plugin package

# 指定输出文件
cmx-cli plugin package --output ./my-plugin.zip

# 包含额外文件
cmx-cli plugin package --include "config/**" --include "static/**"
```

#### 5.2 完整选项

```bash
cmx-cli plugin package [OPTIONS]

选项：
  -o, --output <FILE>     输出文件路径
  -i, --include <PATTERN> 包含匹配的文件（可重复）
  --exclude <PATTERN>      排除匹配的文件（可重复）
  -h, --help              显示帮助信息
```

### 六、生成文档格式

#### 6.1 输出 JSON 结构

```json
{
  "plugin": {
    "name": "my-plugin",
    "version": "1.0.0",
    "description": "我的插件"
  },
  "functions": [
    {
      "name": "process_data",
      "description": "处理输入数据",
      "parameters": [
        {
          "name": "input",
          "type": "FunctionInput",
          "required": true,
          "description": "输入数据"
        }
      ],
      "returns": {
        "type": "FunctionOutput",
        "description": "处理结果"
      }
    }
  ],
  "generated_at": "2024-01-15T10:30:00Z",
  "generator_version": "0.1.0"
}
```

#### 6.2 函数文档结构

```json
{
  "name": "function_name",
  "description": "函数功能描述",
  "input": {
    "type": "FunctionInput",
    "fields": [
      {
        "name": "input",
        "type": "string",
        "description": "当前步骤输入"
      },
      {
        "name": "context",
        "type": "SVRContext",
        "description": "服务调用上下文"
      }
    ]
  },
  "output": {
    "type": "FunctionOutput",
    "fields": [
      {
        "name": "result",
        "type": "Value",
        "description": "函数执行结果"
      }
    ]
  },
  "examples": [
    {
      "title": "基础用法",
      "code": "..."
    }
  ]
}
```

### 七、配置文件

#### 7.1 项目配置 (.cmx-cli.toml)

```toml
[project]
name = "my-plugin"
version = "1.0.0"

[build]
target = "wasm32-unknown-unknown"
out_dir = "dist"

[doc]
output = "docs/api.json"
include_private = false

[package]
include = ["**/*.wasm", "manifest.json"]
exclude = ["**/*.rs", "**/target/**"]
```

#### 7.2 全局配置 (~/.cmx-cli/config.toml)

```toml
[defaults]
plugin_template = "basic"
output_format = "json"

[paths]
default_plugin_dir = "~/plugins"
default_output_dir = "./docs"

[build]
rustup_target = "wasm32-unknown-unknown"
```

### 八、解析规则

#### 8.1 函数识别规则

```rust
// ✅ 被识别的函数格式
#[plugin_fn]
pub fn my_function(Json(input): Json<FunctionInput>) -> FnResult<Json<FunctionOutput>> {
}

// ❌ 不被识别的函数格式（缺少 #[plugin_fn] 属性）
pub fn helper_function() {
}

// ✅ 内部函数也会被解析文档
/// 这是一个内部函数
fn internal_helper() {
}
```

#### 8.2 文档注释解析

```rust
/// 函数功能简述
///
/// # 参数
/// - `input`: 输入数据
/// - `context`: 上下文信息
///
/// # 返回值
/// 返回处理结果
///
/// # 示例
/// ```rust
/// let result = my_function(data);
/// ```
#[plugin_fn]
pub fn documented_function(/* ... */) -> /* ... */ {
}
```

### 九、常见问题

#### 9.1 扫描不到函数

```bash
# 检查是否正确添加了 #[plugin_fn] 属性
# 确保函数是 pub 的
# 使用 verbose 模式查看详细信息
cmx-cli doc scan src/ --verbose
```

#### 9.2 输出格式错误

```bash
# 确保 Rust 代码可以正常编译
# 检查是否有语法错误
cargo check
```

#### 9.3 性能问题

```bash
# 对于大型项目，可以排除不需要的目录
cmx-cli doc scan src/ \
  --exclude "**/tests/**" \
  --exclude "**/ benches/**" \
  --exclude "**/target/**"
```
