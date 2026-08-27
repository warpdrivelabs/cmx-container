//! 两族认证配置的**各自装载入口**（键空间共享 `[auth]` 段、结构分立，避免同键双语义）：
//!
//! - 族 A [`load_delegated`]（model / mdm 形态）：`jwt_secret`（恒 HS256 解票）/
//!   `api_keys`（逗号分隔服务 key 集合，剥 `:` 后缀）/ `whitelist`（免鉴权路径）。
//! - 族 B [`JwtAuthConfig::load`]（flow / rule 形态）：`mode` / `jwt_alg`（HS256|RS256）/
//!   `jwt_secret`|`jwt_public_key` / `jwt_tenant_claim` / `jwt_roles_claim` /
//!   `api_keys`（`k:tenant` 映射，租户功能性参与 scope 构建）。
//!
//! 配置均走**平台统一装配链**（ConfigManager 三源合并：`<svc>-server.toml` ← Nacos ← env，
//! `AUTH__*` 覆盖）。

use std::collections::HashMap;
use std::sync::Arc;

use jsonwebtoken::{Algorithm, DecodingKey};

use crate::tenant::DEFAULT_TENANT;

use cmx_utils::ConfigManager;

// ═══════════════════════════════════════════════════════════════════════════
// 族 A（委托令牌）装载
// ═══════════════════════════════════════════════════════════════════════════

/// 族 A 认证配置快照（首请求经 ConfigManager 装配一次，进程内只读；配置热更不敏感——
/// 密钥类均为部署期定值）。
///
/// 收编自 cmx-model-app / cmx-mdm-app 的 auth.rs（`AuthConfig::load`，两仓逐行一致）。
#[derive(Debug)]
pub struct DelegatedAuthConfig {
    /// 委托令牌验签密钥（HS256）。空 = 不解票（纯服务调用）。
    pub jwt_secret: Arc<str>,
    /// 服务间 API Key 集合（已剥 `:` 租户后缀归一）。空 = 不强制校验。
    pub api_keys: Vec<String>,
    /// 免鉴权路径前缀（内置白名单 + toml 追加，已归一为剥 `/api` 的内部形态）。
    pub whitelist: Vec<String>,
}

impl DelegatedAuthConfig {
    /// 经 ConfigManager 读 `[auth]` 段（缺项回退空值；ConfigManager 未初始化时回退内置白名单）。
    ///
    /// `builtin_whitelist` 为各引擎内置免用户鉴权路径（model 空 / mdm 探针+webhook 回调），
    /// 与 toml 追加合并——语义对齐门户 mw_auth 的「BUILTIN_WHITELIST 与 TOML 合并」制度。
    pub fn load(builtin_whitelist: &[&str]) -> Self {
        let mut cfg = Self {
            jwt_secret: Arc::from(""),
            api_keys: Vec::new(),
            whitelist: builtin_whitelist.iter().map(|s| s.to_string()).collect(),
        };
        let Some(cm) = ConfigManager::try_global() else {
            return cfg;
        };
        if let Ok(v) = cm.get_string("auth.jwt_secret") {
            cfg.jwt_secret = Arc::from(v.trim());
        }
        if let Ok(v) = cm.get_string("auth.api_keys") {
            cfg.api_keys = normalize_api_keys(&v);
        }
        for item in cm.get_as_or::<Vec<String>>("auth.whitelist", Vec::new()) {
            let item = item.trim();
            // 兼容门户带 /api 前缀的写法（中间件看到的是已剥 /api 的路径）。
            let p = item.strip_prefix("/api").unwrap_or(item);
            if !p.is_empty() && !cfg.whitelist.iter().any(|w| w == p) {
                cfg.whitelist.push(p.to_string());
            }
        }
        cfg
    }
}

/// 服务间 API Key 归一：逗号分隔 + 剥 `:` 租户后缀 + 去空白，丢弃空段。
pub(crate) fn normalize_api_keys(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(|k| k.trim().split(':').next().unwrap_or("").trim().to_string())
        .filter(|k| !k.is_empty())
        .collect()
}

// ═══════════════════════════════════════════════════════════════════════════
// 族 B（JWT/租户）装载
// ═══════════════════════════════════════════════════════════════════════════

