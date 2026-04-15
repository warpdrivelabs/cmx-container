# CMX CLI 开发规划

## 1. 项目概述

### 1.1 目标

开发一个多功能的 CLI 工具（crate），为 CMX 插件开发提供文档生成、插件管理等功能。

### 1.2 位置

* 目录：`sdk/cmx-cli`

* Crate 名称：`cmx-cli`

### 1.3 CLI 命令结构

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

### 1.4 使用示例

```bash
# 扫描生成文档
cmx-cli doc scan ./crates/libs/cmx-wasmdemo -o ./docs/api.json --pretty

# 验证文档格式
cmx-cli doc validate ./docs/api.json

# 创建新插件项目
cmx-cli plugin new my-plugin

# 构建 WASM 插件（预留）
cmx-cli plugin build ./my-plugin --release
```

***

## 2. 现有注释评估

### 2.1 cmx-wasmdemo 函数注释分析

对 `cmx-wasmdemo` 中 16 个 `#[plugin_fn]` 函数的注释进行分析：

| 函数名                | 简短描述 | 详细描述 | # 输入处理 | # 输出 | # 示例 | 自定义节   |
| ------------------ | ---- | ---- | ------ | ---- | ---- | ------ |
| count\_vowels      | ✅    | ✅    | ✅      | ✅    | ✅    | -      |
| demo\_log          | ✅    | ✅    | ✅      | ✅    | ❌    | -      |
| demo\_cache        | ✅    | ✅    | ✅      | ✅    | ✅    | -      |
| demo\_database     | ✅    | ✅    | ✅      | ✅    | ❌    | # 事务支持 |
| demo\_plugin\_call | ✅    | ✅    | ✅      | ✅    | ❌    | -      |
| run\_all\_demos    | ✅    | ✅    | ✅      | ✅    | ❌    | # 测试项  |
| route\_check       | ✅    | ✅    | ✅      | ✅    | ✅    | -      |
| branch\_1\_process | ✅    | ✅    | ✅      | ✅    | ❌    | -      |
| branch\_2\_process | ✅    | ✅    | ✅      | ✅    | ❌    | -      |
| branch\_3\_process | ✅    | ✅    | ✅      | ✅    | ❌    | -      |
| merge\_result      | ✅    | ✅    | ✅      | ✅    | ❌    | -      |
| tx\_insert         | ✅    | ✅    | ✅      | ✅    | ❌    | -      |
| tx\_update         | ✅    | ✅    | ✅      | ✅    | ❌    | -      |
| tx\_query          | ✅    | ✅    | ✅      | ✅    | ❌    | -      |
| tx\_delete         | ✅    | ✅    | ✅      | ✅    | ❌    | -      |
| final\_process     | ✅    | ✅    | ✅      | ✅    | ❌    | -      |

### 2.2 发现的问题

#### 问题 1：示例节缺失（严重）

* **现状**：16 个函数中只有 3 个（18.75%）有 `# 示例` 节

* **影响**：用户无法快速了解函数的输入输出格式

* **建议**：所有函数都应提供示例

#### 问题 2：自定义节不统一

* **现状**：

  * `demo_database` 使用 `# 事务支持`

  * `run_all_demos` 使用 `# 测试项`

* **影响**：解析器难以识别，文档结构不一致

* **建议**：标准化节名称，或允许自定义节但需统一格式

#### 问题 3：输入输出描述格式不统一

* **现状**：有些使用列表格式，有些直接描述

  ```
  // 列表格式
  /// # 输入处理
  /// - `input.input`: 要统计的字符串

  // 直接描述
  /// # 输入处理
  /// 忽略 `input.input`，仅用于演示
  ```

* **影响**：解析结果不一致

* **建议**：统一使用列表格式

#### 问题 4：缺少关键字段

* **现状**：没有 `# 错误`、`# 参数`、`# 返回值` 等节

* **影响**：文档信息不完整

* **建议**：添加这些标准节

#### 问题 5：编码方式未在注释中体现

* **现状**：函数签名使用 `Json<T>` 或 `Msgpack<T>`，但注释未说明

* **影响**：用户不知道如何编码/解码数据

* **建议**：添加 `# 编码` 节或在签名信息中自动提取

***

## 3. 推荐的函数注释规范

### 3.1 标准注释模板

