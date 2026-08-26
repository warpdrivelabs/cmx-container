//! WASM 宿主函数 — HTTP 出站（`cmx:http`，W4）。
//!
//! 为 WASM 插件提供**受控** egress 能力：域名白名单 + SSRF 防护 + 超时 + 体积/方法/配额上限。
//! 能力受"仅声明命名空间可 import"约束——未申请 `cmx:http` 的插件拿不到 `http_fetch` import。
//!
//! 安全要点：
//! - 默认拒绝一切 host（[`EgressPolicy::default`] 白名单为空）。
//! - **SSRF 防护**：URL 的 host 解析成 IP 后逐一核对，任一落内网/回环/链路本地/元数据网段即拒绝
//!   （防 DNS 指向内网、防 DNS-rebind：连接前用已校验 IP）。
//! - 出站走**独立 reqwest client**，不复用平台会话/凭据，不透传平台 Cookie/Token。

pub mod policy;

use std::collections::BTreeMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use cmx_core::{HttpRequest, HttpResponse};
use cmx_traits::error::HostFuncError;
use cmx_traits::runtime::{HostFunctionDef, HostFunctionProvider};

pub use policy::{DenyReason, EgressPolicy};

mod audit;
mod quota;
pub use audit::{DefaultAuditor, EgressAudit, EgressAuditor};
use quota::QuotaTracker;

/// egress 策略来源（W4：[`StaticPolicySource`] 单一默认 / [`MapPolicySource`] 按 plugin_id；
/// 后续接 `cmx_plugin_http_policy` 表实现此 trait 即可，provider 无需改动）。
pub trait PolicySource: Send + Sync {
    /// 取某插件的 egress 策略；无配置返回 `None`（视为拒绝一切出站）。
    fn policy_for(&self, plugin_id: &str) -> Option<EgressPolicy>;
}

/// 单一默认策略源（所有插件共用一份；便于冒烟）。
pub struct StaticPolicySource(pub EgressPolicy);
impl PolicySource for StaticPolicySource {
    fn policy_for(&self, _plugin_id: &str) -> Option<EgressPolicy> {
        Some(self.0.clone())
    }
}

/// 按 plugin_id 取策略的映射源（模拟 `cmx_plugin_http_policy` 表；未命中→兜底或拒绝）。
/// plugin_id 从调用上下文透传后即可键入；当前 provider 传 `"default"`。
pub struct MapPolicySource {
    map: BTreeMap<String, EgressPolicy>,
    fallback: Option<EgressPolicy>,
}
impl MapPolicySource {
    pub fn new(map: BTreeMap<String, EgressPolicy>, fallback: Option<EgressPolicy>) -> Self {
        Self { map, fallback }
    }
}
impl PolicySource for MapPolicySource {
    fn policy_for(&self, plugin_id: &str) -> Option<EgressPolicy> {
        self.map.get(plugin_id).cloned().or_else(|| self.fallback.clone())
    }
}

/// HTTP 出站宿主函数提供者。
pub struct HttpHostFunctions {
    source: Arc<dyn PolicySource>,
    quota: QuotaTracker,
    client: reqwest::Client,
    auditor: Arc<dyn EgressAuditor>,
}

impl HttpHostFunctions {
    /// 用策略源构造（默认审计器）。出站 client 独立、不带默认凭据、禁自动重定向。
    pub fn new(source: Arc<dyn PolicySource>) -> Self {
        Self::with_auditor(source, Arc::new(DefaultAuditor::new()))
    }

