# WASM 服务编排系统优化方案（融合版）

## 一、现状评估

### 1.1 模块架构概览

```
┌─────────────────────────────────────────────────────────────────┐
│                         web-server                               │
│  (应用入口，组装各组件)                                            │
└─────────────────────────────────────────────────────────────────┘
                                │
        ┌───────────────────────┼───────────────────────┐
        ▼                       ▼                       ▼
┌───────────────┐       ┌───────────────┐       ┌───────────────┐
│  cmx-service  │       │  cmx-plugin   │       │  cmx-runtime  │
│  (服务编排)    │◄──────│  (插件管理)    │       │  (WASM运行时) │
└───────────────┘       └───────────────┘       └───────────────┘
        │                       │                       │
        └───────────────────────┼───────────────────────┘
                                ▼
                        ┌───────────────┐
                        │  cmx-traits   │
                        │  (trait 抽象)  │
                        └───────────────┘
                                │
        ┌───────────────────────┼───────────────────────┐
        ▼                       ▼                       ▼
┌───────────────┐       ┌───────────────┐       ┌───────────────┐
│ cmx-database  │       │  cmx-buffer   │       │  cmx-utils    │
│ (数据库操作)   │       │  (缓存操作)    │       │  (日志等)      │
└───────────────┘       └───────────────┘       └───────────────┘
        │                       │                       │
        └───────────────────────┼───────────────────────┘
                                ▼
                        ┌───────────────┐
                        │   cmx-core    │
                        │ (基础数据类型) │
                        │ (rkyv 派生)   │  ← WASM 和 Host 共享
                        └───────────────┘
                                ▲
        ┌───────────────────────┴───────────────────────┐
        │                                               │
┌───────────────┐                               ┌───────────────┐
│ cmx-wasm-core │                               │ cmx-wasmdemo  │
│ (WASM端SDK)   │◄──────────────────────────────│ (WASM 模块)    │
│ - Arena       │                               │ target:       │
│ - 调用封装    │                               │ wasm32-wasip1 │
│ - 函数导出    │                               └───────────────┘
└───────────────┘
```

### 1.2 核心技术栈

**wasmtime + rkyv + Arena 三位一体**

| 技术 | 作用 | 使用场景 |
|------|------|---------|
| **wasmtime** | WASM 运行时 | Host 端加载和执行 WASM 模块 |
| **rkyv** | 零拷贝序列化 | Host ↔ WASM 数据传递 |
| **Arena** | 内存分配器 | WASM 端高效内存管理 |

### 1.3 模块职责划分

| 模块 | 职责 | 编译目标 |
|------|------|---------|
| **cmx-core** | 基础数据类型（rkyv 派生） | native + wasm32-wasip1 |
| **cmx-wasm-core** | WASM 端 SDK（Arena + 调用封装） | wasm32-wasip1 only |
| **cmx-runtime** | Host 端运行时（wasmtime + 调用封装） | native only |
| **cmx-wasmdemo** | WASM 业务模块 | wasm32-wasip1 only |

### 1.4 WASM 编译目标说明

**目标平台：`wasm32-wasip1`（WASI Preview 1）**

WASI Preview 1 提供了系统调用接口，允许 WASM 模块：

* 访问文件系统
* 使用标准输入输出
* 获取环境变量
* 支持异步操作

**与 `wasm32-unknown-unknown` 的区别：**

* `wasm32-wasip1`：支持 WASI，可使用更多 Rust 标准库功能
* `wasm32-unknown-unknown`：无系统接口，需要手动实现很多功能

### 1.5 已实现功能评估

| 功能         | 实现状态  | 说明                      |
| ---------- | ----- | ----------------------- |
| 步骤顺序执行     | ✅ 已实现 | 按编排定义顺序执行步骤             |
| 步骤间数据引用    | ✅ 已实现 | 支持 Reference 类型引用前序步骤输出 |
| 静态输入       | ✅ 已实现 | 支持 Static 类型输入          |
| 合并输入       | ✅ 已实现 | 支持 Merge 类型合并多个来源       |
| 插件激活检查     | ✅ 已实现 | 执行前检查插件是否激活             |
| WASM 模块懒加载 | ✅ 已实现 | 未加载时自动加载                |
| 步骤执行耗时统计   | ✅ 已实现 | 记录每个步骤的执行时间             |

***

## 二、问题清单

### 2.1 严重问题（P0）- 必须立即修复

