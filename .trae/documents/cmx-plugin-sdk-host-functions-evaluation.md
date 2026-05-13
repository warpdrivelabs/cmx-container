# cmx-plugin-sdk 宿主函数企业级评估报告

**评估日期**: 2026-04-29
**评估人**: 资深 Rust 架构师
**评估对象**: cmx-plugin-sdk 中各 crate 的 host_functions.rs

---

## 一、现有宿主函数概览

### 1.1 函数清单

| 命名空间 | 函数名 | 功能描述 | 优先级 |
|----------|--------|----------|--------|
| `cmx:database` | `db_query` | 数据库查询 | P0 |
| `cmx:database` | `db_execute` | 数据库增删改 | P0 |
| `cmx:buffer` | `cache_get` | 缓存读取 | P0 |
| `cmx:buffer` | `cache_set` | 缓存写入 | P0 |
| `cmx:buffer` | `cache_delete` | 缓存删除 | P1 |
| `cmx:plugin` | `call_service` | 插件间调用 | P0 |
| `cmx:plugin` | `get_info` | 获取插件信息 | P1 |
| `cmx:log` | `log_info/error/debug/warn` | 日志记录 | P0 |

### 1.2 当前架构设计

```
WASM Plugin (PDK)
        │
        ▼
Extism Runtime (host_function_wrapper)
        │
        ▼
HostFunctionProvider (trait)
        ├── DatabaseHostFunctions
        ├── BufferHostFunctions
        ├── PluginHostFunctions
        └── LoggingHostFunctions
```

**设计优点**：

- 良好的解耦：通过 `HostFunctionProvider` trait 实现，业务模块不依赖 Extism
- 命名空间隔离：使用 `cmx:xxx` 格式避免冲突
- JSON 编解码：统一的请求/响应格式

---

## 二、WASI 能力评估

### 2.1 WASI 标准能力 ✅ 已具备

如果你的 WASM 插件编译为 **WASI (WebAssembly System Interface)** 目标，则以下能力**已由 WASI 标准提供**，**无需通过宿主函数实现**：

| 能力 | WASI 接口 | 说明 |
|------|-----------|------|
| **文件访问** | `wasi:filesystem/*` | 读取、写入、创建、删除文件 |
| **网络访问** | `wasi:sockets/*` | TCP/UDP 连接、HTTP 请求 |
| **随机数** | `wasi:random/*` | 安全随机数生成 |
| **时间** | `wasi:clocks/*` | 系统时间获取 |
| **环境变量** | `wasi:env/*` | 读取环境变量 |
| **标准输出/错误** | `wasi:stdout/stderr` | 日志输出 |

### 2.2 架构建议：宿主函数 vs WASI 原生

```
┌─────────────────────────────────────────────────────────┐
│                    WASM Plugin                          │
│  ┌─────────────────┐    ┌─────────────────────────────┐ │
│  │  WASI 原生能力   │    │    宿主函数 (cmx:xxx)       │ │
│  │  ✅ 文件/网络    │    │  ✅ 数据库/缓存/插件调用     │ │
│  │  ✅ 随机数/时间  │    │  ⚠️ 需要特权控制的能力       │ │
│  │  ✅ 环境变量     │    │  ⚠️ 敏感信息（密钥）         │ │
│  └─────────────────┘    └─────────────────────────────┘ │
└─────────────────────────────────────────────────────────┘
```

**建议**：

- **使用 WASI 原生能力**：文件操作、网络请求（HTTP）、随机数等
- **使用宿主函数**：数据库操作、缓存操作、插件间通信、密钥管理、权限校验

### 2.3 安全考虑 ⚠️

WASI 提供了强大的系统接口能力，但在**不受信任的插件场景**下需要注意：

| 风险 | 说明 | 建议 |
|------|------|------|
| 文件系统访问 | 插件可能访问敏感文件 | 配置 WASI filesystem 沙箱，只允许访问特定目录 |
| 网络访问 | 插件可能发起大量网络请求 | 配置网络限制（允许列表） |
| 资源耗尽 | 插件可能创建大量文件/连接 | 设置资源配额 |

**如果插件是可信的**（如内部业务插件），可以直接使用 WASI 原生能力。

---

## 三、企业级需求差距分析

### 3.1 高可用性 (High Availability)

#### 当前状态 ❌ 不满足

| 需求项 | 当前实现 | 差距分析 |
|--------|----------|----------|
| 缓存高可用 | 仅支持单实例 Redis | **缺失**：无集群支持、无故障转移 |

#### 建议补充


---

### 3.2 安全性 (Security)

#### 当前状态 ⚠️ 部分满足

