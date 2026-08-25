//! serve 模块单元测试：布局 / null 容错 / 越界防护 / 分页 / 过滤 / batch 形态 / 错误映射。

use std::path::PathBuf;

use axum::extract::{Path as AxPath, Query, State};
use axum::response::IntoResponse;
use axum::Json;
use serde_json::{Value, json};

use cmx_api_types::ApiResp;

use super::config::{HtmlLayout, PageServeConfig};
use super::error::PageServeError;
use super::routes::{
    BatchReq, HtmlListQuery, PageQuery, batch_html_pages, batch_native_pages, get_html_page,
    get_native_page, list_html_pages, list_native_pages,
};

/// 测试用错误类型：模拟 rule/flow 自持错误（NotFound → body code=4），
/// 验证"泛型 E 决定错误体字节"的机制。
#[derive(Debug, thiserror::Error)]
enum LegacyStyleError {
    #[error("{0}")]
    Business(String),
    #[error("{0}")]
    NotFound(String),
}

impl From<PageServeError> for LegacyStyleError {
    fn from(e: PageServeError) -> Self {
        match e {
            PageServeError::BadRequest(m) => Self::Business(m),
            PageServeError::NotFound(m) => Self::NotFound(m),
            PageServeError::Io(m) => Self::Business(m),
        }
    }
}

impl IntoResponse for LegacyStyleError {
    fn into_response(self) -> axum::response::Response {
        let code = match &self {
            Self::Business(_) => 1,
            Self::NotFound(_) => 4,
        };
        (
            axum::http::StatusCode::OK,
            Json(json!({ "code": code, "msg": self.to_string() })),
        )
            .into_response()
    }
}

/// 信封序列化为 Value（preserve_order 下键序 = code,msg,data）。
fn envelope<T: serde::Serialize>(api: ApiResp<T>) -> Value {
    serde_json::to_value(api).unwrap()
}

/// 构造临时资产目录：
/// native 扁平页 `n1`、null 容错页 `n2`；html v2 分片（域 fi）一行，doc 为显式 null。
fn fixture() -> (tempfile::TempDir, PageServeConfig) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    // native 扁平布局
    std::fs::create_dir_all(root.join("native/a")).unwrap();
    std::fs::write(
        root.join("native/index.json"),
        json!({"pages":[
            {"id":"n1","name":"页面一","details":"d1","sourceType":"js","relPath":"a/n1.js"},
            {"id":"n2","name":null,"details":null,"sourceType":"","relPath":"a/n2.js"}
        ]})
        .to_string(),
    )
    .unwrap();
    std::fs::write(root.join("native/a/n1.js"), "console.log(1)").unwrap();
    std::fs::write(root.join("native/a/n2.js"), "// n2").unwrap();
    // 嵌套子目录页（规范：relPath 相对 index.json，可含多级段）
    std::fs::create_dir_all(root.join("native/deep/nest")).unwrap();
    std::fs::write(
        root.join("native/index.json"),
        json!({"pages":[
            {"id":"n1","name":"页面一","details":"d1","sourceType":"js","relPath":"a/n1.js"},
            {"id":"n2","name":null,"details":null,"sourceType":"","relPath":"a/n2.js"},
            {"id":"deep1","name":"深层","details":"","sourceType":"","relPath":"deep/nest/d1.js"}
        ]})
        .to_string(),
    )
    .unwrap();
    std::fs::write(root.join("native/deep/nest/d1.js"), "// deep").unwrap();
    // html v2 分片
    std::fs::create_dir_all(root.join("html/index")).unwrap();
    std::fs::write(
        root.join("html/index.json"),
        json!({"domains":["fi"]}).to_string(),
    )
    .unwrap();
    std::fs::write(
        root.join("html/index/fi.pages.json"),
        json!({"pages":[{"id":"fi.app.m.p","name":"表单","details":"","domain":"fi","app":"app","module":"m","doc":null,"relPath":"p.html"}]})
            .to_string(),
    )
    .unwrap();
    std::fs::write(root.join("html/p.html"), "<html></html>").unwrap();
    let cfg = PageServeConfig {
        native_dir: PathBuf::from(root.join("native")),
        html_dir: PathBuf::from(root.join("html")),
        html: HtmlLayout::ShardedV2,
    };
    (dir, cfg)
}

#[tokio::test]
async fn native_list_默认分页与键序() {
    let (_d, cfg) = fixture();
    let body = envelope(
        list_native_pages::<LegacyStyleError>(
            State(cfg.clone()),
            Query(PageQuery { page: None, page_size: None }),
        )
        .await
        .unwrap()
        .0,
    );
    assert_eq!(body["code"], 0);
    assert_eq!(body["msg"], "success");
    assert_eq!(body["data"]["total"], 3);
    assert_eq!(body["data"]["page"], 1);
    assert_eq!(body["data"]["pageSize"], 50);
    // 键序与引擎副本一致：id,name,details,sourceType,relPath
    let keys: Vec<&str> =
        body["data"]["items"][0].as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(keys, ["id", "name", "details", "sourceType", "relPath"]);
}

#[tokio::test]
async fn native_list_空source_type由扩展名兜底且null容错() {
    let (_d, cfg) = fixture();
    let body = envelope(
        list_native_pages::<LegacyStyleError>(
            State(cfg.clone()),
            Query(PageQuery { page: Some(1), page_size: Some(1) }),
        )
        .await
        .unwrap()
        .0,
    );
    assert_eq!(body["data"]["pageSize"], 1);
    assert_eq!(body["data"]["items"][0]["id"], "n1");
    let all = envelope(
        list_native_pages::<LegacyStyleError>(
            State(cfg.clone()),
            Query(PageQuery { page: Some(1), page_size: Some(50) }),
        )
        .await
        .unwrap()
        .0,
    );
    assert_eq!(all["data"]["items"][1]["sourceType"], "js");
    assert_eq!(all["data"]["items"][1]["name"], "");
    assert_eq!(all["data"]["items"][1]["details"], "");
}

