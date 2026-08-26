# cmx-form

> 表单中心：门户三类前端页面资源（表单页 form / 设计器 HTML 页 / 原生页 native）的 JSON 文件存储与读写服务，数据落在 `data/{form,html,native}-pages/**` 目录、不进数据库；另含 `serve` 模块——引擎微服务自投递 native/html 页的通用 axum 只读路由（收编五份引擎副本）。

[![Version](https://img.shields.io/badge/version-0.1.12-blue.svg)]()
[![Edition](https://img.shields.io/badge/rust--edition-2024-orange.svg)]()
[![Authors](https://img.shields.io/badge/authors-skylake%40pansoft.com-lightgrey.svg)]()

---

## 项目简介

`cmx-form` 承接「html_pages 相关」页面资源服务，迁移自 CMXHTMLDesigner / CMXPortalManager 的 Node 后端（`lib/formPagesStore.js`、`lib/htmlPagesStore.js`、`lib/nativePagesStore.js`），用 Rust 复刻了与 Node 版字节兼容的存储行为。CMX 门户的前端页面体系分三类，分别对应三个存储模块：

| 类型 | 目录 | 形态 |
|------|------|------|
| 表单页（form） | `data/form-pages/` | `pages-list.json` 索引 + `sources/<id>_<ts>.json` 版本化文档（内容 `{ "form": "<CMX 表单 JSON>" }`） |
| 设计器 HTML 页（html） | `data/html-pages/` | v2 分片（`index.json` 域清单 + `index/<domain>.pages.json` 分片 + `sources/<ns>.html`）+ v1 扁平兼容（`pages-list.json`） |
| 原生页（native） | `data/native-pages/` | `index.json` 索引 + `sources/<relPath>` 源文件（js/html） |

设计要点：

- **不落库**：全部 JSON 文件存储，数据根经 `cmx_jsonstore::config` 三级解析（toml `[assets]` 段 `assets.root` → `ASSETS__ROOT` → `./data`）。
- **版本化/兼容性**：form 页保存即写带时间戳的新版本文件（历史版本留存）；html 页读优先 v2 分片、回退 v1 列表，写双写保证 list 立即可见。
- **命名空间**：html/native 页 id 为点分 `domain.app.module.page`（2-4 段），源文件按命名空间分层存放；无点的旧式 id 归 `_legacy` 域。
- **rev 内容锚点 + 行字段自愈（C7）**：native/html 页响应带 `rev`（xxhash64 → 16 hex，读时现算"方案2"），作 HTTP ETag 与前端 IndexedDB 缓存校验锚点；rev 不只哈希源码内容，还并入行字段 canonical（native：name/details/sourceType/relPath；html：domain/app/module/doc/name/details/relPath），服务端只改坐标不动源码也能让前端缓存失效重拉，坐标随缓存自愈传播；配合 batch 端点的 `clientRevs` 差异同步协议，前端只拉内容有变的页面。
- **基础设施再导出**：`pub use cmx_jsonstore::{cache, config, error, fsutil, util}`——从 cmx-portal 拆出时保持被移动代码里的 `crate::config` / `crate::error` 等路径无需改动即可解析。

`pages` 模块提供门户本地存储函数（HTTP 端点由 cmx-portalservice 侧的 `cmx-common-api` portal handlers 暴露）；`serve` 模块提供引擎侧只读投递的 axum 路由装配（含 F3-save 写路径），五个引擎微服务（flow / report / rule / mdm / model）直接挂载。

---

## 与其他 crate 的关系

### 上游依赖（本 crate 依赖）

| 依赖 | 用途 |
|------|------|
| `cmx-jsonstore` | 共享基础设施（config / error / fsutil / cache / util），本 crate 原样再导出 |
| `cmx-api-types` | serve 成功体统一信封 `ApiResp` + 默认错误映射 `Error`（mdm/model/rpt 用） |
| `cmx-utils` | serve 目录解析的 ConfigManager 兜底（`[assets]` 段） |
| `axum` | serve 只读投递路由（native/html 六端点） |
| `tokio` | 异步文件读写（tokio::fs）+ 写锁（tokio::sync::Mutex） |
| `serde` / `serde_json` | 入参/出参与 JSON 文档序列化 |
| `tracing` | serve 索引/分片解析失败降级告警 |
| `thiserror` | serve `PageServeError` 派生 |
| `chrono` | form 页版本文件名时间戳（`chrono::Local::now`，与 Node 一致） |

dev-dependencies：`cmx-jsonstore`（启用 `testing` feature，串行化改 `ASSETS__ROOT` 的测试）+ `tempfile`（临时数据根）。

### 下游使用方（谁依赖本 crate）

| 使用方 | 引用方式 | 实际用途 |
|--------|---------|---------|
| `cmx-portalservice`（跨 workspace） | 根 Cargo.toml path 引用 `../cmx-container/crates/libs/cmx-form` | 门户服务编译进页面存储 |
| `cmx-portal`（cmx-portalservice 内） | workspace 依赖 | `pub use cmx_form::pages;` 对外 re-export；agent 工具（`agent/tools/read.rs`）调 `list_html_pages_paged` / `get_html_page_by_id` 读页面 |
| `cmx-common-api`（cmx-portalservice 内） | 经 cmx-portal | portal handlers 的 pages_routes：`/form-pages`、`/form-pages/{id}`、`/native-pages`（+`/batch`、`/{id}`）、`/html-pages`（+`/batch`、`/{id}`），handler 调用形如 `cmx_portal::pages::form::list_form_pages_paged(...)` |
| 五个引擎微服务（跨 workspace） | `cmx-flowengine/cmx-flow-server`、`cmx-rulesengine/cmx-rule-server`、`cmx-mdm/cmx-mdm-server`、`cmx-model/cmx-model-server`、`cmx-report/cmx-rpt-server` | `main.rs` 挂载 `cmx_form::serve::frontend_pages_routes::<(), E>(PageServeConfig::from_assets())` 自投递 `web/ui-native` / `web/ui-html` 页面；错误体 `E` 各引擎自持（flow/rule 传 FlowError/RuleError → `{code:1/4}`，mdm/model/rpt 传 `cmx_api_types::Error` → `{code:400/404}`）；rule 引擎 `HtmlLayout::Disabled` 不注册 html 三端点 |

---

## 核心功能与特性

| 功能 | 说明 |
|------|------|
| form 列表/保存/读取 | `list_form_pages_paged`（分页摘要）/ `save_form_page`（版本化写 + 索引 upsert）/ `get_form_page_by_id`（读最新版本，basename 防穿越） |
| html 命名空间解析 | `parse_page_namespace`：单段归 `_legacy` 域；2-4 段 `domain[.app[.module]].page`；逐段校验（safe-segment、禁空段） |
| html 列表（多维过滤） | `list_html_pages_paged(page, page_size, f_domain, f_app, f_module, f_keyword)`：keyword 对 id/name/details 不区分大小写包含 |
| html v2/v1 双轨 | 读优先分片回退 v1；写双写（分片 upsert + 顶层域清单 + v1 列表合并），保证 list 立即可见 |
| native 读写 | `list_native_pages_paged`（索引项原样）/ `get_native_page_by_id` / `save_native_page`（写源文件 + 索引合并 upsert，与 Node 的 `{...old,...row}` 一致） |
| 批量 + 差异同步 | `get_html_pages_by_ids` / `get_native_pages_by_ids`：单批上限 `MAX_BATCH = 64`；请求体可选 `clientRevs: { id → rev }`，rev 命中则省略 body；返回 `{ pages, revs, errors }`，单条失败不阻断 |
| rev 实时计算（含行字段） | "方案2"：rev 不写入索引行，读路径基于已读 source + 行字段 canonical 现算 xxhash64（`content_rev_with_meta`）——索引保持纯净，跨节点天然一致；行字段变更（改名/改坐标）亦触发前端缓存失效自愈（C7） |
| 原子写 + 写锁 | 全部落盘经 `write_json_atomic` / `write_text_atomic`（临时文件 + rename）；写路径持全局 `write_lock` 串行化，避免并发覆盖 |
| 写后缓存失效 | `save_*` 后调 `invalidate_paths` 失效本进程 moka L1 缓存（源文件 + 索引文件） |
| 防穿越校验 | relPath 拒绝绝对路径/反斜杠/`..` 段，扩展名白名单 js/mjs/html/htm；form 页 `latestFormFile` 仅取 basename |
| serve 只读投递 | `frontend_pages_routes::<S, E>(PageServeConfig)`：native 三端点（`GET /native-pages` 分页列表 / `POST /native-pages/batch` 批量取源码 / `GET /native-pages/{id}`）+ html 三端点（`GET/POST /html-pages`、`POST /html-pages/batch`、`GET /html-pages/{id}`，按 `HtmlLayout` 开关）；每请求同步读盘无进程内状态，索引缺失降级空集、解析失败补 warn；契约与门户 cmx-common-api 前缀一致，供门户 F3 反代 |
| serve 写路径（F3-save） | `POST /html-pages` upsert：设计器保存业务域页时门户按 id 归属反代到属主引擎，引擎把源文件 + v2 分片行写进自己的 assets 工作区；行字段三级回退（显式入参 > 既有行 > id 命名空间推导），`relPath` 取既有行优先（更新不迁移源文件），显式改域时从旧分片摘除防双写 |
| serve 目录规范（v2） | relPath 一律相对索引文件所在目录，加载器不感知布局约定；`PageServeConfig::from_assets()` 按统一 `[assets]` 段解析（`assets.ui_native_dir`/`ASSETS__UI_NATIVE_DIR` 默认 `web/ui-native`；`assets.ui_html_dir`/`ASSETS__UI_HTML_DIR` 默认 `web/ui-html`） |

---

## 模块结构

```text
cmx-form
├── src
│   ├── lib.rs          # 基础设施再导出（cmx_jsonstore::{cache,config,error,fsutil,util}）+ pub mod pages / serve
│   ├── pages
│   │   ├── mod.rs      # 页面资源模块声明（form / html / native）
│   │   ├── form.rs     # 表单页存储：列表索引 + 版本化文档（复刻 Node formPagesStore.js）
│   │   ├── html.rs     # 设计器 HTML 页：v2 分片 + v1 兼容 + 命名空间解析 + batch 差异同步（复刻 htmlPagesStore.js）
│   │   └── native.rs   # 原生页存储：索引 + js/html 源文件（复刻 nativePagesStore.js）
│   └── serve           # 引擎侧页面只读投递（收编五份引擎副本，与门户 F3 反代契约对齐）
│       ├── mod.rs      # 模块文档 + 再导出（PageServeConfig / HtmlLayout / PageServeError / frontend_pages_routes）
│       ├── config.rs   # 投递配置：[assets] 段目录解析 + HtmlLayout 开关 + 资产目录规范 v2
│       ├── error.rs    # 中性错误 PageServeError（BadRequest/NotFound/Io），泛型 E 折入各引擎历史错误体
│       ├── loader.rs   # 索引/v2 分片读取 + 安全拼接 + 源码装载 + rev 计算（含行字段）
│       ├── routes.rs   # 六端点 axum 路由装配（成功体统一 ApiResp，错误体由 E 渲染）
│       ├── save.rs     # F3-save：html 页 upsert（行字段三级回退 + 跨分片挪移防双写）
│       └── tests.rs    # 端点级集成测试（临时资产根）
└── Cargo.toml
```

---

## 关键类型 / API

```rust
// src/lib.rs —— 基础设施再导出（pages 内的 crate::config 等路径因此可用）
pub use cmx_jsonstore::{cache, config, error, fsutil, util};
pub mod pages;

// src/pages/form.rs —— 表单页
pub struct FormPageSummary { pub id: String, pub name: String, pub details: String }
pub struct FormPageInput {
    pub id: String,                       // 仅允许 [a-zA-Z0-9._-]{1,128}
    pub name: Option<String>,
    pub details: Option<String>,
    pub form: Option<String>,             // CMX 表单 JSON 字符串（必填）
}
pub async fn list_form_pages_paged(page: Option<i64>, page_size: Option<i64>) -> PortalResult<Value>;
    // 返回 { items, total, page, pageSize }；page 缺省 1，size 缺省 20（clamp 1-200）
pub async fn save_form_page(input: FormPageInput) -> PortalResult<Value>;
    // 写 form-pages/sources/<id>_<yyyyMMdd>_<HHmmssSSS>.json + pages-list.json 索引 upsert
pub async fn get_form_page_by_id(id: &str) -> PortalResult<Value>;

// src/pages/html.rs —— 设计器 HTML 页
pub struct PageNamespace {
    pub id: String, pub domain: String, pub app: String, pub module: String,
    pub page: String, pub rel_path: String, pub is_legacy: bool,
}
pub fn parse_page_namespace(id: &str) -> PortalResult<PageNamespace>;
pub struct HtmlPageInput {
    pub id: String, pub name: Option<String>, pub details: Option<String>,
    pub html: Option<String>,             // 源码（必填）
    pub domain: Option<String>, pub app: Option<String>,
    pub module: Option<String>, pub doc: Option<String>,
}
pub async fn list_html_pages_paged(
    page: Option<i64>, page_size: Option<i64>,
    f_domain: Option<&str>, f_app: Option<&str>, f_module: Option<&str>, f_keyword: Option<&str>,
) -> PortalResult<Value>;
pub async fn save_html_page(input: HtmlPageInput) -> PortalResult<Value>;
pub async fn get_html_page_by_id(id: &str) -> PortalResult<Value>;
pub async fn get_html_pages_by_ids(body: &serde_json::Value) -> PortalResult<Value>;

// src/pages/native.rs —— 原生页
pub struct NativePageInput {
    pub id: String, pub name: Option<String>, pub details: Option<String>,
    pub source_type: Option<String>,      // serde rename = "sourceType"（js/html）
    pub source: Option<String>,           // 源码（必填）
    pub rel_path: Option<String>,         // serde rename = "relPath"
}
pub struct NativePageFull {
    pub id: String, pub name: String, pub details: String,
    pub source_type: String,              // serde rename = "sourceType"
    pub rel_path: String,                 // serde rename = "relPath"
    pub rev: String,                      // xxhash64 → 16 hex（读时现算）
    pub source: String,
}
pub async fn list_native_pages_paged(page: Option<i64>, page_size: Option<i64>) -> PortalResult<Value>;
pub async fn get_native_page_by_id(id: &str) -> PortalResult<NativePageFull>;
pub async fn get_native_pages_by_ids(body: &serde_json::Value) -> PortalResult<Value>;
pub async fn save_native_page(input: NativePageInput) -> PortalResult<Value>;

// src/serve —— 引擎侧只读投递（各引擎 main.rs 挂载）
pub enum HtmlLayout { Disabled, ShardedV2 }          // html 三端点开关
pub struct PageServeConfig {
    pub native_dir: PathBuf,                          // index.json 所在目录（relPath 相对此）
    pub html_dir: PathBuf,                            // manifest index.json 所在目录
    pub html: HtmlLayout,
}
impl PageServeConfig { pub fn from_assets() -> Self; } // ConfigManager → env → 默认值三级解析
pub enum PageServeError { BadRequest(String), NotFound(String), Io(String) }
    // 实现 From<PageServeError> for cmx_api_types::Error（bad_request/not_found/internal_error）；
    // flow/rule 侧另实现 From<PageServeError> for FlowError/RuleError
pub fn frontend_pages_routes<S, E>(cfg: PageServeConfig) -> axum::Router<S>
where
    S: Clone + Send + Sync + 'static,
    E: IntoResponse + From<PageServeError> + 'static;
    // 挂 /api 下；成功体统一 cmx_api_types::ApiResp，错误体由 E 决定
```

---

## 使用示例

### 场景一：HTTP handler 暴露表单页端点（真实用法，参考 `cmx-common-api` 的 pages_routes）

```rust
use axum::{routing::{get, post}, Router};
use cmx_form::pages::form;

// /api/form-pages 两端点（真实 handler 模式，简化呈现）：
async fn list(Query(q): Query<PageQuery>) -> Result<Json<Value>, PortalError> {
    // q.page / q.page_size 由 query 透传；缺省 1 / 20（内部 clamp 1-200）
    Ok(Json(form::list_form_pages_paged(q.page, q.page_size).await?))
}
async fn save(Json(input): Json<form::FormPageInput>) -> Result<Json<Value>, PortalError> {
    // 写带时间戳版本文件 + 索引 upsert，返回新列表行（含 latestFormFile）
    Ok(Json(form::save_form_page(input).await?))
}

let router: Router<()> = Router::new()
    .route("/form-pages", get(list).post(save))
    .route("/form-pages/{id}", get(|Path(id): Path<String>| async move {
        Ok(Json(form::get_form_page_by_id(&id).await?))
    }));
```

### 场景二：命名空间解析与 HTML 页保存

```rust
use cmx_form::pages::html::{self, HtmlPageInput};

// 点分 id 解析出命名空间：fi.cmxfico.gl.voucher → 域 fi / 应用 cmxfico / 模块 gl / 页 voucher
let ns = html::parse_page_namespace("fi.cmxfico.gl.voucher")?;
assert_eq!(ns.domain, "fi");
assert_eq!(ns.rel_path, "fi/cmxfico/gl/voucher.html");   // 源文件按命名空间分层

let row = html::save_html_page(HtmlPageInput {
    id: "fi.cmxfico.gl.voucher".into(),
    name: Some("凭证录入页".into()),
    details: Some("总账凭证设计器页面".into()),
    html: Some("<div id=\"app\"></div>".into()),
    domain: None, app: None, module: None, doc: None,
}).await?;
// 落盘：sources/fi/cmxfico/gl/voucher.html；索引双写（v2 分片 + v1 列表）+ invalidate_paths

// 旧式无点 id 归 _legacy 域：parse_page_namespace("old_page")?.domain == "_legacy"
```

### 场景三：引擎微服务挂载 serve 只读投递路由（真实用法，参考五个引擎 server 的 main.rs）

```rust
use cmx_form::serve::{HtmlLayout, PageServeConfig, frontend_pages_routes};

// mdm/model/report：错误体用平台统一 cmx_api_types::Error（code 400/404）
let app = axum::Router::new()
    .nest("/api", frontend_pages_routes::<(), cmx_api_types::Error>(
        PageServeConfig::from_assets(),   // [assets] 段 → web/ui-native + web/ui-html
    ));

// rule 引擎：无 html 页资产，关闭 html 三端点，只投 native 页
let app = axum::Router::new()
    .nest("/api", frontend_pages_routes::<(), RuleError>(PageServeConfig {
        html: HtmlLayout::Disabled,
        ..PageServeConfig::from_assets()
    }));
// 端点：GET /api/native-pages?page=&pageSize=（缺省 1/50）；
//       POST /api/native-pages/batch {ids:[]} → {items:[NativePageFull]}；
//       GET /api/html-pages?domain=&app=&module=&keyword=；POST /api/html-pages（F3-save upsert）
```

### 场景四：batch 差异同步（前端缓存校验）

```rust
use serde_json::json;

// 前端首页加载批量取页：带上本地 IndexedDB 已缓存的 rev 清单，服务端只回内容有变的页。
let body = json!({
    "ids": ["portal.flow.todo", "portal.rpt.designer"],
    "clientRevs": { "portal.flow.todo": "0123456789abcdef" }   // 上次拿到的 rev
});
let resp = cmx_form::pages::native::get_native_pages_by_ids(&body).await?;
// resp = { pages: [...], revs: {...}, errors: [...] }：
//   - portal.flow.todo 的 rev 未变 → 只出现在 revs，pages 里省略 body（省流量）
//   - portal.rpt.designer 无 clientRevs 记录 → 全量返回 NativePageFull（含 rev + source）
//   - 单条失败（id 非法/不存在）记入 errors，不阻断整批
```

---

## Features

无 `[features]`。测试基建经 dev-dependencies 启用 `cmx-jsonstore` 的 `testing` feature（`test_data_root_lock` 串行化数据根测试），不影响正常构建产物。
