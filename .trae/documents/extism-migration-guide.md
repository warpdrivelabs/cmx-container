# Extism 迁移指南

## 一、架构差异说明

### 1.1 现有架构（wasmtime）

现有架构使用全局单例模式：

```rust
// 初始化全局引擎
GlobalWasmEngine::initialize(WasmEngineConfig::default());

// 获取引擎引用
let engine = GlobalWasmEngine::get();

// 注册宿主函数提供者
engine.register_provider(Box::new(DatabaseHostFunctions::new(db_manager))).await;

// 获取运行时调用器
let runtime = GlobalWasmEngine::get_as_invoker();
```

### 1.2 新架构（extism）

Extism 采用不同的架构：

```rust
// 创建引擎实例
let engine = ExtismEngine::new(ExtismEngineConfig::default())?;

// 构建宿主函数
let db_functions = DatabaseHostFunctionsBuilder::new().build();
let cache_functions = CacheHostFunctionsBuilder::new().build();
let log_functions = LoggingHostFunctionsBuilder::new().build();

// 在加载插件时注册宿主函数
let manifest = Manifest::new([wasm])
    .with_functions(db_functions)
    .with_functions(cache_functions)
    .with_functions(log_functions);

let plugin = Plugin::new(&manifest, [], true)?;
```

## 二、迁移步骤

### 2.1 修改 web-server 依赖

修改 `crates/web/web-server/Cargo.toml`：

```toml
[dependencies]
# 移除
# cmx-runtime = { path = "../../libs/cmx-runtime" }

# 添加
cmx-extism = { path = "../../libs/cmx-extism" }
```

### 2.2 修改初始化代码

修改 `crates/web/web-server/src/config.rs`：

```rust
/// 初始化 WASM 运行时
pub async fn init_runtime() {
    use cmx_extism::{
        ExtismEngine, ExtismEngineConfig,
        DatabaseHostFunctionsBuilder,
        CacheHostFunctionsBuilder,
        LoggingHostFunctionsBuilder,
    };
    use cmx_database::get_default_db_manager;
    use cmx_buffer::GlobalCacheManager;
    use std::sync::Arc;

    info!("初始化 WASM 运行时...");

    // 创建 Extism 引擎
    let engine = Arc::new(
        ExtismEngine::new(ExtismEngineConfig::default())
            .expect("Extism 引擎初始化失败")
    );

    // 构建宿主函数
    let db_functions = DatabaseHostFunctionsBuilder::new().build();
    let cache_functions = CacheHostFunctionsBuilder::new().build();
    let log_functions = LoggingHostFunctionsBuilder::new().build();

    // 保存到全局状态
    // 注意：需要实现 GlobalExtismEngine 单例模式
    // GlobalExtismEngine::initialize(engine, db_functions, cache_functions, log_functions);

    info!("WASM 运行时初始化完成");
}
```

### 2.3 修改 main.rs

修改 `crates/web/web-server/src/main.rs`：

```rust
// 构建完整的 AppState（注入 trait 实例）
let app_state = CmxAppState::new()
    .with_plugin_query(cmx_plugin::GlobalPluginManager::get_as_plugin_query())
    .with_runtime_invoker(cmx_extism::GlobalExtismEngine::get_as_invoker());
```

## 三、需要实现的功能

### 3.1 GlobalExtismEngine 单例

在 `cmx-extism/src/lib.rs` 中添加：

```rust
use std::sync::{Arc, OnceLock};
use tokio::sync::RwLock;

/// 全局 Extism 引擎
pub struct GlobalExtismEngine {
    engine: Arc<ExtismEngine>,
    functions: Vec<extism::Function>,
}

static GLOBAL_ENGINE: OnceLock<GlobalExtismEngine> = OnceLock::new();

impl GlobalExtismEngine {
    /// 初始化全局引擎
    pub fn initialize(
        engine: Arc<ExtismEngine>,
        functions: Vec<extism::Function>,
    ) -> Result<(), ExtismError> {
        GLOBAL_ENGINE
            .set(GlobalExtismEngine { engine, functions })
            .map_err(|_| ExtismError::InternalError("全局引擎已初始化".to_string()))?;
        Ok(())
    }

    /// 获取全局引擎引用
    pub fn get() -> &'static GlobalExtismEngine {
        GLOBAL_ENGINE
            .get()
            .expect("全局引擎未初始化，请先调用 initialize()")
    }

    /// 获取运行时调用器
    pub fn get_as_invoker() -> Arc<dyn RuntimeInvoker> {
        Arc::new(Self::get().engine.clone())
    }
}
```

### 3.2 修改 ExtismEngine

需要修改 `ExtismEngine` 以支持在加载插件时注册宿主函数：

```rust
pub struct ExtismEngine {
    plugins: Arc<RwLock<HashMap<String, Plugin>>>,
    config: ExtismEngineConfig,
    functions: Vec<extism::Function>,
}

impl ExtismEngine {
    pub fn with_functions(mut self, functions: Vec<extism::Function>) -> Self {
        self.functions.extend(functions);
        self
    }
}
```

## 四、编译 WASM 插件

### 4.1 安装编译目标

```bash
rustup target add wasm32-unknown-unknown
```

### 4.2 编译插件

```bash
cd crates/libs/cmx-wasmdemo
cargo build --release --target wasm32-unknown-unknown
```

### 4.3 输出文件

编译后的 WASM 文件位于：
```
target/wasm32-unknown-unknown/release/cmx_wasmdemo.wasm
```

## 五、测试插件

### 5.1 使用 Extism CLI 测试

```bash
# 安装 Extism CLI
curl -sSf https://extism.org/install | sh

# 测试插件
extism call target/wasm32-unknown-unknown/release/cmx_wasmdemo.wasm count_vowels --input "Hello, World!"
```

### 5.2 使用 Rust 代码测试

```rust
use extism::{Manifest, Plugin, Wasm};

#[tokio::main]
async fn main() {
    let wasm = Wasm::file("target/wasm32-unknown-unknown/release/cmx_wasmdemo.wasm");
    let manifest = Manifest::new([wasm]);
    let mut plugin = Plugin::new(&manifest, [], true).unwrap();
    
    let result = plugin.call::<&str, &str>("count_vowels", "Hello, World!").unwrap();
    println!("{}", result);
}
```

## 六、注意事项

### 6.1 性能考虑

- Extism 基于 wasmtime，性能有保障
- JSON 序列化比 rkyv 慢，但差距在可接受范围内
- 可以考虑使用 MessagePack 提升序列化性能

### 6.2 兼容性

- Extism 插件编译目标为 `wasm32-unknown-unknown`
- 现有的 `wasm32-wasip1` 插件需要重新编译
- 无需向后兼容，可以大胆重构

### 6.3 调试

- 使用 `EXTISM_ENABLE_WASI_OUTPUT=1` 查看 WASI 输出
- 使用 `EXTISM_DEBUG=1` 生成调试信息
- 使用 `EXTISM_PROFILE=perf` 启用性能分析

## 七、后续工作

1. **实现 GlobalExtismEngine 单例模式**
2. **修改 web-server 初始化代码**
3. **测试所有宿主函数**
4. **测试插件加载和调用**
5. **性能测试和优化**