| 编号 | 问题 | 影响 | 涉及模块 | 代码位置 |
|------|------|------|----------|---------|
| P0-1 | invoke 方法未传递输入数据 | WASM 无法接收输入参数 | cmx-runtime | [engine.rs:188-189](file:///e:/rustspace/cmx/cmx-container/crates/libs/cmx-runtime/src/engine.rs#L188-189) |
| P0-2 | invoke 方法未获取返回值 | 无法获取 WASM 执行结果 | cmx-runtime | [engine.rs:199](file:///e:/rustspace/cmx/cmx-container/crates/libs/cmx-runtime/src/engine.rs#L199) |
| P0-3 | OUTPUT_BUFFER 竞态条件 | 多线程/多实例数据覆盖 | cmx-runtime | [linker_adapter.rs:34-35](file:///e:/rustspace/cmx/cmx-container/crates/libs/cmx-runtime/src/linker_adapter.rs#L34-35) |
| P0-4 | 编排定义无法持久化 | 无法保存和加载编排定义 | cmx-service | [handler.rs:140-145](file:///e:/rustspace/cmx/cmx-container/crates/libs/cmx-service/src/handler.rs#L140-145) |
| P0-5 | Arena 内存安全缺陷 | 可能导致崩溃和数据损坏 | cmx-wasm-core | Arena 缺少版本控制和边界检查 |
| P0-6 | 返回值编码缺陷 | 限制内存大小，错误码冲突 | cmx-wasm-core | i64 编码方案限制地址空间 |
| P0-7 | 内存泄漏风险 | 长期运行会耗尽内存 | cmx-wasm-core | 缺少 RAII 内存管理机制 |

**P0 问题详细分析：**

#### P0-1 & P0-2：invoke 方法问题

当前实现：

```rust
// engine.rs:188-189
let input_ptr = 0;  // ❌ 硬编码为 0
let input_len = 0;  // ❌ 硬编码为 0

// engine.rs:199
output: Vec::new()  // ❌ 返回空值，未从 WASM 获取实际返回值
```

**问题链路：**

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│ CmxService  │────►│RuntimeInvoker│────►│ WasmEngine  │────►│ WasmInstance│
│ .invoke()   │     │ .invoke()    │     │ .invoke()   │     │ .get_func() │
└─────────────┘     └─────────────┘     └─────────────┘     └─────────────┘
       │                   │                   │                   │
       │ 序列化输入         │ _input 被忽略      │ input_ptr=0       │
       │ 为 JSON 字节      │ ❌ 问题点          │ input_len=0       │
       │                   │                   │ ❌ 问题点          │
       ▼                   ▼                   ▼                   ▼
```

#### P0-3：OUTPUT_BUFFER 竞态条件

当前实现：

```rust
// linker_adapter.rs:34-35
thread_local! {
    static OUTPUT_BUFFER: RefCell<Vec<u8>> = RefCell::new(Vec::new());
}
```

**问题：** 线程局部变量在多实例调用时会互相覆盖，导致数据错乱。

### 2.2 重要问题（P1）- 建议尽快修复

| 编号 | 问题 | 影响 | 涉及模块 | 代码位置 |
|------|------|------|----------|---------|
| P1-1 | 条件执行未实现 | 无法动态跳过步骤 | cmx-service | [orchestrator.rs:30-31](file:///e:/rustspace/cmx/cmx-container/crates/libs/cmx-service/src/orchestrator.rs#L30-31) |
| P1-2 | 并行执行未实现 | 无法优化执行效率 | cmx-service | [orchestrator.rs:28-29](file:///e:/rustspace/cmx/cmx-container/crates/libs/cmx-service/src/orchestrator.rs#L28-29) |
| P1-3 | 错误重试未实现 | 可靠性不足 | cmx-service | - |
| P1-4 | 缺少宿主函数请求/响应共享类型 | 类型重复定义 | cmx-core | - |

### 2.3 一般问题（P2）- 可后续优化

| 编号 | 问题 | 影响 | 涉及模块 |
|------|------|------|----------|
| P2-1 | 缺少权限控制 | 安全风险 | cmx-runtime |
| P2-2 | 缺少资源限制 | 可能资源滥用 | cmx-runtime |
| P2-3 | 缺少 WASM 端调用封装 | 开发体验差 | cmx-wasm-core（待新建）|
| P2-4 | 缺少编排版本管理 | 无法演进编排定义 | cmx-service |

***

## 三、技术方案

### 3.1 cmx-core：基础数据类型模块

#### 3.1.1 职责定义

**cmx-core 只包含：**
1. 基础数据类型（rkyv 派生）
2. 枚举定义
3. 常量定义
4. 工具函数（纯函数，无 I/O）

**cmx-core 不包含：**
1. WASM 端调用封装
2. Host 端调用封装
3. Arena 分配器
4. 任何 I/O 相关代码

#### 3.1.2 Cargo.toml 配置

```toml
[package]
name = "cmx-core"
version.workspace = true
edition.workspace = true

[dependencies]
# ============================================
# 序列化框架
# ============================================
# serde: 传统序列化（兼容性）
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# rkyv: 零拷贝序列化（高性能）
rkyv = { version = "0.8", features = ["alloc", "validation"] }

# ============================================
# 基础类型
# ============================================
chrono = { version = "0.4", features = ["serde"] }
smol_str = { version = "0.3", features = ["serde"] }
rust_decimal = "1"
uuid = { version = "1.21", features = ["v4", "serde"] }
base64 = "0.22"

# ============================================
# 错误处理
# ============================================
thiserror = "2"

# ============================================
# 枚举增强
# ============================================
strum = "0.27"
strum_macros = "0.27"

[features]
default = ["std"]
std = []
```

#### 3.1.3 数据类型定义

```rust
// cmx-core/src/wasm_types.rs

use rkyv::{Archive, Serialize, Deserialize};

/// 数据库查询请求
#[derive(Debug, Clone, Archive, Serialize, Deserialize, serde::Serialize, serde::Deserialize)]
#[archive_attr(derive(Debug))]
pub struct DbQueryRequest {
    /// SQL 语句
    pub sql: String,
    /// SQL 参数（JSON 字符串）
    #[serde(default)]
    pub params: Option<String>,
    /// 数据集ID（可选）
    #[serde(default)]
    pub dataset_id: Option<String>,
}

/// 数据库操作响应
#[derive(Debug, Clone, Archive, Serialize, Deserialize, serde::Serialize, serde::Deserialize)]
#[archive_attr(derive(Debug))]
pub struct DbResponse {
    /// 是否成功
    pub success: bool,
    /// 影响行数（写操作返回）
    pub affected_rows: Option<u64>,
    /// 查询结果数据集（查询操作返回，JSON 字符串）
    pub dataset: Option<String>,
    /// 事务ID（事务操作返回）
    pub txn_id: Option<String>,
    /// 错误信息
    pub error: Option<String>,
}

/// 缓存读取请求
#[derive(Debug, Clone, Archive, Serialize, Deserialize, serde::Serialize, serde::Deserialize)]
#[archive_attr(derive(Debug))]
pub struct CacheGetRequest {
    /// 缓存键
    pub key: String,
}

/// 缓存写入请求
#[derive(Debug, Clone, Archive, Serialize, Deserialize, serde::Serialize, serde::Deserialize)]
#[archive_attr(derive(Debug))]
pub struct CacheSetRequest {
    /// 缓存键
    pub key: String,
    /// 缓存值
    pub value: String,
    /// 过期时间（秒）
    #[serde(default)]
    pub ttl_seconds: Option<u64>,
}

/// 缓存操作响应
#[derive(Debug, Clone, Archive, Serialize, Deserialize, serde::Serialize, serde::Deserialize)]
#[archive_attr(derive(Debug))]
pub struct CacheResponse {
    /// 是否成功
    pub success: bool,
    /// 缓存值（读取操作返回）
    pub value: Option<String>,
    /// 是否存在
    pub exists: Option<bool>,
    /// 错误信息
    pub error: Option<String>,
}

/// 插件调用请求
#[derive(Debug, Clone, Archive, Serialize, Deserialize, serde::Serialize, serde::Deserialize)]
#[archive_attr(derive(Debug))]
pub struct PluginCallRequest {
    /// 目标插件ID
    pub target_plugin_id: String,
    /// 目标函数名
    pub function_name: String,
    /// 输入数据（JSON 字符串）
    pub input: String,
}

/// 插件调用响应
#[derive(Debug, Clone, Archive, Serialize, Deserialize, serde::Serialize, serde::Deserialize)]
#[archive_attr(derive(Debug))]
pub struct PluginCallResponse {
    /// 是否成功
    pub success: bool,
    /// 输出数据（JSON 字符串）
    pub output: Option<String>,
    /// 执行耗时（微秒）
    pub elapsed_us: Option<u64>,
    /// 错误信息
    pub error: Option<String>,
}

/// 插件信息响应
#[derive(Debug, Clone, Archive, Serialize, Deserialize, serde::Serialize, serde::Deserialize)]
#[archive_attr(derive(Debug))]
pub struct PluginInfoResponse {
    /// 当前插件ID
    pub plugin_id: String,
    /// 数据库ID
    pub db_id: String,
    /// 当前事务ID
    pub txn_id: Option<String>,
    /// 请求ID
    pub request_id: String,
    /// 租户ID
    pub tenant_id: Option<String>,
}

/// 通用 WASM 函数请求
/// 
/// 用于 Host 调用 WASM 函数时的通用请求包装。
#[derive(Debug, Clone, Archive, Serialize, Deserialize, serde::Serialize, serde::Deserialize)]
#[archive_attr(derive(Debug))]
pub struct WasmFunctionRequest<T> {
    /// 调用上下文
    pub context: WasmContext,
    /// 业务请求数据
    pub data: T,
}

/// 通用 WASM 函数响应
/// 
/// 用于 WASM 函数返回时的通用响应包装。
#[derive(Debug, Clone, Archive, Serialize, Deserialize, serde::Serialize, serde::Deserialize)]
#[archive_attr(derive(Debug))]
pub struct WasmFunctionResponse<T> {
    /// 是否成功
    pub success: bool,
    /// 业务响应数据
    pub data: Option<T>,
    /// 错误信息
    pub error: Option<String>,
}

/// WASM 调用上下文
#[derive(Debug, Clone, Archive, Serialize, Deserialize, serde::Serialize, serde::Deserialize)]
#[archive_attr(derive(Debug))]
pub struct WasmContext {
    /// 请求ID
    pub request_id: String,
    /// 租户ID
    pub tenant_id: Option<String>,
    /// 数据库ID
    pub db_id: String,
    /// 事务ID
    pub txn_id: Option<String>,
    /// 插件ID
    pub plugin_id: String,
}
```

#### 3.1.4 模块结构

```
cmx-core/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   ├── model/
│   │   ├── mod.rs
│   │   ├── cell.rs         # DataValue 定义
│   │   ├── data/           # 数据相关类型
│   │   ├── domain/         # 领域模型
│   │   └── meta/           # 元数据
│   └── wasm_types.rs       # WASM 共享类型（rkyv 派生）
└── README.md
```

### 3.2 cmx-wasm-core：WASM 端 SDK 模块（新建）

#### 3.2.1 职责定义

**cmx-wasm-core 包含：**
1. Arena 分配器实现
2. WASM → Host 调用封装
3. WASM 函数导出宏
4. WASM 端错误类型

**编译目标：** `wasm32-wasip1` only

#### 3.2.2 Cargo.toml 配置

```toml
[package]
name = "cmx-wasm-core"
version.workspace = true
edition.workspace = true

[dependencies]
# 基础类型
cmx-core = { path = "../cmx-core" }

# rkyv 零拷贝序列化
rkyv = { version = "0.8", features = ["alloc", "validation"] }

# 错误处理
thiserror = "2"
```

#### 3.2.3 Arena 分配器（安全增强版）

```rust
// cmx-wasm-core/src/arena.rs

use std::alloc::{alloc, dealloc, Layout};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicU64, Ordering};

/// 全局 Arena 版本计数器
/// 
/// 用于检测 Arena 是否被重置，防止访问已失效的内存区域。
static ARENA_VERSION_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Arena 内存块句柄
/// 
/// RAII 封装，确保内存自动释放。
pub struct ArenaBlock {
    /// 内存起始指针
    ptr: NonNull<u8>,
    /// 内存大小
    size: usize,
    /// 对齐要求
    align: usize,
}

impl ArenaBlock {
    /// 创建新的内存块
    pub fn new(size: usize, align: usize) -> Result<Self, ArenaError> {
        if !align.is_power_of_two() {
            return Err(ArenaError::InvalidAlignment);
        }
        
        let layout = Layout::from_size_align(size, align)
            .map_err(|_| ArenaError::InvalidLayout)?;
        
        let ptr = unsafe { alloc(layout) };
        let ptr = NonNull::new(ptr).ok_or(ArenaError::AllocationFailed)?;
        
        Ok(Self { ptr, size, align })
    }
    
    /// 获取指针
    pub fn as_ptr(&self) -> *const u8 {
        self.ptr.as_ptr()
    }
    
    /// 获取可变指针
    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.ptr.as_ptr()
    }
    
    /// 获取大小
    pub fn size(&self) -> usize {
        self.size
    }
}

impl Drop for ArenaBlock {
    fn drop(&mut self) {
        let layout = Layout::from_size_align(self.size, self.align).unwrap();
        unsafe {
            dealloc(self.ptr.as_ptr(), layout);
        }
    }
}

/// WASM 线性内存 Arena 分配器（安全增强版）
/// 
/// 采用 Bump 分配策略，分配速度快，适合 rkyv 序列化场景。
/// 
/// # 安全特性
/// - 版本控制：检测 Arena 是否被重置
/// - 边界检查：防止越界访问
/// - RAII 管理：自动释放内存
/// - 对齐验证：确保对齐要求正确
/// 
/// # 内存布局
/// ```text
/// ┌────────────────────────────────────────────────┐
/// │ Arena 内存区域                                  │
/// │                                                │
/// │ [0..current)     已分配区域                     │
/// │ [current..capacity) 可用区域                    │
/// └────────────────────────────────────────────────┘
/// ```
pub struct WasmArena {
    /// 内存块（RAII 管理）
    block: ArenaBlock,
    /// 当前分配位置
    current: usize,
    /// Arena 版本号
    version: u64,
    /// 内存统计
    stats: MemoryStats,
    /// Arena ID（用于全局管理）
    id: Option<u64>,
}

impl WasmArena {
    /// 创建新的 Arena
    /// 
    /// # 参数
    /// - `capacity`: Arena 容量（字节）
    /// 
    /// # 返回值
    /// 返回 Arena 实例或错误
    pub fn new(capacity: usize) -> Result<Self, ArenaError> {
        let block = ArenaBlock::new(capacity, 8)?;
        let version = ARENA_VERSION_COUNTER.fetch_add(1, Ordering::SeqCst);
        
        let mut arena = Self {
            block,
            current: 0,
            version,
            stats: MemoryStats::new(),
            id: None,
        };
        
        // 注册到全局内存管理器
        #[cfg(feature = "memory_tracking")]
        {
            use crate::memory_manager::MemoryManager;
            use std::sync::Arc;
            let manager = MemoryManager::global();
            let weak = Arc::new(arena.clone()).downgrade();
            arena.id = Some(manager.register(weak));
        }
        
        Ok(arena)
    }
    
    /// 分配指定大小的内存
    /// 
    /// # 参数
    /// - `size`: 分配大小（字节）
    /// - `align`: 对齐要求（必须是 2 的幂）
    /// 
    /// # 返回值
    /// 返回分配的内存偏移量（相对于 Arena 起始位置）
    /// 
    /// # 安全性
    /// - 自动进行边界检查
    /// - 验证对齐要求
    pub fn alloc(&mut self, size: usize, align: usize) -> Result<usize, ArenaError> {
        // 验证对齐要求
        if !align.is_power_of_two() || align == 0 {
            self.stats.record_allocation_failure();
            return Err(ArenaError::InvalidAlignment);
        }
        
        // 计算对齐后的位置
        let aligned_current = (self.current + align - 1) & !(align - 1);
        
        // 检查溢出
        let new_current = aligned_current.checked_add(size)
            .ok_or_else(|| {
                self.stats.record_allocation_failure();
                ArenaError::Overflow
            })?;
        
        // 边界检查
        if new_current > self.block.size() {
            self.stats.record_allocation_failure();
            return Err(ArenaError::OutOfMemory {
                requested: size,
                available: self.block.size() - self.current,
            });
        }
        
        self.current = new_current;
        self.stats.record_allocation(size);
        Ok(aligned_current)
    }
    
    /// 获取 Arena 起始指针
    pub fn as_ptr(&self) -> *const u8 {
        self.block.as_ptr()
    }
    
    /// 获取 Arena 可变指针
    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.block.as_mut_ptr()
    }
    
    /// 重置 Arena（不释放内存）
    /// 
    /// 调用后，所有之前分配的内存都可以被覆盖。
    /// 同时递增版本号，使旧的引用失效。
    pub fn reset(&mut self) {
        // 记录释放统计
        if self.current > 0 {
            self.stats.record_deallocation(self.current);
        }
        
        self.current = 0;
        self.version = ARENA_VERSION_COUNTER.fetch_add(1, Ordering::SeqCst);
    }
    
    /// 获取当前版本号
    pub fn version(&self) -> u64 {
        self.version
    }
    
    /// 获取内存统计信息
    pub fn stats(&self) -> &MemoryStats {
        &self.stats
    }
    
    /// 获取可变内存统计信息
    pub fn stats_mut(&mut self) -> &mut MemoryStats {
        &mut self.stats
    }
    
    /// 获取已使用的大小
    pub fn used(&self) -> usize {
        self.current
    }
    
    /// 获取剩余容量
    pub fn remaining(&self) -> usize {
        self.block.size() - self.current
    }
    
    /// 获取总容量
    pub fn capacity(&self) -> usize {
        self.block.size()
    }
    
    /// 验证偏移量和长度的有效性
    /// 
    /// # 参数
    /// - `offset`: 内存偏移量
    /// - `len`: 数据长度
    /// 
    /// # 返回值
    /// 如果有效返回 Ok(())，否则返回错误
    pub fn validate_bounds(&self, offset: usize, len: usize) -> Result<(), ArenaError> {
        let end = offset.checked_add(len)
            .ok_or(ArenaError::Overflow)?;
        
        if end > self.block.size() {
            return Err(ArenaError::OutOfBounds {
                offset,
                len,
                capacity: self.block.size(),
            });
        }
        
        Ok(())
    }
}

// 确保 Arena 是 Send 安全的
unsafe impl Send for WasmArena {}

impl Drop for WasmArena {
    fn drop(&mut self) {
        // 从全局内存管理器注销
        #[cfg(feature = "memory_tracking")]
        {
            if let Some(id) = self.id {
                use crate::memory_manager::MemoryManager;
                let manager = MemoryManager::global();
                manager.unregister(id);
            }
        }
        
        // 记录释放统计
        if self.current > 0 {
            self.stats.record_deallocation(self.current);
        }
    }
}

/// 内存使用统计
#[derive(Debug, Clone, Default)]
pub struct MemoryStats {
    /// 总分配次数
    pub total_allocations: u64,
    /// 总释放次数
    pub total_deallocations: u64,
    /// 当前使用内存
    pub current_usage: usize,
    /// 峰值使用内存
    pub peak_usage: usize,
    /// 分配失败次数
    pub allocation_failures: u64,
}

impl MemoryStats {
    /// 创建新的统计实例
    pub fn new() -> Self {
        Self::default()
    }
    
    /// 记录分配
    pub fn record_allocation(&mut self, size: usize) {
        self.total_allocations += 1;
        self.current_usage += size;
        if self.current_usage > self.peak_usage {
            self.peak_usage = self.current_usage;
        }
    }
    
    /// 记录释放
    pub fn record_deallocation(&mut self, size: usize) {
        self.total_deallocations += 1;
        self.current_usage = self.current_usage.saturating_sub(size);
    }
    
    /// 记录分配失败
    pub fn record_allocation_failure(&mut self) {
        self.allocation_failures += 1;
    }
}

/// Arena 错误类型
#[derive(Debug, Clone, thiserror::Error)]
pub enum ArenaError {
    #[error("内存分配失败")]
    AllocationFailed,
    
    #[error("无效的对齐要求")]
    InvalidAlignment,
    
    #[error("无效的内存布局")]
    InvalidLayout,
    
    #[error("内存溢出")]
    Overflow,
    
    #[error("内存不足: 请求 {requested} 字节，可用 {available} 字节")]
    OutOfMemory {
        requested: usize,
        available: usize,
    },
    
    #[error("内存越界: 偏移 {offset}，长度 {len}，容量 {capacity}")]
    OutOfBounds {
        offset: usize,
        len: usize,
        capacity: usize,
    },
}
```

#### 3.2.4 WASM → Host 调用封装

```rust
// cmx-wasm-core/src/host_caller.rs

use rkyv::{Archive, Serialize, Deserialize};
use rkyv::de::deserializers::SharedDeserializeMap;
use crate::arena::WasmArena;
use crate::error::WasmError;

/// WASM 端宿主函数调用器
/// 
/// 用于 WASM 调用 Host 函数，使用 rkyv + Arena 实现零拷贝。
/// 
/// # 使用示例
/// ```rust
/// use cmx_wasm_core::HostCaller;
/// use cmx_core::wasm_types::{DbQueryRequest, DbResponse};
/// 
/// let mut caller = HostCaller::default();
/// let request = DbQueryRequest {
///     sql: "SELECT 1".to_string(),
///     params: None,
///     dataset_id: None,
/// };
/// let response: DbResponse = caller.call("cmx:database", "query_sql", &request)?;
/// ```
pub struct HostCaller {
    /// Arena 分配器（用于接收 Host 响应）
    arena: WasmArena,
}

impl HostCaller {
    /// 创建新的调用器
    /// 
    /// # 参数
    /// - `arena_capacity`: Arena 容量（字节），默认 64KB
    pub fn new(arena_capacity: usize) -> Self {
        Self {
            arena: WasmArena::new(arena_capacity),
        }
    }
    
    /// 使用默认配置创建调用器（64KB Arena）
    pub fn default() -> Self {
        Self::new(64 * 1024)
    }
    
    /// 调用宿主函数（零拷贝）
    /// 
    /// # 类型参数
    /// - `T`: 请求类型，必须实现 Archive + Serialize
    /// - `R`: 响应类型，必须实现 Archive + Deserialize
    /// 
    /// # 参数
    /// - `namespace`: 命名空间（如 "cmx:database"）
    /// - `function`: 函数名（如 "query_sql"）
    /// - `request`: 请求对象
    /// 
    /// # 返回值
    /// 返回响应对象或错误
    pub fn call<T, R>(
        &mut self,
        namespace: &str,
        function: &str,
        request: &T,
    ) -> Result<R, WasmError>
    where
        T: Serialize<rkyv::ser::serializers::AllocSerializer<256>>,
        R: Archive,
        R::Archived: Deserialize<R, SharedDeserializeMap>,
    {
        // 1. 使用 rkyv 序列化请求
        let bytes = rkyv::to_bytes::<_, 256>(request)
            .map_err(|e| WasmError::SerializationError(e.to_string()))?;
        
        // 2. 重置 Arena 并准备输出缓冲区
        self.arena.reset();
        let output_ptr = self.arena.as_mut_ptr() as i32;
        let output_capacity = self.arena.capacity() as i32;
        
        // 3. 调用宿主函数
        let result_len = unsafe {
            Self::call_host_raw(
                namespace.as_ptr() as i32,
                namespace.len() as i32,
                function.as_ptr() as i32,
                function.len() as i32,
                bytes.as_ptr() as i32,
                bytes.len() as i32,
                output_ptr,
                output_capacity,
            )
        };
        
        // 4. 处理返回值
        if result_len < 0 {
            let required_len = (-result_len) as usize;
            return Err(WasmError::BufferTooSmall { required: required_len });
        }
        
        // 5. 零拷贝反序列化响应
        let output_bytes = unsafe {
            std::slice::from_raw_parts(self.arena.as_ptr(), result_len as usize)
        };
        
        let archived = unsafe { rkyv::archived_root::<R>(output_bytes) };
        let response = archived
            .deserialize(&mut SharedDeserializeMap::new())
            .map_err(|e| WasmError::DeserializationError(e.to_string()))?;
        
        Ok(response)
    }
    
    /// 底层宿主函数调用（由 cmx-runtime 的 linker_adapter 实现）
    #[link(wasm_import_module = "cmx:host")]
    unsafe extern "C" fn call_host_raw(
        ns_ptr: i32,
        ns_len: i32,
        fn_ptr: i32,
        fn_len: i32,
        input_ptr: i32,
        input_len: i32,
        output_ptr: i32,
        output_capacity: i32,
    ) -> i32;
}
```

#### 3.2.5 WASM 函数导出封装

```rust
// cmx-wasm-core/src/export.rs

use rkyv::{Archive, Serialize, Deserialize};
use rkyv::de::deserializers::SharedDeserializeMap;
use crate::arena::WasmArena;
use crate::error::WasmError;

/// WASM 函数处理器
/// 
/// 用于处理 Host 调用 WASM 函数，使用 rkyv + Arena 实现零拷贝。
/// 
/// # 使用示例
/// ```rust
/// use cmx_wasm_core::WasmFunctionHandler;
/// use cmx_core::wasm_types::{WasmFunctionRequest, WasmFunctionResponse, WasmContext};
/// 
/// static mut HANDLER: Option<WasmFunctionHandler> = None;
/// 
/// fn get_handler() -> &'static mut WasmFunctionHandler {
///     unsafe {
///         if HANDLER.is_none() {
///             HANDLER = Some(WasmFunctionHandler::default());
///         }
///         HANDLER.as_mut().unwrap()
///     }
/// }
/// 
/// #[no_mangle]
/// pub extern "C" fn process(input_ptr: i32, input_len: i32) -> i64 {
///     let handler = get_handler();
///     handler.handle(input_ptr, input_len, |request: WasmFunctionRequest<MyData>| {
///         // 业务逻辑
///         Ok(WasmFunctionResponse {
///             success: true,
///             data: Some(result),
///             error: None,
///         })
///     })
/// }
/// ```
pub struct WasmFunctionHandler {
    /// Arena 分配器（用于存储响应）
    arena: WasmArena,
}

impl WasmFunctionHandler {
    /// 创建新的处理器
    pub fn new(arena_capacity: usize) -> Self {
        Self {
            arena: WasmArena::new(arena_capacity),
        }
    }
    
    /// 使用默认配置创建处理器（64KB Arena）
    pub fn default() -> Self {
        Self::new(64 * 1024)
    }
    
    /// 处理 Host 调用
    /// 
    /// # 参数
    /// - `input_ptr`: 输入数据指针（WASM 线性内存偏移）
    /// - `input_len`: 输入数据长度
    /// - `handler`: 业务处理函数
    /// 
    /// # 返回值
    /// 返回 i128，正数表示成功（编码为 ptr 和 len），负数表示错误码
    pub fn handle<T, R, F>(
        &mut self,
        input_ptr: i32,
        input_len: i32,
        handler: F,
    ) -> i128
    where
        T: Archive,
        T::Archived: Deserialize<T, SharedDeserializeMap>,
        R: Serialize<rkyv::ser::serializers::AllocSerializer<256>>,
        F: FnOnce(T) -> Result<R, WasmError>,
    {
        // 1. 零拷贝解析输入
        let request = unsafe {
            self.parse_input::<T>(input_ptr, input_len)
        };
        
        let request = match request {
            Ok(r) => r,
            Err(e) => return encode_error(e.to_error_code()),
        };
        
        // 2. 执行业务逻辑
        let response = match handler(request) {
            Ok(r) => r,
            Err(e) => return encode_error(e.to_error_code()),
        };
        
        // 3. 序列化响应到 Arena
        match self.serialize_output(&response) {
            Ok((ptr, len)) => encode_success(ptr, len),
            Err(e) => encode_error(e.to_error_code()),
        }
    }
    
    /// 解析输入请求（零拷贝）
    unsafe fn parse_input<T>(&self, input_ptr: i32, input_len: i32) -> Result<T, WasmError>
    where
        T: Archive,
        T::Archived: Deserialize<T, SharedDeserializeMap>,
    {
        let bytes = std::slice::from_raw_parts(input_ptr as *const u8, input_len as usize);
        let archived = rkyv::archived_root::<T>(bytes);
        archived
            .deserialize(&mut SharedDeserializeMap::new())
            .map_err(|e| WasmError::DeserializationError(e.to_string()))
    }
    
    /// 序列化输出响应到 Arena
    fn serialize_output<T>(&mut self, response: &T) -> Result<(u32, u32), WasmError>
    where
        T: Serialize<rkyv::ser::serializers::AllocSerializer<256>>,
    {
        // 重置 Arena
        self.arena.reset();
        
        // 序列化响应
        let bytes = rkyv::to_bytes::<_, 256>(response)
            .map_err(|e| WasmError::SerializationError(e.to_string()))?;
        
        // 写入 Arena
        let len = bytes.len();
        let offset = self.arena.alloc(len, 1).ok_or(WasmError::ArenaFull)?;
        unsafe {
            std::ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                self.arena.as_mut_ptr().add(offset),
                len,
            );
        }
        
        // 返回指针和长度
        let ptr = self.arena.as_ptr() as u32 + offset as u32;
        Ok((ptr, len as u32))
    }
    
    /// 获取 Arena 引用
    pub fn arena(&self) -> &WasmArena {
        &self.arena
    }
    
    /// 获取 Arena 可变引用
    pub fn arena_mut(&mut self) -> &mut WasmArena {
        &mut self.arena
    }
}

