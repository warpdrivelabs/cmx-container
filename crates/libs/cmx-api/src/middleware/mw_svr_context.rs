use crate::error::{Error, Result};
use axum::body::Body;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::Request;
use axum::middleware::Next;
use axum::response::Response;
use chrono::Utc;
use cmx_core::model::cell::CellValue;
use cmx_core::model::data::context::{svrkey, SVRContext};
use cmx_utils::time::now_utc;
use cmx_utils::UuidGenerator;
use serde_json::Value;
use time::OffsetDateTime;
use tracing::debug;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct CmxSvrContext(pub SVRContext);

pub async fn mw_svr_context_resolver(mut req: Request<Body>, next: Next) -> Result<Response> {
    debug!("{:<12} - mw_svr_context_resolver", "MIDDLEWARE");

    // let time_in = now_utc();
    let time_in = Utc::now();
    let uuid = UuidGenerator::new_v4_compact();

    let svr_context = SVRContext::new();

    svr_context.set(svrkey::KEY_TIME_IN, CellValue::DateTime(time_in));
    svr_context.set(svrkey::KEY_REQUEST_ID, CellValue::String(uuid));

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