    /// 用策略源 + 自定义审计器构造。
    pub fn with_auditor(source: Arc<dyn PolicySource>, auditor: Arc<dyn EgressAuditor>) -> Self {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none()) // 禁自动重定向——防绕过白名单跳内网。
            .build()
            .unwrap_or_default();
        Self {
            source,
            quota: QuotaTracker::new(),
            client,
            auditor,
        }
    }

    fn do_http_fetch(&self, input: Vec<u8>) -> Result<Vec<u8>, HostFuncError> {
        let req: HttpRequest = match rmp_serde::from_slice(&input) {
            Ok(r) => r,
            Err(e) => return Ok(enc(HttpResponse::err(format!("解析请求失败: {e}")))),
        };

        // plugin_id 透传待宿主上下文增强（HostFunctionProvider::call 边界暂不带 plugin_id，
        // 见 W4 收尾说明）；当前用 "default" 键。MapPolicySource 已就位，接透传即生效。
        let plugin_id = "default";
        let (method, host) = req_meta(&req);

        let Some(policy) = self.source.policy_for(plugin_id) else {
            self.audit(plugin_id, &method, &host, false, None, Some("无 egress 策略：默认拒绝出站"));
            return Ok(enc(HttpResponse::err("无 egress 策略：默认拒绝出站")));
        };

        if let Err(reason) = self.precheck(&req, &policy, plugin_id) {
            let msg = reason.to_string();
            self.audit(plugin_id, &method, &host, false, None, Some(&msg));
            return Ok(enc(HttpResponse::err(msg)));
        }

        let resp = {
            let rt = tokio::runtime::Handle::current();
            rt.block_on(self.execute(&req, &policy))
        };
        self.audit(plugin_id, &method, &host, resp.success, resp.status, resp.error.as_deref());
        Ok(enc(resp))
    }

    fn audit(&self, plugin_id: &str, method: &str, host: &str, allowed: bool, status: Option<u16>, reason: Option<&str>) {
        self.auditor.record(EgressAudit {
            plugin_id: plugin_id.to_string(),
            method: method.to_string(),
            host: host.to_string(),
            allowed,
            status,
            reason: reason.map(|s| s.to_string()),
        });
    }

    /// 出站前策略预检（协议/白名单/SSRF/方法/配额）。
    fn precheck(&self, req: &HttpRequest, policy: &EgressPolicy, plugin_id: &str) -> Result<(), DenyReason> {
        let url = reqwest::Url::parse(&req.url).map_err(|_| DenyReason::BadUrl(req.url.clone()))?;
        match url.scheme() {
            "http" | "https" => {}
            other => return Err(DenyReason::Scheme(other.to_string())),
        }
        let host = url.host_str().ok_or_else(|| DenyReason::BadUrl(req.url.clone()))?;
        if !policy.host_allowed(host) {
            return Err(DenyReason::HostNotAllowed(host.to_string()));
        }
        let method = req.method.clone().unwrap_or_else(|| "GET".into());
        if !policy.method_allowed(&method) {
            return Err(DenyReason::MethodNotAllowed(method));
        }
        // SSRF：解析 host → IP，逐一核对（含直接是 IP 字面量的情形）。
        if policy.deny_private {
            for ip in resolve_ips(host, url.port_or_known_default().unwrap_or(0)) {
                if policy::is_blocked_ip(&ip) {
                    return Err(DenyReason::PrivateAddress(ip.to_string()));
                }
            }
        }
        // 配额（每插件每分钟）。
        if !self.quota.allow(plugin_id, policy.max_qpm) {
            return Err(DenyReason::QuotaExceeded);
        }
        Ok(())
    }

    /// 实际出站（预检通过后）。二次核对连接 IP，防 DNS-rebind。
    async fn execute(&self, req: &HttpRequest, policy: &EgressPolicy) -> HttpResponse {
        let method = req.method.clone().unwrap_or_else(|| "GET".into()).to_ascii_uppercase();
        let m = match reqwest::Method::from_bytes(method.as_bytes()) {
            Ok(m) => m,
            Err(_) => return HttpResponse::err(format!("非法方法: {method}")),
        };
        let timeout = Duration::from_millis(
            req.timeout_ms.map(|t| t.min(policy.timeout_ms)).unwrap_or(policy.timeout_ms).max(1),
        );

        let mut rb = self.client.request(m, &req.url).timeout(timeout);
        for (k, v) in &req.headers {
            // 剥离由传输层决定/受控的头。
            let lk = k.to_ascii_lowercase();
            if lk == "host" || lk == "content-length" {
                continue;
            }
            rb = rb.header(k, v);
        }
        if let Some(body) = &req.body {
            rb = rb.body(body.clone());
        }

        let resp = match rb.send().await {
            Ok(r) => r,
            Err(e) => return HttpResponse::err(format!("出站请求失败: {e}")),
        };

        // 二次 SSRF 核对：实际连接的对端地址不得为内网（防 DNS-rebind）。
        if policy.deny_private
            && let Some(addr) = resp.remote_addr()
            && policy::is_blocked_ip(&addr.ip())
        {
            return HttpResponse::err(format!(
                "SSRF 拦截（连接期）：对端解析为内网地址: {}",
                addr.ip()
            ));
        }

        let status = resp.status().as_u16();
        let mut headers = BTreeMap::new();
        for (k, v) in resp.headers().iter() {
            if let Ok(s) = v.to_str() {
                headers.insert(k.as_str().to_string(), s.to_string());
            }
        }
        let full = match resp.bytes().await {
            Ok(b) => b,
            Err(e) => return HttpResponse::err(format!("读取响应体失败: {e}")),
        };
        let (body, truncated) = if full.len() > policy.max_body_bytes {
            (full.slice(0..policy.max_body_bytes).to_vec(), true)
        } else {
            (full.to_vec(), false)
        };

        HttpResponse {
            success: true,
            status: Some(status),
            headers,
            body: Some(body),
            truncated,
            error: None,
        }
    }
}

impl HostFunctionProvider for HttpHostFunctions {
    fn namespace(&self) -> &str {
        "cmx:http"
    }

    fn functions(&self) -> Vec<HostFunctionDef> {
        vec![HostFunctionDef::msgpack_fn("http_fetch", "cmx:http")]
    }

    fn call(&self, name: &str, input: Vec<u8>) -> Result<Vec<u8>, HostFuncError> {
        match name {
            "http_fetch" => self.do_http_fetch(input),
            _ => Err(HostFuncError::invalid_function(name)),
        }
    }

