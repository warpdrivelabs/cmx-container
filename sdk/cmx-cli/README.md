# CMX CLI - CMX 插件开发工具集

CMX CLI 是一个多功能的命令行工具，为 CMX 插件开发提供文档生成、插件管理等功能。

## 功能特性

- 📄 **文档生成**：扫描 Rust 代码，识别 `#[plugin_fn]` 函数，生成 JSON 格式的 API 文档
- 🔧 **插件管理**：初始化新插件项目、构建 WASM 插件（预留）
- ✅ **文档验证**：验证生成的文档格式（预留）

## 安装

```bash
# 在项目根目录下编译
cargo build -p cmx-cli --release

# 可执行文件位于
./target/release/cmx-cli
```

## 快速开始（使用源码运行）

无需编译，直接使用 `cargo run` 运行：

```bash
# 在项目根目录下运行
cargo run -p cmx-cli -- doc scan ./crates/libs/cmx-wasmdemo -o ./docs/api.json --pretty

# 查看帮助
cargo run -p cmx-cli -- --help
cargo run -p cmx-cli -- doc --help
cargo run -p cmx-cli -- doc scan --help
cargo run -p cmx-cli -- plugin --help
```

> **说明**：`--` 后面的参数会传递给 cmx-cli 程序。

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

## 使用方法

### 帮助命令

查看各层级命令的帮助信息：

```bash
# 查看主命令帮助
cmx-cli --help

# 查看文档命令帮助
cmx-cli doc --help

# 查看扫描命令帮助
cmx-cli doc scan --help

# 查看插件命令帮助
cmx-cli plugin --help

# 查看新建插件命令帮助
cmx-cli plugin new --help
```

### 文档命令 (doc)

#### 扫描生成文档 (doc scan)

```bash
# 扫描指定目录并输出到控制台
cmx-cli doc scan ./crates/libs/cmx-wasmdemo

# 扫描并输出到文件
cmx-cli doc scan ./crates/libs/cmx-wasmdemo -o ./docs/plugin-api.json

# 美化 JSON 输出
cmx-cli doc scan ./crates/libs/cmx-wasmdemo -o ./docs/api.json --pretty

# 扫描多个目录
cmx-cli doc scan ./crates/libs/plugin1 ./crates/libs/plugin2 -o ./docs/api.json

# 排除特定文件
cmx-cli doc scan ./crates/libs/cmx-wasmdemo --exclude "tests"
```

**参数说明：**

| 参数 | 简写 | 说明 |
|------|------|------|
| `PATHS` | | 要扫描的目录或文件路径（支持多个） |
| `--output` | `-o` | 输出文件路径，不指定则输出到 stdout |
| `--pretty` | | 美化 JSON 输出 |
| `--exclude` | | 排除的文件模式 |
| `--plugin-name` | | 指定插件名称（默认从 Cargo.toml 读取） |

#### 验证文档格式 (doc validate)

```bash
# 验证文档 JSON 格式
cmx-cli doc validate ./docs/plugin-api.json
```

### 插件命令 (plugin)

#### 初始化新插件项目 (plugin new)

```bash
# 在当前目录创建新插件项目
cmx-cli plugin new my-plugin

# 在指定目录创建
cmx-cli plugin new my-plugin -p ./plugins
```

生成的项目结构：

```
my-plugin/
├── Cargo.toml
└── src/
    └── lib.rs    # 包含示例函数模板
```

#### 构建 WASM 插件 (plugin build)

```bash
# 构建 WASM 插件（预留功能）
cmx-cli plugin build ./my-plugin

# 发布模式构建
cmx-cli plugin build ./my-plugin --release
```

> **注意**：此功能尚未完全实现，请使用 `cargo build --target wasm32-unknown-unknown`

#### 显示插件信息 (plugin info)

```bash
# 显示 WASM 插件信息（预留功能）
cmx-cli plugin info ./target/my-plugin.wasm
```

## 函数注释规范

为了让解析器正确提取文档信息，建议使用以下注释格式：

### 标准注释模板

