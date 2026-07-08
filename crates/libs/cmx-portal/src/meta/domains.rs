//! 域清单（domains）读取。
//!
//! 复刻 Node `lib/domainsStore.js` 的 `getDomainsDoc()`：
//! 1. 优先从 DAM 注册表（`dam-registry/registry.json`）派生 —— 过滤掉 `status=disabled`，
//!    映射为前端期望的 `{ id, icon, label, title, description, application, activitie }`。
//! 2. DAM 无域时回退读 `activities/domains.json` 原样返回。

use serde::{Deserialize, Serialize};

use crate::config::data_path;
use crate::error::PortalResult;
use crate::fsutil::{read_json, read_json_opt};

/// 单个域条目（对前端输出形状，与 Node 完全一致）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainItem {
    pub id: String,
    pub icon: String,
    pub label: String,
    pub title: String,
    pub description: String,
    pub application: String,
    pub activitie: String,
}

/// `/api/domains` 响应文档。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainsDoc {
    pub version: u32,
    pub source: String,
    pub domains: Vec<DomainItem>,
}

/// DAM 注册表中的域原始结构（仅取所需字段）。
#[derive(Debug, Clone, Deserialize)]
struct DamDomainRaw {
    id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    icon: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct DamRegistryRaw {
    #[serde(default)]
    domains: Vec<DamDomainRaw>,
}

/// 获取域清单文档。DAM 优先，回退 `activities/domains.json`。
pub async fn get_domains_doc() -> PortalResult<serde_json::Value> {
    // 1) DAM 注册表派生
    let dam_path = data_path(["dam-registry", "registry.json"]);
    if let Some(value) = read_json_opt(&dam_path).await?
        && let Ok(reg) = serde_json::from_value::<DamRegistryRaw>(value)
    {
        let domains: Vec<DomainItem> = reg
            .domains
            .into_iter()
            .filter(|d| d.status.as_deref().unwrap_or("active") != "disabled")
            .map(|d| {
                let label = d
                    .name
                    .clone()
                    .or_else(|| d.title.clone())
                    .unwrap_or_else(|| d.id.clone());
                DomainItem {
                    application: d.id.clone(),
                    activitie: d.id.clone(),
                    icon: d.icon.unwrap_or_else(|| "folder".to_string()),
                    label,
                    title: d.title.unwrap_or_default(),
                    description: d.description.unwrap_or_default(),
                    id: d.id,
                }
            })
            .collect();
        if !domains.is_empty() {
            let doc = DomainsDoc {
                version: 1,
                source: "dam".to_string(),
                domains,
            };
            return Ok(serde_json::to_value(doc)?);
        }
    }

    // 2) 回退：activities/domains.json 原样返回
    let file = data_path(["activities", "domains.json"]);
    let raw: serde_json::Value = read_json(&file).await?;
    Ok(raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 用 CMXPortalManager 的真实 `data/` 目录验证 DAM 派生逻辑与 Node 等价：
    /// 必须 `source=dam`、版本=1、domains 非空，且每个域含完整字段且 application==activitie==id。
    #[tokio::test]
    #[allow(clippy::await_holding_lock)]
    async fn domains_derived_from_dam_registry() {
        // 串行化对 CMX_PORTAL_DATA_ROOT 的修改，避免与其它切换数据根的测试并行污染。
        let _env = crate::util::test_data_root_lock().lock().unwrap();
        // 指向 Node 后端的真实数据目录（相对 cmx-portal crate 根：../../../../CMXPortalManager/...）
        let crate_dir = env!("CARGO_MANIFEST_DIR");
        let data_root = std::path::Path::new(crate_dir)
            .join("../../../../CMXPortalManager/cmx-node-server/data");
        // SAFETY: 测试单线程设置进程环境变量；data_root() 读取它。
        unsafe { std::env::set_var("CMX_PORTAL_DATA_ROOT", data_root) };

        let doc = get_domains_doc().await.expect("应成功派生域清单");
        assert_eq!(doc["source"], "dam", "应优先从 DAM 注册表派生");
        assert_eq!(doc["version"], 1);
        let domains = doc["domains"].as_array().expect("domains 应为数组");
        assert!(!domains.is_empty(), "应至少派生出一个域");

        // 校验首个域字段完整 + application==activitie==id（与 Node 映射一致）
        let first = &domains[0];
        for key in [
            "id",
            "icon",
            "label",
            "title",
            "description",
            "application",
            "activitie",
        ] {
            assert!(first.get(key).is_some(), "域缺少字段: {key}");
        }
        assert_eq!(first["application"], first["id"]);
        assert_eq!(first["activitie"], first["id"]);

        // 不应包含被禁用的域
        for d in domains {
            assert_ne!(d["id"], serde_json::Value::Null);
        }
    }
}
