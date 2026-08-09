//! 请求遥测中间件 + 客户端连接聚合（从 flow-app::observe 上提，通用化）。
//!
//! 采集「谁（身份/IP）、用什么（协议/UA）、调了什么（方法/路径/参数）、结果如何（状态/耗时/字节）」
//! 进进程级环形缓冲（cap 500）+ 原子计数。身份读取经 [`crate::identity`] 注入钩子（各服务不同）。
//! SSE 活跃连接由 [`sse_connect`]/[`sse_disconnect`] 计数。全进程内内存，零 DB。

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use axum::Json;
use axum::extract::Request;
use axum::http::header;
use axum::middleware::Next;
use axum::response::Response;
use serde::Serialize;
use serde_json::{Value, json};

use crate::identity::current_identity;
use crate::resp::ApiResp;

/// 环形缓冲上限（近 N 条调用明细）。
const RING_CAP: usize = 500;

/// 单条请求遥测记录（一次调用的全维度快照）。
#[derive(Debug, Clone, Serialize)]
pub struct CallRecord {
    /// 单调序号（最新最大）。
    pub seq: u64,
    /// 相对进程启动的毫秒时间戳。
    pub at_ms: u64,
    /// HTTP 方法。
    pub method: String,
    /// 请求路径（不含 query）。
    pub path: String,
    /// query 串（参数，截断）。
    pub query: String,
    /// 协议（HTTP/1.1、HTTP/2、SSE）。
    pub protocol: String,
    /// 客户端 IP（X-Forwarded-For / X-Real-IP，代理链首个）。
    pub client_ip: String,
    /// User-Agent（截断）。
    pub user_agent: String,
    /// 认证方式：apikey | jwt | delegated | header | anon。
    pub auth: String,
    /// 租户。
    pub tenant: String,
    /// 用户（可空）。
    pub user: Option<String>,
    /// 角色。
    pub roles: Vec<String>,
    /// 是否经平台代理（有 X-Request-Id + X-Delegated-User-Token）。
    pub via_proxy: bool,
    /// 平台链路请求 ID（若有）。
    pub request_id: Option<String>,
    /// 状态码。
    pub status: u16,
    /// 耗时（毫秒）。
    pub latency_ms: u64,
    /// 响应体字节数（Content-Length；无则 0）。
    pub resp_bytes: u64,
}

/// 进程级监控状态。
struct Monitor {
    ring: Mutex<VecDeque<CallRecord>>,
    seq: AtomicU64,
    total: AtomicU64,
    errors: AtomicU64,
    sse_active: AtomicU64,
    sse_total: AtomicU64,
    started: Instant,
}

fn monitor() -> &'static Monitor {
    static M: OnceLock<Monitor> = OnceLock::new();
    M.get_or_init(|| Monitor {
        ring: Mutex::new(VecDeque::with_capacity(RING_CAP)),
        seq: AtomicU64::new(0),
        total: AtomicU64::new(0),
        errors: AtomicU64::new(0),
        sse_active: AtomicU64::new(0),
        sse_total: AtomicU64::new(0),
        started: Instant::now(),
    })
}

/// SSE 连接建立（服务的 SSE handler 入口调）。
pub fn sse_connect() {
    let m = monitor();
    m.sse_active.fetch_add(1, Ordering::Relaxed);
    m.sse_total.fetch_add(1, Ordering::Relaxed);
}

/// SSE 连接断开（服务的 SSE 流 Drop 时调）。
pub fn sse_disconnect() {
    monitor().sse_active.fetch_sub(1, Ordering::Relaxed);
}

/// 可观测中间件：采集每请求全维度遥测 → 环形缓冲 + 计数器。
///
/// **建议夹在认证中间件之内层**（`.layer(observe).layer(auth)`），使 next.run 时身份 scope 已建立，
/// [`current_identity`] 能读到 tenant/user/roles。未注入身份 provider 时记为匿名。
pub async fn observe(req: Request, next: Next) -> Response {
    let m = monitor();
    let started = Instant::now();

    // —— 请求侧维度 ——
    let method = req.method().as_str().to_string();
    let uri = req.uri().clone();
    let path = uri.path().to_string();
    let query = truncate(uri.query().unwrap_or(""), 200);
    let version = format!("{:?}", req.version());
    let headers = req.headers();
    let client_ip = client_ip_of(headers);
    let user_agent = truncate(
        headers
            .get(header::USER_AGENT)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("-"),
        160,
    );
    let has_apikey = headers.contains_key("x-api-key");
    let has_delegated = headers.contains_key("x-delegated-user-token");
    let has_bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_ascii_lowercase().starts_with("bearer "))
        .unwrap_or(false);
    let request_id = headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let via_proxy = request_id.is_some() || has_delegated;
    let is_sse = path.ends_with("/events");
    let protocol = if is_sse {
        "SSE".to_string()
    } else {
        version.replace("Version::", "").replace("HTTP_", "HTTP/")
    };

    // —— 执行 ——
    let resp = next.run(req).await;

    // —— 响应侧 + 身份（scope 在 auth 之后仍活于本层）——
    let status = resp.status().as_u16();
    let latency_ms = started.elapsed().as_millis() as u64;
    let resp_bytes = resp
        .headers()
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

    let id = current_identity();
    let auth = if has_apikey && has_delegated {
        "delegated"
    } else if has_apikey {
        "apikey"
    } else if has_bearer {
        "jwt"
    } else if id.as_ref().map(|i| i.user.is_some()).unwrap_or(false) {
        "header"
    } else {
        "anon"
    }
    .to_string();

    let seq = m.seq.fetch_add(1, Ordering::Relaxed) + 1;
    m.total.fetch_add(1, Ordering::Relaxed);
    if status >= 400 {
        m.errors.fetch_add(1, Ordering::Relaxed);
    }

    let rec = CallRecord {
        seq,
        at_ms: m.started.elapsed().as_millis() as u64,
        method,
        path,
        query,
        protocol,
        client_ip,
        user_agent,
        auth,
        tenant: id.as_ref().map(|i| i.tenant.clone()).unwrap_or_else(|| "-".into()),
        user: id.as_ref().and_then(|i| i.user.clone()),
        roles: id.map(|i| i.roles).unwrap_or_default(),
        via_proxy,
        request_id,
        status,
        latency_ms,
        resp_bytes,
    };

    if let Ok(mut ring) = m.ring.lock() {
        if ring.len() >= RING_CAP {
            ring.pop_front();
        }
        ring.push_back(rec);
    }

    resp
}