````rust
/// <简短描述>（必填，一行）
///
/// <详细描述>（可选，可多行）
///
/// # 输入
///
/// | 字段 | 类型 | 必填 | 说明 |
/// |------|------|------|------|
/// | `input.input` | string | 是 | 输入数据说明 |
/// | `input.context.initial_input` | string | 否 | 初始入参 |
///
/// # 输出
///
/// | 字段 | 类型 | 说明 |
/// |------|------|------|
/// | `result` | json | 输出数据说明 |
///
/// # 编码
///
/// - 输入编码: `msgpack` / `json`
/// - 输出编码: `msgpack` / `json`
///
/// # 示例
///
/// **输入:**
/// ```json
/// {
///   "input": "{\"name\":\"test\",\"count\":100}",
///   "context": {}
/// }
/// ```
///
/// **输出:**
/// ```json
/// {
///   "result": "{\"message\":\"操作成功\",\"total\":100}"
/// }
/// ```
///
/// # 错误
///
/// - `解析错误`: 输入 JSON 格式不正确
/// - `数据库错误`: 数据库连接失败
///
/// # 注意
///
/// - 特殊说明1
/// - 特殊说明2
#[plugin_fn]
pub fn my_function(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    // ...
}
````

### 3.2 标准节定义

| 节名称    | 必填 | 格式          | 说明                      |
| ------ | -- | ----------- | ----------------------- |
| 简短描述   | ✅  | 纯文本         | 第一行，简洁描述函数功能（不超过 50 字符） |
| 详细描述   | ❌  | 纯文本         | 空行后的多行描述                |
| `# 输入` | ✅  | Markdown 表格 | 输入参数说明                  |
| `# 输出` | ✅  | Markdown 表格 | 输出结果说明                  |
| `# 编码` | ✅  | 列表          | 编码方式说明（可从签名自动提取）        |
| `# 示例` | ✅  | 代码块         | 输入输出示例                  |
| `# 错误` | ❌  | 列表          | 可能的错误情况                 |
| `# 注意` | ❌  | 列表          | 特殊说明                    |

### 3.3 输入/输出表格格式

```markdown
# 输入

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `input.input` | string | 是 | 当前步骤输入数据 |
| `input.context.initial_input` | string | 否 | 初始入参（API 请求传入的参数） |
| `input.context.headers` | object | 否 | HTTP 请求头 |
| `input.context.txn_id` | string | 否 | 事务 ID |

# 输出

| 字段 | 类型 | 说明 |
|------|------|------|
| `result` | string | 函数执行结果（JSON 字符串） |
| `binary_data` | object | 二进制数据（可选） |
```

### 3.4 示例格式

````markdown
# 示例

**输入:**
```json
{
  "input": "{\"name\":\"test\",\"count\":100}",
  "context": {
    "initial_input": "...",
    "headers": {}
  }
}
````

**输出:**

```json
{
  "result": "{\"message\":\"操作成功\",\"total\":100}"
}
```

````

### 3.5 编码节格式

```markdown
# 编码

- 输入编码: `msgpack`
- 输出编码: `msgpack`
````

支持的编码方式：

* `json` - JSON 编码

* `msgpack` - MessagePack 编码

* `raw` - 原始字节

***

## 4. JSON 数据格式规范

### 4.1 完整 JSON 结构

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
      "description": "这是一个简单的字符串处理函数，演示标准入参出参的使用。",
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
          "input": "{\"input\": \"hello world\", \"context\": {}}",
          "output": "{\"result\": \"{\\\"count\\\":3,\\\"total\\\":3}\"}"
        }
      ],
      "errors": [],
      "notes": [],
      "location": {
        "file": "src/lib.rs",
        "line": 97
      }
    }
  ],
  "types": {
    "FunctionInput": {
      "description": "函数输入结构体",
      "fields": [
        {"name": "input", "type": "String", "description": "当前步骤输入数据"},
        {"name": "context", "type": "SVRContext", "description": "服务调用上下文"}
      ]
    },
    "FunctionOutput": {
      "description": "函数输出结构体",
      "fields": [
        {"name": "result", "type": "String", "description": "函数执行结果"}
      ]
    }
  }
}
```

### 4.2 JSON Schema 定义

#### PluginDocument 根结构

```typescript
interface PluginDocument {
  plugin: PluginInfo;
  functions: FunctionDoc[];
  types?: Record<string, TypeDefinition>;
}
```

#### PluginInfo 插件信息

```typescript
interface PluginInfo {
  name: string;           // 插件名称
  version: string;        // 版本号
  description?: string;   // 描述
  generated_at: string;   // 生成时间 (ISO 8601)
}
```

#### FunctionDoc 函数文档

```typescript
interface FunctionDoc {
  name: string;              // 函数名
  summary: string;           // 简短描述
  description?: string;      // 详细描述
  input: InputSpec;          // 输入说明
  output: OutputSpec;        // 输出说明
  examples: Example[];       // 示例（必填）
  errors: string[];          // 错误说明
  notes: string[];           // 注意事项
  location: SourceLocation;  // 源码位置
}
```

#### InputSpec 输入规格

```typescript
interface InputSpec {
  encoding: "json" | "msgpack" | "raw";  // 编码方式
  type: string;           // 类型名称
  fields: FieldSpec[];    // 字段说明
}

