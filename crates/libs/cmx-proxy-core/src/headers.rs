//! 反代转发的头处理——出站请求头卫生、X-Forwarded-\* 补齐、响应头拷贝。
//!
//! 三块纯函数均无 IO，行为全量由单测锁定（P0 修复点集中于此）：
//!   - 出站剥除：逐跳头 + 平台注入型头（防客户端伪造服务身份/委托令牌）+ Cookie（门户会话
//!     不下发内部服务），剥除后由 [`crate::core::ProxyCore::forward`] 从可信源重新注入。
//!   - X-Forwarded-\*：`For` append 直连 IP（链式代理语义）、`Proto`/`Host` 缺失补齐。
//!   - 响应头 append：多值头（`Set-Cookie` 等）全保留。

use std::net::SocketAddr;

use axum::http::{HeaderMap, HeaderName, HeaderValue, Uri};
use axum::response::{IntoResponse, Response};

/// 逐跳头（RFC 7230 §6.1）+ host + content-length：转发时剥掉，由 reqwest/目标自定
/// （host 换成目标基址的、content-length 由流式 body 重算）。
pub(crate) fn is_hop_by_hop(name: &HeaderName) -> bool {
    matches!(
        name.as_str(),
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
            | "host"
            | "content-length"
    )
}

/// 出站前必须剥除的"平台注入型"头：客户端可伪造、而下游微服务信任的字段——不剥则外部
/// 请求可携带伪造的 `X-API-Key`（冒充服务身份）或 `X-Delegated-User-Token`（冒充任意用户）
/// 打穿到内部服务。剥除后由转发核从可信源（配置/task-local）重新注入。
/// `Cookie` 是门户会话（含门户 JWT），对内部服务无意义且属凭据泄漏面，一并剥除。
pub(crate) fn is_injected_or_session_header(name: &HeaderName) -> bool {
    matches!(
        name.as_str(),
        "x-api-key" | "x-delegated-user-token" | "x-request-id" | "cookie"
    )
}

/// 过滤出站请求头：
///   - 剥逐跳/host/content-length 与平台注入型头/`Cookie`（见 [`is_injected_or_session_header`]）；
///   - 保留其余（`Authorization`/`Accept`/`Content-Type` 及已存在的 `X-Forwarded-*`）；
///   - `X-Forwarded-For` 走 append 语义（直连 IP 追加到已有值后，取不到则保留原值）；
///   - `X-Forwarded-Proto` 缺失补 `uri` 的 scheme（axum 原生路径下通常无 scheme，缺省 http）；
///   - `X-Forwarded-Host` 缺失时从入站 `Host` 补（下游据此还原对外域名）。
pub(crate) fn sanitize_request_headers(
    src: &HeaderMap,
    peer: Option<&SocketAddr>,
    uri: &Uri,
) -> HeaderMap {
    let mut out = HeaderMap::new();
    for (k, v) in src.iter() {
        // x-forwarded-for 单独处理（append 语义，不能简单拷贝）。
        if is_hop_by_hop(k) || is_injected_or_session_header(k) || k.as_str() == "x-forwarded-for" {
            continue;
        }
        out.insert(k.clone(), v.clone());
    }

    // X-Forwarded-For：已有值（可信上游代理注入）+ 直连客户端 IP；拿不到直连 IP 时保留原值。
    let xff = match (src.get("x-forwarded-for"), peer) {
        (Some(prev), Some(p)) => Some(format!("{}, {}", prev.to_str().unwrap_or_default(), p.ip())),
        (None, Some(p)) => Some(p.ip().to_string()),
        (Some(prev), None) => Some(prev.to_str().unwrap_or_default().to_string()),
        (None, None) => None,
    };
    if let Some(v) = xff
        && let Ok(hv) = HeaderValue::from_str(&v)
    {
        out.insert("x-forwarded-for", hv);
    }

    // X-Forwarded-Proto：入站已有（链式代理）则保留，缺失补 scheme（缺省 http）。
    if !out.contains_key("x-forwarded-proto")
        && let Ok(hv) = HeaderValue::from_str(uri.scheme_str().unwrap_or("http"))
    {
        out.insert("x-forwarded-proto", hv);
    }

    // X-Forwarded-Host：入站已有则保留，缺失从入站 Host 补。
    if !out.contains_key("x-forwarded-host")
        && let Some(host) = src.get("host")
    {
        out.insert("x-forwarded-host", host.clone());
    }
    out
}

/// 拷贝下游响应头为回给客户端的响应头：剥逐跳（`transfer-encoding` 由 axum 侧重定），
/// **append 语义**保留多值头（`Set-Cookie` 等多个值全保留，`insert` 会只留最后一个）。
pub(crate) fn copy_response_headers(src: &reqwest::header::HeaderMap) -> HeaderMap {
    let mut out = HeaderMap::new();
    for (k, v) in src.iter() {
        let (Ok(name), Ok(val)) = (
            HeaderName::from_bytes(k.as_ref()),
            HeaderValue::from_bytes(v.as_bytes()),
        ) else {
            continue;
        };
        if is_hop_by_hop(&name) {
            continue;
        }
        out.append(name, val);
    }
    out
}

