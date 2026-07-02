//! DAM 注册表 store 实现。

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::config::{data_path, data_root};
use crate::error::{PortalError, PortalResult};
use crate::fsutil::{read_json_opt, write_json_atomic};
use crate::util::write_lock;

/// DAM 树根：upsert 时在每个根下创建 `<domain>[/<app>[/<module>]]` 目录；改名时整体搬移。
const DAM_TREE_ROOTS: &[&[&str]] = &[
    &["dict", "entries"],
    &["dict", "seeds"],
    &["fact"],
    &["meta", "definitions"],
    &["meta", "context-profile"],
    &["form-pages", "sources"],
    &["html-pages", "sources"],
    &["menu-pages"],
    &["modules"],
    &["native-pages", "sources"],
    &["service-catalog"],
];

/// id 段：`[a-zA-Z0-9_-]{1,64}`。
fn is_dam_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn assert_id(field: &str, value: &str) -> PortalResult<String> {
    let s = value.trim();
    if !is_dam_id(s) {
        return Err(PortalError::bad_request(format!(
            "{field} 仅允许字母、数字、_-，长度 1-64"
        )));
    }
    Ok(s.to_string())
}

fn clean_text(v: Option<&str>) -> String {
    v.unwrap_or("").trim().to_string()
}

fn clean_status(v: Option<&str>) -> String {
    let s = v.unwrap_or("active").trim();
    if s.is_empty() {
        "active".to_string()
    } else {
        s.to_string()
    }
}

// ───────────────────────── 实体结构 ─────────────────────────

/// 域。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DamDomain {
    pub id: String,
    pub name: String,
    pub title: String,
    pub icon: String,
    pub status: String,
    pub description: String,
}

/// 应用。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DamApplication {
    pub domain: String,
    pub id: String,
    pub name: String,
    pub title: String,
    pub icon: String,
    pub status: String,
    pub description: String,
}

/// 模块（含 app/module 别名字段，与 Node 输出一致）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DamModule {
    pub domain: String,
    pub application: String,
    pub app: String,
    pub id: String,
    pub module: String,
    pub name: String,
    pub title: String,
    pub icon: String,
    pub status: String,
    pub description: String,
    #[serde(rename = "resourceRoot")]
    pub resource_root: String,
    #[serde(rename = "manifestPath")]
    pub manifest_path: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<serde_json::Value>,
    #[serde(rename = "themeColor")]
    pub theme_color: String,
}

/// 规范化后的完整注册表。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DamRegistry {
    pub version: u32,
    pub domains: Vec<DamDomain>,
    pub applications: Vec<DamApplication>,
    pub modules: Vec<DamModule>,
}

fn registry_path() -> std::path::PathBuf {
    data_path(["dam-registry", "registry.json"])
}

// ───────────────────────── normalize ─────────────────────────

fn str_field<'a>(v: &'a serde_json::Value, keys: &[&str]) -> Option<&'a str> {
    for k in keys {
        if let Some(s) = v
            .get(*k)
            .and_then(|x| x.as_str())
            .filter(|s| !s.trim().is_empty())
        {
            return Some(s);
        }
    }
    None
}

