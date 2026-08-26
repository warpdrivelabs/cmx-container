//! egress 策略 —— cmx:http 的安全核心。
//!
//! 四层裁决：① 协议只允许 http/https；② host 必须命中域名白名单；③ **SSRF 防护**——解析出的
//! IP 若落内网/回环/链路本地/云元数据网段一律拒绝；④ 方法/体积/超时/配额上限。默认拒绝一切，
//! 只有显式白名单放行。

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

/// 每插件 egress 策略（W4：先内存/默认；后续接 `cmx_plugin_http_policy` 表加载）。
#[derive(Debug, Clone)]
pub struct EgressPolicy {
    /// 域名白名单（精确 host 或 `.example.com` 后缀通配）。空 = 拒绝一切出站。
    pub allow_hosts: Vec<String>,
    /// 允许的方法（大写）。空 = 默认允许 GET/HEAD。
    pub allow_methods: Vec<String>,
    /// 是否拒绝内网/元数据地址（SSRF 防护，默认 true——**不建议关闭**）。
    pub deny_private: bool,
    /// 单请求超时上限（毫秒）。
    pub timeout_ms: u64,
    /// 响应体大小上限（字节，超出截断）。
    pub max_body_bytes: usize,
    /// 每插件每分钟最大请求数（简单窗口配额）。
    pub max_qpm: u32,
}

impl Default for EgressPolicy {
    fn default() -> Self {
        Self {
            allow_hosts: Vec::new(), // 默认拒绝一切——必须显式配置放行。
            allow_methods: Vec::new(),
            deny_private: true,
            timeout_ms: 5_000,
            max_body_bytes: 5 * 1024 * 1024, // 5 MiB
            max_qpm: 60,
        }
    }
}

/// 裁决错误（用于 HttpResponse.error）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DenyReason {
    BadUrl(String),
    Scheme(String),
    HostNotAllowed(String),
    PrivateAddress(String),
    MethodNotAllowed(String),
    QuotaExceeded,
}

impl std::fmt::Display for DenyReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DenyReason::BadUrl(u) => write!(f, "非法 URL: {u}"),
            DenyReason::Scheme(s) => write!(f, "协议不允许（仅 http/https）: {s}"),
            DenyReason::HostNotAllowed(h) => write!(f, "host 不在 egress 白名单: {h}"),
            DenyReason::PrivateAddress(a) => write!(f, "SSRF 拦截：目标解析为内网/元数据地址: {a}"),
            DenyReason::MethodNotAllowed(m) => write!(f, "方法不允许: {m}"),
            DenyReason::QuotaExceeded => write!(f, "超出每分钟请求配额"),
        }
    }
}

impl EgressPolicy {
    /// host 是否命中白名单（精确匹配，或以 `.suffix` 结尾的后缀通配）。
    pub fn host_allowed(&self, host: &str) -> bool {
        let host = host.trim_end_matches('.').to_ascii_lowercase();
        self.allow_hosts.iter().any(|allow| {
            let a = allow.trim().to_ascii_lowercase();
            if let Some(suffix) = a.strip_prefix('.') {
                host == suffix || host.ends_with(&format!(".{suffix}"))
            } else if let Some(suffix) = a.strip_prefix("*.") {
                host == suffix || host.ends_with(&format!(".{suffix}"))
            } else {
                host == a
            }
        })
    }

    /// 方法是否允许（大小写不敏感；未配置时默认允许 GET/HEAD）。
    pub fn method_allowed(&self, method: &str) -> bool {
        let m = method.to_ascii_uppercase();
        if self.allow_methods.is_empty() {
            m == "GET" || m == "HEAD"
        } else {
            self.allow_methods.iter().any(|x| x.to_ascii_uppercase() == m)
        }
    }
}

/// 判断一个 IP 是否属于禁止出站的内网/回环/链路本地/元数据网段（SSRF 防护）。
pub fn is_blocked_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => is_blocked_v4(v4),
        IpAddr::V6(v6) => is_blocked_v6(v6),
    }
}

