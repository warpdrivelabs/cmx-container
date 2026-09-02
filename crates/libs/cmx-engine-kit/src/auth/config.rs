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
    /// 服务间 API Key → 结构化声明（技术债 003）。legacy 逗号格式解析出的 key 生成
    /// allow-all 声明（system=None、白名单空 = 不限），行为与存量逐字节一致；
    /// JSON 声明格式见 [`parse_api_keys_declared`]。
    pub api_key_decls: HashMap<String, KeyDecl>,
}

/// 一把服务间 API Key 的结构化声明（技术债 003：key 不再等价全量权限）。
///
/// 载体仍是 ConfigManager 配置（`auth.api_keys`），不进库（019 口径：凭据出库而非入更多的库）。
/// `allowed_definition_keys` 为空表示**不限**（allow-all）——这是存量 legacy key 的隐式
/// 语义，也是两阶段迁移的过渡态：存量共享 key 零破坏，新增 key 建议最小声明。
#[derive(Debug, Clone)]
pub struct KeyDecl {
    /// key 明文（映射键，同时在 [`JwtAuthConfig::api_keys`] 里有 tenant 映射）。
    pub key: String,
    /// 绑定租户（同 legacy 冒号格式的 tenant 段）。
    pub tenant: String,
    /// 调用方业务系统标识（`TenantCtx.system`；None = 未声明 = 归属校验放行）。
    pub system: Option<String>,
    /// 可发起/操作的流程定义 key 白名单（空 = 全部）。命中判定 = 精确全等。
    pub allowed_definition_keys: Vec<String>,
}

impl KeyDecl {
    /// legacy key 的 allow-all 声明（两阶段迁移过渡态；加载时按把数打审计告警）。
    fn allow_all(key: String, tenant: String) -> Self {
        Self {
            key,
            tenant,
            system: None,
            allowed_definition_keys: Vec::new(),
        }
    }

    /// 定义 key 是否在白名单内（空白名单 = 全部放行）。
    pub fn definition_allowed(&self, definition_key: &str) -> bool {
        self.allowed_definition_keys.is_empty()
            || self.allowed_definition_keys.iter().any(|k| k == definition_key)
    }
}