fn normalize(doc: &serde_json::Value) -> PortalResult<DamRegistry> {
    let empty = vec![];
    let domains_raw = doc
        .get("domains")
        .and_then(|v| v.as_array())
        .unwrap_or(&empty);
    let apps_raw = doc
        .get("applications")
        .and_then(|v| v.as_array())
        .unwrap_or(&empty);
    let mods_raw = doc
        .get("modules")
        .and_then(|v| v.as_array())
        .unwrap_or(&empty);

    let mut domains = Vec::with_capacity(domains_raw.len());
    for d in domains_raw {
        let id = assert_id("domain.id", str_field(d, &["id"]).unwrap_or(""))?;
        domains.push(DamDomain {
            name: clean_text(str_field(d, &["name", "label", "id"])),
            title: clean_text(str_field(d, &["title"])),
            icon: clean_text(str_field(d, &["icon"])),
            status: clean_status(d.get("status").and_then(|v| v.as_str())),
            description: clean_text(str_field(d, &["description"])),
            id,
        });
    }

    let mut applications = Vec::with_capacity(apps_raw.len());
    for a in apps_raw {
        let domain = assert_id(
            "application.domain",
            str_field(a, &["domain"]).unwrap_or(""),
        )?;
        let id = assert_id(
            "application.id",
            str_field(a, &["id", "application", "app"]).unwrap_or(""),
        )?;
        applications.push(DamApplication {
            name: clean_text(str_field(a, &["name", "label", "id", "application", "app"])),
            title: clean_text(str_field(a, &["title"])),
            icon: clean_text(str_field(a, &["icon"])),
            status: clean_status(a.get("status").and_then(|v| v.as_str())),
            description: clean_text(str_field(a, &["description"])),
            domain,
            id,
        });
    }

    let mut modules = Vec::with_capacity(mods_raw.len());
    for m in mods_raw {
        let domain = assert_id("module.domain", str_field(m, &["domain"]).unwrap_or(""))?;
        let application = assert_id(
            "module.application",
            str_field(m, &["application", "app"]).unwrap_or(""),
        )?;
        let id = assert_id("module.id", str_field(m, &["id", "module"]).unwrap_or(""))?;
        let aliases = m
            .get("aliases")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|x| x.as_str().map(|s| s.trim().to_string()))
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        let theme = m.get("theme").filter(|v| v.is_object()).cloned();
        modules.push(DamModule {
            name: clean_text(str_field(m, &["name", "label", "title", "id", "module"])),
            title: clean_text(str_field(m, &["title", "name", "id", "module"])),
            icon: clean_text(str_field(m, &["icon"])),
            status: clean_status(m.get("status").and_then(|v| v.as_str())),
            description: clean_text(str_field(m, &["description"])),
            resource_root: {
                let rr = clean_text(str_field(m, &["resourceRoot"]));
                if rr.is_empty() {
                    format!("{domain}/{application}/{id}")
                } else {
                    rr
                }
            },
            manifest_path: {
                let mp = clean_text(str_field(m, &["manifestPath"]));
                if mp.is_empty() {
                    format!("modules/{domain}/{application}/{id}/module.json")
                } else {
                    mp
                }
            },
            aliases,
            theme,
            theme_color: clean_text(str_field(m, &["themeColor", "accentColor", "color"])),
            app: application.clone(),
            module: id.clone(),
            domain,
            application,
            id,
        });
    }

    Ok(DamRegistry {
        version: 1,
        domains,
        applications,
        modules,
    })
}

async fn load_registry() -> PortalResult<DamRegistry> {
    let doc = read_json_opt(&registry_path()).await?.unwrap_or_else(
        || json!({ "version": 1, "domains": [], "applications": [], "modules": [] }),
    );
    normalize(&doc)
}

async fn save_registry(reg: &DamRegistry) -> PortalResult<()> {
    write_json_atomic(&registry_path(), reg, true).await
}

// ───────────────────────── 目录同步 ─────────────────────────

/// 在每个 DAM 树根下创建 `parts` 目录（parts 已校验）。
async fn ensure_tree_dirs(parts: &[String]) -> PortalResult<()> {
    for seg in parts {
        assert_id("path.segment", seg)?;
    }
    for root in DAM_TREE_ROOTS {
        let mut p = data_root();
        for r in *root {
            p.push(r);
        }
        for seg in parts {
            p.push(seg);
        }
        tokio::fs::create_dir_all(&p)
            .await
            .map_err(PortalError::Io)?;
    }
    Ok(())
}

/// 为整份注册表创建所有层级目录。
async fn ensure_registry_dirs(reg: &DamRegistry) -> PortalResult<()> {
    for d in &reg.domains {
        ensure_tree_dirs(&[d.id.clone()]).await?;
    }
    for a in &reg.applications {
        ensure_tree_dirs(&[a.domain.clone(), a.id.clone()]).await?;
    }
    for m in &reg.modules {
        ensure_tree_dirs(&[m.domain.clone(), m.application.clone(), m.id.clone()]).await?;
    }
    Ok(())
}

