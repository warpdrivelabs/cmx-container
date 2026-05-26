# CMX CLI - CMX 插件开发工具集

CMX CLI 是一个多功能的命令行工具，为 CMX 插件开发提供文档生成、插件管理等功能。

## 功能特性

- 📄 **文档生成**：扫描 Rust 代码，生成 JSON 格式的 API 文档
  - **Doc 模式**：基于代码注释生成
  - **AST 模式**：基于 AST 展开结构体，自动提取字段描述
- 🔧 **插件管理**：初始化新插件项目、构建 WASM 插件（预留）
- ✅ **文档验证**：验证生成的文档格式（预留）

## 安装

### 方式一：源码编译（推荐）

```bash
# 在项目根目录下编译
cargo build -p cmx-cli --release

# 可执行文件位于
./target/release/cmx-cli
```

### 方式二：安装到本地（全局使用）

```bash
# 从私有仓库安装
cargo install --registry nora cmx-cli

# 从本地源码安装
cargo install --path sdk/cmx-cli

# 安装后可以直接使用
cmx-cli doc scan ./crates/libs/cmx-wasmdemo -o ./docs/api.json --pretty
```

### 方式三：添加到 PATH（便携使用）

```bash
# Windows: 将编译后的可执行文件路径添加到系统 PATH
# 或创建符号链接到已有 PATH 目录

# Linux/macOS:
ln -s "$(pwd)/target/release/cmx-cli" ~/.local/bin/cmx-cli
```

## 快速开始

### 生成 API 文档

```bash
# 生成文档（默认 AST 模式，展开结构体）
cargo run -p cmx-cli -- doc scan ./crates/libs/cmx-wasmdemo -o ./docs/api.json --pretty

# Doc 模式（基于注释）
cargo run -p cmx-cli -- doc scan ./crates/libs/cmx-wasmdemo --mode doc -o ./docs/api-doc.json

# AST 模式（展开结构体）
cargo run -p cmx-cli -- doc scan ./crates/libs/cmx-wasmdemo --mode ast -o ./docs/api-ast.json

# 同时生成两种文档
cargo run -p cmx-cli -- doc scan ./crates/libs/cmx-wasmdemo --mode both
# 生成 api-doc.json 和 api-ast.json
```

### 查看帮助

```bash
cargo run -p cmx-cli -- --help
cargo run -p cmx-cli -- doc --help
cargo run -p cmx-cli -- doc scan --help
cargo run -p cmx-cli -- plugin --help
```

## 命令概览

```
cmx-cli
├── doc           # 文档相关命令
│   ├── scan      # 扫描 Rust 代码，生成 WASM 函数文档
│   └── validate  # 验证文档格式
└── plugin        # 插件相关命令
    ├── new       # 初始化新插件项目
    ├── build     # 构建 WASM 插件（预留）
    └── info      # 显示插件信息（预留）
```

## 文档生成模式

### Doc 模式（基于注释）

依赖代码中的文档注释生成 API 文档。解析器会识别标准化的章节标题（如 `# Arguments`、`# 输入`）。

```bash
cmx-cli doc scan ./crates/libs/cmx-wasmdemo --mode doc -o api.json
```

**特点**：
- 灵活度高，注释内容完全可控
- 支持手动指定嵌套结构
- 适合复杂业务场景

### AST 模式（展开结构体，默认）

自动解析 Rust 结构体定义，提取字段名、类型和文档注释，递归展开嵌套结构。

```bash
# 默认即为 AST 模式
cmx-cli doc scan ./crates/libs/cmx-wasmdemo -o api.json

# 控制展开深度（默认 3）
cmx-cli doc scan ./crates/libs/cmx-wasmdemo --expand-depth 2 -o api.json
```

**特点**：
- 自动从结构体定义提取信息
- 递归展开嵌套类型（如 `InsertData`）
- 自动提取字段的 `///` 文档注释作为描述

## 注释规范

### 标准注释模板

```rust
/// <简短描述>（必填，一行）
///
/// <详细描述>（可选，可多行）
///
/// # Arguments
///
/// * `input` - 输入数据
///   * `input` - 业务输入数据
///   * `context` - 上下文
///     * `context.txn_id` - 事务ID
///
/// # Returns
///
/// * `result` - 操作结果
#[plugin_fn]
pub fn my_function(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    // ...
}
```

### 章节名称标准化

解析器会自动将以下章节名标准化：

| 输入章节 | 输出章节 | 其他章节 |
|---------|---------|---------|
| `# Arguments` | `# Returns` | `# Errors` |
| `# 输入` | `# 输出` | `# Panic` |
| `# 输入处理` | - | `# Safety` |

### 结构体字段注释

AST 模式会从结构体字段的文档注释中提取描述：

```rust
/// 插入数据
///
/// 用于事务插入函数的输入参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InsertData {
    /// 表名
    pub table: String,
    /// 名称字段值
    pub name: String,
    /// 数值字段值
    pub value: i32,
}
```

生成的 JSON 中 `InsertData` 的 `properties` 会包含：
```json
"properties": [
  {"name": "table", "type": "string", "description": "表名"},
  {"name": "name", "type": "string", "description": "名称字段值"},
  {"name": "value", "type": "integer", "description": "数值字段值"}
]
```

### 函数类型标注

使用 `#[doc_type]` 属性标注函数类型：

| 属性 | 类型 | 说明 |
|------|------|------|
| 无 `#[doc_type]` 属性 | `func` | 普通处理函数 |
| `#[doc_type = "func"]` | `func` | 普通处理函数（显式声明） |
| `#[doc_type = "branch_fn"]` | `branch_fn` | 分支函数 |