| 安全维度 | 当前实现 | 差距分析 |
|----------|----------|----------|
| 插件隔离 | `plugin:{plugin_id}:` 前缀 | ⚠️ **不完整**：`get_info` 硬编码 "default" 值 |

#### 关键问题

**问题 1：`BufferHostFunctions` 硬编码 plugin_id**

```rust
// cmx-buffer/src/host_functions.rs:38
let full_key = Self::build_key("default", &req.key);
// ❌ 问题：所有插件使用 "default" 前缀，无法实现插件隔离
```

**问题 2：`PluginInfoResponse` 硬编码响应值**

```rust
// cmx-plugin/src/host_functions.rs:75-81
let info = PluginInfoResponse {
    plugin_id: "current_plugin".to_string(),  // ❌ 硬编码
    db_id: "default".to_string(),             // ❌ 硬编码
    txn_id: None,
    request_id: "default".to_string(),        // ❌ 硬编码
    tenant_id: None,
};
```

#### 建议改造


---

### 3.3 扩展性 (Extensibility)

#### 当前状态 ⚠️ 基本满足

| 需求项 | 当前实现 | 差距分析 |
|--------|----------|----------|
| 新增宿主函数 | 实现 `HostFunctionProvider` | ✅ 可扩展，但需修改 runtime |
| 多数据库支持 | `DatabaseManager` 支持 | ✅ 已实现 |
| 自定义缓存策略 | 无 | **缺失** |
| 插件生命周期钩子 | 无 | **缺失** |

#### 关键问题

**问题：宿主函数参数过于简单**

当前 `DbRequest.params` 使用 `Option<JsonValue>`，这导致：
- 无法支持命名参数（如 `$name` vs `$1`）
- 无法预编译语句缓存
- 参数类型不明确


---

### 3.4 事务支持 (Transaction Support)

#### 当前状态 ⚠️ 有基础但不完善

| 需求项 | 当前实现 | 差距分析 |
|--------|----------|----------|
| 事务传播 | `txn_id` 字段 | ⚠️ **不足**：WASM 无法主动开启/提交事务 |

#### 建议补充

```rust
// 建议：显式事务管理宿主函数
#[host_fn("cmx:database")]
extern "ExtismHost" {
    fn txn_begin(db_id: String) -> String;      // ⭐ 开启事务
    fn txn_commit(txn_id: String) -> String;       // ⭐ 提交事务
    fn txn_rollback(txn_id: String) -> String;    // ⭐ 回滚事务
}
```

---

## 四、缓存类型设计改造 ⭐

### 4.1 问题描述

**原设计问题**：`CacheSetRequest` 和 `CacheResponse` 的 `value` 字段固定为 `String` 类型，无法灵活存储任意 JSON 数据。

```rust
// ❌ 原设计 - value 只能是字符串
pub struct CacheSetRequest {
    pub key: String,
    pub value: String,  // ❌ 无法存储对象、数组、数字等 JSON 类型
    pub ttl_seconds: Option<u64>,
}
```

### 4.2 改造方案 ✅ 已实施

**改造后**：使用 `serde_json::Value` 支持任意 JSON 类型

```rust
// ✅ 改造后 - value 可以是任意 JSON 类型
pub struct CacheSetRequest {
    pub key: String,
    pub value: serde_json::Value,  // ✅ 支持 String, Number, Object, Array, Bool, Null
    pub ttl_seconds: Option<u64>,
}

pub struct CacheResponse {
    pub success: bool,
    pub value: Option<serde_json::Value>,  // ✅ 读取时返回任意 JSON 类型
    pub exists: Option<bool>,
    pub error: Option<String>,
}
```

### 4.3 使用示例

```rust
// 写入各种 JSON 类型
let set_req = CacheSetRequest {
    key: "user:123".to_string(),
    value: serde_json::json!({
        "name": "张三",
        "age": 30,
        "scores": [95, 87, 92],
        "active": true
    }),
    ttl_seconds: Some(3600),
};

// 读取后直接使用，无需手动解析
let resp: CacheResponse = serde_json::from_str(&result)?;
if let Some(value) = resp.value {
    if let Some(name) = value.get("name").and_then(|v| v.as_str()) {
        println!("用户名称: {}", name);
    }
}
```

### 4.4 优势

| 优势 | 说明 |
|------|------|
| **类型丰富** | 支持对象、数组、数字、布尔值等所有 JSON 类型 |
| **减少序列化开销** | 写入时直接传递 `serde_json::Value`，无需手动 `to_string()` |
| **类型安全** | 宿主函数统一处理 JSON 序列化，SDK 端直接使用 JSON 类型 |
| **向后兼容** | 字符串值仍然有效（会被包装为 `Value::String`） |

---

## 五、功能完善建议