```rust
/// <简短描述>（必填，一行）
///
/// <详细描述>（可选，可多行）
///
/// # 输入
///
/// | 字段 | 类型 | 必填 | 说明 |
/// |------|------|------|------|
/// | `input.input` | string | 是 | 输入数据说明 |
///
/// # 输出
///
/// | 字段 | 类型 | 说明 |
/// |------|------|------|
/// | `result` | json | 输出数据说明 |
///
/// # 编码
///
/// - 输入编码: `msgpack`
/// - 输出编码: `msgpack`
///
/// # 示例
///
/// **输入:**
/// ```json
/// {"input": "hello world", "context": {}}
/// ```
///
/// **输出:**
/// ```json
/// {"result": "{\"count\":3}"}
/// ```
///
/// # 错误
///
/// - `解析错误`: 输入 JSON 格式不正确
///
/// # 注意
///
/// - 特殊说明
#[plugin_fn]
pub fn my_function(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    // ...
}
```

### 标准节定义

| 节名称 | 必填 | 格式 | 说明 |
|--------|------|------|------|
| 简短描述 | ✅ | 纯文本 | 第一行，简洁描述函数功能 |
| 详细描述 | ❌ | 纯文本 | 空行后的多行描述 |
| `# 输入` | ✅ | Markdown 表格 | 输入参数说明 |
| `# 输出` | ✅ | Markdown 表格 | 输出结果说明 |
| `# 编码` | ❌ | 列表 | 编码方式（可从签名自动提取） |
| `# 示例` | ✅ | 代码块 | 输入输出示例 |
| `# 错误` | ❌ | 列表 | 可能的错误情况 |
| `# 注意` | ❌ | 列表 | 特殊说明 |

## JSON 输出格式

```json
{
  "plugin": {
    "name": "cmx-wasmdemo",
    "version": "0.1.0",
    "description": "CMX WASM 插件演示模块",
    "generated_at": "2026-04-14T10:30:00Z"
  },
  "functions": [
    {
      "name": "count_vowels",
      "summary": "统计字符串中的元音字母数量",
      "description": "这是一个简单的字符串处理函数",
      "input": {
        "encoding": "msgpack",
        "type": "FunctionInput",
        "fields": [
          {
            "name": "input.input",
            "type": "string",
            "required": true,
            "description": "要统计的字符串"
          }
        ]
      },
      "output": {
        "encoding": "msgpack",
        "type": "FunctionOutput",
        "fields": [
          {
            "name": "result",
            "type": "json",
            "description": "统计结果 JSON"
          }
        ]
      },
      "examples": [
        {
          "input": "{\"input\": \"hello world\"}",
          "output": "{\"result\": \"{\\\"count\\\":3}\"}"
        }
      ],
      "errors": [],
      "notes": [],
      "location": {
        "file": "src/lib.rs",
        "line": 97
      }
    }
  ]
}
```

## 编码方式识别

工具会自动从函数签名中识别编码方式：

| 签名类型 | 编码方式 |
|----------|----------|
| `Json<T>` | json |
| `Msgpack<T>` | msgpack |
| 其他 | raw |

## 示例

### 扫描 cmx-wasmdemo 项目

```bash
cmx-cli doc scan ./crates/libs/cmx-wasmdemo --pretty -o ./docs/wasmdemo-api.json
```

输出：

```
文档已生成: ./docs/wasmdemo-api.json
```

### 创建新插件项目

```bash
cmx-cli plugin new my-awesome-plugin
```

输出：

```
✓ 插件项目已创建: ./my-awesome-plugin

下一步:
  cd my-awesome-plugin
  cargo build --target wasm32-unknown-unknown
```

## 项目结构

```
sdk/cmx-cli/
├── Cargo.toml
├── README.md
├── src/
│   ├── main.rs           # CLI 入口
│   ├── lib.rs            # 库入口
│   ├── parser/
│   │   ├── mod.rs
│   │   ├── ast_parser.rs # AST 解析器
│   │   └── doc_parser.rs # 文档注释解析器
│   ├── generator/
│   │   ├── mod.rs
│   │   └── json_gen.rs   # JSON 生成器
│   ├── models/
│   │   ├── mod.rs
│   │   └── doc_types.rs  # 文档类型定义
│   └── cli/
│       ├── mod.rs
│       └── commands.rs   # CLI 命令
└── tests/
    └── integration_test.rs
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