/// WASM 函数返回值编码方案（增强版）
/// 
/// # 编码方案
/// 
/// 使用 i128 编码返回值，支持更大的地址空间和明确的错误码分离：
/// 
/// - **成功返回**: 正数，格式为 `(ptr << 64) | len`
///   - ptr: u64，内存偏移量（支持 64 位地址空间）
///   - len: u64，数据长度
/// 
/// - **错误返回**: 负数，格式为 `-(error_code)`
///   - error_code: u32，错误码
/// 
/// # 示例
/// ```rust
/// // 成功返回
/// let ptr = 0x1000u64;
/// let len = 256u64;
/// let result = encode_success(ptr, len);  // 正数
/// 
/// // 错误返回
/// let error_code = 1u32;  // 序列化错误
/// let result = encode_error(error_code);  // 负数
/// ```

/// 成功返回值编码
/// 
/// 将指针和长度编码为正数 i128
#[inline]
pub fn encode_success(ptr: u64, len: u64) -> i128 {
    ((ptr as i128) << 64) | (len as i128)
}

/// 错误返回值编码
/// 
/// 将错误码编码为负数 i128
#[inline]
pub fn encode_error(error_code: u32) -> i128 {
    -(error_code as i128)
}

/// 解码返回值
/// 
/// 返回 Ok((ptr, len)) 表示成功，返回 Err(error_code) 表示错误
#[inline]
pub fn decode_result(value: i128) -> Result<(u64, u64), u32> {
    if value < 0 {
        Err((-value) as u32)
    } else {
        let ptr = (value >> 64) as u64;
        let len = value as u64;
        Ok((ptr, len))
    }
}