/// 递归把 from 目录内容并入 to（已存在的子目录递归合并；冲突文件报错）。
async fn move_dir_contents(from: &std::path::Path, to: &std::path::Path) -> PortalResult<()> {
    if tokio::fs::metadata(from).await.is_err() {
        return Ok(());
    }
    tokio::fs::create_dir_all(to)
        .await
        .map_err(PortalError::Io)?;
    // 用栈做迭代式递归，避免 async 递归装箱
    let mut stack = vec![(from.to_path_buf(), to.to_path_buf())];
    while let Some((fd, td)) = stack.pop() {
        let mut rd = match tokio::fs::read_dir(&fd).await {
            Ok(rd) => rd,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(PortalError::Io(e)),
        };
        tokio::fs::create_dir_all(&td)
            .await
            .map_err(PortalError::Io)?;
        while let Some(entry) = rd.next_entry().await.map_err(PortalError::Io)? {
            let from_path = entry.path();
            let to_path = td.join(entry.file_name());
            let ft = entry.file_type().await.map_err(PortalError::Io)?;
            let to_exists = tokio::fs::metadata(&to_path).await.is_ok();
            let to_is_dir = tokio::fs::metadata(&to_path)
                .await
                .map(|m| m.is_dir())
                .unwrap_or(false);
            if ft.is_dir() && to_exists && to_is_dir {
                stack.push((from_path, to_path));
                continue;
            }
            if to_exists {
                return Err(PortalError::bad_request(format!(
                    "目标路径已存在，不能覆盖：{}",
                    to_path.display()
                )));
            }
            tokio::fs::rename(&from_path, &to_path)
                .await
                .map_err(PortalError::Io)?;
        }
        // 该层处理完后删源目录（叶子在出栈时已空）
        let _ = tokio::fs::remove_dir_all(&fd).await;
    }
    Ok(())
}

/// 把每个 DAM 树根下 from_parts 目录搬到 to_parts。
async fn rename_tree_dirs(from_parts: &[String], to_parts: &[String]) -> PortalResult<()> {
    for seg in from_parts.iter().chain(to_parts.iter()) {
        assert_id("path.segment", seg)?;
    }
    if from_parts.join("/") == to_parts.join("/") {
        return Ok(());
    }
    for root in DAM_TREE_ROOTS {
        let mut from = data_root();
        let mut to = data_root();
        for r in *root {
            from.push(r);
            to.push(r);
        }
        for seg in from_parts {
            from.push(seg);
        }
        for seg in to_parts {
            to.push(seg);
        }
        move_dir_contents(&from, &to).await?;
    }
    Ok(())
}

// ───────────────────────── 读 ─────────────────────────

/// 完整注册表。
pub async fn get_dam_registry() -> PortalResult<DamRegistry> {
    load_registry().await
}

/// 域列表。
pub async fn list_domains() -> PortalResult<Vec<DamDomain>> {
    Ok(load_registry().await?.domains)
}

/// 应用列表（按 domain 过滤）。
pub async fn list_applications(domain: Option<&str>) -> PortalResult<Vec<DamApplication>> {
    let d = domain.unwrap_or("").trim();
    Ok(load_registry()
        .await?
        .applications
        .into_iter()
        .filter(|a| d.is_empty() || a.domain == d)
        .collect())
}

/// 模块列表（按 domain/application 过滤）。
pub async fn list_modules(
    domain: Option<&str>,
    application: Option<&str>,
) -> PortalResult<Vec<DamModule>> {
    let d = domain.unwrap_or("").trim();
    let a = application.unwrap_or("").trim();
    Ok(load_registry()
        .await?
        .modules
        .into_iter()
        .filter(|m| (d.is_empty() || m.domain == d) && (a.is_empty() || m.application == a))
        .collect())
}

// ───────────────────────── upsert / delete ─────────────────────────

