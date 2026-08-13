//! cmx-doc-api 的 OpenApi 切片。
//!
//! 业务单据（DOC）域的 paths（handler 的 `#[utoipa::path]` 注解）+ `DocChildrenReq` schema，
//! 由 platform-app 用 `OpenApi::merge()` 聚合到主文档。响应 schema（`ApiResp<Value>` 等）由
//! utoipa 从各 path 的 `body=` 自动收集，无需在此显式声明。

use utoipa::OpenApi;

/// 业务单据（DOC）OpenApi 切片。
#[derive(OpenApi)]
#[openapi(
    paths(
        crate::handlers::doc_data_sqlx_dataset_json,
        crate::handlers::doc_data_tokio_zmc_msgpack,
        crate::handlers::doc_data_sqlx_zmc_msgpack,
        crate::handlers::doc_data_tokio_zmc_json,
        crate::handlers::doc_data_sqlx_zmc_json,
        crate::handlers::doc_data_stream,
        crate::handlers::doc_children,
        crate::handlers::doc_meta,
        crate::handlers::doc_save,
        crate::handlers::doc_save_batch,
        crate::handlers::doc_revisions,
        crate::handlers::doc_revision,
        crate::handlers::doc_restore,
    ),
    components(
        schemas(
            // 懒下钻端点的结构化 POST body（其余 POST body 为动态 Value，在 description 详述）
            crate::handlers::DocChildrenReq,
        )
    )
)]
pub struct DocApiDoc;