/// 旧版编码函数（向后兼容）
/// 
/// 编码指针和长度为 i64
/// 
/// 高 32 位是指针，低 32 位是长度。
#[inline]
#[deprecated(note = "使用 encode_success 代替，支持更大的地址空间")]
pub fn encode_ptr_len(ptr: u32, len: u32) -> i64 {
    ((ptr as i64) << 32) | (len as i64)
}

/// 旧版解码函数（向后兼容）
/// 
/// 解码 i64 为指针和长度
#[inline]
#[deprecated(note = "使用 decode_result 代替，支持错误码分离")]
pub fn decode_ptr_len(value: i64) -> (u32, u32) {
    let ptr = (value >> 32) as u32;
    let len = value as u32;
    (ptr, len)
}

/// 错误码定义
pub mod error_codes {
    /// 成功
    pub const SUCCESS: u32 = 0;
    /// 序列化错误
    pub const SERIALIZATION_ERROR: u32 = 1;
    /// 反序列化错误
    pub const DESERIALIZATION_ERROR: u32 = 2;
    /// 缓冲区不足
    pub const BUFFER_TOO_SMALL: u32 = 3;
    /// 内存分配失败
    pub const ALLOCATION_FAILED: u32 = 4;
    /// 业务逻辑错误
    pub const BUSINESS_ERROR: u32 = 5;
    /// 参数无效
    pub const INVALID_ARGUMENT: u32 = 6;
    /// 内存越界
    pub const OUT_OF_BOUNDS: u32 = 7;
    /// Arena 已满
    pub const ARENA_FULL: u32 = 8;
}
```

#### 3.2.6 WASM 端错误类型

```rust
// cmx-wasm-core/src/error.rs

/// WASM 端错误类型
#[derive(Debug, Clone, thiserror::Error)]
pub enum WasmError {
    #[error("序列化错误: {0}")]
    SerializationError(String),
    
    #[error("反序列化错误: {0}")]
    DeserializationError(String),
    
    #[error("缓冲区不足，需要 {required} 字节")]
    BufferTooSmall { required: usize },
    
    #[error("宿主函数调用失败: {0}")]
    HostCallFailed(String),
    
    #[error("验证错误: {0}")]
    ValidationError(String),
    
    #[error("Arena 内存已满")]
    ArenaFull,
    
    #[error("业务逻辑错误: {0}")]
    BusinessError(String),
}