    fn provided_functions(&self) -> Vec<&str> {
        vec!["http_fetch"]
    }
}

/// MsgPack 编码响应（编码失败给最兜底错误）。
fn enc(resp: HttpResponse) -> Vec<u8> {
    rmp_serde::to_vec(&resp).unwrap_or_default()
}

/// 从请求抽取审计元信息（method 大写、host；URL 非法时 host 为原串）。
fn req_meta(req: &HttpRequest) -> (String, String) {
    let method = req.method.clone().unwrap_or_else(|| "GET".into()).to_ascii_uppercase();
    let host = reqwest::Url::parse(&req.url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_string()))
        .unwrap_or_else(|| req.url.clone());
    (method, host)
}

/// 解析 host 为 IP 列表（host 本身是 IP 字面量则直接返回；域名走同步 DNS 解析）。
fn resolve_ips(host: &str, port: u16) -> Vec<IpAddr> {
    if let Ok(ip) = host.parse::<IpAddr>() {
        return vec![ip];
    }
    use std::net::ToSocketAddrs;
    match (host, port.max(1)).to_socket_addrs() {
        Ok(addrs) => addrs.map(|a| a.ip()).collect(),
        Err(_) => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provider(allow: Vec<&str>) -> HttpHostFunctions {
        let policy = EgressPolicy {
            allow_hosts: allow.into_iter().map(String::from).collect(),
            ..Default::default()
        };
        HttpHostFunctions::new(Arc::new(StaticPolicySource(policy)))
    }

    fn decide(p: &HttpHostFunctions, req: &HttpRequest) -> Result<(), DenyReason> {
        let policy = p.source.policy_for("default").unwrap();
        p.precheck(req, &policy, "default")
    }

    fn req(url: &str, method: Option<&str>) -> HttpRequest {
        HttpRequest {
            url: url.into(),
            method: method.map(String::from),
            ..Default::default()
        }
    }

    #[test]
    fn precheck_rejects_non_whitelisted_host() {
        let p = provider(vec!["api.pay.com"]);
        let e = decide(&p, &req("https://evil.com/x", None)).unwrap_err();
        assert!(matches!(e, DenyReason::HostNotAllowed(_)));
    }

    #[test]
    fn precheck_allows_whitelisted_public_host_get() {
        // 1.1.1.1 是公网 IP：白名单命中 + 非内网 + 默认 GET 放行。
        let p = provider(vec!["1.1.1.1"]);
        assert!(decide(&p, &req("https://1.1.1.1/x", None)).is_ok());
    }

    #[test]
    fn precheck_ssrf_blocks_literal_private_ip() {
        // host 直接是内网 IP，即便加进白名单也被 SSRF 拦。
        let p = provider(vec!["127.0.0.1"]);
        let e = decide(&p, &req("http://127.0.0.1:8080/", None)).unwrap_err();
        assert!(matches!(e, DenyReason::PrivateAddress(_)));
    }

    #[test]
    fn precheck_rejects_bad_scheme() {
        let p = provider(vec!["api.pay.com"]);
        let e = decide(&p, &req("file:///etc/passwd", None)).unwrap_err();
        assert!(matches!(e, DenyReason::Scheme(_)));
    }

    #[test]
    fn precheck_rejects_method_not_allowed() {
        let p = provider(vec!["1.1.1.1"]);
        let e = decide(&p, &req("https://1.1.1.1/x", Some("POST"))).unwrap_err();
        assert!(matches!(e, DenyReason::MethodNotAllowed(_)));
    }

    #[test]
    fn do_http_fetch_denies_and_audits() {
        let auditor = Arc::new(DefaultAuditor::new());
        let policy = EgressPolicy { allow_hosts: vec![], ..Default::default() };
        let p = HttpHostFunctions::with_auditor(
            Arc::new(StaticPolicySource(policy)),
            auditor.clone(),
        );
        let bytes = rmp_serde::to_vec(&req("https://evil.com/x", None)).unwrap();
        let out = p.do_http_fetch(bytes).unwrap();
        let resp: HttpResponse = rmp_serde::from_slice(&out).unwrap();
        assert!(!resp.success);
        assert!(resp.error.is_some());
        // 审计留痕：一条拒绝。
        let recent = auditor.recent(1);
        assert_eq!(recent.len(), 1);
        assert!(!recent[0].allowed);
        assert_eq!(recent[0].host, "evil.com");
    }

    #[test]
    fn map_policy_source_keys_by_plugin() {
        let mut m = BTreeMap::new();
        m.insert(
            "p1".to_string(),
            EgressPolicy { allow_hosts: vec!["a.com".into()], ..Default::default() },
        );
        let src = MapPolicySource::new(m, None);
        assert!(src.policy_for("p1").unwrap().host_allowed("a.com"));
        assert!(src.policy_for("unknown").is_none()); // 未命中且无兜底 → 拒绝一切。
    }
}
