# cmx-cli 文档生成原理详解

> 本文档详细介绍 `cmx-cli` 如何从 Rust 源码解析生成 `api.json` 的完整流程。

---

## 一、整体架构

```
┌─────────────────────────────────────────────────────────────────────┐
│                         cmx-cli doc scan                            │
│                                                                     │
│  ┌──────────┐    ┌──────────────┐    ┌──────────────┐    ┌───────┐ │
│  │  CLI 入口 │───▶│  源码扫描收集  │───▶│  AST 解析引擎  │───▶│ JSON  │ │
│  │ commands  │    │ collect_files│    │  syn + doc    │    │ 生成  │ │
│  └──────────┘    └──────────────┘    └──────────────┘    └───────┘ │
│                                                                     │
│  模块结构:                                                          │
│  ├── cli/commands.rs        CLI 命令定义与入口                       │
│  ├── parser/                解析层                                   │
│  │   ├── ast_parser.rs      AST 解析（识别 #[plugin_fn]）            │
│  │   └── doc_parser.rs      文档注释解析（提取摘要/参数/表格）         │
│  ├── ast_parser/            结构体解析层                              │
│  │   ├── struct_parser.rs   结构体定义解析                            │
│  │   └── type_resolver.rs   类型注册表与递归展开                      │
│  ├── generator/             生成层                                   │
│  │   └── ast_json_gen.rs    文档生成器（注释 + 结构体结合）            │
│  └── models/                数据模型                                 │
│      └── doc_types.rs       输出 JSON 结构定义                       │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 二、执行流程（Pipeline）

### 2.1 命令入口

```bash
cmx-cli doc scan ./src --pretty --output ./api/api.json
```

入口函数 `handle_scan_command`（[commands.rs:188](file:///media/yqs/工作/rustspace/cmx/cmx-container/sdk/cmx-cli/src/cli/commands.rs#L188)）执行以下步骤：

```
1. collect_rust_files()     → 递归扫描目录，收集 .rs 文件
2. parse_rust_file()        → 使用 syn 解析每个文件，提取 #[plugin_fn] 函数
3. parse_doc_comments()     → 解析每个函数的 /// 文档注释
4. parse_structs()          → 解析文件中的结构体定义
5. TypeRegistry::register_all() → 将结构体注册到类型注册表
6. generate_ast_document()  → 生成最终 JSON 文档
7. output_json()            → 输出到文件或标准输出
```

### 2.2 生成模式

cmx-cli 使用唯一的 AST 模式，结合文档注释和结构体定义生成完整文档：

```bash
cmx-cli doc scan ./src --pretty --output ./api/api.json
```

---

## 三、解析层详解

### 3.1 AST 解析器 — 识别 `#[plugin_fn]` 函数