impl WasmError {
    /// 将错误转换为错误码
    /// 
    /// # 返回值
    /// 返回对应的错误码
    pub fn to_error_code(&self) -> u32 {
        use super::export::error_codes::*;
        
        match self {
            WasmError::SerializationError(_) => SERIALIZATION_ERROR,
            WasmError::DeserializationError(_) => DESERIALIZATION_ERROR,
            WasmError::BufferTooSmall { .. } => BUFFER_TOO_SMALL,
            WasmError::HostCallFailed(_) => BUSINESS_ERROR,
            WasmError::ValidationError(_) => INVALID_ARGUMENT,
            WasmError::ArenaFull => ARENA_FULL,
            WasmError::BusinessError(_) => BUSINESS_ERROR,
        }
    }
}
```

#### 3.2.7 模块结构

```
cmx-wasm-core/
├── Cargo.toml
├── src/
│   ├── lib.rs           # 模块入口
│   ├── arena.rs         # Arena 分配器（安全增强版）
│   ├── host_caller.rs   # WASM → Host 调用封装
│   ├── export.rs        # WASM 函数导出封装
│   ├── error.rs         # 错误类型
│   └── memory_manager.rs # 全局内存管理器（新增）
└── README.md
```

#### 3.2.8 全局内存管理器（新增）

```rust
// cmx-wasm-core/src/memory_manager.rs

use std::sync::{Arc, Mutex, Weak};
use std::collections::HashMap;
use crate::arena::{WasmArena, MemoryStats};

/// Arena 实例 ID
type ArenaId = u64;

/// 全局内存管理器
/// 
/// 负责监控所有 Arena 实例的内存使用情况，检测内存泄漏。
/// 
/// # 功能
/// - 追踪所有 Arena 实例
/// - 监控内存使用情况
/// - 检测内存泄漏
/// - 提供内存使用报告
/// 
/// # 使用示例
/// ```rust
/// use cmx_wasm_core::MemoryManager;
/// 
/// // 获取全局管理器实例
/// let manager = MemoryManager::global();
/// 
/// // 创建 Arena（自动注册）
/// let arena = WasmArena::new(1024)?;
/// 
/// // 获取内存报告
/// let report = manager.generate_report();
/// println!("Total arenas: {}", report.total_arenas);
/// println!("Total memory: {} bytes", report.total_memory);
/// ```
pub struct MemoryManager {
    /// Arena 实例注册表
    arenas: Mutex<HashMap<ArenaId, Weak<WasmArena>>>,
    /// 下一个 Arena ID
    next_id: Mutex<ArenaId>,
}

impl MemoryManager {
    /// 创建新的内存管理器
    fn new() -> Self {
        Self {
            arenas: Mutex::new(HashMap::new()),
            next_id: Mutex::new(1),
        }
    }
    
    /// 获取全局实例
    pub fn global() -> Arc<Self> {
        static INSTANCE: std::sync::OnceLock<Arc<MemoryManager>> = std::sync::OnceLock::new();
        INSTANCE.get_or_init(|| Arc::new(Self::new())).clone()
    }
    
    /// 注册 Arena 实例
    /// 
    /// # 参数
    /// - `arena`: Arena 实例的弱引用
    /// 
    /// # 返回值
    /// 返回分配的 Arena ID
    pub fn register(&self, arena: Weak<WasmArena>) -> ArenaId {
        let mut next_id = self.next_id.lock().unwrap();
        let id = *next_id;
        *next_id += 1;
        
        let mut arenas = self.arenas.lock().unwrap();
        arenas.insert(id, arena);
        
        id
    }
    
    /// 注销 Arena 实例
    /// 
    /// # 参数
    /// - `id`: Arena ID
    pub fn unregister(&self, id: ArenaId) {
        let mut arenas = self.arenas.lock().unwrap();
        arenas.remove(&id);
    }
    
    /// 清理已释放的 Arena 实例
    /// 
    /// 移除所有弱引用已失效的 Arena 记录。
    pub fn cleanup(&self) {
        let mut arenas = self.arenas.lock().unwrap();
        arenas.retain(|_, weak| weak.strong_count() > 0);
    }
    
    /// 生成内存使用报告
    /// 
    /// # 返回值
    /// 返回内存使用报告
    pub fn generate_report(&self) -> MemoryReport {
        let arenas = self.arenas.lock().unwrap();
        
        let mut report = MemoryReport::default();
        report.total_arenas = arenas.len();
        
        for (id, weak) in arenas.iter() {
            if let Some(arena) = weak.upgrade() {
                let stats = arena.stats();
                report.active_arenas += 1;
                report.total_memory += arena.capacity();
                report.used_memory += arena.used();
                report.total_allocations += stats.total_allocations;
                report.total_deallocations += stats.total_deallocations;
                report.allocation_failures += stats.allocation_failures;
                
                if stats.current_usage > report.peak_memory {
                    report.peak_memory = stats.peak_usage;
                }
            } else {
                report.leaked_arenas += 1;
            }
        }
        
        report
    }
    
    /// 检测内存泄漏
    /// 
    /// # 返回值
    /// 返回泄漏的 Arena ID 列表
    pub fn detect_leaks(&self) -> Vec<ArenaId> {
        let arenas = self.arenas.lock().unwrap();
        arenas
            .iter()
            .filter(|(_, weak)| weak.strong_count() == 0)
            .map(|(id, _)| *id)
            .collect()
    }
}

/// 内存使用报告
#[derive(Debug, Clone, Default)]
pub struct MemoryReport {
    /// 总 Arena 数量
    pub total_arenas: usize,
    /// 活跃 Arena 数量
    pub active_arenas: usize,
    /// 泄漏 Arena 数量
    pub leaked_arenas: usize,
    /// 总内存容量（字节）
    pub total_memory: usize,
    /// 已使用内存（字节）
    pub used_memory: usize,
    /// 峰值内存使用（字节）
    pub peak_memory: usize,
    /// 总分配次数
    pub total_allocations: u64,
    /// 总释放次数
    pub total_deallocations: u64,
    /// 分配失败次数
    pub allocation_failures: u64,
}

impl MemoryReport {
    /// 计算内存使用率
    pub fn usage_ratio(&self) -> f64 {
        if self.total_memory == 0 {
            0.0
        } else {
            self.used_memory as f64 / self.total_memory as f64
        }
    }
    
    /// 计算分配成功率
    pub fn allocation_success_rate(&self) -> f64 {
        if self.total_allocations == 0 {
            1.0
        } else {
            (self.total_allocations - self.allocation_failures) as f64 / self.total_allocations as f64
        }
    }
    
    /// 检查是否存在内存泄漏
    pub fn has_leaks(&self) -> bool {
        self.leaked_arenas > 0
    }
}
```

### 3.3 cmx-runtime：Host 端运行时模块

#### 3.3.1 职责定义

**cmx-runtime 包含：**
1. wasmtime 运行时封装
2. Host → WASM 调用封装（WasmInvoker）
3. Host 函数注册（处理 WASM → Host 调用）
4. Host 端错误类型

**编译目标：** `native` only

#### 3.3.2 Host → WASM 调用封装

```rust
// cmx-runtime/src/wasm_invoker.rs

use wasmtime::{Memory, Store, Instance, Func};
use rkyv::{Archive, Deserialize, Serialize};
use rkyv::de::deserializers::SharedDeserializeMap;
use rkyv::ser::serializers::AllocSerializer;
use crate::error::InvokeError;

/// Host 端 WASM 调用器
/// 
/// 用于 Host 调用 WASM 函数（编排执行场景），使用 wasmtime + rkyv + Arena。
/// 
/// # 使用示例
/// ```rust
/// use cmx_runtime::WasmInvoker;
/// use cmx_core::wasm_types::{WasmFunctionRequest, WasmFunctionResponse, WasmContext};
/// 
/// let mut invoker = WasmInvoker::new(&instance, &mut store)?;
/// 
/// let request = WasmFunctionRequest {
///     context: WasmContext { ... },
///     data: MyData { ... },
/// };
/// 
/// let response: WasmFunctionResponse<MyResult> = invoker.invoke("process", &request)?;
/// ```
pub struct WasmInvoker<'a> {
    /// WASM 实例
    instance: &'a Instance,
    /// Store 上下文
    store: &'a mut Store<()>,
    /// WASM 线性内存
    memory: Memory,
}

impl<'a> WasmInvoker<'a> {
    /// 创建新的调用器
    pub fn new(instance: &'a Instance, store: &'a mut Store<()>) -> Result<Self, InvokeError> {
        let memory = instance
            .get_memory(store, "memory")
            .ok_or(InvokeError::MemoryNotFound)?;
        Ok(Self { instance, store, memory })
    }
    
    /// 调用 WASM 函数（零拷贝）
    /// 
    /// # 类型参数
    /// - `T`: 请求类型，必须实现 Archive + Serialize
    /// - `R`: 响应类型，必须实现 Archive + Deserialize
    /// 
    /// # 参数
    /// - `func_name`: WASM 函数名
    /// - `request`: 请求对象
    /// 
    /// # 返回值
    /// 返回响应对象或错误
    pub fn invoke<T, R>(
        &mut self,
        func_name: &str,
        request: &T,
    ) -> Result<R, InvokeError>
    where
        T: Serialize<AllocSerializer<256>>,
        R: Archive,
        R::Archived: Deserialize<R, SharedDeserializeMap>,
    {
        // 1. 序列化请求
        let input_bytes = rkyv::to_bytes::<_, 256>(request)
            .map_err(|e| InvokeError::SerializationError(e.to_string()))?;
        
        // 2. 在 WASM 内存中分配空间（调用 WASM 的 wasm_alloc 函数）
        let input_ptr = self.alloc_wasm_memory(input_bytes.len())?;
        
        // 3. 写入输入数据到 WASM 内存
        self.write_to_wasm_memory(input_ptr, &input_bytes)?;
        
        // 4. 调用 WASM 函数
        let func = self.instance
            .get_func(&mut self.store, func_name)
            .ok_or_else(|| InvokeError::FunctionNotFound(func_name.to_string()))?;
        
        let mut result = [wasmtime::Val::I128(0)];
        func.call(
            &mut self.store,
            &[wasmtime::Val::I32(input_ptr as i32), wasmtime::Val::I32(input_bytes.len() as i32)],
            &mut result,
        ).map_err(|e| InvokeError::CallError(e.to_string()))?;
        
        // 5. 解析返回值
        let packed = match result[0] {
            wasmtime::Val::I128(v) => v,
            _ => return Err(InvokeError::InvalidReturnType),
        };
        
        // 6. 解码返回值
        let (output_ptr, output_len) = decode_result(packed)
            .map_err(|error_code| InvokeError::WasmError(error_code))?;
        
        // 7. 从 WASM 内存读取响应
        let output_bytes = self.read_from_wasm_memory(output_ptr, output_len)?;
        
        // 7. 零拷贝反序列化响应
        let archived = unsafe { rkyv::archived_root::<R>(&output_bytes) };
        let response = archived
            .deserialize(&mut SharedDeserializeMap::new())
            .map_err(|e| InvokeError::DeserializationError(e.to_string()))?;
        
        // 8. 重置 WASM Arena
        self.reset_wasm_arena()?;
        
        Ok(response)
    }
    
