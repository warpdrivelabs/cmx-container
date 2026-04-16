# WASM 插件调用安全防护机制设计

## 1. 问题背景

在 WASM 插件系统中，插件可以通过宿主函数 `call_service` 调用另一个插件的 WASM 函数。
如果插件编写不当（如循环调用或无限递归），会导致：

- **无限递归**：调用栈无限增长，最终耗尽资源
- **长时间阻塞**：恶意或错误的 WASM 代码长时间不返回，阻塞整个服务

**注意**：由于采用多实例池架构（每个插件有多个 Plugin 实例），不会出现因同一插件 Mutex 不可重入导致的死锁问题。

**递归调用示例**：
```
A.a → B.b → A.a  (循环)
A.a → A.a       (自我递归)
```

## 2. 技术选型分析

### 方案对比

| 方案 | 优点 | 缺点 | 选用 |
|------|------|------|------|
| **Extism 原生超时** | 零侵入，由运行时层面中断 | 只能中断单次 `plugin.call()` | ✅ 第1层 |
| **调用深度限制** | 简单高效，O(1) 检测 | 无法检测同层循环 | ✅ 第2层 |
| **循环检测（调用链追踪）** | 精确检测 A.a → B.b → A.a 循环 | 需维护调用栈，有少量开销 | ✅ 第3层 |
| **CancelHandle 外部取消** | 可从外部线程强制取消 | 实现复杂，需额外管理线程 | ❌ 备选 |
| **Fuel 计量** | 精确控制计算资源 | Extism Rust SDK 暴露不完整 | ❌ 备选 |
| **独立线程 + 超时** | 完全隔离 | 线程开销大，上下文切换慢 | ❌ 不选 |

### 最终方案：三层防护

```
请求进入
    │
    ▼
┌──────────────────────────────────┐
│ 第1层: 调用深度限制                │
│   检查 thread_local 深度计数器     │
│   超过 max_depth → 立即拒绝        │
└──────────────┬───────────────────┘
               │ 通过
               ▼
┌──────────────────────────────────┐
│ 第2层: 循环检测                    │
│   检查调用链中是否已存在            │
│   (plugin_id, function_name)      │
│   存在 → 拒绝（检测到循环）         │
└──────────────┬───────────────────┘
               │ 通过
               ▼
┌──────────────────────────────────┐
│ 第3层: Extism 原生超时             │
│   Manifest::with_timeout(30s)     │
│   超时 → 运行时自动中断 WASM       │
└──────────────┬───────────────────┘
               │ 执行
               ▼
         plugin.call()
```

## 3. 接口设计

### 3.1 InvokeOptions — 调用选项

```rust
pub struct InvokeOptions {
    /// 超时时间，默认 30 秒
    pub timeout: Duration,
    /// 最大调用深度，默认 8 层
    pub max_depth: u32,
}
```

### 3.2 InvokeContext — 调用上下文（线程局部）

```rust
// 线程局部变量
// 调用链使用 "plugin_id/function_name" 作为 key，用于检测函数级别的循环调用
// 例如 A.a → B.b → A.a 这种递归调用
thread_local! {
    static CALL_DEPTH: RefCell<u32>;           // 调用深度计数
    static CALL_CHAIN: RefCell<HashSet<String>>; // 调用链追踪
}

impl InvokeContext {
    /// 进入调用（RAII 模式），返回 InvokeGuard
    fn enter(plugin_id, function_name, max_depth) -> Result<InvokeGuard, InvokeGuardError>;
    /// 获取当前深度
    fn current_depth() -> u32;
    /// 检测循环（基于 plugin_id/function_name 组合）
    fn is_cycle(plugin_id: &str, function_name: &str) -> bool;
}
```

### 3.3 InvokeGuard — RAII 守卫

```rust
pub struct InvokeGuard {
    plugin_id: String,
    function_name: String,
}

impl InvokeGuard {
    pub fn depth(&self) -> u32;
    pub fn plugin_id(&self) -> &str;
    pub fn function_name(&self) -> &str;
}

impl Drop for InvokeGuard {
    fn drop(&mut self) {
        // 自动递减深度
        CALL_DEPTH.with(|d| *d.borrow_mut() -= 1);
        // 自动从调用链中移除 plugin_id/function_name
        let call_key = format!("{}/{}", self.plugin_id, self.function_name);
        CALL_CHAIN.with(|c| c.borrow_mut().remove(&call_key));
    }
}
```

### 3.4 RuntimeInvoker trait

```rust
#[async_trait]
pub trait RuntimeInvoker: Send + Sync {
    /// 带选项的调用
    async fn invoke_with_options(
        &self,
        plugin_id: &str,
        function_name: &str,
        input: &[u8],
        caller_data: &CallerData,
        options: &InvokeOptions,
    ) -> Result<WasmInvokeResult, TraitError>;

    /// 加载 WASM 模块
    async fn load_module(&self, plugin_id: &str, wasm_path: &Path) -> Result<(), TraitError>;

    /// 卸载 WASM 模块
    async fn unload_module(&self, plugin_id: &str) -> Result<(), TraitError>;

    /// 检查模块是否已加载
    async fn is_loaded(&self, plugin_id: &str) -> bool;
}
```

## 4. 实现细节

### 4.1 engine.rs 调用流程

