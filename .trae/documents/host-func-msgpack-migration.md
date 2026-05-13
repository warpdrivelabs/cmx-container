# WASM 宿主函数调用迁移：JSON String → MsgPack Vec<u8>

## 背景

当前 WASM 插件调用宿主函数时，参数和返回值使用 **JSON 字符串 (String)** 传递：
- WASM 侧：`serde_json::to_string()` → `extern "ExtismHost" fn(request: String) -> String` → 宿主
- 宿主侧：`memory_get_val::<String>()` → `provider.call(name, input: String)` → 业务逻辑

而 Host 调用 WASM（入口方向）已经使用 **MsgPack**（`rmp_serde::to_vec` / `Msgpack<T>`）。
需要统一内层通信也使用 MsgPack，实现 **结构体直传**，消除 JSON 字符串中间步骤。

## 技术可行性

### extism-pdk 对 Vec<u8> 的支持

经源码分析确认：

1. **`ToBytes` trait** 已为 `Vec<u8>` 提供实现（`extism-convert-1.20.0/src/to_bytes.rs:71-76`）
2. **`FromBytesOwned` trait** 已为 `Vec<u8>` 提供实现（`extism-convert-1.20.0/src/from_bytes.rs:96-99`）
3. **`host_fn` 宏** 内部通过 `ToMemory` → `ToBytes` 编码，`FromBytes` 解码，**原生支持 `Vec<u8>` 类型参数**

因此可以直接在 `extern "ExtismHost"` 块中将 `String` 改为 `Vec<u8>`。

### 数据流变化

```
修改前（JSON String）:
  WASM: DbRequest → serde_json::to_string() → String → Extism FFI → String → serde_json::from_str() → DbRequest → 业务
  宿主: DbResponse → serde_json::to_string() → String → Extism FFI → String → serde_json::from_str() → DbResponse

修改后（MsgPack Vec<u8>）:
  WASM: DbRequest → rmp_serde::to_vec() → Vec<u8> → Extism FFI → Vec<u8> → rmp_serde::from_slice() → DbRequest → 业务
  宿主: DbResponse → rmp_serde::to_vec() → Vec<u8> → Extism FFI → Vec<u8> → rmp_serde::from_slice() → DbResponse
```

## 修改范围

### 层级1: cmx-traits — Trait 签名变更

**文件**: `crates/libs/cmx-traits/src/host_func.rs`

- `HostFunctionProvider::call()` 签名：`fn call(&self, name: &str, input: String) -> Result<String, HostFuncError>` → `fn call(&self, name: &str, input: Vec<u8>) -> Result<Vec<u8>, HostFuncError>`
- `HostFunctionDef::json_fn()` 重命名为 `HostFunctionDef::msgpack_fn()`（语义更准确）
- 更新文档注释

### 层级2: cmx-runtime — 桥接层适配

**文件**: `crates/libs/cmx-runtime/src/host_function.rs`

- `host_function_wrapper()` 中：
  - `let input: String = plugin.memory_get_val(&inputs[0])` → `let input: Vec<u8> = plugin.memory_get_val(&inputs[0])`
  - `plugin.memory_set_val(&mut outputs[0], output_str)` → `plugin.memory_set_val(&mut outputs[0], output_bytes)`
- 错误兜底改为 MsgPack 编码的错误响应

### 层级3: cmx-plugin-sdk — WASM 侧改造

**文件**: `crates/libs/cmx-plugin-sdk/src/host_calls.rs`

- `extern "ExtismHost"` 声明：
  - 日志函数：保持 `String`（纯文本消息无需 MsgPack）
  - 数据函数：`fn db_query(request: String) -> String` → `fn db_query(request: Vec<u8>) -> Vec<u8>`
  - 缓存函数：同上
  - 插件调用函数：同上
- `HostCaller` 方法：
  - `serde_json::to_string()` → `rmp_serde::to_vec()`
  - `serde_json::from_str()` → `rmp_serde::from_slice()`

**文件**: `crates/libs/cmx-plugin-sdk/Cargo.toml`

- 添加 `rmp-serde = { workspace = true }` 依赖

### 层级4: 宿主函数提供者实现

#### cmx-database (`crates/libs/cmx-infra/cmx-database/src/host_functions.rs`)

- `do_query()` / `do_execute()`：
  - `serde_json::from_str(&input)` → `rmp_serde::from_slice(&input)`
  - `serde_json::to_string(&response)` → `rmp_serde::to_vec(&response)`
- `HostFunctionProvider::call()` 签名适配
- `functions()` 中 `json_fn()` → `msgpack_fn()`

**Cargo.toml**: 添加 `rmp-serde = { workspace = true }`

#### cmx-buffer (`crates/libs/cmx-infra/cmx-buffer/src/host_functions.rs`)

- `do_cache_get()` / `do_cache_set()` / `do_cache_delete()`：
  - `serde_json::from_str(&input)` → `rmp_serde::from_slice(&input)`
  - `serde_json::to_string(&response)` → `rmp_serde::to_vec(&response)`
- 辅助方法 `ok_response()` / `err_response()` 返回类型 `String` → `Vec<u8>`
- `HostFunctionProvider::call()` 签名适配

**Cargo.toml**: 添加 `rmp-serde = { workspace = true }`

#### cmx-plugin (`crates/libs/cmx-plugin/src/host_functions.rs`)

- `do_call_service()` / `do_get_info()`：
  - `serde_json::from_str(&input)` → `rmp_serde::from_slice(&input)`
  - `serde_json::to_string(&response)` → `rmp_serde::to_vec(&response)`
- 辅助方法 `ok_response()` / `err_response()` 返回类型 `String` → `Vec<u8>`

**Cargo.toml**: 添加 `rmp-serde = { workspace = true }`

#### cmx-utils (`crates/libs/cmx-utils/src/host_functions.rs`)

- `LoggingHostFunctions::call()`：
  - `input: String` → `input: Vec<u8>`
  - 将 `Vec<u8>` 转为 `String` 用于日志输出：`String::from_utf8(input).unwrap_or_default()`
- 日志函数本身语义不变，只是接口签名适配

### 修改顺序（依赖方向）

```
1. cmx-traits (接口层 — 最底层)
   ↓
2. cmx-runtime (桥接层 — 依赖 cmx-traits)
   ↓
3. cmx-plugin-sdk (WASM 侧 — 依赖 extism-pdk)
   ↓
4. cmx-database, cmx-buffer, cmx-plugin, cmx-utils (宿主实现 — 依赖 cmx-traits)
   ↓
5. cargo check 编译验证
```

## 优势

1. **性能提升**: MsgPack 是二进制格式，比 JSON 字符串更紧凑、序列化/反序列化更快
2. **统一编码**: 与 Host→WASM 方向的 MsgPack 编码保持一致
3. **类型安全**: `Vec<u8>` 直接承载序列化后的结构体，消除字符串中间态
4. **向前兼容**: 改动仅涉及通信层，不影响业务逻辑