interface FieldSpec {
  name: string;           // 字段名
  type: string;           // 字段类型
  required?: boolean;     // 是否必填
  description: string;    // 字段说明
}
```

#### OutputSpec 输出规格

```typescript
interface OutputSpec {
  encoding: "json" | "msgpack" | "raw";  // 编码方式
  type: string;           // 类型名称
  fields: FieldSpec[];    // 字段说明
}
```

#### Example 示例

```typescript
interface Example {
  input: string;    // 输入 JSON
  output: string;   // 输出 JSON
}
```

#### SourceLocation 源码位置

```typescript
interface SourceLocation {
  file: string;    // 相对文件路径
  line: number;    // 起始行号
}
```

#### TypeDefinition 类型定义

```typescript
interface TypeDefinition {
  description?: string;
  fields: FieldSpec[];
}
```

***

## 5. 实现步骤

### 5.1 项目结构

```
sdk/cmx-cli/
├── Cargo.toml
├── src/
│   ├── main.rs              # CLI 入口
│   ├── lib.rs               # 库入口
│   ├── parser/
│   │   ├── mod.rs           # 解析模块
│   │   ├── ast_parser.rs    # AST 解析器（使用 syn）
│   │   ├── doc_parser.rs    # 文档注释解析器
│   │   └── table_parser.rs  # Markdown 表格解析器
│   ├── generator/
│   │   ├── mod.rs           # 生成模块
│   │   └── json_gen.rs      # JSON 生成器
│   ├── models/
│   │   ├── mod.rs           # 模型模块
│   │   └── doc_types.rs     # 文档类型定义
│   └── cli/
│       ├── mod.rs           # CLI 模块
│       └── commands.rs      # 命令定义
└── tests/
    ├── integration_test.rs
    └── fixtures/
        └── sample_plugin.rs  # 测试用例
```

### 5.2 依赖项

```toml
[package]
name = "cmx-cli"
version.workspace = true
edition.workspace = true

[dependencies]
# AST 解析
syn = { version = "2", features = ["full", "parsing", "extra-traits"] }
quote = "1"
proc-macro2 = "1"

# 序列化
serde = { workspace = true }
serde_json = { workspace = true }

# CLI
clap = { workspace = true }

# 错误处理
anyhow = { workspace = true }
thiserror = { workspace = true }

# 日志
tracing = { workspace = true }

# 文件系统
walkdir = "2"

# 时间
chrono = { workspace = true }

# Markdown 解析（用于解析表格）
pulldown-cmark = "0.9"

[dev-dependencies]
tempfile = "3"
```

### 5.3 核心模块实现

#### 步骤 1：创建项目骨架

* 创建 `sdk/cmx-cli` 目录结构

* 配置 `Cargo.toml`

* 更新 workspace `Cargo.toml` 添加成员

#### 步骤 2：定义文档模型（models/doc\_types.rs）

* 定义 `PluginDocument` 结构体

* 定义 `FunctionDoc` 结构体

* 定义 `InputSpec`、`OutputSpec`、`FieldSpec` 结构体

* 实现 `Serialize` trait

#### 步骤 3：实现 AST 解析器（parser/ast\_parser.rs）

* 使用 `syn` 解析 Rust 源文件

* 识别 `#[plugin_fn]` 属性

* 提取函数签名信息（参数类型、返回类型）

* 自动识别编码方式（Json/Msgpack/Raw）

* 获取函数位置信息

#### 步骤 4：实现文档注释解析器（parser/doc\_parser.rs）

* 解析 `///` 文档注释

* 提取简短描述（第一行）

* 提取详细描述（后续行直到第一个 `#` 节）

* 解析各节：`# 输入`、`# 输出`、`# 编码`、`# 示例`、`# 错误`、`# 注意`

#### 步骤 5：实现表格解析器（parser/table\_parser.rs）

* 使用 `pulldown-cmark` 解析 Markdown 表格

* 提取表格字段：字段名、类型、必填、说明

#### 步骤 6：实现 JSON 生成器（generator/json\_gen.rs）

* 组装完整文档结构

* 生成格式化的 JSON 输出

* 支持输出到文件或标准输出

#### 步骤 7：实现 CLI 命令（cli/commands.rs）

* `scan` 命令：扫描指定目录

* 支持配置输出路径

* 支持美化输出

#### 步骤 8：集成测试

* 使用 `cmx-wasmdemo` 作为测试目标

* 验证生成的 JSON 格式正确

* 验证所有函数都被正确识别

***

## 6. CLI 使用方式