```rust
/// 分支1处理函数
///
/// # Arguments
///
/// * `input` - 前序步骤的输出
///
/// # Returns
///
/// * `result` - 处理结果
#[doc_type = "branch_fn"]
#[plugin_fn]
pub fn branch_1_process(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    // ...
}
```

## JSON 输出格式

```json
{
  "plugin": {
    "name": "cmx-wasmdemo",
    "version": "0.1.0",
    "generated_at": "2026-05-26T02:00:00Z"
  },
  "functions": [
    {
      "name": "tx_insert",
      "type": "func",
      "summary": "事务插入函数",
      "description": "在事务中执行插入操作，通过上下文获取事务ID确保在同一事务中执行",
      "input": {
        "encoding": "msgpack",
        "type": "FunctionInput",
        "fields": [
          {
            "name": "input",
            "type": "object",
            "required": true,
            "description": "函数输入，包含 `InsertData` 格式的插入数据",
            "properties": [
              {"name": "table", "type": "string", "required": true, "description": "表名"},
              {"name": "name", "type": "string", "required": true, "description": "名称字段值"},
              {"name": "value", "type": "integer", "required": true, "description": "数值字段值"}
            ]
          }
        ]
      },
      "output": {
        "encoding": "msgpack",
        "type": "FunctionOutput"
      },
      "examples": [],
      "location": {
        "file": "./crates/libs/cmx-wasmdemo/src/extism_layer.rs",
        "line": 301
      }
    }
  ]
}
```

### FieldSpec 字段说明

| 字段 | 类型 | 说明 |
|------|------|------|
| `name` | string | 字段名 |
| `type` | string | JSON Schema 类型（string, integer, object, array 等） |
| `format` | string? | 格式（如类型名） |
| `required` | bool? | 是否必填 |
| `description` | string | 字段描述（末尾句号已去除） |
| `properties` | FieldSpec[]? | 嵌套字段（object 类型时展开） |
| `items` | FieldSpec? | 数组元素类型（array 类型时使用） |

## 编码方式识别

工具会自动从函数签名中识别编码方式：

| 签名类型 | 编码方式 |
|----------|----------|
| `Json<T>` | json |
| `Msgpack<T>` | msgpack |
| 其他 | raw |

## 常用命令

```bash
# 生成 API 文档（自动创建 docs 目录）
cargo run -p cmx-cli -- doc scan ./crates/libs/cmx-wasmdemo -o ./docs/api.json --pretty

# AST 模式（默认），控制展开深度
cargo run -p cmx-cli -- doc scan ./crates/libs/cmx-wasmdemo --expand-depth 2 -o ./docs/api.json

# Doc 模式
cargo run -p cmx-cli -- doc scan ./crates/libs/cmx-wasmdemo --mode doc -o ./docs/api-doc.json

# 同时生成两种文档
cargo run -p cmx-cli -- doc scan ./crates/libs/cmx-wasmdemo --mode both -o ./docs/api.json

# 排除特定文件
cargo run -p cmx-cli -- doc scan ./crates/libs/cmx-wasmdemo --exclude "tests" -o ./docs/api.json

# 指定插件名称
cargo run -p cmx-cli -- doc scan ./crates/libs/cmx-wasmdemo --plugin-name "my-plugin" -o ./docs/api.json

# 验证文档格式
cargo run -p cmx-cli -- doc validate ./docs/api.json

# 创建新插件项目
cargo run -p cmx-cli -- plugin new my-plugin
```

### doc scan 参数说明

| 参数 | 简写 | 默认值 | 说明 |
|------|------|--------|------|
| `PATHS` | | | 要扫描的目录或文件路径（支持多个） |
| `--output` | `-o` | stdout | 输出文件路径 |
| `--pretty` | | | 美化 JSON 输出 |
| `--mode` | | `ast` | 生成模式：doc（基于注释）、ast（基于 AST）、both（两者都生成） |
| `--expand-depth` | | `3` | AST 模式下的结构体展开深度 |
| `--exclude` | | | 排除的文件模式 |
| `--plugin-name` | | 从 Cargo.toml 读取 | 指定插件名称 |

## 项目结构

```
sdk/cmx-cli/
├── Cargo.toml
├── README.md
└── src/
    ├── main.rs              # CLI 入口
    ├── lib.rs               # 库入口
    ├── ast_parser/          # AST 解析模块
    │   ├── mod.rs
    │   ├── struct_parser.rs # 结构体定义解析
    │   └── type_resolver.rs # 类型注册与解析
    ├── parser/              # 注释解析模块
    │   ├── mod.rs
    │   └── doc_parser.rs    # 文档注释解析
    ├── generator/           # JSON 生成模块
    │   ├── mod.rs
    │   ├── json_gen.rs     # Doc 模式生成器
    │   └── ast_json_gen.rs  # AST 模式生成器
    ├── models/              # 数据模型
    │   ├── mod.rs
    │   └── doc_types.rs     # 文档类型定义
    └── cli/                 # CLI 命令
        ├── mod.rs
        └── commands.rs      # 命令定义
```

## 依赖

- `syn` - Rust 源码解析
- `clap` - 命令行参数解析
- `serde` / `serde_json` - JSON 序列化
- `pulldown-cmark` - Markdown 解析
- `walkdir` - 目录遍历
- `toml` - TOML 解析

## 许可证

MIT License