fn is_blocked_v4(ip: &Ipv4Addr) -> bool {
    let o = ip.octets();
    ip.is_loopback()            // 127.0.0.0/8
        || ip.is_private()      // 10/8, 172.16/12, 192.168/16
        || ip.is_link_local()   // 169.254/16（含云元数据 169.254.169.254）
        || ip.is_broadcast()
        || ip.is_unspecified()  // 0.0.0.0
        || ip.is_multicast()
        || o[0] == 100 && (64..=127).contains(&o[1]) // 100.64/10 CGNAT
        || o[0] == 192 && o[1] == 0 && o[2] == 0     // 192.0.0.0/24
        || o[0] == 198 && (o[1] == 18 || o[1] == 19) // 198.18/15 benchmark
}

fn is_blocked_v6(ip: &Ipv6Addr) -> bool {
    if ip.is_loopback() || ip.is_unspecified() || ip.is_multicast() {
        return true;
    }
    let seg = ip.segments();
    // fc00::/7 唯一本地地址（ULA）
    let ula = (seg[0] & 0xfe00) == 0xfc00;
    // fe80::/10 链路本地
    let link_local = (seg[0] & 0xffc0) == 0xfe80;
    // ::ffff:0:0/96 IPv4 映射——按其内嵌 v4 判定
    if seg[0] == 0 && seg[1] == 0 && seg[2] == 0 && seg[3] == 0 && seg[4] == 0 && seg[5] == 0xffff {
        let v4 = Ipv4Addr::new(
            (seg[6] >> 8) as u8,
            (seg[6] & 0xff) as u8,
            (seg[7] >> 8) as u8,
            (seg[7] & 0xff) as u8,
        );
        return is_blocked_v4(&v4);
    }
    ula || link_local
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_denies_all_hosts() {
        let p = EgressPolicy::default();
        assert!(!p.host_allowed("example.com"));
    }

    #[test]
    fn host_exact_and_suffix() {
        let p = EgressPolicy {
            allow_hosts: vec!["api.pay.com".into(), ".example.com".into()],
            ..Default::default()
        };
        assert!(p.host_allowed("api.pay.com"));
        assert!(!p.host_allowed("evil-api.pay.com.attacker.net"));
        assert!(p.host_allowed("example.com"));
        assert!(p.host_allowed("a.b.example.com"));
        assert!(!p.host_allowed("notexample.com"));
        assert!(!p.host_allowed("example.com.evil.net"));
    }

    #[test]
    fn wildcard_prefix_matches_suffix() {
        let p = EgressPolicy { allow_hosts: vec!["*.svc.local".into()], ..Default::default() };
        assert!(p.host_allowed("a.svc.local"));
        assert!(p.host_allowed("svc.local"));
        assert!(!p.host_allowed("svc.local.evil.net"));
    }

    #[test]
    fn method_default_get_head() {
        let p = EgressPolicy::default();
        assert!(p.method_allowed("get"));
        assert!(p.method_allowed("HEAD"));
        assert!(!p.method_allowed("POST"));
    }

    #[test]
    fn method_configured() {
        let p = EgressPolicy { allow_methods: vec!["POST".into()], ..Default::default() };
        assert!(p.method_allowed("post"));
        assert!(!p.method_allowed("GET"));
    }

    #[test]
    fn ssrf_blocks_private_v4() {
        for s in ["127.0.0.1", "10.1.2.3", "172.16.5.5", "192.168.1.1", "169.254.169.254", "0.0.0.0", "100.64.0.1"] {
            let ip: IpAddr = s.parse().unwrap();
            assert!(is_blocked_ip(&ip), "{s} 应被拦截");
        }
    }

    #[test]
    fn ssrf_allows_public_v4() {
        for s in ["1.1.1.1", "8.8.8.8", "93.184.216.34"] {
            let ip: IpAddr = s.parse().unwrap();
            assert!(!is_blocked_ip(&ip), "{s} 应放行");
        }
    }

    #[test]
    fn ssrf_blocks_v6_and_mapped() {
        for s in ["::1", "fe80::1", "fc00::1", "::ffff:127.0.0.1", "::ffff:10.0.0.1"] {
            let ip: IpAddr = s.parse().unwrap();
            assert!(is_blocked_ip(&ip), "{s} 应被拦截");
        }
        let pub6: IpAddr = "2606:4700:4700::1111".parse().unwrap();
        assert!(!is_blocked_ip(&pub6));
    }
}