#[tokio::test]
async fn native_get_含源码且rev为16位hex() {
    let (_d, cfg) = fixture();
    let full = get_native_page::<LegacyStyleError>(State(cfg.clone()), AxPath("n1".into()))
        .await
        .unwrap()
        .0
        .data
        .unwrap();
    assert_eq!(full.source_type, "js");
    assert_eq!(full.source, "console.log(1)");
    assert_eq!(full.rev.len(), 16);
}

#[tokio::test]
async fn native_get_不存在走自定义错误码4() {
    let (_d, cfg) = fixture();
    let err = get_native_page::<LegacyStyleError>(State(cfg.clone()), AxPath("__x__".into()))
        .await
        .unwrap_err();
    let body: Value = serde_json::from_slice(
        &axum::body::to_bytes(err.into_response().into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(body["code"], 4);
    assert_eq!(body["msg"], "native page 不存在: __x__");
}

#[tokio::test]
async fn native_get_rel_path越界走平台_bad_request() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("index.json"),
        r#"{"pages":[{"id":"bad","relPath":"../evil.js"}]}"#,
    )
    .unwrap();
    let cfg = PageServeConfig {
        native_dir: dir.path().to_path_buf(),
        html_dir: dir.path().to_path_buf(),
        html: HtmlLayout::Disabled,
    };
    let err = get_native_page::<cmx_api_types::Error>(State(cfg), AxPath("bad".into()))
        .await
        .unwrap_err();
    // 平台错误映射：BadRequest → HTTP 400 信封
    let body: Value = serde_json::from_slice(
        &axum::body::to_bytes(err.into_response().into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(body["code"], 400);
}

#[tokio::test]
async fn rel_path相对索引目录_多级嵌套装载成功() {
    let (_d, cfg) = fixture();
    let full = get_native_page::<LegacyStyleError>(State(cfg.clone()), AxPath("deep1".into()))
        .await
        .unwrap()
        .0
        .data
        .unwrap();
    assert_eq!(full.rel_path, "deep/nest/d1.js");
    assert_eq!(full.source, "// deep");
}

#[tokio::test]
async fn native_batch_缺失id静默跳过形态为items() {
    let (_d, cfg) = fixture();
    let data = batch_native_pages::<LegacyStyleError>(
        State(cfg.clone()),
        Json(BatchReq { ids: vec!["n1".into(), "__no__".into()] }),
    )
    .await
    .unwrap()
    .0
    .data
    .unwrap();
    let items = data["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"], "n1");
    assert_eq!(items[0]["rev"].as_str().unwrap().len(), 16);
}

#[tokio::test]
async fn html_list_过滤与列表键序() {
    let (_d, cfg) = fixture();
    let data = list_html_pages::<LegacyStyleError>(
        State(cfg.clone()),
        Query(HtmlListQuery {
            page: None,
            page_size: None,
            domain: Some("fi".into()),
            app: None,
            module: None,
            keyword: Some("表单".into()),
        }),
    )
    .await
    .unwrap()
    .0
    .data
    .unwrap();
    assert_eq!(data["total"], 1);
    let keys: Vec<&str> =
        data["items"][0].as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(keys, ["id", "name", "details", "domain", "app", "module", "relPath"]);
    // keyword 不命中 → 空
    let miss = list_html_pages::<LegacyStyleError>(
        State(cfg.clone()),
        Query(HtmlListQuery {
            page: None,
            page_size: None,
            domain: None,
            app: None,
            module: None,
            keyword: Some("__none__".into()),
        }),
    )
    .await
    .unwrap()
    .0
    .data
    .unwrap();
    assert_eq!(miss["total"], 0);
}

#[tokio::test]
async fn html_get_doc_null容错且完整字段序() {
    let (_d, cfg) = fixture();
    let full = get_html_page::<LegacyStyleError>(State(cfg.clone()), AxPath("fi.app.m.p".into()))
        .await
        .unwrap()
        .0
        .data
        .unwrap();
    assert_eq!(full["doc"], "");
    assert_eq!(full["html"], "<html></html>");
    let keys: Vec<&str> = full.as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(
        keys,
        ["id", "name", "details", "domain", "app", "module", "doc", "relPath", "rev", "html"]
    );
}

#[tokio::test]
async fn html_batch_pages_revs_errors三段形态() {
    let (_d, cfg) = fixture();
    let data = batch_html_pages::<LegacyStyleError>(
        State(cfg.clone()),
        Json(BatchReq { ids: vec!["fi.app.m.p".into(), "__no__".into()] }),
    )
    .await
    .unwrap()
    .0
    .data
    .unwrap();
    assert_eq!(data["pages"].as_array().unwrap().len(), 1);
    assert!(data["revs"]["fi.app.m.p"].is_string());
    assert_eq!(data["errors"][0]["error"], "不存在");
}

#[tokio::test]
async fn 索引缺失降级空集不panic() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = PageServeConfig {
        native_dir: dir.path().join("nope"),
        html_dir: dir.path().join("nope-html"),
        html: HtmlLayout::ShardedV2,
    };
    let body = envelope(
        list_native_pages::<LegacyStyleError>(
            State(cfg),
            Query(PageQuery { page: None, page_size: None }),
        )
        .await
        .unwrap()
        .0,
    );
    assert_eq!(body["data"]["total"], 0);
}
