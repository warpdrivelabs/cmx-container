//! cmx-doc-api —— 业务单据（DOC）模块的 HTTP 层。
//!
//! 薄 axum handler：提取参数 → 解析 DocMetaView(带缓存) → 调 `cmx_doc_store_pg` 装载/回存
//! → `ApiResp`/msgpack 信封。`DocModule` 实现 cmx-api 的 `ModuleRoutes`，聚合单据装载/回存/
//! 版本化路由。由 web-server（而非 cmx-api）合并 `DocModule.routes()`，故 cmx-api 不反向依赖
//! 本 crate（无环）。端点路径与迁移前完全一致（`/doc/*`，`/api` 前缀由 web-server nest 加）。

pub mod handlers;

use axum::Router;
use axum::routing::{get, post};

use cmx_api::CmxAppState;
use cmx_api::routes::traits::ModuleRoutes;

use handlers as doc;

/// 业务单据模块路由聚合（实现 cmx-api 的 ModuleRoutes，由 web-server 合并进主路由）。
pub struct DocModule;

impl ModuleRoutes for DocModule {
    fn routes(self) -> Router<CmxAppState> {
        Router::new()
            // 业务单据数据装载/回存（方案 Phase 4/5）
            // 端点命名：/doc/data/<驱动>-<内存模式>-<传输> —— 一眼可辨驱动/内存/传输三维度。
            //   驱动 sqlx|tokio · 内存 dataset(全拷贝)|zmc(零拷贝) · 传输 json|msgpack
            // 每个端点 GET(便捷 URL query) + POST(body=DocQuery 富查询：每层条件/排序/分页/游标)。
            // ① sqlx + 老 DataSet(全拷贝) + JSON（老链路）
            .route(
                "/doc/data/sqlx-dataset-json",
                get(doc::doc_data_sqlx_dataset_json).post(doc::doc_data_sqlx_dataset_json),
            )
            // ④ tokio-postgres + ZmcDataSet(零拷贝) + msgpack 二进制
            .route(
                "/doc/data/tokio-zmc-msgpack",
                get(doc::doc_data_tokio_zmc_msgpack).post(doc::doc_data_tokio_zmc_msgpack),
            )
            // ③ sqlx + ZmcDataSet(零拷贝) + msgpack 二进制
            .route(
                "/doc/data/sqlx-zmc-msgpack",
                get(doc::doc_data_sqlx_zmc_msgpack).post(doc::doc_data_sqlx_zmc_msgpack),
            )
            // ⑤ tokio-postgres + ZmcDataSet(零拷贝) + 纯 JSON 出口
            .route(
                "/doc/data/tokio-zmc-json",
                get(doc::doc_data_tokio_zmc_json).post(doc::doc_data_tokio_zmc_json),
            )
            // ⑥ sqlx + ZmcDataSet(零拷贝) + 纯 JSON 出口（补齐驱动×内存×传输最后一种组合）
            .route(
                "/doc/data/sqlx-zmc-json",
                get(doc::doc_data_sqlx_zmc_json).post(doc::doc_data_sqlx_zmc_json),
            )
            // 懒下钻：装载某层在给定父 id 下的子树（前端 grid 展开时调用）
            .route("/doc/data/children", post(doc::doc_children))
            // 真·流式：超大扁平单层结果零内存 chunked 传输（长度分帧二进制）
            .route(
                "/doc/data/tokio-zmc-stream",
                get(doc::doc_data_stream).post(doc::doc_data_stream),
            )
            // 业务单据**显示元数据**(层序/各层列 caption·类型/父子关系)——通用单据前端页动态建表用
            .route("/doc/meta", get(doc::doc_meta))
            .route("/doc/save", post(doc::doc_save))
            .route("/doc/save/batch", post(doc::doc_save_batch))
            // 业务单据版本化（方案 §6A / Phase 8）
            .route("/doc/revisions", get(doc::doc_revisions))
            .route("/doc/revision", get(doc::doc_revision))
            .route("/doc/restore", post(doc::doc_restore))
    }

    fn prefix() -> &'static str {
        "doc"
    }

    fn module_name(&self) -> &'static str {
        "doc"
    }
}
