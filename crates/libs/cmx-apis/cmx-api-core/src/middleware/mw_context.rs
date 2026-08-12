use crate::{Error, Result};
use axum::body::Body;
use axum::extract::FromRequestParts;
use axum::http::Request;
use axum::http::request::Parts;
use axum::middleware::Next;
use axum::response::Response;
use chrono::Utc;
use cmx_core::model::service::context::SVRContext;
use cmx_utils::UuidGenerator;
use tracing::debug;

#[derive(Debug, Clone)]
pub struct CmxSvrContext(pub SVRContext);

pub async fn mw_context_resolver(mut req: Request<Body>, next: Next) -> Result<Response> {
    debug!("{:<12} - mw_context_resolver", "MIDDLEWARE");

    //请求进入时间
    let time_in = Utc::now();
    //请求追踪id
    let request_id = UuidGenerator::new_v4_compact();
    //请求头信息
    let headers: std::collections::HashMap<String, String> = req
        .headers()
        .iter()
        .filter_map(|(k, v)| v.to_str().ok().map(|s| (k.to_string(), s.to_string())))
        .collect();

    //构建svr_context
    let svr_context = SVRContext::new(serde_json::Value::Null, headers, time_in, request_id);

    req.extensions_mut().insert(CmxSvrContext(svr_context));

    Ok(next.run(req).await)
}

// region:    --- SvrContext Extractor
impl<S: Send + Sync> FromRequestParts<S> for CmxSvrContext {
    type Rejection = Error;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self> {
        debug!("{:<12} - CmxSvrContext", "EXTRACTOR");

        parts
            .extensions
            .get::<CmxSvrContext>()
            .cloned()
            .ok_or(Error::SvrContextNotInReqExt)
    }
}