fn parse_original_key(v: Option<&str>) -> Vec<String> {
    v.unwrap_or("")
        .split('/')
        .map(|x| x.trim().to_string())
        .filter(|x| !x.is_empty())
        .collect()
}

/// upsert 域（含改名级联 + 目录搬移）。返回保存后的域。
pub async fn upsert_domain(input: &serde_json::Value) -> PortalResult<DamDomain> {
    let _guard = write_lock().lock().await;
    let mut reg = load_registry().await?;
    let original = parse_original_key(input.get("originalKey").and_then(|v| v.as_str()));

    let item = DamDomain {
        id: assert_id("domain.id", str_field(input, &["id"]).unwrap_or(""))?,
        name: clean_text(str_field(input, &["name", "label", "id"])),
        title: clean_text(str_field(input, &["title"])),
        icon: clean_text(str_field(input, &["icon"])),
        status: clean_status(input.get("status").and_then(|v| v.as_str())),
        description: clean_text(str_field(input, &["description"])),
    };
    let old_id = match original.first() {
        Some(s) => assert_id("domain.originalKey", s)?,
        None => item.id.clone(),
    };
    if old_id != item.id && reg.domains.iter().any(|d| d.id == item.id) {
        return Err(PortalError::bad_request(format!(
            "Domain 已存在：{}",
            item.id
        )));
    }
    if let Some(existing) = reg.domains.iter_mut().find(|d| d.id == old_id) {
        *existing = item.clone();
    } else {
        reg.domains.push(item.clone());
    }
    if old_id != item.id {
        for a in reg.applications.iter_mut().filter(|a| a.domain == old_id) {
            a.domain = item.id.clone();
        }
        for m in reg.modules.iter_mut().filter(|m| m.domain == old_id) {
            let rr_old = format!("{}/{}/{}", old_id, m.application, m.id);
            let mp_old = format!("modules/{}/{}/{}/module.json", old_id, m.application, m.id);
            if m.resource_root == rr_old {
                m.resource_root = format!("{}/{}/{}", item.id, m.application, m.id);
            }
            if m.manifest_path == mp_old {
                m.manifest_path =
                    format!("modules/{}/{}/{}/module.json", item.id, m.application, m.id);
            }
            m.domain = item.id.clone();
        }
        rename_tree_dirs(&[old_id.clone()], &[item.id.clone()]).await?;
    }
    ensure_registry_dirs(&reg).await?;
    save_registry(&reg).await?;
    Ok(item)
}

/// upsert 应用（含改名级联 + 目录搬移）。
pub async fn upsert_application(input: &serde_json::Value) -> PortalResult<DamApplication> {
    let _guard = write_lock().lock().await;
    let mut reg = load_registry().await?;
    let original = parse_original_key(input.get("originalKey").and_then(|v| v.as_str()));

    let item = DamApplication {
        domain: assert_id(
            "application.domain",
            str_field(input, &["domain"]).unwrap_or(""),
        )?,
        id: assert_id(
            "application.id",
            str_field(input, &["id", "application", "app"]).unwrap_or(""),
        )?,
        name: clean_text(str_field(
            input,
            &["name", "label", "id", "application", "app"],
        )),
        title: clean_text(str_field(input, &["title"])),
        icon: clean_text(str_field(input, &["icon"])),
        status: clean_status(input.get("status").and_then(|v| v.as_str())),
        description: clean_text(str_field(input, &["description"])),
    };
    if !reg.domains.iter().any(|d| d.id == item.domain) {
        return Err(PortalError::bad_request(format!(
            "Domain 不存在：{}",
            item.domain
        )));
    }
    let old_domain = match original.first() {
        Some(s) => assert_id("application.originalDomain", s)?,
        None => item.domain.clone(),
    };
    let old_app = match original.get(1) {
        Some(s) => assert_id("application.originalId", s)?,
        None => item.id.clone(),
    };
    let new_key = format!("{}/{}", item.domain, item.id);
    if (old_domain != item.domain || old_app != item.id)
        && reg
            .applications
            .iter()
            .any(|a| format!("{}/{}", a.domain, a.id) == new_key)
    {
        return Err(PortalError::bad_request(format!(
            "Application 已存在：{new_key}"
        )));
    }
    if let Some(existing) = reg
        .applications
        .iter_mut()
        .find(|a| a.domain == old_domain && a.id == old_app)
    {
        *existing = item.clone();
    } else {
        reg.applications.push(item.clone());
    }
    if old_domain != item.domain || old_app != item.id {
        for m in reg
            .modules
            .iter_mut()
            .filter(|m| m.domain == old_domain && m.application == old_app)
        {
            let rr_old = format!("{}/{}/{}", old_domain, old_app, m.id);
            let mp_old = format!("modules/{}/{}/{}/module.json", old_domain, old_app, m.id);
            if m.resource_root == rr_old {
                m.resource_root = format!("{}/{}/{}", item.domain, item.id, m.id);
            }
            if m.manifest_path == mp_old {
                m.manifest_path =
                    format!("modules/{}/{}/{}/module.json", item.domain, item.id, m.id);
            }
            m.domain = item.domain.clone();
            m.application = item.id.clone();
            m.app = item.id.clone();
        }
        rename_tree_dirs(
            &[old_domain.clone(), old_app.clone()],
            &[item.domain.clone(), item.id.clone()],
        )
        .await?;
    }
    ensure_registry_dirs(&reg).await?;
    save_registry(&reg).await?;
    Ok(item)
}