    /// 在 WASM 内存中分配空间
    fn alloc_wasm_memory(&mut self, size: usize) -> Result<u32, InvokeError> {
        let alloc_func = self.instance
            .get_func(&mut self.store, "wasm_alloc")
            .ok_or(InvokeError::AllocFunctionNotFound)?;
        
        let mut result = [wasmtime::Val::I32(0)];
        alloc_func.call(
            &mut self.store,
            &[wasmtime::Val::I32(size as i32)],
            &mut result,
        ).map_err(|e| InvokeError::AllocError(e.to_string()))?;
        
        match result[0] {
            wasmtime::Val::I32(ptr) if ptr > 0 => Ok(ptr as u32),
            _ => Err(InvokeError::MemoryAllocationFailed),
        }
    }
    
    /// 写入数据到 WASM 内存
    fn write_to_wasm_memory(&mut self, ptr: u32, data: &[u8]) -> Result<(), InvokeError> {
        let memory_data = self.memory.data_mut(&mut self.store);
        let start = ptr as usize;
        let end = start + data.len();
        
        if end > memory_data.len() {
            return Err(InvokeError::MemoryOutOfBounds);
        }
        
        memory_data[start..end].copy_from_slice(data);
        Ok(())
    }
    
    /// 从 WASM 内存读取数据
    fn read_from_wasm_memory(&self, ptr: u32, len: u32) -> Result<Vec<u8>, InvokeError> {
        let memory_data = self.memory.data(&self.store);
        let start = ptr as usize;
        let end = start + len as usize;
        
        if end > memory_data.len() {
            return Err(InvokeError::MemoryOutOfBounds);
        }
        
        Ok(memory_data[start..end].to_vec())
    }
    
    /// 重置 WASM Arena
    fn reset_wasm_arena(&mut self) -> Result<(), InvokeError> {
        if let Some(reset_func) = self.instance.get_func(&mut self.store, "wasm_reset_arena") {
            reset_func.call(&mut self.store, &[], &mut [])
                .map_err(|e| InvokeError::ResetError(e.to_string()))?;
        }
        Ok(())
    }
}

/// 解码 i64 为指针和长度
fn decode_ptr_len(value: i64) -> (u32, u32) {
    let ptr = (value >> 32) as u32;
    let len = value as u32;
    (ptr, len)
}
```

#### 3.3.3 Host 函数注册（处理 WASM → Host）

```rust
// cmx-runtime/src/host_function.rs

use wasmtime::{Caller, Memory, Store};
use rkyv::{Archive, Deserialize, Serialize};
use rkyv::de::deserializers::SharedDeserializeMap;
use rkyv::ser::serializers::AllocSerializer;
use cmx_core::wasm_types::*;
use crate::error::HostFuncError;

/// Host 函数处理上下文
pub struct HostFuncContext {
    /// 数据库 ID
    pub db_id: String,
    /// 请求 ID
    pub request_id: String,
    /// 租户 ID
    pub tenant_id: Option<String>,
    /// 事务 ID
    pub txn_id: Option<String>,
}

/// Host 函数签名
/// 
/// 所有 Host 函数都通过此签名被 WASM 调用。
/// 
/// # 参数
/// - `input_ptr`: 输入数据指针（WASM 线性内存偏移）
/// - `input_len`: 输入数据长度
/// - `output_ptr`: 输出缓冲区指针（WASM 线性内存偏移）
/// - `output_capacity`: 输出缓冲区容量
/// 
/// # 返回值
/// - 正数: 实际写入的字节数
/// - 负数: 需要的缓冲区大小（容量不足时）
pub type HostFuncSignature = fn(i32, i32, i32, i32) -> i32;

/// 通用的 Host 函数处理逻辑
/// 
/// # 使用示例
/// ```rust
/// use cmx_runtime::host_function::handle_host_call;
/// use cmx_core::wasm_types::{DbQueryRequest, DbResponse};
/// 
/// fn query_sql_handler(
///     caller: &mut Caller<HostFuncContext>,
///     input_ptr: i32,
///     input_len: i32,
///     output_ptr: i32,
///     output_capacity: i32,
/// ) -> Result<i32, HostFuncError> {
///     handle_host_call::<DbQueryRequest, DbResponse, _>(
///         caller,
///         input_ptr,
///         input_len,
///         output_ptr,
///         output_capacity,
///         |ctx, request| {
///             // 业务逻辑
///             Ok(DbResponse { success: true, ... })
///         },
///     )
/// }
/// ```
pub fn handle_host_call<T, R, F>(
    caller: &mut Caller<HostFuncContext>,
    input_ptr: i32,
    input_len: i32,
    output_ptr: i32,
    output_capacity: i32,
    handler: F,
) -> Result<i32, HostFuncError>
where
    T: Archive,
    T::Archived: Deserialize<T, SharedDeserializeMap>,
    R: Serialize<AllocSerializer<256>>,
    F: FnOnce(&HostFuncContext, T) -> Result<R, HostFuncError>,
{
    // 1. 获取 WASM 内存
    let memory = caller
        .get_export("memory")
        .and_then(|e| e.into_memory())
        .ok_or(HostFuncError::MemoryNotFound)?;
    
    // 2. 从 WASM 内存读取输入数据
    let input_bytes = read_from_wasm_memory(&memory, caller, input_ptr as u32, input_len as u32)?;
    
    // 3. 零拷贝反序列化请求
    let archived = unsafe { rkyv::archived_root::<T>(&input_bytes) };
    let request = archived
        .deserialize(&mut SharedDeserializeMap::new())
        .map_err(|e| HostFuncError::DeserializationError(e.to_string()))?;
    
    // 4. 执行业务逻辑
    let context = caller.data();
    let response = handler(context, request)?;
    
    // 5. 序列化响应
    let output_bytes = rkyv::to_bytes::<_, 256>(&response)
        .map_err(|e| HostFuncError::SerializationError(e.to_string()))?;
    
    // 6. 检查缓冲区大小
    if output_bytes.len() > output_capacity as usize {
        return Ok(-(output_bytes.len() as i32)); // 返回所需大小
    }
    
    // 7. 写入响应到 WASM 内存
    write_to_wasm_memory(&memory, caller, output_ptr as u32, &output_bytes)?;
    
    Ok(output_bytes.len() as i32)
}

/// 从 WASM 内存读取数据
fn read_from_wasm_memory(
    memory: &Memory,
    caller: &Caller<HostFuncContext>,
    ptr: u32,
    len: u32,
) -> Result<Vec<u8>, HostFuncError> {
    let data = memory.data(caller);
    let start = ptr as usize;
    let end = start + len as usize;
    
    if end > data.len() {
        return Err(HostFuncError::MemoryOutOfBounds);
    }
    
    Ok(data[start..end].to_vec())
}

/// 写入数据到 WASM 内存
fn write_to_wasm_memory(
    memory: &Memory,
    caller: &mut Caller<HostFuncContext>,
    ptr: u32,
    data: &[u8],
) -> Result<(), HostFuncError> {
    let data_mut = memory.data_mut(caller);
    let start = ptr as usize;
    let end = start + data.len();
    
    if end > data_mut.len() {
        return Err(HostFuncError::MemoryOutOfBounds);
    }
    
    data_mut[start..end].copy_from_slice(data);
    Ok(())
}
```

#### 3.3.4 Host 端错误类型

```rust
// cmx-runtime/src/error.rs

/// Host 端调用错误类型
#[derive(Debug, Clone, thiserror::Error)]
pub enum InvokeError {
    #[error("内存未找到")]
    MemoryNotFound,
    
    #[error("函数未找到: {0}")]
    FunctionNotFound(String),
    
    #[error("分配函数未找到")]
    AllocFunctionNotFound,
    
    #[error("序列化错误: {0}")]
    SerializationError(String),
    
    #[error("反序列化错误: {0}")]
    DeserializationError(String),
    
    #[error("调用错误: {0}")]
    CallError(String),
    
    #[error("分配错误: {0}")]
    AllocError(String),
    
    #[error("重置错误: {0}")]
    ResetError(String),
    
    #[error("无效的返回类型")]
    InvalidReturnType,
    
    #[error("WASM 函数返回错误，错误码: {0}")]
    WasmError(u32),
    
    #[error("内存分配失败")]
    MemoryAllocationFailed,
    
    #[error("内存越界")]
    MemoryOutOfBounds,
}

/// Host 函数处理错误类型
#[derive(Debug, Clone, thiserror::Error)]
pub enum HostFuncError {
    #[error("内存未找到")]
    MemoryNotFound,
    
    #[error("序列化错误: {0}")]
    SerializationError(String),
    
