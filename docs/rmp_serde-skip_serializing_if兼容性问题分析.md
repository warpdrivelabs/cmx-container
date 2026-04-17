# rmp_serde 与 `skip_serializing_if` 兼容性问题分析

## 问题描述

在使用 `rmp_serde` 对 `SVRContext`、`FunctionInput`、`FunctionOutput` 等 cmx-core 结构体进行 MessagePack 序列化/反序列化时，反序列化失败，报错：

```
Syntax("input contains invalid characters")
```

## 涉及的结构体

### SVRContext（根本问题所在）

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SVRContext {
    pub initial_input: serde_json::Value,       // 字段0
    pub headers: HashMap<String, String>,        // 字段1
    #[serde(default)]
    pub step_outputs: HashMap<String, serde_json::Value>, // 字段2
    #[serde(skip_serializing_if = "Option::is_none")]     // ← 问题根源
    pub txn_id: Option<String>,                  // 字段3
    pub time_in: DateTime<Utc>,                  // 字段4
    pub request_id: String,                      // 字段5
}
```

### FunctionInput（嵌套了 SVRContext）

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionInput {
    pub input: serde_json::Value,
    pub context: SVRContext,
    #[serde(default)]
    pub binary_data: HashMap<String, Vec<u8>>,
}
```

### FunctionOutput

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionOutput {
    pub result: serde_json::Value,
    #[serde(default)]
    pub binary_data: HashMap<String, Vec<u8>>,
}
```

## 根本原因

### 1. rmp_serde 默认使用数组编码（无字段名）

`rmp_serde::to_vec()` 默认将结构体序列化为 **MessagePack 数组**（fixarray），字段按位置排列，**不包含字段名**。

例如，`SVRContext` 有 6 个字段，正常情况下序列化为 `fixarray(6)`（首字节 `0x96` = 150）。

### 2. `skip_serializing_if` 导致数组元素数量不匹配

当 `txn_id = None` 时，`#[serde(skip_serializing_if = "Option::is_none")]` 会跳过该字段的序列化，导致实际只序列化了 5 个元素：

```
序列化结果: fixarray(5) = 0x95 = 149
  [0] initial_input = Value
  [1] headers       = HashMap
  [2] step_outputs  = HashMap
  [3] time_in       = "2024-01-15T10:30:00Z"   ← 本来是字段4
  [4] request_id    = "req-001"                  ← 本来是字段5
```

### 3. 反序列化按位置读取，导致字段错位

反序列化时，Deserializer **不知道字段被跳过了**，仍然按 6 个字段依次读取：

```
期望读取 6 个字段:
  [0] initial_input ← 读到 initial_input        ✓
  [1] headers       ← 读到 headers               ✓
  [2] step_outputs  ← 读到 step_outputs          ✓
  [3] txn_id        ← 读到 "2024-01-15T10:30:00Z" ✓ (Option<String> 可以接受字符串)
  [4] time_in       ← 读到 "req-001"              ✗ (不是合法的 DateTime 格式！)
  [5] request_id    ← 没有数据可读                ✗

报错: "input contains invalid characters"
```

### 验证数据

| 场景 | 数组长度 | 首字节 | 反序列化结果 |
|------|---------|--------|-------------|
| `txn_id = None`（skip_serializing_if 生效） | 5 | `0x95` (149) | ❌ 失败 |
| `txn_id = Some("txn-123")`（不跳过） | 6 | `0x96` (150) | ✅ 成功 |
| 使用 `with_struct_map` 编码 | 5（Map） | `0x85` (133) | ✅ 成功 |

## 之前的诊断误入歧途的原因

之前的诊断代码使用了 `std::panic::catch_unwind`，这导致了 **false positive**：

```rust
// ❌ 错误的诊断方式
let result = std::panic::catch_unwind(|| {
    rmp_serde::from_slice::<SVRContext>(&bytes)
});
result.is_ok()  // 永远是 true！
```

原因：`from_slice` 返回的是 `Result`（不是 panic），`catch_unwind` 只能捕获 panic，对 `Result::Err` 会返回 `Ok(Err(...))`，外层 `is_ok()` 始终为 `true`。

