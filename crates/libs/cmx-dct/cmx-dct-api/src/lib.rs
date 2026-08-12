//! cmx-dct-api —— 数据字典（DCT）模块的 HTTP 层。
//!
//! 薄 axum handler：提取参数 → `store::resolve_dict` 解析字典视图 → 调 `cmx_dct_store_pg`
//! 服务 → `ApiResp`/msgpack 信封。`DctModule` 实现 cmx-api 的 `ModuleRoutes`，聚合字典数据
//! 服务路由。由 web-server（而非 cmx-api）合并 `DctModule.routes()`，故 cmx-api 不反向依赖
//! 本 crate（无环）。端点路径与迁移前完全一致（`/dct/*`，`/api` 前缀由 web-server nest 加）。

pub mod handlers;

use axum::Router;
use axum::routing::{get, post};

use cmx_api_core::CmxAppState;
use cmx_api_core::routes::traits::ModuleRoutes;

use handlers as dct;

/// 数据字典模块路由聚合（实现 cmx-api 的 ModuleRoutes，由 web-server 合并进主路由）。
pub struct DctModule;

impl ModuleRoutes for DctModule {
    fn routes(self) -> Router<CmxAppState> {
        Router::new()
            // ── DCT 数据服务（tokio-postgres 直读/写 cf_* 物理表）──
            // 取字典显示元数据（层序 / 列 caption·类型 / 父子关系，前端动态建表用）
            .route("/dct/meta", get(dct::dct_meta))
            // 字典条目查询：GET（URL query 便捷）/ POST（DocQuery 富查询：条件/排序/分页/游标）
            .route(
                "/dct/data/search",
                get(dct::dct_search).post(dct::dct_search),
            )
            // 零拷贝装载（对标 doc）：tokio-postgres + ZmcDataSet + 列式 msgpack 二进制
            .route(
                "/dct/data/tokio-zmc-msgpack",
                get(dct::dct_search_zmc_msgpack).post(dct::dct_search_zmc_msgpack),
            )
            // 字典条目批量新增 / 更新（upsert）
            .route("/dct/entries", post(dct::dct_upsert))
            // 按 id 删除字典条目（既有接口，保留 DELETE 方法）
            .route("/dct/entries/{id}", axum::routing::delete(dct::dct_delete))
            // 基于 changeset 的回存（对标 doc ChangeSetCollector/DocSaver）
            .route("/dct/save", post(dct::dct_save))
            // 流式导出全表（JSON NDJSON / CSV）：keyset 分页 + mpsc + Body::from_stream
            .route("/dct/export", get(dct::dct_export))
            // 流式导入（multipart/form-data：file + mode）：自动格式识别 + 批量 INSERT
            .route("/dct/import", post(dct::dct_import))
    }

    fn prefix() -> &'static str {
        "dct"
    }

    fn module_name(&self) -> &'static str {
        "dct"
    }
}