    #[error("反序列化错误: {0}")]
    DeserializationError(String),
    
    #[error("内存越界")]
    MemoryOutOfBounds,
    
    #[error("业务逻辑错误: {0}")]
    BusinessError(String),
}
```

### 3.4 数据传递协议总结

#### 3.4.1 场景一：WASM → Host

```
┌──────────────────────────────────────────────────────────────────┐
│                    WASM (使用 cmx-wasm-core)                      │
│                                                                  │
│  1. 构建请求对象（cmx-core 类型）                                  │
│     let request = DbQueryRequest { sql: "SELECT ...", ... };     │
│                                                                  │
│  2. rkyv 序列化请求                                               │
│     let bytes = rkyv::to_bytes(&request)?;                       │
│                                                                  │
│  3. 准备 Arena 输出缓冲区                                         │
│     arena.reset();                                               │
│                                                                  │
│  4. 调用宿主函数                                                   │
│     host_caller.call("cmx:database", "query_sql", &request)?;    │
│                                                                  │
│  5. rkyv 零拷贝反序列化响应                                        │
│     let archived = rkyv::archived_root::<R>(output_bytes);       │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
                                    │
                                    │ (input_ptr, input_len, output_ptr, capacity)
                                    ▼
┌──────────────────────────────────────────────────────────────────┐
│                    Host (使用 cmx-runtime)                        │
│                                                                  │
│  1. 从 WASM 线性内存读取 rkyv 归档数据                             │
│     let bytes = memory.data(&store)[ptr..ptr+len];               │
│                                                                  │
│  2. 零拷贝访问请求                                                 │
│     let archived = rkyv::archived_root::<T>(bytes);              │
│                                                                  │
│  3. 执行业务逻辑                                                   │
│     let response = handler(context, archived)?;                  │
│                                                                  │
│  4. rkyv 序列化响应                                                │
│     let output = rkyv::to_bytes(&response)?;                     │
│                                                                  │
│  5. 写入 WASM 线性内存                                             │
│     memory.data_mut(&store)[out_ptr..] = output;                 │
│                                                                  │
│  6. 返回写入字节数                                                 │
│     return output.len() as i32;                                  │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
```

#### 3.4.2 场景二：Host → WASM

```
┌──────────────────────────────────────────────────────────────────┐
│                    Host (使用 cmx-runtime)                        │
│                                                                  │
│  1. rkyv 序列化请求                                                │
│     let input = rkyv::to_bytes(&request)?;                       │
│                                                                  │
│  2. 调用 WASM 分配函数                                             │
│     let input_ptr = wasm_alloc(input.len())?;                    │
│                                                                  │
│  3. 写入输入数据到 WASM 内存                                       │
│     memory[input_ptr..input_ptr+input.len()] = input;            │
│                                                                  │
│  4. 调用 WASM 业务函数                                             │
│     let result = invoker.invoke("process", &request)?;           │
│                                                                  │
│  5. 零拷贝读取响应                                                 │
│     let response = rkyv::archived_root::<R>(output);             │
│                                                                  │
│  6. 重置 WASM Arena                                               │
│     wasm_reset_arena();                                          │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
                                    │
                                    │ (input_ptr, input_len) -> i64(ptr, len)
                                    ▼
┌──────────────────────────────────────────────────────────────────┐
│                    WASM (使用 cmx-wasm-core)                      │
│                                                                  │
│  1. 零拷贝解析输入请求                                             │
│     let request = parse_input::<T>(input_ptr, input_len)?;       │
│                                                                  │
│  2. 执行业务逻辑                                                   │
│     let response = process_request(request)?;                    │
│                                                                  │
│  3. rkyv 序列化响应到 Arena                                        │
│     let (ptr, len) = serialize_to_arena(&response)?;             │
│                                                                  │
│  4. 返回 (ptr, len) 编码为 i64                                    │
│     return encode_ptr_len(ptr, len);                             │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
```

#### 3.4.3 两种场景对比

| 特性 | WASM → Host | Host → WASM |
|------|-------------|-------------|
| 调用方模块 | cmx-wasm-core | cmx-runtime |
| 被调用方模块 | cmx-runtime | cmx-wasm-core |
| 内存分配方 | WASM (Arena) | WASM (Arena) |
| 输出缓冲区 | WASM 预分配 | WASM 动态分配 |
| 返回值类型 | i32（字节数） | i64（ptr, len） |
| 典型场景 | 调用数据库、缓存 | 编排执行插件函数 |

***

## 四、技术约束

### 4.1 模块依赖约束

| 模块 | 允许依赖 | 禁止依赖 |
|------|----------|----------|
| cmx-core | serde, chrono, uuid, rkyv 等基础库 | 业务模块 |
| cmx-traits | cmx-core | 业务模块 |
| cmx-runtime | cmx-core, cmx-traits, cmx-utils, wasmtime | cmx-database, cmx-buffer, cmx-plugin, cmx-service |
| cmx-wasm-core | cmx-core, rkyv | 业务模块、运行时模块 |
| cmx-service | cmx-core, cmx-traits, cmx-database | 无 |
| cmx-wasmdemo | cmx-core, cmx-wasm-core | 无限制 |

### 4.2 WASM 编译约束

**目标平台：** `wasm32-wasip1`（WASI Preview 1）

**约束：**

1. cmx-core 必须支持 `wasm32-wasip1` 目标编译
2. cmx-wasm-core 只编译为 `wasm32-wasip1`
3. cmx-runtime 只编译为 `native`
4. 所有依赖必须支持 WASM 目标
5. 不使用 `no_std`，利用 WASI 的标准库支持

### 4.3 数据传递约束

**技术栈：** wasmtime + rkyv + Arena

**约束：**

1. Host → Guest 和 Guest → Host 均使用此技术栈
2. rkyv 版本必须在 Host 和 Guest 之间保持一致
3. 使用 bytecheck 验证数据对齐
4. Arena 定期重置以避免内存碎片

### 4.4 性能约束

| 指标 | 目标值 |
|------|--------|
| 单次宿主函数调用延迟 | < 1ms |
| 编排执行吞吐量 | > 1000 steps/s |
| rkyv 序列化性能 | 比 JSON 快 10x 以上 |
| 零拷贝反序列化 | 无额外开销 |

***

## 五、文件修改清单

| 模块 | 文件 | 修改类型 | 说明 |
|------|------|----------|------|
| cmx-core | Cargo.toml | 修改 | 添加 rkyv 依赖 |
| cmx-core | src/wasm_types.rs | 新增 | WASM 共享类型（rkyv 派生） |
| cmx-wasm-core | 新模块 | 新增 | WASM 端 SDK |
| cmx-wasm-core | src/arena.rs | 新增 | Arena 分配器 |
| cmx-wasm-core | src/host_caller.rs | 新增 | WASM → Host 调用封装 |
| cmx-wasm-core | src/export.rs | 新增 | WASM 函数导出封装 |
| cmx-wasm-core | src/error.rs | 新增 | WASM 端错误类型 |
| cmx-runtime | src/wasm_invoker.rs | 新增 | Host → WASM 调用封装 |
| cmx-runtime | src/host_function.rs | 修改 | Host 函数注册（rkyv 版本） |
| cmx-runtime | src/error.rs | 修改 | 添加新错误类型 |
| cmx-runtime | src/linker_adapter.rs | 修改 | 适配 rkyv 零拷贝 |
| cmx-runtime | src/engine.rs | 修改 | 修复 P0-1、P0-2 问题 |
| cmx-database | host_functions.rs | 修改 | 使用 rkyv 类型 |
| cmx-buffer | host_functions.rs | 修改 | 使用 rkyv 类型 |
| cmx-utils | host_functions.rs | 修改 | 使用 rkyv 类型 |
| cmx-plugin | host_functions.rs | 修改 | 使用 rkyv 类型 |
| cmx-service | src/orchestrator.rs | 修改 | 实现条件/并行/重试 |
| cmx-service | src/registry.rs | 新增 | 编排定义持久化（解决 P0-4） |
| cmx-wasmdemo | Cargo.toml | 修改 | 添加 cmx-core + cmx-wasm-core 依赖 |
| cmx-wasmdemo | src/lib.rs | 修改 | 使用 cmx-wasm-core |
| cmx-wasmdemo | src/demo.rs | 修改 | 使用零拷贝调用 |

***

## 六、实施优先级

### P0 - 核心功能（必须实现）

| 序号 | 任务 | 解决问题 | 预计工作量 |
|------|------|----------|------------|
| 1 | cmx-core 扩展 | P1-4 | 0.5 天 |
| 2 | cmx-wasm-core 模块 | P2-3 | 1 天 |
| 3 | cmx-runtime WasmInvoker | P0-1, P0-2 | 1 天 |
| 4 | cmx-runtime HostFunction | P0-3 | 0.5 天 |
| 5 | 编排持久化 | P0-4 | 0.5 天 |

### P1 - 重要功能（建议实现）

| 序号 | 任务 | 解决问题 | 预计工作量 |
|------|------|----------|------------|
| 1 | 条件执行 | P1-1 | 0.5 天 |
| 2 | 错误重试 | P1-3 | 0.5 天 |
| 3 | 宿主函数迁移 | P1-4 | 1 天 |

### P2 - 增强功能（可选实现）

| 序号 | 任务 | 解决问题 | 预计工作量 |
|------|------|----------|------------|
| 1 | 并行执行 | P1-2 | 1 天 |
| 2 | 权限控制 | P2-1 | 1 天 |
| 3 | 资源限制 | P2-2 | 0.5 天 |
| 4 | 编排版本管理 | P2-4 | 0.5 天 |

***

## 七、验收标准

### 7.1 数据传递（WASM → Host）

- [ ] cmx-wasm-core 的 HostCaller 可正确调用 Host 函数
- [ ] Host 函数可正确解析 WASM 传入的 rkyv 数据
- [ ] Host 函数响应可正确写入 WASM Arena
- [ ] 零拷贝反序列化正常工作
- [ ] 无竞态条件（P0-3 已解决）

### 7.2 数据传递（Host → WASM）

- [ ] cmx-runtime 的 WasmInvoker 可正确调用 WASM 函数
- [ ] WASM 函数可正确解析 Host 传入的 rkyv 数据
- [ ] WASM 函数返回 (ptr, len) 正确编码为 i64
- [ ] Host 可正确解析 WASM 返回的响应数据
- [ ] 输入数据正确传递（P0-1 已解决）
- [ ] 返回值正确获取（P0-2 已解决）

### 7.3 类型共享

- [ ] cmx-core 可编译为 wasm32-wasip1 目标
- [ ] cmx-core 可编译为 native 目标
- [ ] 所有数据类型使用 rkyv 派生
- [ ] rkyv 序列化/反序列化正确

### 7.4 模块隔离

- [ ] cmx-core 不包含任何 I/O 或运行时代码
- [ ] cmx-wasm-core 只编译为 wasm32-wasip1
- [ ] cmx-runtime 只编译为 native
- [ ] cmx-runtime 不依赖任何业务模块

### 7.5 编排功能

- [ ] 编排定义可持久化到数据库（P0-4 已解决）
- [ ] 条件表达式可正确解析和执行（P1-1 已解决）
- [ ] 并行步骤可同时执行（P1-2 已解决）
- [ ] 错误重试可按配置执行（P1-3 已解决）

### 7.6 性能指标

- [ ] 单次宿主函数调用延迟 < 1ms
- [ ] 编排执行吞吐量 > 1000 steps/s
- [ ] 内存占用合理，无内存泄漏
- [ ] rkyv 序列化比 JSON 快 10x 以上
- [ ] 零拷贝反序列化无额外开销

***

## 八、风险与缓解

| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| cmx-core 依赖兼容性 | 某些依赖可能不支持 wasm32-wasip1 | 仔细验证每个依赖，必要时寻找替代方案 |
| rkyv 版本兼容性 | Guest 和 Host 版本必须一致 | 锁定相同版本，添加版本检查 |
| 内存对齐问题 | rkyv 要求数据对齐 | 使用 bytecheck 验证 |
| 并行执行复杂性 | 可能引入新的并发问题 | 充分测试，提供回退选项 |
| Arena 内存碎片 | 长期运行可能产生碎片 | 定期重置 Arena，监控内存使用 |
| 新模块学习成本 | 开发者需要学习新模块 | 提供详细文档和示例 |

***

## 九、编译命令参考

```bash
# 安装 WASI 目标
rustup target add wasm32-wasip1