### 5.1 缺失的关键功能

| 功能 | 优先级 | 说明 | WASI 原生支持 |
|------|--------|------|--------------|
| **HTTP 客户端** | P0 | 插件需要调用外部 API | ⚠️ 可用 WASI sockets，但无 HTTP 封装 |
| **消息队列** | P1 | 异步任务处理 | ❌ 不支持 |
| **文件操作** | P1 | 插件需要读写文件 | ✅ **已支持**（WASI filesystem） |
| **加密/解密** | P1 | 敏感数据处理 | ❌ 不支持 |
| **配置获取** | P0 | 获取插件配置 | ❌ 不支持 |
| **密钥管理** | P0 | 安全的密钥存储和访问 | ❌ 不支持 |
| **服务发现** | P1 | 发现其他插件服务 | ❌ 不支持 |
| **限流器** | P1 | 防止资源耗尽 | ❌ 不支持 |

### 5.2 建议新增宿主函数

#### P0 - 核心必需

```rust
// 1. HTTP 客户端（外部 API 调用）
#[host_fn("cmx:http")]
extern "ExtismHost" {
    fn http_request(request: HttpRequest) -> String;
}

pub struct HttpRequest {
    pub method: String,           // GET/POST/PUT/DELETE
    pub url: String,
    pub headers: HashMap<String, String>,
    pub body: Option<String>,
    pub timeout_ms: Option<u64>,
}

// 2. 配置获取
#[host_fn("cmx:config")]
extern "ExtismHost" {
    fn get_config(key: String) -> String;
    fn get_secret(key: String) -> String;  // ⭐ 安全获取密钥
}

// 3. 服务发现
#[host_fn("cmx:discovery")]
extern "ExtismHost" {
    fn find_service(service_name: String) -> String;
    fn list_services() -> String;
}
```

#### P1 - 重要功能

```rust
// 4. 消息队列
#[host_fn("cmx:mq")]
extern "ExtismHost" {
    fn publish(topic: String, message: String) -> String;
    fn subscribe(topic: String) -> String;
}

// 5. 加密操作
#[host_fn("cmx:crypto")]
extern "ExtismHost" {
    fn encrypt(algorithm: String, data: String, key_ref: String) -> String;
    fn decrypt(algorithm: String, data: String, key_ref: String) -> String;
    fn hash(algorithm: String, data: String) -> String;
}

// 6. 限流
#[host_fn("cmx:rate_limit")]
extern "ExtismHost" {
    fn acquire(key: String, permits: u32, timeout_ms: u64) -> String;
    fn release(key: String, permits: u32) -> ();
}
```

> **注意**：文件操作（`read_file`, `write_file` 等）**无需实现为宿主函数**，WASM 插件可以直接使用 WASI filesystem 接口。

---

## 六、参数设计问题与建议

### 6.1 当前参数设计问题

#### 问题 1：PluginInfoResponse 硬编码

```rust
// cmx-plugin/src/host_functions.rs:74-83
fn do_get_info(&self, _input: String) -> Result<String, HostFuncError> {
    let info = PluginInfoResponse {
        plugin_id: "current_plugin".to_string(),  // ❌ 硬编码
        db_id: "default".to_string(),             // ❌ 硬编码
        txn_id: None,
        request_id: "default".to_string(),        // ❌ 硬编码
        tenant_id: None,
    };
    // ❌ 无法获取真实运行时上下文
}
```

**影响**：WASM 插件无法获取自身的真实 `plugin_id`、`tenant_id` 等信息。

#### 问题 2：BufferHostFunctions 硬编码 plugin_id

```rust
// cmx-buffer/src/host_functions.rs:38
let full_key = Self::build_key("default", &req.key);
```

**影响**：所有插件的缓存键都使用 `plugin:default:` 前缀，无法实现插件间隔离。

#### 问题 3：CacheSetRequest 缺少必要参数（已修复 value 类型）

```rust
// cmx-core/src/wasm_types/cache.rs ✅ 已改造
pub struct CacheSetRequest {
    pub key: String,
    pub value: serde_json::Value,  // ✅ 已改为 serde_json::Value
    pub ttl_seconds: Option<u64>,
    // ❌ 仍然缺少：条件写入（NX）、版本控制
}
```

### 6.2 参数设计建议

