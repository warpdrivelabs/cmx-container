//! cmx-dct-api 的 OpenApi 切片。
//!
//! 数据字典（DCT）域的 paths（handler 的 `#[utoipa::path]` 注解），由 platform-app
//! 用 `OpenApi::merge()` 聚合到主文档。响应 schema（`ApiResp<Value>` 等）由 utoipa
//! 从各 path 的 `body=` 自动收集，无需在此显式声明 components。

use utoipa::OpenApi;

/// 数据字典（DCT）OpenApi 切片。
#[derive(OpenApi)]
#[openapi(
    paths(
        crate::handlers::dct_meta,
        crate::handlers::dct_search,
        crate::handlers::dct_search_zmc_msgpack,
        crate::handlers::dct_upsert,
        crate::handlers::dct_delete,
        crate::handlers::dct_save,
        crate::handlers::dct_export,
        crate::handlers::dct_import,
    )
)]
pub struct DctApiDoc;