/// upsert 模块（含改名 + 目录搬移）。
pub async fn upsert_module(input: &serde_json::Value) -> PortalResult<DamModule> {
    let _guard = write_lock().lock().await;
    let mut reg = load_registry().await?;
    let original = parse_original_key(input.get("originalKey").and_then(|v| v.as_str()));

    let domain = assert_id("module.domain", str_field(input, &["domain"]).unwrap_or(""))?;
    let application = assert_id(
        "module.application",
        str_field(input, &["application", "app"]).unwrap_or(""),
    )?;
    let id = assert_id(
        "module.id",
        str_field(input, &["id", "module"]).unwrap_or(""),
    )?;
    if !reg.domains.iter().any(|d| d.id == domain) {
        return Err(PortalError::bad_request(format!("Domain 不存在：{domain}")));
    }
    if !reg
        .applications
        .iter()
        .any(|a| a.domain == domain && a.id == application)
    {
        return Err(PortalError::bad_request(format!(
            "Application 不存在：{domain}/{application}"
        )));
    }
    let resource_root = {
        let rr = clean_text(str_field(input, &["resourceRoot"]));
        if rr.is_empty() {
            format!("{domain}/{application}/{id}")
        } else {
            rr
        }
    };
    let manifest_path = {
        let mp = clean_text(str_field(input, &["manifestPath"]));
        if mp.is_empty() {
            format!("modules/{domain}/{application}/{id}/module.json")
        } else {
            mp
        }
    };
    let aliases = input
        .get("aliases")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str().map(|s| s.trim().to_string()))
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();
    let item = DamModule {
        name: clean_text(str_field(
            input,
            &["name", "label", "title", "id", "module"],
        )),
        title: clean_text(str_field(input, &["title", "name", "id", "module"])),
        icon: clean_text(str_field(input, &["icon"])),
        status: clean_status(input.get("status").and_then(|v| v.as_str())),
        description: clean_text(str_field(input, &["description"])),
        resource_root,
        manifest_path,
        aliases,
        theme: input.get("theme").filter(|v| v.is_object()).cloned(),
        theme_color: clean_text(str_field(input, &["themeColor", "accentColor", "color"])),
        app: application.clone(),
        module: id.clone(),
        domain: domain.clone(),
        application: application.clone(),
        id: id.clone(),
    };
    let old_domain = original
        .first()
        .map(|s| assert_id("module.originalDomain", s))
        .transpose()?
        .unwrap_or_else(|| domain.clone());
    let old_app = original
        .get(1)
        .map(|s| assert_id("module.originalApplication", s))
        .transpose()?
        .unwrap_or_else(|| application.clone());
    let old_id = original
        .get(2)
        .map(|s| assert_id("module.originalId", s))
        .transpose()?
        .unwrap_or_else(|| id.clone());
    let new_key = format!("{}/{}/{}", domain, application, id);
    if (old_domain != domain || old_app != application || old_id != id)
        && reg
            .modules
            .iter()
            .any(|m| format!("{}/{}/{}", m.domain, m.application, m.id) == new_key)
    {
        return Err(PortalError::bad_request(format!(
            "Module 已存在：{new_key}"
        )));
    }
    if let Some(existing) = reg
        .modules
        .iter_mut()
        .find(|m| m.domain == old_domain && m.application == old_app && m.id == old_id)
    {
        *existing = item.clone();
    } else {
        reg.modules.push(item.clone());
    }
    if old_domain != domain || old_app != application || old_id != id {
        rename_tree_dirs(
            &[old_domain.clone(), old_app.clone(), old_id.clone()],
            &[domain.clone(), application.clone(), id.clone()],
        )
        .await?;
    }
    ensure_registry_dirs(&reg).await?;
    save_registry(&reg).await?;
    Ok(item)
}