/// 客户端 IP：优先 X-Forwarded-For 链首、其次 X-Real-IP，都无则 "-"。
fn client_ip_of(headers: &axum::http::HeaderMap) -> String {
    if let Some(xff) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
        if let Some(first) = xff.split(',').next() {
            let ip = first.trim();
            if !ip.is_empty() {
                return ip.to_string();
            }
        }
    }
    headers
        .get("x-real-ip")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "-".to_string())
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let t: String = s.chars().take(max).collect();
        format!("{t}…")
    }
}

/// 请求遥测聚合（内层 data 对象）。供 `client_stats` 与 `tech_stats` 共用。
pub fn requests_snapshot() -> Value {
    let m = monitor();
    let ring: Vec<CallRecord> = m
        .ring
        .lock()
        .map(|r| r.iter().cloned().collect())
        .unwrap_or_default();

    let total = m.total.load(Ordering::Relaxed);
    let errors = m.errors.load(Ordering::Relaxed);
    let uptime_s = m.started.elapsed().as_secs().max(1);

    let by_protocol = count_by(&ring, |r| r.protocol.clone());
    let by_auth = count_by(&ring, |r| r.auth.clone());
    let by_client = count_by(&ring, |r| r.client_ip.clone());
    let by_user = count_by(&ring, |r| r.user.clone().unwrap_or_else(|| "(匿名)".into()));
    let by_endpoint = count_by(&ring, |r| format!("{} {}", r.method, strip_id(&r.path)));

    let (sum_lat, max_lat) = ring.iter().fold((0u64, 0u64), |(s, mx), r| {
        (s + r.latency_ms, mx.max(r.latency_ms))
    });
    let avg_lat = if ring.is_empty() { 0 } else { sum_lat / ring.len() as u64 };

    let mut recent = ring.clone();
    recent.reverse();
    recent.truncate(100);

    json!({
        "overview": {
            "totalRequests": total,
            "errorRequests": errors,
            "errorRate": if total > 0 { (errors as f64 / total as f64 * 100.0 * 10.0).round() / 10.0 } else { 0.0 },
            "windowSize": ring.len(),
            "avgLatencyMs": avg_lat,
            "maxLatencyMs": max_lat,
            "qps": (total as f64 / uptime_s as f64 * 100.0).round() / 100.0,
            "uptimeSecs": uptime_s,
            "sseActive": m.sse_active.load(Ordering::Relaxed),
            "sseTotal": m.sse_total.load(Ordering::Relaxed),
            "distinctClients": by_client.len(),
            "distinctUsers": by_user.len(),
        },
        "byProtocol": pairs(&by_protocol),
        "byAuth": pairs(&by_auth),
        "byClient": pairs_top(&by_client, 10),
        "byUser": pairs_top(&by_user, 10),
        "byEndpoint": pairs_top(&by_endpoint, 12),
        "recent": recent,
    })
}

/// `GET .../clients` —— 聚合客户端连接监控（信封包 [`requests_snapshot`]）。兼容旧 flow `/clients` 端点。
pub async fn client_stats() -> Json<ApiResp<Value>> {
    Json(ApiResp::ok(requests_snapshot()))
}

/// 计数聚合。
fn count_by<F: Fn(&CallRecord) -> String>(ring: &[CallRecord], f: F) -> Vec<(String, i64)> {
    let mut map: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
    for r in ring {
        *map.entry(f(r)).or_insert(0) += 1;
    }
    let mut v: Vec<(String, i64)> = map.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1));
    v
}

fn pairs(v: &[(String, i64)]) -> Vec<Value> {
    v.iter().map(|(k, c)| json!({ "label": k, "value": c })).collect()
}

fn pairs_top(v: &[(String, i64)], n: usize) -> Vec<Value> {
    v.iter().take(n).map(|(k, c)| json!({ "label": k, "value": c })).collect()
}

/// 路径里的 UUID / 数字 id 段归一为 {id}，聚合成端点热点。
fn strip_id(path: &str) -> String {
    path.split('/')
        .map(|seg| {
            let is_uuidish = seg.len() >= 20 && seg.contains('-');
            let is_num = !seg.is_empty() && seg.chars().all(|c| c.is_ascii_digit());
            if is_uuidish || is_num { "{id}" } else { seg }
        })
        .collect::<Vec<_>>()
        .join("/")
}