# 编译 cmx-core 为 WASM（验证兼容性）
cd crates/libs/cmx-core
cargo build --target wasm32-wasip1

# 编译 cmx-wasm-core
cd crates/libs/cmx-wasm-core
cargo build --target wasm32-wasip1

# 编译 cmx-wasmdemo
cd crates/libs/cmx-wasmdemo
cargo build --release --target wasm32-wasip1

# 输出文件
# target/wasm32-wasip1/release/cmx_wasmdemo.wasm
```

***

## 十、参考资料

### 10.1 rkyv 文档

* [rkyv 官方文档](https://docs.rs/rkyv/)
* [rkyv GitHub](https://github.com/rkyv/rkyv)
* [rkyv Book](https://rkyv.org/)

### 10.2 wasmtime 文档

* [wasmtime API 文档](https://docs.wasmtime.dev/)
* [wasmtime LinearMemory](https://docs.wasmtime.dev/api/wasmtime/trait.LinearMemory.html)
* [wasmtime Memory](https://docs.rs/wasmtime/latest/wasmtime/struct.Memory.html)

### 10.3 Arena 分配器

* [Arena Allocator 概念](https://lib.rs/crates/arena-allocator)
* [Bump 分配策略](https://fitzgeraldnick.com/2019/11/01/always-bump-downwards.html)

### 10.4 零拷贝序列化

* [rkyv 零拷贝原理](https://blog.csdn.net/gitblog_01101/article/details/141846121)
* [bytecheck 验证机制](https://docs.rs/rkyv/latest/rkyv/trait.CheckBytes.html)

***

## 十一、P0 问题修正总结

### 11.1 P0-5：Arena 内存安全缺陷

**问题描述：**
- 缺少版本控制，无法检测 Arena 是否被重置
- 缺少边界检查，可能导致越界访问
- 缺少对齐验证，可能导致未定义行为
- 缺少溢出检查，可能导致整数溢出

**修正方案：**

1. **版本控制机制**
   - 添加全局版本计数器 `ARENA_VERSION_COUNTER`
   - 每次 `reset()` 时递增版本号
   - 通过版本号检测 Arena 是否被重置

2. **边界检查增强**
   - 添加 `validate_bounds()` 方法验证偏移量和长度
   - 在 `alloc()` 中进行溢出检查
   - 返回详细的错误信息（包含请求大小和可用大小）

3. **对齐验证**
   - 验证对齐参数是否为 2 的幂
   - 验证对齐参数是否为 0
   - 返回明确的错误类型

4. **RAII 内存管理**
   - 引入 `ArenaBlock` 封装内存分配
   - 实现 `Drop` trait 自动释放内存
   - 确保内存不会泄漏

**修正效果：**
- ✅ 防止访问已失效的内存区域
- ✅ 防止越界访问
- ✅ 防止对齐错误
- ✅ 防止整数溢出
- ✅ 自动内存管理，无内存泄漏

### 11.2 P0-6：返回值编码缺陷

**问题描述：**
- 使用 i64 编码，限制内存大小为 4GB（32 位指针）
- 错误码（返回 0）与正常返回值冲突
- 无法区分成功和错误返回

**修正方案：**

1. **使用 i128 编码**
   - 成功返回：正数，格式为 `(ptr << 64) | len`
   - 错误返回：负数，格式为 `-(error_code)`
   - 支持 64 位地址空间

2. **错误码分离**
   - 定义明确的错误码常量（`error_codes` 模块）
   - 错误码范围：1-255
   - 成功返回值为正数，错误返回值为负数

3. **编码/解码函数**
   - `encode_success(ptr, len)` - 编码成功返回
   - `encode_error(error_code)` - 编码错误返回
   - `decode_result(value)` - 解码返回值（返回 Result）

4. **向后兼容**
   - 保留旧版 `encode_ptr_len()` 和 `decode_ptr_len()` 函数
   - 标记为 `#[deprecated]`
   - 提供迁移路径

**修正效果：**
- ✅ 支持更大的地址空间（64 位）
- ✅ 明确的错误码分离
- ✅ 无错误码冲突
- ✅ 更好的错误处理

### 11.3 P0-7：内存泄漏风险

**问题描述：**
- 缺少内存使用监控
- 无法检测内存泄漏
- 长期运行可能耗尽内存

**修正方案：**

1. **内存统计**
   - 添加 `MemoryStats` 结构体记录：
     - 总分配次数
     - 总释放次数
     - 当前使用内存
     - 峰值使用内存
     - 分配失败次数
   - 在 `alloc()` 和 `reset()` 中记录统计信息

2. **全局内存管理器**
   - 添加 `MemoryManager` 单例
   - 追踪所有 Arena 实例（使用弱引用）
   - 提供 `generate_report()` 生成内存报告
   - 提供 `detect_leaks()` 检测内存泄漏

3. **自动注册/注销**
   - Arena 创建时自动注册到全局管理器
   - Arena 销毁时自动从全局管理器注销
   - 使用 `#[cfg(feature = "memory_tracking")]` 条件编译

4. **内存报告**
   - 提供 `MemoryReport` 结构体：
     - 总 Arena 数量
     - 活跃 Arena 数量
     - 泄漏 Arena 数量
     - 总内存容量
     - 已使用内存
     - 峰值内存使用
   - 提供计算方法：
     - `usage_ratio()` - 内存使用率
     - `allocation_success_rate()` - 分配成功率
     - `has_leaks()` - 是否存在泄漏

**修正效果：**
- ✅ 实时监控内存使用情况
- ✅ 自动检测内存泄漏
- ✅ 提供详细的内存报告
- ✅ 防止长期运行内存耗尽

### 11.4 修正前后对比

| 维度 | 修正前 | 修正后 |
|------|--------|--------|
| **内存安全** | 无版本控制，可能访问失效内存 | 版本控制 + 边界检查 + RAII |
| **地址空间** | 限制 4GB（32 位） | 支持 64 位地址空间 |
| **错误处理** | 返回 0 无法区分错误 | 明确的错误码分离 |
| **内存管理** | 手动管理，可能泄漏 | 自动管理 + 全局监控 |
| **可观测性** | 无监控 | 完整的统计和报告 |
| **安全性** | 多处 unsafe 无检查 | 最小化 unsafe + 验证 |

### 11.5 使用建议

1. **启用内存跟踪**
   ```toml
   [features]
   memory_tracking = []
   ```

2. **定期检查内存报告**
   ```rust
   let manager = MemoryManager::global();
   let report = manager.generate_report();
   if report.has_leaks() {
       log::warn!("检测到内存泄漏: {} 个 Arena", report.leaked_arenas);
   }
   ```

3. **监控内存使用率**
   ```rust
   if report.usage_ratio() > 0.8 {
       log::warn!("内存使用率过高: {:.2}%", report.usage_ratio() * 100.0);
   }
   ```

4. **定期清理失效引用**
   ```rust
   manager.cleanup();  // 清理已释放的 Arena 记录
   ```