/// 同步所有 DAM 目录（按当前注册表补建）。
pub async fn sync_dirs() -> PortalResult<serde_json::Value> {
    let _guard = write_lock().lock().await;
    let reg = load_registry().await?;
    ensure_registry_dirs(&reg).await?;
    Ok(json!({ "ok": true }))
}

/// 删除域（要求其下无 app/module）。
pub async fn delete_domain(id: &str) -> PortalResult<serde_json::Value> {
    let domain = assert_id("domain.id", id)?;
    let _guard = write_lock().lock().await;
    let mut reg = load_registry().await?;
    if reg.applications.iter().any(|a| a.domain == domain)
        || reg.modules.iter().any(|m| m.domain == domain)
    {
        return Err(PortalError::bad_request(format!(
            "Domain {domain} 下仍有 application/module，不能删除"
        )));
    }
    let before = reg.domains.len();
    reg.domains.retain(|d| d.id != domain);
    if reg.domains.len() == before {
        return Err(PortalError::not_found(format!("Domain 不存在：{domain}")));
    }
    save_registry(&reg).await?;
    Ok(json!({ "ok": true }))
}

/// 删除应用（要求其下无 module）。
pub async fn delete_application(domain: &str, app: &str) -> PortalResult<serde_json::Value> {
    let domain = assert_id("application.domain", domain)?;
    let id = assert_id("application.id", app)?;
    let _guard = write_lock().lock().await;
    let mut reg = load_registry().await?;
    if reg
        .modules
        .iter()
        .any(|m| m.domain == domain && m.application == id)
    {
        return Err(PortalError::bad_request(format!(
            "Application {domain}/{id} 下仍有 module，不能删除"
        )));
    }
    let before = reg.applications.len();
    reg.applications
        .retain(|a| !(a.domain == domain && a.id == id));
    if reg.applications.len() == before {
        return Err(PortalError::not_found(format!(
            "Application 不存在：{domain}/{id}"
        )));
    }
    save_registry(&reg).await?;
    Ok(json!({ "ok": true }))
}

/// 删除模块。
pub async fn delete_module(
    domain: &str,
    app: &str,
    module: &str,
) -> PortalResult<serde_json::Value> {
    let domain = assert_id("module.domain", domain)?;
    let application = assert_id("module.application", app)?;
    let id = assert_id("module.id", module)?;
    let _guard = write_lock().lock().await;
    let mut reg = load_registry().await?;
    let before = reg.modules.len();
    reg.modules
        .retain(|m| !(m.domain == domain && m.application == application && m.id == id));
    if reg.modules.len() == before {
        return Err(PortalError::not_found(format!(
            "Module 不存在：{domain}/{application}/{id}"
        )));
    }
    save_registry(&reg).await?;
    Ok(json!({ "ok": true }))
}