/// reqwest 响应 → axum 响应（状态 + 头 + 流式体）。
///
/// 流式体是 SSE 逐块透传的前提：`text/event-stream` 的头原样保留，body 不整体缓冲。
pub(crate) fn build_response(resp: reqwest::Response) -> Response {
    let status = axum::http::StatusCode::from_u16(resp.status().as_u16())
        .unwrap_or(axum::http::StatusCode::BAD_GATEWAY);
    let headers = copy_response_headers(resp.headers());
    let stream = resp.bytes_stream();
    let body = axum::body::Body::from_stream(stream);
    let mut out = (status, body).into_response();
    *out.headers_mut() = headers;
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn uri_of(s: &str) -> Uri {
        s.parse().expect("测试 Uri 解析失败")
    }

    /// 平台注入型头/会话头/逐跳头必须剥除；普通业务头原样保留。
    #[test]
    fn strips_untrusted_and_hop_by_hop_headers() {
        let mut src = HeaderMap::new();
        src.insert("x-api-key", HeaderValue::from_static("forged-key"));
        src.insert(
            "x-delegated-user-token",
            HeaderValue::from_static("Bearer forged"),
        );
        src.insert("x-request-id", HeaderValue::from_static("forged-rid"));
        src.insert("cookie", HeaderValue::from_static("session=portal-jwt"));
        src.insert("connection", HeaderValue::from_static("keep-alive"));
        src.insert("host", HeaderValue::from_static("portal.example"));
        src.insert("content-length", HeaderValue::from_static("128"));
        src.insert("authorization", HeaderValue::from_static("Bearer user-jwt"));
        src.insert("content-type", HeaderValue::from_static("application/json"));

        let out = sanitize_request_headers(&src, None, &uri_of("/flow/stats"));

        for stripped in [
            "x-api-key",
            "x-delegated-user-token",
            "x-request-id",
            "cookie",
            "connection",
            "host",
            "content-length",
        ] {
            assert!(
                out.get(stripped).is_none(),
                "应剥除出站头 {stripped}，实际={:?}",
                out.get(stripped)
            );
        }
        assert_eq!(out.get("authorization").unwrap(), "Bearer user-jwt");
        assert_eq!(out.get("content-type").unwrap(), "application/json");
    }

    /// 已有 X-Forwarded-For（上游代理注入）+ 直连 IP → append 语义拼接。
    #[test]
    fn forwarded_for_appends_peer_ip() {
        let mut src = HeaderMap::new();
        src.insert("x-forwarded-for", HeaderValue::from_static("10.0.0.1"));
        let peer: SocketAddr = "192.168.1.50:33441".parse().unwrap();

        let out = sanitize_request_headers(&src, Some(&peer), &uri_of("/rpt/x"));

        assert_eq!(out.get("x-forwarded-for").unwrap(), "10.0.0.1, 192.168.1.50");
    }

    /// 无已有 XFF + 有直连 IP → 只写直连 IP；无直连 IP + 有 XFF → 保留原值。
    #[test]
    fn forwarded_for_without_peer_or_prior() {
        let peer: SocketAddr = "192.168.1.50:33441".parse().unwrap();
        let out = sanitize_request_headers(&HeaderMap::new(), Some(&peer), &uri_of("/a"));
        assert_eq!(out.get("x-forwarded-for").unwrap(), "192.168.1.50");

        let mut src = HeaderMap::new();
        src.insert("x-forwarded-for", HeaderValue::from_static("10.0.0.1"));
        let out = sanitize_request_headers(&src, None, &uri_of("/a"));
        assert_eq!(out.get("x-forwarded-for").unwrap(), "10.0.0.1");
    }

    /// X-Forwarded-Proto 缺失补 http；已有（链式代理）则保留。
    #[test]
    fn forwarded_proto_default_and_kept() {
        let out = sanitize_request_headers(&HeaderMap::new(), None, &uri_of("/a"));
        assert_eq!(out.get("x-forwarded-proto").unwrap(), "http");

        let mut src = HeaderMap::new();
        src.insert("x-forwarded-proto", HeaderValue::from_static("https"));
        let out = sanitize_request_headers(&src, None, &uri_of("/a"));
        assert_eq!(out.get("x-forwarded-proto").unwrap(), "https");
    }

    /// X-Forwarded-Host 缺失时从入站 Host 补；已有则保留。
    #[test]
    fn forwarded_host_from_inbound_host() {
        let mut src = HeaderMap::new();
        src.insert("host", HeaderValue::from_static("portal.example"));
        let out = sanitize_request_headers(&src, None, &uri_of("/a"));
        assert_eq!(out.get("x-forwarded-host").unwrap(), "portal.example");

        let mut src = HeaderMap::new();
        src.insert("x-forwarded-host", HeaderValue::from_static("upstream.example"));
        let out = sanitize_request_headers(&src, None, &uri_of("/a"));
        assert_eq!(out.get("x-forwarded-host").unwrap(), "upstream.example");
    }

    /// 响应头 append 语义：多个 Set-Cookie 全保留；transfer-encoding 剥除。
    #[test]
    fn response_headers_append_multi_values() {
        let mut src = reqwest::header::HeaderMap::new();
        src.append("set-cookie", HeaderValue::from_static("a=1; Path=/"));
        src.append("set-cookie", HeaderValue::from_static("b=2; Path=/"));
        src.insert("transfer-encoding", HeaderValue::from_static("chunked"));
        src.insert("etag", HeaderValue::from_static("\"v1\""));

        let out = copy_response_headers(&src);

        let cookies = out.get_all("set-cookie");
        assert_eq!(cookies.iter().count(), 2, "多值 Set-Cookie 应全保留");
        assert!(out.get("transfer-encoding").is_none());
        assert_eq!(out.get("etag").unwrap(), "\"v1\"");
    }
}
