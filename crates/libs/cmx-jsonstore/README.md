# cmx-jsonstore

> 门户系列 crate 共用的 **JSON 文件存储基础设施**（原 cmx-portal-base）：数据根解析、统一错误、原子读写、moka L1 缓存与 xxhash64 内容锚点（rev）、ID 安全校验、写锁——页面/元数据「JSON 落盘不落库」的地基。

[![Version](https://img.shields.io/badge/version-0.1.12-blue.svg)]()
[![Edition](https://img.shields.io/badge/rust--edition-2024-orange.svg)]()
[![Authors](https://img.shields.io/badge/authors-skylake%40pansoft.com-lightgrey.svg)]()

---

## 项目简介

`cmx-jsonstore` 从 `cmx-portal` 下沉而来。下沉的根因：`cmx-portal`（含 agent）需依赖 `cmx-form` / `cmx-model`，而后两者又共用门户基础设施（数据根解析、错误、文件读写、缓存）；若基础设施留在 `cmx-portal`，将形成「cmx-portal → cmx-form/cmx-model → 基础设施」的**循环依赖**——独立成 base crate 即打破环。此后 `cmx-form` / `cmx-model-meta` / `cmx-biz` / `cmx-portal` 都直接依赖本 crate（`cmx-form` 还原样再导出，使迁移代码的 `crate::config` 路径无需改动）。

六个模块各司其职：

- **config**：数据根三级解析（`assets.root` 配置（toml `[assets]` 段）→ `ASSETS__ROOT` 环境变量 → `./data` 兜底），不做存在性校验——文件缺失在具体读写时再报 `NotFound`。
- **error**：`PortalError`（NotFound/BadRequest/Json/Io/Business）+ `PortalResult<T>`，并 `impl From<PortalError> for cmx_api_types::Error`——上层 HTTP handler 可直接 `?` 传播。
- **fsutil**：JSON/文本的**原子读写**——同目录临时文件（`<name>.tmp.<pid>.<nanos>`）+ fsync + rename，POSIX 原子替换；自动创建父目录；pretty 格式与 Node 后端落盘格式一致（便于 diff）。
- **cache**：进程内 moka L1 缓存（页面源码/索引文件）+ 内容版本锚点 `rev`（xxhash64 → 16 hex 小写）。
- **util**：ID/路径段安全校验（防注入与穿越）、进程级全局写锁、`resolve_within` 受限路径拼接。
- **time**：统一 epoch 毫秒时间戳（取值失败返 0，绝不 panic）。

两个关键设计：

1. **rev = xxhash64(source bytes) 截断 16 hex**：非密码学哈希，但 CMX 页面缓存属"非安全上下文"（服务端算、内部作者编辑、无对抗方、碰撞后果轻微=一次刷新即修）。rev 同时作 HTTP ETag 值与前端 IndexedDB 缓存校验锚点；`content_rev_with_meta` 另在源内容前前置行字段 canonical 串（固定字段序、`\u{1F}` 分隔、缺失/null 归一为空串），使行字段（坐标/名称等）变更而源码不变时 rev 也随之变化——前端缓存自愈，无需用户手动清站点数据。
2. **缓存开关与 TTL 的生效语义不同**：`cache_enabled()`（`assets.page_cache_enabled`，**缺省 false**）是运行时热读，改配置即生效；moka 实例的 TTL（`assets.page_cache_ttl_secs` 缺省 30s）与容量（`assets.page_cache_max_entries` 缺省 4096）在 `LazyLock` 首次访问时固定，**改配置需重启**（重建实例代价大且丢缓存，不值得）。

---

## 与其他 crate 的关系

### 上游依赖（本 crate 依赖）

| 依赖 | 用途 |
|------|------|
| `cmx-api-types` | `PortalError → cmx_api_types::Error` 映射（handler `?` 传播的关键） |
| `cmx-utils` | `ConfigManager` 读取 `assets.root` / `assets.page_cache_*` 配置 |
| `tokio` | 异步文件读写（tokio::fs）与并发锁 |
| `serde` / `serde_json` | JSON 文档序列化/反序列化 |
| `thiserror` | `PortalError` 错误派生 |
| `moka` | 高性能本地缓存（L1） |
| `xxhash-rust` | 非加密内容哈希（rev 计算） |

### 下游使用方（谁依赖本 crate）

| 使用方 | 引用方式 | 实际用途 |
|--------|---------|---------|
| `cmx-form` | workspace 依赖（+dev 启用 `testing`） | 三类页面存储的地基；再导出全部模块 |
| `cmx-model-meta` | workspace 依赖 | 元数据 JSON 文件读写 |
| `cmx-biz` | workspace 依赖 | `dam_asset_service.rs` 等经 `data_root` 定位数据目录 |
| `cmx-portalservice` / `cmx-portal`（跨 workspace） | path 引用（+dev 启用 `testing`） | 门户本体及其数据文件访问 |

---

## 核心功能与特性

| 功能 | 说明 |
|------|------|
| 数据根解析 | `data_root()`：配置 `assets.root`（toml `[assets]` 段）→ 环境变量 `ASSETS__ROOT` → `./data`；`data_path(segments)` 在根下拼路径 |
| 原子写 | `write_json_atomic`（可选 pretty）/ `write_text_atomic`：同目录临时文件 + fsync + rename，读侧永不看到半截文件 |
| 容错读 | `read_json<T>`（缺失→`NotFound`）/ `read_json_opt`（缺失→`None`）/ `read_text_opt`——「首次运行尚无数据」是正常态 |
| 统一错误 | `PortalError` 五类变体 + HTTP 状态映射语义（404/400/500）；`From<PortalError> for cmx_api_types::Error` |
| L1 缓存 | `cached_read_text` / `cached_read_json`：命中直返，未命中读盘回填；Text/Json 形态互转按需回填；开关关闭时全部穿透 |
| 写后失效 | `invalidate_path` / `invalidate_paths`（索引双写场景）/ `invalidate_all`（索引重建全清空，moka 同步方法） |
| rev 锚点 | `content_rev(bytes)`：xxhash64 → 16 hex；`content_rev_with_meta(fields, source)`：行字段 canonical + 源码同哈希（行字段变更也触发前端缓存失效重拉，缓存自愈）；`REV_LEN = 16`；保存后读比较均为 O(1) 字符串相等 |
| 集群一致性 | 跨节点各自 moka TTL（30s）收敛，不依赖即时广播；rev 实时算天然一致（秒级一致留远期 Redis pub-sub） |
| ID 校验 | `is_safe_id`（`[a-zA-Z0-9._-]{1,128}`）/ `is_safe_segment`（不含点）/ `is_safe_json_file` / `validate_id` |
| 路径防穿越 | `resolve_within(base, rel)`：去前导 `/` 与 `data/` 前缀、逐段拼接、拒绝 `..`/根/盘符 |
| 全局写锁 | `write_lock()`：单把进程级 tokio Mutex，串行化「文件 JSON 低频写」，实现简单不会死锁；热点可再拆细粒度 |
| 测试基建 | `#[cfg(any(test, feature = "testing"))] test_data_root_lock()`：串行化改 `ASSETS__ROOT` 的跨 crate 测试 |

---

## 模块结构

```text
cmx-jsonstore
├── src
│   ├── lib.rs     # 模块声明；顶层再导出 data_root / PortalError / PortalResult / now_millis
│   ├── config.rs  # data_root() 三级解析 + data_path(segments)
│   ├── error.rs   # PortalError（thiserror）+ PortalResult + From<PortalError> for cmx_api_types::Error
│   ├── fsutil.rs  # read_json / read_json_opt / read_text_opt / write_json_atomic / write_text_atomic（含单元测试）
│   ├── cache.rs   # REV_LEN / content_rev / cache_enabled / cached_read_* / invalidate_*（含单元测试）
│   ├── util.rs    # is_safe_id / is_safe_segment / is_safe_json_file / validate_id / write_lock / resolve_within / test_data_root_lock
│   └── time.rs    # now_millis()
└── Cargo.toml
```

---

## 关键类型 / API

```rust
// src/config.rs
pub fn data_root() -> PathBuf;                       // assets.root → ASSETS__ROOT → ./data
pub fn data_path<I, S>(segments: I) -> PathBuf
where I: IntoIterator<Item = S>, S: AsRef<Path>;     // data_path(["form-pages", "pages-list.json"])

// src/error.rs
pub enum PortalError { NotFound(String), BadRequest(String), Json(serde_json::Error),
                       Io(std::io::Error), Business(String) }
pub type PortalResult<T> = Result<T, PortalError>;

// src/fsutil.rs
pub async fn read_json<T: DeserializeOwned>(path: &Path) -> PortalResult<T>;
pub async fn read_json_opt(path: &Path) -> PortalResult<Option<serde_json::Value>>;
pub async fn read_text_opt(path: &Path) -> PortalResult<Option<String>>;
pub async fn write_json_atomic<T: Serialize>(path: &Path, value: &T, pretty: bool) -> PortalResult<()>;
pub async fn write_text_atomic(path: &Path, text: &str) -> PortalResult<()>;

// src/cache.rs
pub const REV_LEN: usize = 16;
pub fn cache_enabled() -> bool;                      // assets.page_cache_enabled，缺省 false（运行时热读）
pub fn content_rev(bytes: &[u8]) -> String;          // xxhash64 → 16 hex 小写
pub fn content_rev_with_meta(fields: &[&str], source: &str) -> String;
    // 行字段 canonical（\u{1F} 分隔、缺失/null 归一空串）+ 源码 → 16 hex；
    // html 全量读字段序 [domain,app,module,doc,name,details,rel_path]，
    // native 全量读字段序 [name,details,source_type,rel_path]（两侧读路径须一致）
pub async fn cached_read_text(path: &Path) -> PortalResult<Option<String>>;
pub async fn cached_read_json(path: &Path) -> PortalResult<Option<serde_json::Value>>;
pub async fn invalidate_path(path: &Path);
pub async fn invalidate_paths(paths: &[&Path]);
pub fn invalidate_all();

// src/util.rs
pub fn is_safe_id(s: &str) -> bool;                  // [a-zA-Z0-9._-]{1,128}
pub fn is_safe_segment(s: &str) -> bool;             // [a-zA-Z0-9_-]+（不含点）
pub fn is_safe_json_file(s: &str) -> bool;
pub fn validate_id(id: &str, field: &str) -> PortalResult<String>;
pub fn write_lock() -> &'static tokio::sync::Mutex<()>;
pub fn resolve_within(base: &Path, rel: &str) -> PortalResult<PathBuf>;
#[cfg(any(test, feature = "testing"))]
pub fn test_data_root_lock() -> &'static std::sync::Mutex<()>;

// src/time.rs
pub fn now_millis() -> i64;
```

---

## 使用示例

### 场景一：实现一个 JSON 索引存储（cmx-form 的真实模式）

```rust
use cmx_jsonstore::{config::data_path, error::PortalResult,
                    fsutil::{read_json_opt, write_json_atomic}, util::write_lock};

/// 读索引的 pages 数组（首次运行文件缺失 → 空，正常态）
async fn load_index() -> PortalResult<Vec<serde_json::Value>> {
    Ok(read_json_opt(&data_path(["my-pages", "index.json"])).await?
        .and_then(|d| d.get("pages").and_then(|p| p.as_array()).cloned())
        .unwrap_or_default())
}

/// upsert 一行：全局写锁串行化 + 原子写（临时文件 + rename）
async fn upsert_row(row: serde_json::Value) -> PortalResult<()> {
    let _guard = write_lock().lock().await;          // 并发写不覆盖
    let mut pages = load_index().await?;
    pages.push(row);
    write_json_atomic(                               // pretty 落盘，与 Node 格式一致便于 diff
        &data_path(["my-pages", "index.json"]),
        &serde_json::json!({ "version": 1, "pages": pages }), true).await
}
```

### 场景二：rev 锚点 + 缓存读（页面源码服务模式）

```rust
use cmx_jsonstore::cache::{cached_read_text, content_rev, invalidate_path};

async fn read_page(path: &std::path::Path) -> Option<(String, String)> {
    // 开关开启时命中 moka 直返；关闭时穿透读盘（行为等价加缓存前）
    let source = cached_read_text(path).await.ok()??;
    // rev 读时现算：同一份内容在任意节点算出同一 rev（ETag / 前端缓存校验锚点）
    let rev = content_rev(source.as_bytes());
    Some((source, rev))
}

async fn write_page(path: &std::path::Path, text: &str) -> PortalResult<()> {
    cmx_jsonstore::fsutil::write_text_atomic(path, text).await?;
    invalidate_path(path).await;                     // 失效本进程 L1；跨节点靠 TTL 30s 收敛
    Ok(())
}
```

### 场景三：ID 与路径校验（防注入/穿越）

```rust
use cmx_jsonstore::util::{validate_id, resolve_within};

// 整体 ID：拒绝空串、超长、非法字符（[a-zA-Z0-9._-]{1,128} 之外）
let id = validate_id("fi.cmxfico.gl.voucher", "页面 ID")?;
assert!(validate_id("../etc/passwd", "ID").is_err());

// 受限拼接：去前导 / 与 data/ 前缀；拒绝 .. / 绝对路径 / 盘符
let base = cmx_jsonstore::data_root();
let p = resolve_within(&base, "data/html-pages/sources/fi/app.html")?;
assert!(resolve_within(&base, "../../etc/passwd").is_err());
```

---

## Features

| Feature | 说明 |
|---------|------|
| `testing` | 向下游 crate 的**单元测试**暴露 `util::test_data_root_lock()`——串行化会改写 `ASSETS__ROOT` 环境变量的跨 crate 测试，避免数据根互踩。仅由下游 `[dev-dependencies]` 启用（如 `cmx-jsonstore = { workspace = true, features = ["testing"] }`），正常构建不进入产物。 |