### 6.1 基本命令

```bash
# 扫描指定目录并生成文档
cmx-cli scan ./crates/libs/cmx-wasmdemo -o ./docs/plugin-api.json

# 扫描并输出到控制台
cmx-cli scan ./crates/libs/cmx-wasmdemo

# 美化输出
cmx-cli scan ./crates/libs/cmx-wasmdemo -o ./docs/api.json --pretty

# 排除某些文件
cmx-cli scan ./crates/libs/cmx-wasmdemo --exclude "**/tests/**"
```

### 6.2 命令行参数

| 参数              | 简写     | 说明                        |
| --------------- | ------ | ------------------------- |
| `--output`      | `-o`   | 输出文件路径，不指定则输出到 stdout     |
| `--pretty`      | <br /> | 美化 JSON 输出                |
| `--exclude`     | <br /> | 排除的文件模式（glob）             |
| `--plugin-name` | <br /> | 指定插件名称（默认从 Cargo.toml 读取） |
| `--version`     | `-V`   | 显示版本信息                    |
| `--help`        | `-h`   | 显示帮助信息                    |

***

## 7. 示例输出

基于规范化后的 `count_vowels` 函数：

```json
{
  "plugin": {
    "name": "cmx-wasmdemo",
    "version": "0.1.0",
    "description": "CMX WASM 插件演示模块，基于 Extism PDK 开发",
    "generated_at": "2026-04-14T10:30:00Z"
  },
  "functions": [
    {
      "name": "count_vowels",
      "summary": "统计字符串中的元音字母数量",
      "description": "这是一个简单的字符串处理函数，演示标准入参出参的使用。",
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
            "description": "统计结果 JSON，包含 count、total、input 字段"
          }
        ]
      },
      "examples": [
        {
          "input": "{\"input\": \"hello world\", \"context\": {}}",
          "output": "{\"result\": \"{\\\"count\\\":3,\\\"total\\\":3,\\\"input\\\":\\\"hello world\\\"}\"}"
        }
      ],
      "errors": [],
      "notes": [],
      "location": {
        "file": "src/lib.rs",
        "line": 97
      }
    }
  ],
  "types": {
    "FunctionInput": {
      "description": "函数输入结构体",
      "fields": [
        {"name": "input", "type": "String", "description": "当前步骤输入数据"},
        {"name": "context", "type": "SVRContext", "description": "服务调用上下文"},
        {"name": "binary_data", "type": "HashMap<String, Vec<u8>>", "description": "二进制数据"}
      ]
    },
    "FunctionOutput": {
      "description": "函数输出结构体",
      "fields": [
        {"name": "result", "type": "String", "description": "函数执行结果"},
        {"name": "binary_data", "type": "HashMap<String, Vec<u8>>", "description": "二进制数据"}
      ]
    }
  }
}
```

***

## 8. 任务清单

### Phase 1: 项目初始化

* [ ] 创建 `sdk/cmx-cli` 目录结构

* [ ] 创建 `Cargo.toml` 配置

* [ ] 更新 workspace 配置

* [ ] 创建基础模块文件

### Phase 2: 核心解析

* [ ] 实现文档模型结构体

* [ ] 实现 AST 解析器（识别 `#[plugin_fn]` 函数）

* [ ] 实现文档注释解析器（解析各节）

* [ ] 实现 Markdown 表格解析器

### Phase 3: 文档生成

* [ ] 实现 JSON 生成器

* [ ] 实现文件输出

* [ ] 实现标准输出

### Phase 4: CLI 实现

* [ ] 实现命令行参数解析

* [ ] 实现 scan 命令

* [ ] 实现帮助信息

### Phase 5: 测试与验证

* [ ] 编写单元测试

* [ ] 使用 cmx-wasmdemo 进行集成测试

* [ ] 验证 JSON 格式正确性

***

## 9. 注意事项

1. **编码方式自动识别**：从函数签名自动提取编码方式

   * `Json<T>` → json 编码

   * `Msgpack<T>` → msgpack 编码

   * 其他 → raw 编码

2. **向后兼容**：解析器应兼容现有注释格式，但建议规范化

3. **错误处理**：解析错误时提供清晰的错误信息，包括文件路径和行号

4. **性能考虑**：对于大型项目，考虑并行解析多个文件

5. **类型提取**：需要递归提取自定义类型的定义

***

## 10. 现有代码规范化建议

建议对 `cmx-wasmdemo` 中的函数注释进行规范化，主要改进：

1. **添加示例节**：所有函数都应添加 `# 示例` 节
2. **统一输入输出格式**：使用 Markdown 表格格式
3. **添加编码节**：说明编码方式（或从签名自动提取）
4. **添加错误节**：说明可能的错误情况