```rust
fn invoke_plugin_sync(pool, plugin_id, function_name, input, options) {
    let start = Instant::now();

    // 第1层 + 第2层: 深度限制 + 循环检测
    let _guard = InvokeContext::enter(plugin_id, function_name, options.max_depth)?;

    // 使用 Pool::with_plugin 自动获取和归还实例
    let result = pool
        .with_plugin(options.timeout, |plugin| {
            plugin.call::<&[u8], Vec<u8>>(function_name, input)
        });
}
```

### 4.2 超时配置（load_module 时）

```rust
async fn load_module(&self, plugin_id: &str, wasm_path: &Path) -> Result<(), TraitError> {
    // 双重检查锁定（DCLP）模式
    {
        let pools = self.plugin_pools.read().unwrap();
        if pools.contains_key(plugin_id) {
            return Ok(());  // 快速路径：已加载则直接返回
        }
    }

    // 读取并编译 WASM（在锁外执行，避免长时间持锁）
    let wasm_bytes = std::fs::read(wasm_path)?;

    let manifest = Manifest::new([wasm])
        .with_timeout(self.config.timeout)   // Extism 原生超时
        .with_memory_max(self.config.memory_max);

    let pool = PoolBuilder::new()
        .with_max_instances(self.config.pool_max_instances)
        .build(factory);

    // 第二次检查 + 插入：使用写锁进行原子操作
    {
        let mut pools = self.plugin_pools.write().unwrap();
        if pools.contains_key(plugin_id) {  // 防止竞态条件下重复加载
            return Ok(());
        }
        pools.insert(plugin_id.to_string(), pool);
    }
}
```

### 4.3 日志记录

```
WARN  调用深度超限: 当前深度 8 >= 最大深度 8，插件 plugin-b 函数 process
WARN  检测到循环调用: 插件 plugin-a 函数 handle_request
ERROR 插件 plugin-a WASM 调用超时: function=process, timeout=30s, depth=3
INFO  插件调用完成: plugin=plugin-a, function=handle_request, elapsed=1234us, depth=2
```

## 5. 线程安全与并发模型

### 5.1 spawn_blocking 线程迁移

调用链使用 `tokio::task::spawn_blocking` 将同步阻塞的 `plugin.call()` 迁移到专用阻塞线程池执行：

```
tokio worker
  → spawn_blocking {  (任务迁移到阻塞线程池，worker 被释放)
      pool.with_plugin { plugin.call() }
        → 宿主函数回调
          → Handle::current().block_on(async { ... })
            → tokio worker 已空闲 → 正常驱动 future
    }
  → .await JoinHandle (获取结果)
```

### 5.2 线程安全分析

| 组件 | 线程安全方式 | 说明 |
|------|-------------|------|
| `CALL_DEPTH` | `thread_local!` + `RefCell` | 每个线程独立，无竞争 |
| `CALL_CHAIN` | `thread_local!` + `RefCell` | 每个线程独立，无竞争 |
| `InvokeGuard` | RAII Drop | 即使 panic 也能恢复 |
| `Pool` (Extism) | 内部 Condvar + 多实例 | 实例池等待机制，无死锁 |
| `plugin_pools` | `Arc<RwLock<HashMap>>` | invoke 只用读锁，load_module 用写锁 |

### 5.3 循环检测原理

使用 `plugin_id/function_name` 组合作为调用链的 key，检测函数级别的递归调用：

```
调用链追踪: HashSet<String>
  - "A/a"      (A.a 函数)
  - "B/b"      (B.b 函数)
  - "A/a"      (再次调用 A.a → 检测到循环!)
```

例如 `A.a → B.b → A.a` 这种调用：
1. A.a 进入 → 调用链添加 "A/a"
2. B.b 进入 → 调用链添加 "B/b"
3. A.a 再次进入 → 调用链已存在 "A/a" → 检测到循环，拒绝

## 6. 性能优化要点

### 6.1 高并发架构

```
ExtismEngine
  └── plugin_pools: RwLock<HashMap<String, Pool>>
        │
        └── Pool (每个 plugin_id 一个)
              ├── CompiledPlugin (预编译 WASM，避免重复编译)
              ├── 工厂函数 (从 CompiledPlugin 快速创建实例)
              ├── 多实例池 (避免单实例死锁，支持并发)
              └── 内置 Condvar 等待机制
```

### 6.2 性能影响

| 操作 | 开销 | 频率 |
|------|------|------|
| 深度计数器增减 | ~10ns（thread_local） | 每次 invoke |
| HashSet insert/remove | ~50ns（String key） | 每次 invoke |
| RAII Guard Drop | ~20ns | 每次 invoke 退出 |
| **总额外开销** | **~80ns** | **每次 invoke** |

对比 `plugin.call()` 通常需要 ms 级执行时间，80ns 的额外开销可以忽略不计。

### 6.3 优化措施总结

| 优化点 | 实现方式 | 效果 |
|--------|----------|------|
| **CompiledPlugin** | 预编译 WASM 模块 | 避免每次创建实例时重新编译 |
| **多实例 Pool** | 每个插件多个实例 | 支持并发调用，避免单实例死锁 |
| **Pool 实例池** | 内置 Condvar 等待 | 避免轮询浪费 CPU |
| **Pool::with_plugin** | RAII 模式 | 自动获取和归还实例 |
| **DCLP 模式** | 双重检查锁定 | 减少锁竞争，只在首次加载持写锁 |
| **读锁优先** | `invoke` 只用读锁 | 允许多并发读，提高吞吐 |
| **spawn_blocking** | 线程迁移 | 释放 tokio worker，避免阻塞 |
