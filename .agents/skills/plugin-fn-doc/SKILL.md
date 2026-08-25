---
name: plugin-fn-doc
description: 规范 #[plugin_fn] 函数的文档注释，确保 cmx-cli 正确解析生成 api.json。当用户编写或审查 #[plugin_fn] 函数的文档注释、或要求生成 api.json 文档时必用。
---

# cmx-cli 代码注释规范

> 本文档定义了 `#[plugin_fn]` 函数的文档注释规范，确保 `cmx-cli` 能正确解析生成 `api.json`。

---

## 一、注释整体结构

每个 `#[plugin_fn]` 函数的文档注释必须包含以下结构：

```rust
/// 一句话摘要（第一行，不以句号结尾）
///
/// 详细描述（可选，第二段）
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
```

---

## 二、各部分规则

### 2.1 摘要（Summary）

- **必须**是注释的第一行
- 以 `/// ` 开头（注意空格）
- 不以句号（`.` 或 `。`）结尾
- 一行描述函数用途

```rust
/// 路由判断函数
```

### 2.2 详细描述（Description）

- 摘要之后空一行
- 可选，可多行
- 描述函数的详细行为

```rust
/// 路由判断函数
///
/// 根据输入的 route 字段决定返回哪个分支标识。
```

### 2.3 参数说明（# Arguments）

**核心规则**：每个参数用 `* \`name\` - description.` 格式声明，如果该参数是复杂对象，**紧接其下方**用 Markdown 表格声明其子字段。

#### 格式

```
* `参数名` - 参数描述，包含 `TypeName` 格式的数据。
```

#### 表格规范

表格紧跟在对应的参数声明之后，中间用空行分隔。表格列：

| 列序号 | 列名 | 说明 |
|--------|------|------|
| 第1列 | 字段 | 子字段名，用反引号包裹 |
| 第2列 | 类型 | JSON Schema 类型 |
| 第3列 | 必填 | `是` 或 `否` |
| 第4列 | 说明 | 字段描述 |

**支持的类型值**：

| 类型值 | 含义 |
|--------|------|
| `string` | 字符串 |
| `integer` | 整数 |
| `number` | 浮点数 |
| `boolean` | 布尔值 |
| `object` | 对象 |
| `array` | 数组 |

#### 单层字段示例

```rust
/// # Arguments
///
/// * `input` - 函数输入，包含 `InsertData` 格式的插入数据。
///
/// | 字段 | 类型 | 必填 | 说明 |
/// |------|------|------|------|
/// | `table` | string | 是 | 表名 |
/// | `name` | string | 是 | 名称字段值 |
/// | `value` | integer | 是 | 数值字段值 |
```

**生成的 api.json 结构**：

```json
{
  "name": "input",
  "type": "object",
  "required": true,
  "description": "函数输入，包含 `InsertData` 格式的插入数据",
  "properties": [
    { "name": "table", "type": "string", "required": true, "description": "表名" },
    { "name": "name", "type": "string", "required": true, "description": "名称字段值" },
    { "name": "value", "type": "integer", "required": true, "description": "数值字段值" }
  ]
}
```

#### 多层嵌套字段

使用点号 `.` 表示嵌套层级。子字段必须在父字段之后声明：

```rust
/// # Arguments
///
/// * `input` - 订单数据。
///
/// | 字段 | 类型 | 必填 | 说明 |
/// |------|------|------|------|
/// | `order_id` | string | 是 | 订单ID |
/// | `customer.name` | string | 是 | 客户姓名 |
/// | `customer.phone` | string | 否 | 客户电话 |
/// | `items` | array | 是 | 商品列表 |
```

> **注意**：当前 cmx-cli 对 `.` 分隔的嵌套会解析为扁平字段名。如果需要深层嵌套，建议为嵌套结构定义独立的 Rust struct。

#### 独立 struct 自动展开（推荐）

如果嵌套类型在源码中定义了独立的 struct 并有文档注释，cmx-cli 会自动递归展开：

```rust
/// 地址信息
#[derive(Serialize, Deserialize)]
pub struct Address {
    /// 城市
    pub city: String,
    /// 街道
    pub street: String,
}

/// 客户信息
#[derive(Serialize, Deserialize)]
pub struct Customer {
    /// 客户姓名
    pub name: String,
    /// 地址
    pub address: Address,
}
```

在函数注释中引用：

```rust
/// * `input` - 函数输入，包含 `Customer` 格式的客户数据。
```

cmx-cli 会从 `TypeRegistry` 中查找 `Customer`，自动展开为包含 `name` 和 `address.city`/`address.street` 的完整字段结构。

#### 无特定结构的参数