**文件**: [parser/ast_parser.rs](file:///media/yqs/工作/rustspace/cmx/cmx-container/sdk/cmx-cli/src/parser/ast_parser.rs)

**职责**: 使用 `syn` 库解析 Rust 源文件，识别带有 `#[plugin_fn]` 属性的函数，提取函数签名信息。

**输出结构** `ParsedFunction`:

```rust
pub struct ParsedFunction {
    pub name: String,            // 函数名，如 "route_check"
    pub doc_comments: Vec<String>, // 所有 /// 注释行
    pub doc_type: String,        // "func" 或 "branch_fn"
    pub input_type: String,      // 输入类型，如 "FunctionInput"
    pub output_type: String,     // 输出类型，如 "FunctionOutput"
    pub input_encoding: String,  // "json" / "msgpack" / "raw"
    pub output_encoding: String, // "json" / "msgpack" / "raw"
    pub line: usize,             // 函数起始行号
}
```

**关键解析逻辑**:

1. **识别 `#[plugin_fn]`**: 遍历函数属性，检查是否有 `plugin_fn` 标识
2. **提取文档注释**: 过滤 `#[doc = "..."]` 属性，收集所有 `///` 注释内容
3. **提取 `#[doc_type]`**: 读取 `#[doc_type = "branch_fn"]` 属性，默认为 `"func"`
4. **解析输入类型**: 从 `Msgpack(input): Msgpack<FunctionInput>` 中提取：
   - 内部类型 `FunctionInput`
   - 编码方式 `msgpack`
5. **解析输出类型**: 从 `FnResult<Msgpack<FunctionOutput>>` 中提取：
   - 先剥离 `FnResult<...>` 外壳
   - 再从 `Msgpack<FunctionOutput>` 提取类型和编码

**编码方式识别规则**:

| 签名中的包装类型 | 识别的编码方式 |
|------------------|----------------|
| `Json<T>` | `json` |
| `Msgpack<T>` | `msgpack` |
| 其他 | `raw` |

### 3.2 文档注释解析器 — 提取结构化信息

**文件**: [parser/doc_parser.rs](file:///media/yqs/工作/rustspace/cmx/cmx-container/sdk/cmx-cli/src/parser/doc_parser.rs)

**职责**: 将 `///` 注释文本解析为结构化的 `ParsedDoc`，这是整个解析流程中最复杂的部分。

**输出结构** `ParsedDoc`:

```rust
pub struct ParsedDoc {
    pub summary: String,                // 摘要（第一行）
    pub description: Option<String>,    // 详细描述
    pub input_fields: Vec<FieldInfo>,   // 输入字段
    pub output_fields: Vec<FieldInfo>,  // 输出字段
    pub input_encoding: Option<String>, // 输入编码
    pub output_encoding: Option<String>,// 输出编码
    pub examples: Vec<ExampleInfo>,     // 示例
    pub errors: Vec<String>,            // 错误说明
    pub notes: Vec<String>,             // 注意事项
    pub panics: Vec<String>,            // Panic 场景
    pub safety: Option<String>,         // Safety 说明
}
```

**解析流程**:

```
原始注释文本
    │
    ▼
parse_sections()          ← 按标题分节（# Arguments, # Returns 等）
    │
    ├── "" (无标题)       → 提取 summary + description
    ├── "Arguments"       → parse_nested_fields() → input_fields
    ├── "Returns"         → parse_nested_fields() → output_fields
    ├── "Examples"        → parse_examples()      → examples
    ├── "Errors"          → parse_list()          → errors
    ├── "Notes"           → parse_list()          → notes
    ├── "Panics"          → parse_list()          → panics
    └── "Safety"          → 原文保存              → safety
```

#### 3.2.1 章节分割 — `parse_sections()`

将注释文本按 `#` 标题分割为 HashMap。支持中英文标题：

| 英文 | 中文 | 标准化名称 |
|------|------|-----------|
| Arguments | 参数 / 输入 | Arguments |
| Returns | 返回值 / 输出 | Returns |
| Examples | 示例 | Examples |
| Errors | 错误 | Errors |
| Notes | 注意 | Notes |
| Panics | — | Panics |
| Safety | — | Safety |

空字符串 key `""` 保存标题前的内容（摘要和描述）。

#### 3.2.2 摘要和描述提取

```
/// 路由判断函数              ← summary（自动去除末尾句号）
///
/// 根据输入的 route 字段决定返回哪个分支标识。  ← description
```

- **summary**: 第一行，自动去除末尾的 `.` 和 `。`
- **description**: 第二段及之后，跳过空行

#### 3.2.3 嵌套字段解析 — `parse_nested_fields()`

这是最核心的解析逻辑，支持两种模式：

**模式一：列表行 + 表格（推荐）**

```rust
/// * `input` - 函数输入，包含 `RouteInput` 格式的路由参数。
///
/// | 字段 | 类型 | 必填 | 说明 |
/// |------|------|------|------|
/// | `route` | string | 是 | 路由标识 |
```

解析步骤：
1. `parse_field_line()` 解析 `* \`input\` - ...` 列表行，生成 `input` 字段
2. `skip_empty_lines()` 跳过空行
3. `is_table_start()` 检测到 `| 字段 | 类型 |` + `|------|` 表头
4. `collect_table_lines()` 收集连续的 `|` 开头行
5. `parse_table()` 解析表格，将子字段挂载到 `input.sub_fields`
6. `input.type_name` 自动设为 `"object"`

**模式二：纯表格**

```rust
/// | 字段 | 类型 | 必填 | 说明 |
/// |------|------|------|------|
/// | `table` | string | 是 | 表名 |
```

当没有列表行时，直接解析为顶级字段列表。

#### 3.2.4 列表行解析 — `parse_field_line()`

支持两种格式：

```
* `name` - description.     ← 短横线分隔（推荐）
* `name`: description.      ← 冒号分隔
```

解析后从 description 中提取类型信息（`extract_type_from_description`）：
- 扫描反引号中的大写开头的标识符（如 `RouteInput`、`InsertData`）
- 优先返回在 TypeRegistry 中已注册的类型名
- 如果没有匹配，返回 `"unknown"`

#### 3.2.5 表格解析 — `parse_table()`

表格格式要求：

```markdown
| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `route` | string | 是 | 路由标识 |
| `count` | integer | 否 | 计数值 |
```

解析规则：
- 第1列：字段名（自动去除反引号）
- 第2列：类型（自动去除多余空格，使用 `clean_field_name`）
- 第3列：必填（包含 "是" 或 "true" 则为必填）
- 第4列：说明
- 包含 `---` 的分隔行被跳过
- 表头行被跳过

---

## 四、结构体解析层详解

### 4.1 结构体解析器

**文件**: [ast_parser/struct_parser.rs](file:///media/yqs/工作/rustspace/cmx/cmx-container/sdk/cmx-cli/src/ast_parser/struct_parser.rs)

**职责**: 使用 `syn` 解析 Rust 源文件中的结构体定义，提取字段名、类型和文档注释。

**输出结构** `StructDefinition`:

```rust
pub struct StructDefinition {
    pub name: String,               // 结构体名，如 "RouteInput"
    pub fields: Vec<FieldDefinition>, // 字段列表
    pub is_tuple: bool,             // 是否为元组结构体
}

pub struct FieldDefinition {
    pub name: String,        // 字段名
    pub type_name: String,   // 类型名，如 "String", "i32"
    pub required: bool,      // 是否必填（默认 true）
    pub description: String, // 文档注释（多行合并）
}
```

**解析逻辑**:
- 遍历 AST 中的 `Item::Struct`
- 提取 `Named` 字段和 `Unnamed`（元组）字段
- 每个字段的 `///` 注释合并为 `description`
- 类型通过递归 `type_to_string()` 转为字符串（支持泛型、引用、切片等）

### 4.2 类型注册表与递归展开

**文件**: [ast_parser/type_resolver.rs](file:///media/yqs/工作/rustspace/cmx/cmx-container/sdk/cmx-cli/src/ast_parser/type_resolver.rs)

**职责**: 维护结构体定义注册表，支持递归展开嵌套类型。

**核心数据结构** `TypeRegistry`:

```rust
pub struct TypeRegistry {
    structs: HashMap<String, StructDefinition>,
}
```

**关键方法**:

#### `resolve_type(type_name, field_name, description, max_depth)`

递归展开类型的入口方法。流程：

```
resolve_type("InsertData", "input", "插入数据", 3)
    │
    ▼ 在注册表中查找 "InsertData"
    │
    ├── 找到 → 递归展开子字段
    │   ├── table: String   → ResolvedField { name: "table", type_name: "String", ... }
    │   ├── name: String    → ResolvedField { name: "name", type_name: "String", ... }
    │   └── value: i32      → ResolvedField { name: "value", type_name: "i32", ... }
    │
    └── 未找到 → 返回基本字段
        ResolvedField { name: "input", type_name: "InsertData", sub_fields: [], is_expanded: false }
```

**深度控制**: `max_depth` 参数限制递归深度，防止循环引用导致无限递归。默认值为 5。

**类型名清理** `clean_type_name()`:
- 去除 `&` 和 `mut` 前缀
- 去除 `crate::` 前缀，取最后一段
- 处理路径中的 `::` 分隔符

**类型判断辅助函数**:

| 函数 | 用途 |
|------|------|
| `is_primitive_type()` | 判断是否为基本类型（String, i32, bool 等） |
| `is_container_type()` | 判断是否为容器类型（Vec, Option, HashMap 等） |
| `extract_container_element()` | 提取容器元素类型（Vec\<String\> → String） |

---

## 五、生成层详解

### 5.1 生成器

**文件**: [generator/ast_json_gen.rs](file:///media/yqs/工作/rustspace/cmx/cmx-container/sdk/cmx-cli/src/generator/ast_json_gen.rs)

**职责**: 结合文档注释和结构体定义，生成完整的嵌套字段文档。

**核心流程**:

```
build_ast_function_doc(func, doc, file_path, registry, expand_depth)
    │
    ├── build_ast_fields(doc.input_fields, registry, expand_depth)
    │   │
    │   └── 对每个 FieldInfo 调用 resolve_field_to_spec()
    │       │
    │       ├── 有 sub_fields（表格子字段）?
    │       │   └── table_field_to_spec()  ← 直接使用表格中的类型和描述
    │       │
    │       ├── type_name != "unknown"?
    │       │   └── 使用注释中提取的类型
    │       │
    │       └── type_name == "unknown"?
    │           └── extract_type_from_description()  ← 从描述的反引号中提取类型名
    │               │
    │               └── registry.resolve_type()  ← 递归展开结构体
    │                   │
    │                   └── resolved_field_to_spec()  ← 转为 FieldSpec
    │
    └── build_ast_fields(doc.output_fields, registry, expand_depth)
```

#### 关键函数详解

**`resolve_field_to_spec(field, registry, max_depth)`**

字段转换的核心决策函数：

1. **有 `sub_fields`**（来自表格解析）→ 调用 `table_field_to_spec()`
   - 直接使用表格中声明的类型（string/integer/boolean 等）
   - 直接使用表格中的描述（不被 TypeRegistry 覆盖）
   - 类型映射：`string`→`string`, `integer`→`integer`, `boolean`→`boolean` 等

2. **`type_name` 不为 "unknown"** → 使用该类型名，通过 `registry.resolve_type()` 展开

3. **`type_name` 为 "unknown"** → 从描述中提取类型：
   - `extract_type_from_description()` 扫描反引号中的大写标识符
   - 优先匹配 TypeRegistry 中已注册的类型
   - 如果都没匹配，默认为 `serde_json::Value`

**`table_field_to_spec(field)`**

表格子字段的专用转换函数，确保表格中的信息不被 TypeRegistry 覆盖：

| 表格中的类型 | 映射为 |
|-------------|--------|
| string / str / String | string |
| integer / i8..i64 / u8..u64 | integer |
| number / f32 / f64 | number |
| boolean / bool | boolean |
| array | array |
| 其他 | object |

**`resolved_field_to_spec(resolved)`**

将 TypeRegistry 展开后的 `ResolvedField` 转为 `FieldSpec`：

- 容器类型（Vec, Option 等）→ `type: "array"`
- 基本类型 → 映射为 JSON Schema 类型（`map_primitive_to_json_type`）
- 非基本类型且有子字段 → `type: "object"`
- 非基本类型且无子字段 → `type: "object"`

**`map_primitive_to_json_type()`** 映射表：

| Rust 类型 | JSON Schema 类型 |
|-----------|-----------------|
| i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize | integer |
| f32, f64 | number |
| bool | boolean |
| char, String, str | string |
| () | null |

**`trim_description()`**: 自动去除描述末尾的 `。` 和 `.`。

---

## 六、输出数据模型

**文件**: [models/doc_types.rs](file:///media/yqs/工作/rustspace/cmx/cmx-container/sdk/cmx-cli/src/models/doc_types.rs)

最终输出的 JSON 结构：

```json
{
  "plugin": {
    "name": "cmx-wasmdemo",
    "version": "0.1.0",
    "description": "...",
    "generated_at": "2026-05-28T..."
  },
  "functions": [
    {
      "name": "route_check",
      "type": "branch_fn",
      "summary": "路由判断函数",
      "description": "根据输入的 route 字段决定返回哪个分支标识",
      "input": {
        "encoding": "msgpack",
        "type": "FunctionInput",
        "fields": [
          {
            "name": "input",
            "type": "object",
            "required": true,
            "description": "函数输入，包含 RouteInput 格式的路由参数",
            "properties": [
              {
                "name": "route",
                "type": "string",
                "required": true,
                "description": "路由标识，取值为 \"1\"、\"2\"、\"3\" 或 \"4\""
              }
            ]
          }
        ]
      },
      "output": {
        "encoding": "msgpack",
        "type": "FunctionOutput",
        "fields": []
      },
      "examples": [],
      "errors": [],
      "notes": [],
      "panics": [],
      "location": {
        "file": "./crates/libs/cmx-wasmdemo/src/extism_layer.rs",
        "line": 211
      }
    }
  ]
}
```

**关键字段说明**:

| 字段路径 | 说明 |
|----------|------|
| `functions[].type` | `func`（普通函数）或 `branch_fn`（分支函数） |
| `functions[].input.fields[].properties` | 对象类型的子字段（嵌套） |
| `functions[].input.fields[].required` | 是否必填（来自表格第3列） |
| `functions[].input.encoding` | 编码方式（从函数签名提取） |
| `functions[].location` | 源码位置（文件路径 + 行号） |

---

## 七、完整数据流图

```
Rust 源码 (.rs)
    │
    ├── syn::parse_file()
    │   │
    │   ├── Item::Fn + #[plugin_fn]     ──→  ParsedFunction
    │   │   ├── name: "route_check"
    │   │   ├── doc_comments: ["路由判断函数", ...]
    │   │   ├── doc_type: "branch_fn"
    │   │   ├── input_type: "FunctionInput"
    │   │   ├── input_encoding: "msgpack"
    │   │   └── line: 211
    │   │
    │   └── Item::Struct                  ──→  StructDefinition
    │       ├── name: "RouteInput"
    │       └── fields: [{ name: "route", type_name: "String", description: "路由标识" }]
    │
    ├── parse_doc_comments(doc_comments)  ──→  ParsedDoc
    │   ├── summary: "路由判断函数"
    │   ├── description: "根据输入的 route 字段..."
    │   └── input_fields: [FieldInfo {
    │         name: "input",
    │         type_name: "object",
    │         sub_fields: [FieldInfo { name: "route", type_name: "string" }]
    │       }]
    │
    └── TypeRegistry.register_all(structs)
            └── structs: { "RouteInput": StructDefinition { ... } }

                         │
                         ▼

            generate_ast_document(result, pretty, expand_depth)
                         │
                         ▼

            resolve_field_to_spec(field, registry, max_depth)
                │
                ├── field 有 sub_fields?  ──是──→  table_field_to_spec()
                │                                     直接使用表格类型和描述
                │
                ├── type_name != "unknown"?  ──是──→  registry.resolve_type()
                │                                     递归展开结构体
                │
                └── type_name == "unknown"?  ──是──→  extract_type_from_description()
                                                        从反引号中提取类型名
                                                         │
                                                         ▼
                                                    registry.resolve_type()
                                                         递归展开

                         │
                         ▼

                   PluginDocument (JSON)
```

---

## 八、注释规范与生成结果的映射关系

### 8.1 摘要 → `summary`

```rust
/// 路由判断函数
```
→ `"summary": "路由判断函数"`

### 8.2 详细描述 → `description`

```rust
/// 根据输入的 route 字段决定返回哪个分支标识。
```
→ `"description": "根据输入的 route 字段决定返回哪个分支标识"`

### 8.3 列表行 + 表格 → `input.fields[].properties`

```rust
/// * `input` - 函数输入，包含 `RouteInput` 格式的路由参数。
///
/// | 字段 | 类型 | 必填 | 说明 |
/// |------|------|------|------|
/// | `route` | string | 是 | 路由标识 |
```

→

```json
{
  "name": "input",
  "type": "object",
  "required": true,
  "description": "函数输入，包含 RouteInput 格式的路由参数",
  "properties": [
    {
      "name": "route",
      "type": "string",
      "required": true,
      "description": "路由标识"
    }
  ]
}
```

### 8.4 无表格的列表行 → TypeRegistry 展开

```rust
/// * `input` - 函数输入，包含 `InsertData` 格式的插入数据。
```

当 `InsertData` 在 TypeRegistry 中注册时：

```rust
pub struct InsertData {
    pub table: String,
    pub name: String,
    pub value: i32,
}
```

→

```json
{
  "name": "input",
  "type": "object",
  "required": true,
  "description": "函数输入，包含 InsertData 格式的插入数据",
  "properties": [
    { "name": "table", "type": "string", "required": true, "description": "表名" },
    { "name": "name", "type": "string", "required": true, "description": "名称字段值" },
    { "name": "value", "type": "integer", "required": true, "description": "数值字段值" }
  ]
}
```

### 8.5 `#[doc_type]` → `type`

```rust
#[doc_type = "branch_fn"]
#[plugin_fn]
```

→ `"type": "branch_fn"`

---

## 九、常见问题与排查

### 9.1 字段类型为 "object" 而非预期类型

**原因**: 表格中的类型列使用了 Rust 类型名而非 JSON Schema 类型名。

```rust
// ❌ 错误
/// | `count` | i32 | 是 | 数量 |

// ✅ 正确
/// | `count` | integer | 是 | 数量 |
```

### 9.2 表格子字段描述被覆盖

**原因**: 旧版本中表格子字段也经过 TypeRegistry 解析，导致结构体字段注释覆盖表格描述。当前版本已修复，`table_field_to_spec()` 直接使用表格中的信息。

### 9.3 表格被跳过未解析

**原因**: 列表行和表格之间有多余内容（非空行），导致解析器无法将表格关联到列表行。确保格式为：

```rust
/// * `input` - 描述。
///
/// | 字段 | 类型 | 必填 | 说明 |
/// |------|------|------|------|
```

列表行和表格之间**只能有空行**，不能有其他文字。

### 9.4 类型未展开（sub_fields 为空）

**原因**: 结构体未在同一源文件中定义，或结构体名与注释中引用的名称不一致。检查：
1. 结构体是否在同一扫描目录下
2. 注释中的类型名是否与 `struct` 名称完全一致
3. `--expand-depth` 参数是否足够大（默认 3）

---

## 十、扩展点

### 10.1 添加新的编码方式

在 [parser/ast_parser.rs](file:///media/yqs/工作/rustspace/cmx/cmx-container/sdk/cmx-cli/src/parser/ast_parser.rs) 的 `extract_encoding()` 函数中添加新的模式匹配。

### 10.2 支持新的章节类型

在 [parser/doc_parser.rs](file:///media/yqs/工作/rustspace/cmx/cmx-container/sdk/cmx-cli/src/parser/doc_parser.rs) 的 `normalize_section_name()` 中添加新的映射，并在 `parse_doc_comments()` 中添加对应的解析逻辑。

### 10.3 自定义类型映射

在 [generator/ast_json_gen.rs](file:///media/yqs/工作/rustspace/cmx/cmx-container/sdk/cmx-cli/src/generator/ast_json_gen.rs) 的 `map_primitive_to_json_type()` 和 `table_field_to_spec()` 中修改类型映射规则。

---

## 十一、注释、表格与结构体的优先级规则

cmx-cli 将文档注释解析和结构体定义解析结合使用，遵循严格的优先级：

```
优先级从高到低：

  ① 表格子字段（最高）  ←  注释中 Markdown 表格显式声明的字段
  ② TypeRegistry 展开    ←  描述中反引号引用的已注册类型
  ③ 默认值（最低）       ←  都没有时 fallback 为 serde_json::Value
```

### 信息来源分工

```
┌──────────────────────────────────────────────────────┐
│               各来源提供的信息                         │
│                                                      │
│  注释提供:                结构体提供:                  │
│  ├── summary (摘要)       ├── 字段名                  │
│  ├── description (描述)   ├── 字段类型 (Rust → JSON)  │
│  ├── 函数级参数名          ├── 字段描述 (/// 注释)      │
│  ├── 表格子字段 (如有)     └── 嵌套结构递归展开         │
│  └── required (必填标记)                               │
└──────────────────────────────────────────────────────┘
```

### 情况1：有表格注释（表格优先，结构体不介入）

当注释中同时有列表行和 Markdown 表格时，表格中的字段定义具有最高优先级。

**注释写法**：

```rust
/// * `input` - 函数输入，包含 `InsertData` 格式的数据。
///
/// | 字段 | 类型 | 必填 | 说明 |
/// |------|------|------|------|
/// | `table` | string | 是 | 表名 |
/// | `value` | integer | 否 | 数值 |
```

**处理逻辑**：`resolve_field_to_spec()` 检测到 `sub_fields` 不为空 → 调用 `table_field_to_spec()` → 直接使用表格中的类型和描述，**TypeRegistry 完全不介入**。

**生成结果**：

```json
{
  "name": "input",
  "type": "object",
  "properties": [
    { "name": "table", "type": "string", "required": true, "description": "表名" },
    { "name": "value", "type": "integer", "required": false, "description": "数值" }
  ]
}
```

> 表格说 `string` 就是 `string`，说"表名"就是"表名"，不会被结构体定义覆盖。

### 情况2：无表格，注释引用了已注册类型（注释 + 结构体结合）

当注释中没有表格，但描述中用反引号引用了一个在 TypeRegistry 中已注册的类型名。

**注释写法**：

```rust
/// * `input` - 函数输入，包含 `InsertData` 格式的数据。
```

**处理逻辑**：
1. `parse_field_line()` 解析列表行 → `FieldInfo { type_name: "unknown" }`
2. `resolve_field_to_spec()` 发现 type_name == "unknown"
3. `extract_type_from_description()` 扫描反引号，提取到 `InsertData`
4. `registry.resolve_type("InsertData")` 在 TypeRegistry 中查找
5. 递归展开 `InsertData` 的所有字段

**结构体定义**（同一扫描目录下）：

```rust
pub struct InsertData {
    /// 表名
    pub table: String,
    /// 名称字段值
    pub name: String,
    /// 数值字段值
    pub value: i32,
}
```

**生成结果**：

```json
{
  "name": "input",
  "type": "object",
  "description": "函数输入，包含 InsertData 格式的数据",
  "properties": [
    { "name": "table", "type": "string", "required": true, "description": "表名" },
    { "name": "name", "type": "string", "required": true, "description": "名称字段值" },
    { "name": "value", "type": "integer", "required": true, "description": "数值字段值" }
  ]
}
```

> 字段名、类型、描述全部来自结构体定义，但函数级的 description（"函数输入，包含 InsertData 格式的数据"）保留自注释。

### 情况3：无表格，注释也没引用类型（纯注释 fallback）

当注释中没有表格，描述中也没有反引号类型名。

**注释写法**：

```rust
/// * `input` - 函数输入，输入为动态数据，来源于上一步骤的输出。
```

**处理逻辑**：
1. `parse_field_line()` → `FieldInfo { type_name: "unknown" }`
2. `extract_type_from_description()` 未找到反引号类型名
3. fallback 为 `serde_json::Value`

**生成结果**：

```json
{
  "name": "input",
  "type": "object",
  "description": "函数输入，输入为动态数据，来源于上一步骤的输出"
}
```

> 无 properties 展开，前端看到的是一个通用的 object。

### 情况4：参数本身就是基本类型（`string` / `integer` / `boolean` 等）

当参数不是复杂对象，而是直接的字符串、数字等基本类型时，在描述中用反引号标注 JSON Schema 类型名。

**注释写法**：

```rust
/// * `input` - `string` 待统计的字符串。
```

或：

```rust
/// * `count` - `integer` 数量参数。
```

**处理逻辑**：
1. `extract_type_from_description()` 识别到反引号中的 `string`（JSON Schema 基本类型关键字）
2. `resolve_field_to_spec()` 检测到类型是 JSON Schema 基本类型，直接构建 FieldSpec，不走 TypeRegistry
3. 描述中 `` `string` `` 前缀被自动清理

**生成结果**：

```json
{
  "name": "input",
  "type": "string",
  "required": true,
  "description": "待统计的字符串"
}
```

**支持的 JSON Schema 类型关键字**：

| 关键字 | 含义 |
|--------|------|
| `string` | 字符串 |
| `integer` | 整数 |
| `number` | 浮点数 |
| `boolean` | 布尔值 |
| `array` | 数组 |
| `object` | 对象 |