impl JwtAuthConfig {
    /// 经 ConfigManager 读 `[auth]` 段。
    ///
    /// **auth-off 显式 opt-in + fail-fast**（技术债 004 小项，对齐 f407bbb 给 DB 立的先例）：
    /// ConfigManager 已初始化但 `auth.mode` 缺失/为空 → **panic**——「配置缺失静默 Off」等于
    /// 一次配置丢失即无鉴权网关；无鉴权必须是显式选择（`mode = "off"`），未知 mode 值同样
    /// fail-fast（宁严勿漏）。ConfigManager 未初始化（单测/工具形态）→ 维持 Off 空配置不 panic。
    /// 行为变更：部署若既未配 `mode = "jwt"` 也未配 `mode = "off"`，启动后首个鉴权调用即失败，
    /// 须补配置（发布说明义务）。
    pub fn load() -> Self {
        let get = |key: &str| {
            ConfigManager::try_global()
                .and_then(|cm| cm.get_string(key).ok())
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        };
        let mode_value = get("auth.mode");
        if ConfigManager::try_global().is_some() {
            match mode_value.as_deref().map(str::to_ascii_lowercase).as_deref() {
                Some("jwt") | Some("off") => {}
                other => panic!(
                    "auth.mode 配置缺失或非法（当前值: {:?}）。无鉴权必须是显式选择：\
                     请在配置中设置 auth.mode = \"jwt\"（生产）或 auth.mode = \"off\"（显式放弃鉴权）",
                    other
                ),
            }
        }
        let mode = match mode_value.as_deref() {
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
        let (api_keys, api_key_decls, legacy_count) =
            parse_api_keys_declared(get("auth.api_keys").unwrap_or_default());
        if legacy_count > 0 {
            // 两阶段迁移（红队 R21）的过渡态审计告警：legacy key = 隐式 allow-all，
            // 建议迁移为 JSON 结构化声明（最小权限）。
            tracing::warn!(
                legacy_keys = legacy_count,
                total = api_keys.len(),
                "auth.api_keys 存在 {} 把 legacy 格式 key（未声明 system/白名单 = allow-all）\
                 ——建议迁移为 JSON 结构化声明以启用最小权限",
                legacy_count
            );
        }
        Self {
            mode,
            alg,
            decoding_key,
            tenant_claim: get("auth.jwt_tenant_claim").unwrap_or_else(|| "tenant".to_string()),
            roles_claim: get("auth.jwt_roles_claim").unwrap_or_else(|| "roles".to_string()),
            api_keys,
            api_key_decls,
        }
    }
}

/// 解析 `auth.api_keys` → (key→tenant 映射, key→声明映射, legacy 把数)。
///
/// 两形态（首字符判定，与值内容解耦）：
/// - **legacy 逗号格式** `"k1:tenantA,k2"`：解析为 allow-all 声明（system/白名单全空），
///   行为与存量逐字节一致；legacy 把数由调用方打审计告警。
/// - **JSON 数组格式**（技术债 003 结构化）：`[{"key":"k1","tenant":"t1","system":"mdm",
///   "allowedDefinitionKeys":["mdm_x"]}]`——camelCase 字段全部可选（key 必填；tenant
///   缺省 default；其余缺省 = 不限）。`endpoints` 端点类别字段已删（审查 S-02：声明后
///   无任何消费方校验，属虚假安全感——端点级限权有诉求时按路由前缀实现并随 003 二期）。
pub(crate) fn parse_api_keys_declared(
    raw: String,
) -> (HashMap<String, String>, HashMap<String, KeyDecl>, usize) {
    let trimmed = raw.trim();
    if trimmed.starts_with('[') {
        match serde_json::from_str::<Vec<KeyDeclJson>>(trimmed) {
            Ok(decls) => {
                let mut tenants = HashMap::new();
                let mut map = HashMap::new();
                for d in decls {
                    if d.key.trim().is_empty() {
                        continue;
                    }
                    let tenant = d.tenant.unwrap_or_else(|| DEFAULT_TENANT.to_string());
                    tenants.insert(d.key.clone(), tenant.clone());
                    map.insert(
                        d.key.clone(),
                        KeyDecl {
                            key: d.key,
                            tenant,
                            system: d.system.filter(|s| !s.trim().is_empty()),
                            allowed_definition_keys: d.allowed_definition_keys.unwrap_or_default(),
                        },
                    );
                }
                return (tenants, map, 0);
            }
            Err(e) => {
                tracing::error!("auth.api_keys 以 '[' 开头但不是合法的 JSON 声明数组，按空配置处理: {e}");
                return (HashMap::new(), HashMap::new(), 0);
            }
        }
    }
    let map = parse_api_keys(raw);
    let legacy = map.len();
    let decls = map
        .iter()
        .map(|(k, t)| (k.clone(), KeyDecl::allow_all(k.clone(), t.clone())))
        .collect();
    (map, decls, legacy)
}

/// JSON 声明的中间壳（camelCase 对外、缺省宽容）。
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct KeyDeclJson {
    key: String,
    #[serde(default)]
    tenant: Option<String>,
    #[serde(default)]
    system: Option<String>,
    #[serde(default)]
    allowed_definition_keys: Option<Vec<String>>,
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

    #[test]
    fn legacy_keys_parse_as_allow_all_decls() {
        // legacy 逗号格式：解析为 allow-all 声明（system/白名单空），legacy 计数如实上报。
        let (tenants, decls, legacy) =
            parse_api_keys_declared("k1:tenantA, k2".to_string());
        assert_eq!(legacy, 2);
        assert_eq!(tenants.get("k1").map(String::as_str), Some("tenantA"));
        let d = decls.get("k1").expect("legacy key 也应有声明");
        assert_eq!(d.system, None);
        assert!(d.definition_allowed("any_def"));
    }

    #[test]
    fn json_keys_parse_structured_decls() {
        // JSON 声明格式：system/定义白名单逐字段生效；tenant 缺省 default。
        let raw = r#"[{"key":"k_mdm","system":"mdm","allowedDefinitionKeys":["mdm_x"]},
                       {"key":"k_fi","tenant":"t_fi","system":"fi"}]"#
            .to_string();
        let (tenants, decls, legacy) = parse_api_keys_declared(raw);
        assert_eq!(legacy, 0);
        assert_eq!(tenants.get("k_fi").map(String::as_str), Some("t_fi"));
        assert_eq!(tenants.get("k_mdm").map(String::as_str), Some("default"));
        let mdm = decls.get("k_mdm").expect("结构化 key 应有声明");
        assert_eq!(mdm.system.as_deref(), Some("mdm"));
        assert!(mdm.definition_allowed("mdm_x"));
        assert!(!mdm.definition_allowed("fi_y"));
        let fi = decls.get("k_fi").expect("第二把 key 应有声明");
        assert!(fi.allowed_definition_keys.is_empty(), "缺省白名单 = 不限");
    }

    #[test]
    fn malformed_json_array_falls_back_to_empty() {
        // 以 '[' 开头但非法 JSON：按空配置处理（error 日志），不 panic。
        let (tenants, decls, _) = parse_api_keys_declared("[not json".to_string());
        assert!(tenants.is_empty());
        assert!(decls.is_empty());
    }
}
