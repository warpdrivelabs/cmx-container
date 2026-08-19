# CMX CLI - CMX 插件开发工具集

CMX CLI 是一个多功能的命令行工具，为 CMX 插件开发提供文档生成、插件管理等功能。

## 功能特性

- 📄 **文档生成**：扫描 Rust 代码，结合文档注释和结构体定义，生成 JSON 格式的 API 文档
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
# 生成文档（结合注释和结构体定义）
cargo run -p cmx-cli -- doc scan ./crates/libs/cmx-wasmdemo -o ./docs/api.json --pretty

# 控制结构体展开深度（默认 5）
cargo run -p cmx-cli -- doc scan ./crates/libs/cmx-wasmdemo --expand-depth 5 -o ./docs/api.json
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

## 文档生成原理

cmx-cli 采用唯一的 AST 模式，将**文档注释解析**和**结构体定义解析**结合使用：

- **注释提供**：summary（摘要）、description（描述）、函数级参数名、表格子字段、required（必填标记）
- **结构体提供**：字段名、字段类型、字段描述、嵌套展开
- **优先级**：表格子字段 > TypeRegistry 展开 > 默认值

### 三种情况

#### 情况1：有表格注释（注释优先）

```rust
/// * `input` - 函数输入，包含 `InsertData` 格式的数据。
///
/// | 字段 | 类型 | 必填 | 说明 |
/// |------|------|------|------|
/// | `table` | string | 是 | 表名 |
/// | `value` | integer | 否 | 数值 |
```

→ 直接使用表格中的类型和描述，结构体定义不介入。

#### 情况2：无表格，注释引用了已注册类型（结合使用）

```rust
/// * `input` - 函数输入，包含 `InsertData` 格式的数据。
```

→ 从描述中提取 `InsertData` → 查 TypeRegistry → 递归展开所有字段。

#### 情况3：无表格，注释也没引用类型（只有注释）

```rust
/// * `input` - 函数输入，输入为动态数据，来源于上一步骤的输出。
```

→ fallback 为 `serde_json::Value`。

## 注释规范

### 标准注释模板

```rust
/// <简短描述>（必填，一行，不以句号结尾）
///
/// <详细描述>（可选，可多行）
///
/// # Arguments
///
/// * `input` - 函数输入，包含 `XxxData` 格式的数据。
///
/// | 字段 | 类型 | 必填 | 说明 |
/// |------|------|------|------|
/// | `field1` | string | 是 | 字段1说明 |
/// | `field2` | integer | 是 | 字段2说明 |
///
/// # Returns
///
/// 返回描述。
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

结构体字段的文档注释会被自动提取：

```rust
/// 插入数据
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

当注释中引用 `InsertData` 时，会自动递归展开为包含 `table`、`name`、`value` 的完整字段结构。

### 函数类型标注

使用 `#[doc_type]` 属性标注函数类型：

| 属性 | 类型 | 说明 |
|------|------|------|
| 无 `#[doc_type]` 属性 | `func` | 普通处理函数 |
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
/// 返回处理结果。
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
            "description": "函数输入，包含 InsertData 格式的插入数据",
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

# 控制展开深度
cargo run -p cmx-cli -- doc scan ./crates/libs/cmx-wasmdemo --expand-depth 2 -o ./docs/api.json

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
| `--expand-depth` | | `5` | 结构体展开深度（与代码 `default_value = "5"` 一致） |
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
    │   ├── ast_parser.rs    # 函数签名解析
    │   └── doc_parser.rs    # 文档注释解析
    ├── generator/           # JSON 生成模块
    │   ├── mod.rs
    │   └── ast_json_gen.rs  # 文档生成器
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

Apache-2.0（随仓库根 [LICENSE](../../LICENSE)，Cargo.toml 无独立 license 声明）。