/// 认证模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMode {
    /// 不验签（开发/单租户：租户取 `X-Tenant` 头或默认租户，用户取 `X-User` 头）。
    Off,
    /// 强制验签（`Authorization: Bearer <jwt>` 缺失/坏签/过期 → 401）。
    Jwt,
}

/// 族 B 进程级认证配置（一次经 ConfigManager 读定；启动后改配置需重启）。
///
/// 收编自 cmx-flow-app auth.rs 的 `AuthConfig`（flow 超集形态；rule 原每请求热读的裁剪版
/// 随抽取升级为快照装载）。
pub struct JwtAuthConfig {
    pub mode: AuthMode,
    pub alg: Algorithm,
    pub decoding_key: Option<DecodingKey>,
    pub tenant_claim: String,
    pub roles_claim: String,
    /// 服务间 API Key → 租户映射。`auth.api_keys="k1:tenantA,k2:tenantB"`。
    pub api_keys: HashMap<String, String>,
}

impl JwtAuthConfig {
    /// 经 ConfigManager 读 `[auth]` 段（未初始化/缺项 → off 模式空配置）。
    pub fn load() -> Self {
        let get = |key: &str| {
            ConfigManager::try_global()
                .and_then(|cm| cm.get_string(key).ok())
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        };
        let mode = match get("auth.mode").as_deref() {
            Some(m) if m.eq_ignore_ascii_case("jwt") => AuthMode::Jwt,
            _ => AuthMode::Off,
        };
        let alg = match get("auth.jwt_alg").as_deref() {
            Some(a) if a.eq_ignore_ascii_case("RS256") => Algorithm::RS256,
            _ => Algorithm::HS256,
        };
        // 解码密钥（jwt 模式才需要；off 模式不用）。
        let decoding_key = if mode == AuthMode::Jwt {
            match alg {
                Algorithm::RS256 => get("auth.jwt_public_key").and_then(|pem| {
                    DecodingKey::from_rsa_pem(pem.as_bytes())
                        .map_err(|e| tracing::error!(error = %e, "auth.jwt_public_key 解析失败"))
                        .ok()
                }),
                _ => get("auth.jwt_secret").map(|s| DecodingKey::from_secret(s.as_bytes())),
            }
        } else {
            None
        };
        if mode == AuthMode::Jwt && decoding_key.is_none() {
            tracing::error!("auth.mode=jwt 但缺密钥（auth.jwt_secret / auth.jwt_public_key），所有请求将 401");
        }
        Self {
            mode,
            alg,
            decoding_key,
            tenant_claim: get("auth.jwt_tenant_claim").unwrap_or_else(|| "tenant".to_string()),
            roles_claim: get("auth.jwt_roles_claim").unwrap_or_else(|| "roles".to_string()),
            api_keys: parse_api_keys(get("auth.api_keys").unwrap_or_default()),
        }
    }
}

/// 解析 `auth.api_keys="k1:tenantA,k2:tenantB"` → {k1→tenantA, k2→tenantB}。
/// 无冒号的 key 绑定默认租户。
pub(crate) fn parse_api_keys(raw: String) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for entry in raw.split(',') {
        let entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        match entry.split_once(':') {
            Some((k, t)) => {
                map.insert(k.trim().to_string(), t.trim().to_string());
            }
            None => {
                map.insert(entry.to_string(), DEFAULT_TENANT.to_string());
            }
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delegated_api_keys_tolerate_colon_suffix() {
        let raw = "key1, key2:tenant ,key3";
        assert_eq!(
            normalize_api_keys(raw),
            vec!["key1".to_string(), "key2".to_string(), "key3".to_string()]
        );
    }

    #[test]
    fn jwt_api_keys_bind_default_tenant_when_no_colon() {
        let map = parse_api_keys("k1:tenantA, k2 ,k3:tenantC".to_string());
        assert_eq!(map.get("k1").map(String::as_str), Some("tenantA"));
        assert_eq!(map.get("k2").map(String::as_str), Some("default"));
        assert_eq!(map.get("k3").map(String::as_str), Some("tenantC"));
    }
}