如果参数是透传数据（来自上一步骤的输出），不需要表格：

```rust
/// # Arguments
///
/// * `input` - 函数输入，输入为动态数据，来源于上一步骤的输出。
```

#### 基本类型参数（string / integer / boolean 等）

当参数本身就是一个基本类型（不是复杂对象），在描述中用反引号标注 JSON Schema 类型名：

```rust
/// * `input` - `string` 待统计的字符串。
```

```rust
/// * `count` - `integer` 数量参数。
```

支持的类型关键字：`string`、`integer`、`number`、`boolean`、`array`、`object`

类型关键字会自动从描述中清理，不会出现在最终文档中。

### 2.4 返回值说明（# Returns）

描述函数的返回值：

```rust
/// # Returns
///
/// 返回包含合并结果的 `FunctionOutput`。
```

### 2.5 分支函数（branch_fn）

分支函数需要添加 `#[doc_type = "branch_fn"]` 属性：

```rust
/// 路由判断函数
///
/// 根据输入的 route 字段决定返回哪个分支标识。
///
/// # Arguments
///
/// * `input` - 函数输入，包含 `RouteInput` 格式的路由参数。
///
/// | 字段 | 类型 | 必填 | 说明 |
/// |------|------|------|------|
/// | `route` | string | 是 | 路由标识，取值为 "1"、"2"、"3" 或 "4" |
///
/// # Returns
///
/// 返回分支标识字符串 "1"、"2"、"3" 或 "4"。
#[doc_type = "branch_fn"]
#[plugin_fn]
pub fn route_check(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
```

---

## 三、完整示例

### 示例1：带表格的标准函数

```rust
/// 事务插入函数
///
/// 在事务中执行插入操作，通过上下文获取事务ID确保在同一事务中执行。
///
/// # Arguments
///
/// * `input` - 函数输入，包含 `InsertData` 格式的插入数据。
///
/// | 字段 | 类型 | 必填 | 说明 |
/// |------|------|------|------|
/// | `table` | string | 是 | 表名 |
/// | `name` | string | 是 | 名称字段值 |
/// | `value` | integer | 是 | 数值字段值 |
///
/// # Returns
///
/// 返回包含插入结果的 `FunctionOutput`。
#[plugin_fn]
pub fn tx_insert(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
```

### 示例2：透传参数的函数

```rust
/// 分支1处理函数
///
/// 处理分支1的业务逻辑。
///
/// # Arguments
///
/// * `input` - 函数输入，输入为动态数据，来源于上一步骤的输出。
///
/// # Returns
///
/// 返回包含分支1处理结果的 `FunctionOutput`。
#[plugin_fn]
pub fn branch_1_process(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
```

### 示例3：多参数函数

```rust
/// 多参数示例
///
/// 演示多个参数的注释方式。
///
/// # Arguments
///
/// * `input` - 函数输入，包含查询条件。
///
/// | 字段 | 类型 | 必填 | 说明 |
/// |------|------|------|------|
/// | `query` | string | 是 | 查询条件 |
/// | `page` | integer | 否 | 页码，默认1 |
/// | `size` | integer | 否 | 每页数量，默认20 |
///
/// # Returns
///
/// 返回查询结果。
#[plugin_fn]
pub fn search(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
```

---

## 四、常见错误

### 错误1：表格字段名带 `input.` 前缀

```rust
// ❌ 错误 - 多余的 input. 前缀
/// | `input.route` | string | 是 | 路由标识 |
```

```rust
// ✅ 正确 - 直接写子字段名
/// | `route` | string | 是 | 路由标识 |
```

### 错误2：表格与参数声明之间有其他内容

```rust
// ❌ 错误 - 中间插入了其他说明
/// * `input` - 函数输入。
///
/// 注意：此处有额外说明。
///
/// | 字段 | 类型 | 必填 | 说明 |
```

```rust
// ✅ 正确 - 表格紧跟参数声明
/// * `input` - 函数输入。
///
/// | 字段 | 类型 | 必填 | 说明 |
```

### 错误3：表格缺少分隔行

```rust
// ❌ 错误 - 缺少 |---| 分隔行
/// | 字段 | 类型 | 必填 | 说明 |
/// | `name` | string | 是 | 名称 |
```

```rust
// ✅ 正确 - 必须有分隔行
/// | 字段 | 类型 | 必填 | 说明 |
/// |------|------|------|------|
/// | `name` | string | 是 | 名称 |
```

### 错误4：类型列使用了 Rust 类型而非 JSON 类型

```rust
// ❌ 错误
/// | `count` | i32 | 是 | 数量 |

// ✅ 正确
/// | `count` | integer | 是 | 数量 |
```