```rust
// 建议：完整的请求上下文
pub struct WasmCallContext {
    /// 插件ID
    pub plugin_id: String,
    /// 租户ID
    pub tenant_id: Option<String>,
    /// 请求ID（用于追踪）
    pub request_id: String,
    /// 事务ID
    pub txn_id: Option<String>,
    /// 数据库ID
    pub db_id: String,
    /// 调用深度
    pub depth: u32,
    /// 时间戳
    pub timestamp: i64,
}

// 建议：增强型缓存请求
pub struct CacheSetRequest {
    pub key: String,
    pub value: serde_json::Value,
    pub ttl_seconds: Option<u64>,
    pub condition: Option<CacheCondition>,  // ⭐ 新增：写入条件
    pub version: Option<u64>,                // ⭐ 新增：乐观锁版本
}

pub enum CacheCondition {
    /// 仅当不存在时写入
    NX,
    /// 仅当存在时写入
    XX,
    /// 仅当版本匹配时写入
    VersionMatch(u64),
}

// 建议：数据库查询增强
pub struct DbRequest {
    pub sql: String,
    pub params: Option<Vec<ParamValue>>,
    pub dataset_id: Option<String>,
    pub db_id: Option<String>,
    pub txn_id: Option<String>,
    pub timeout_ms: Option<u64>,              // ⭐ 新增
    pub retry_count: Option<u32>,             // ⭐ 新增
    pub prepared_statement: Option<String>,   // ⭐ 新增
    pub consistency: Option<QueryConsistency>, // ⭐ 新增：查询一致性级别
}

pub enum QueryConsistency {
    Strong,      // 强一致性读
    Eventual,    // 最终一致性读（更快）
}
```

---

## 七、改造优先级与实施计划

### 7.1 第一阶段：安全性修复（P0）

| 任务 | 工作量 | 说明 |
|------|--------|------|
| 修复 `get_info` 硬编码问题 | 0.5d | 传递真实运行时上下文 |
| 修复 `cache_*` plugin_id 隔离 | 0.5d | 传递真实 plugin_id |
| 集成 `PermissionManager` 校验 | 1d | 宿主函数调用前校验权限 |
| SQL 注入防护增强 | 1d | 添加 SQL 语法检查 |

### 7.2 第二阶段：高可用性增强（P0）

| 任务 | 工作量 | 说明 |
|------|--------|------|
| 添加数据库操作超时参数 | 0.5d | 防止慢查询阻塞 |
| 添加缓存操作超时参数 | 0.5d | 防止 Redis 阻塞 |
| 实现重试机制 | 1d | 指数退避重试 |
| 添加熔断机制 | 2d | 防止级联故障 |

### 7.3 第三阶段：新增核心功能（P1）

| 任务 | 工作量 | 说明 |
|------|--------|------|
| HTTP 客户端宿主函数 | 2d | 外部 API 调用 |
| 配置获取宿主函数 | 1d | 插件配置 |
| 密钥管理宿主函数 | 2d | 安全密钥访问 |
| 事务管理宿主函数 | 2d | 显式事务控制 |

### 7.4 第四阶段：可观测性完善（P1）

| 任务 | 工作量 | 说明 |
|------|--------|------|
| 结构化审计日志 | 1d | 敏感操作记录 |
| 宿主函数指标采集 | 1d | 监控告警 |
| 调用链追踪集成 | 2d | span 关联 |

---

## 八、总结

### 8.1 当前评估

| 维度 | 评分 | 说明 |
|------|------|------|
| 功能完整性 | 7/10 | 基础功能具备，**缓存类型已优化**，核心功能缺失 |
| 安全性 | 5/10 | 有基础但硬编码问题严重 |
| 高可用性 | 4/10 | 无超时、重试、熔断机制 |
| 可扩展性 | 7/10 | 架构设计良好 |
| 可观测性 | 5/10 | 基础日志有，结构化不足 |
| WASI 兼容性 | 9/10 | **文件/网络等基础能力已由 WASI 提供** |

### 8.2 核心问题

1. **硬编码问题**：`get_info` 和 `cache_*` 函数存在硬编码，无法满足企业级多租户需求
2. **安全缺失**：权限校验未与宿主函数集成
3. **超时缺失**：数据库和缓存操作无独立超时控制
4. **功能缺失**：缺少 HTTP、配置、密钥管理等核心企业功能

### 8.3 WASI 能力说明 ⭐

如果你的 WASM 插件编译为 **WASI 目标**，则以下能力**已原生支持**，**无需通过宿主函数实现**：

- ✅ **文件操作**（`wasi:filesystem`）
- ✅ **网络访问**（`wasi:sockets`）
- ✅ **随机数**（`wasi:random`）
- ✅ **时间获取**（`wasi:clocks`）
- ✅ **环境变量**（`wasi:env`）

### 8.4 建议

1. **立即修复**：硬编码问题（1-2天）
2. **短期完善**：超时、重试、权限集成（1周）
3. **中期扩展**：新增 HTTP、配置、密钥管理等（2周）
4. **长期演进**：完善可观测性、限流熔断等（持续）

---

**报告完毕**