正确的做法是直接使用 `Result`：

```rust
// ✅ 正确的诊断方式
let result: Result<SVRContext, _> = rmp_serde::from_slice(&bytes);
result.is_ok()  // 正确反映成功/失败
```

## 解决方案

### 方案一：使用 `rmp_serde::to_vec_named` 替代 `to_vec`（推荐）

使用 Map 编码（带字段名），反序列化时按字段名匹配而非位置，不受 `skip_serializing_if` 影响。

```rust
// 序列化
let bytes = rmp_serde::to_vec_named(&input)?;

// 反序列化（与 to_vec 相同）
let deserialized: T = rmp_serde::from_slice(&bytes)?;
```

**优点**：不侵入 cmx-core 结构体定义，向后兼容。
**缺点**：序列化体积略大（包含字段名）。

### 方案二：移除 `skip_serializing_if`

在 `SVRContext` 中移除 `#[serde(skip_serializing_if = "Option::is_none")]`，始终序列化 `txn_id` 字段。

```rust
// 修改前
#[serde(skip_serializing_if = "Option::is_none")]
pub txn_id: Option<String>,

// 修改后
pub txn_id: Option<String>,
```

**优点**：最简单，数组编码体积最小。
**缺点**：`None` 值也会序列化，增加少量体积；需要修改 cmx-core 代码。

### 方案三：同时添加 `#[serde(default)]`

```rust
#[serde(skip_serializing_if = "Option::is_none", default)]
pub txn_id: Option<String>,
```

**注意**：此方案**不能**解决数组编码的位置错位问题。`default` 只在 Map 编码中有效，在数组编码中字段仍然按位置读取。

### 方案四：使用自定义序列化

为需要 `skip_serializing_if` 的结构体实现自定义的 `Serialize`/`Deserialize`。

**优点**：完全控制。
**缺点**：代码量大，维护成本高。

## 推荐方案

**推荐方案一**（`to_vec_named`），理由：

1. **不侵入 cmx-core 结构体定义** — 无需修改核心库代码
2. **向后兼容** — 对 JSON 等其他序列化格式无影响
3. **可靠性高** — Map 编码天然支持字段名匹配，不受 `skip_serializing_if` 影响
4. **通用性** — 解决了所有包含 `skip_serializing_if` 的结构体的兼容性问题

### 封装工具函数

建议在 cmx-utils 或 cmx-core 中封装 MessagePack 序列化工具函数：

```rust
/// MessagePack 序列化（使用 Map 编码，兼容 skip_serializing_if）
pub fn msgpack_serialize<T: Serialize>(value: &T) -> Result<Vec<u8>, rmp_serde::encode::Error> {
    rmp_serde::to_vec_named(value)
}

/// MessagePack 反序列化
pub fn msgpack_deserialize<'de, T: Deserialize<'de>>(bytes: &'de [u8]) -> Result<T, rmp_serde::decode::Error> {
    rmp_serde::from_slice(bytes)
}
```

## 测试代码位置

所有诊断测试位于 `crates/tests/cmx-serde-test/src/lib.rs`，包含：

- `test_diagnose_array_encoding` — 验证 rmp_serde 默认使用数组编码
- `test_diagnose_skip_serializing_if_mismatch` — 验证 skip_serializing_if 导致错位
- `test_diagnose_with_txn_id_set` — 验证 txn_id=Some 时正常工作
- `test_diagnose_struct_map_fixes_issue` — 验证 with_struct_map 可修复问题
- `test_diagnose_catch_unwind_false_positive` — 验证 catch_unwind 的 false positive 问题

## 相关文件

| 文件 | 说明 |
|------|------|
| `crates/libs/cmx-core/src/model/service/context.rs` | SVRContext 定义 |
| `crates/libs/cmx-core/src/model/service/wasm_io.rs` | FunctionInput / FunctionOutput 定义 |
| `crates/tests/cmx-serde-test/` | 兼容性测试 crate |
