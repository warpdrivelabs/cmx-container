//! 菜单页读取：每个菜单对应一个 `.json` 文件，支持点分命名空间映射到分层目录；
//! 另支持 `dam:<domain>/<app>[/<module>]` 形式从 DAM 注册表 + 模块 manifest 派生菜单。
//!
//! 复刻 Node `lib/menuPagesStore.js`：`parseMenuRef`（文件读）+ `getDamMenuPageJson`（DAM 派生）。

use serde_json::{json, Value};

use crate::config::data_path;
use crate::dam::store::list_modules;
use crate::error::{PortalError, PortalResult};
use crate::fsutil::read_json;
use crate::meta::module_theme::resolve_module_theme;
use crate::meta::modules::load_module_manifest;
use crate::util::{is_safe_id, is_safe_segment};

/// 资源类型中文名（与 Node resourceCaption 一致，仅列常用）。
fn resource_caption(t: &str) -> &str {
    match t {
        "activities" => "活动入口",
        "menus" => "菜单",
        "htmlPages" => "HTML 页面",
        "htmlPageIndex" => "页面索引",
        "metaDefinitions" => "元数据定义",
        "contextProfiles" => "上下文配置",
        "dictRegistry" => "字典注册",
        "dictEntries" => "字典条目",
        "dictSeeds" => "字典种子",
        "facts" => "事实数据",
        "serviceCatalog" => "服务目录",
        "tools" => "工具",
        other => other,
    }
}

/// 解析 menu 引用为相对路径段（最后一段补 `.json`）。
fn parse_menu_ref(menu_ref: &str) -> PortalResult<Vec<String>> {
    let r = menu_ref.trim();
    if r.is_empty() {
        return Err(PortalError::bad_request("缺少必填查询参数 menu"));
    }
    if !is_safe_id(r) {
        return Err(PortalError::bad_request("menu 仅允许字母、数字、._-，长度 1–128"));
    }
    let segs: Vec<&str> = r.split('.').collect();
    for s in &segs {
        if s.is_empty() {
            return Err(PortalError::bad_request("menu 段不能为空（禁止前导/尾随点或连续点）"));
        }
        if !is_safe_segment(s) {
            return Err(PortalError::bad_request(format!("menu 段非法：\"{s}\"（仅允许字母、数字、_-）")));
        }
    }
    let mut parts: Vec<String> = if segs.len() == 1 {
        vec![format!("{}.json", segs[0])]
    } else {
        let mut middle: Vec<String> = segs[..segs.len() - 1].iter().map(|s| s.to_string()).collect();
        middle.push(format!("{}.json", segs[segs.len() - 1]));
        middle
    };
    let mut rel = vec!["menu-pages".to_string()];
    rel.append(&mut parts);
    Ok(rel)
}

/// 解析 `dam:<domain>/<app>[/<module>]`。
fn parse_dam_menu_ref(menu_name: &str) -> Option<(String, String, String)> {
    let ref_ = menu_name.trim();
    let rest = ref_.strip_prefix("dam:")?;
    let parts: Vec<&str> = rest.split('/').filter(|s| !s.is_empty()).collect();
    let seg = |i: usize| parts.get(i).map(|s| s.to_string()).unwrap_or_default();
    let domain = seg(0);
    if domain.is_empty() || !is_safe_segment(&domain) {
        return None;
    }
    let app = seg(1);
    let module = seg(2);
    if !app.is_empty() && !is_safe_segment(&app) {
        return None;
    }
    if !module.is_empty() && !is_safe_segment(&module) {
        return None;
    }
    Some((domain, app, module))
}

/// 从 manifest.resources.menus 取第一个 menuRef / path。
fn first_menu_ref(manifest: &Value) -> String {
    let menus = manifest.get("resources").and_then(|r| r.get("menus"));
    let list: Vec<Value> = match menus {
        Some(Value::Array(a)) => a.clone(),
        Some(v @ Value::Object(_)) => vec![v.clone()],
        Some(Value::String(s)) => vec![json!({ "path": s })],
        _ => vec![],
    };
    for item in list {
        if let Some(mr) = item.get("menuRef").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
            return mr.to_string();
        }
        if let Some(p) = item.get("path").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
            return menu_ref_from_path(p);
        }
    }
    String::new()
}

/// 从资源 path（`menu-pages/<...>.json`）反推点分 menuRef。
fn menu_ref_from_path(entry_path: &str) -> String {
    let rel = entry_path.trim_start_matches('/').strip_prefix("data/").unwrap_or(entry_path.trim_start_matches('/'));
    let prefix = "menu-pages/";
    if !rel.starts_with(prefix) || !rel.ends_with(".json") {
        return String::new();
    }
    rel[prefix.len()..rel.len() - 5].split('/').filter(|s| !s.is_empty()).collect::<Vec<_>>().join(".")
}

/// 从菜单文档取 items 数组。
fn menu_items_of(doc: &Value) -> Value {
    if doc.is_array() {
        doc.clone()
    } else if let Some(items) = doc.get("items").filter(|v| v.is_array()) {
        items.clone()
    } else {
        json!([])
    }
}

/// 用 manifest.resources 合成一棵资源菜单（无 menus 资源时的回退）。
fn build_dam_resource_menu(manifest: &Value) -> Value {
    let title = manifest
        .get("title")
        .or_else(|| manifest.get("name"))
        .or_else(|| manifest.get("module"))
        .and_then(|v| v.as_str())
        .unwrap_or("模块资源")
        .to_string();
    let domain = manifest.get("domain").and_then(|v| v.as_str()).unwrap_or("");
    let application = manifest.get("application").and_then(|v| v.as_str()).unwrap_or("");
    let module = manifest.get("module").and_then(|v| v.as_str()).unwrap_or("");
    let mut types: Vec<String> = manifest
        .get("resources")
        .and_then(|r| r.as_object())
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default();
    types.sort_by(|a, b| resource_caption(a).cmp(resource_caption(b)));
    let children: Vec<Value> = types
        .iter()
        .map(|t| {
            let cap = resource_caption(t);
            json!({
                "id": format!("resource-{t}"),
                "name": t,
                "caption": cap,
                "icon": "documents",
                "workspace": { "content": { "caption": cap, "icon": "documents", "views": [
                    { "tabLabel": "资源", "type": "json", "data": { "value": {
                        "domain": domain, "application": application, "module": module, "type": t,
                        "resources": manifest.get("resources").and_then(|r| r.get(t)).cloned().unwrap_or(json!([]))
                    }}}
                ]}}
            })
        })
        .collect();
    json!({
        "version": 1, "source": "dam",
        "items": [ { "id": format!("{domain}-{application}-{module}"), "name": module, "caption": title, "icon": "folder", "expanded": true, "children": children } ]
    })
}

/// 构建单模块菜单组。
async fn build_dam_module_group(
    domain: &str,
    application: &str,
    module: &str,
    registry_module: Option<&crate::dam::store::DamModule>,
    index: usize,
) -> PortalResult<Value> {
    let manifest = load_module_manifest(domain, application, module).await.unwrap_or(json!({
        "domain": domain, "application": application, "module": module, "resources": {}
    }));
    let menu_ref = first_menu_ref(&manifest);
    let doc = if !menu_ref.is_empty() {
        // 递归读引用的菜单文件
        get_menu_page_json_inner(&menu_ref, 1).await.unwrap_or_else(|_| build_dam_resource_menu(&manifest))
    } else {
        build_dam_resource_menu(&manifest)
    };
    let key = format!("{domain}/{application}/{module}");
    let theme_color = registry_module
        .map(|m| m.theme_color.clone())
        .filter(|s| !s.is_empty())
        .or_else(|| manifest.get("themeColor").and_then(|v| v.as_str()).map(|s| s.to_string()))
        .unwrap_or_default();
    let raw_theme = registry_module
        .and_then(|m| m.theme.clone())
        .or_else(|| manifest.get("theme").filter(|v| v.is_object()).cloned());
    let theme = resolve_module_theme(&key, raw_theme.as_ref(), index, &theme_color);
    let title = registry_module
        .map(|m| if !m.name.is_empty() { m.name.clone() } else if !m.title.is_empty() { m.title.clone() } else { m.id.clone() })
        .filter(|s| !s.is_empty())
        .or_else(|| manifest.get("name").and_then(|v| v.as_str()).map(|s| s.to_string()))
        .or_else(|| manifest.get("title").and_then(|v| v.as_str()).map(|s| s.to_string()))
        .unwrap_or_else(|| module.to_string());
    let icon = registry_module
        .map(|m| m.icon.clone())
        .filter(|s| !s.is_empty())
        .or_else(|| manifest.get("icon").and_then(|v| v.as_str()).map(|s| s.to_string()))
        .unwrap_or_else(|| "folder".to_string());
    Ok(json!({
        "id": format!("{domain}-{application}-{module}"),
        "domain": domain, "application": application, "module": module,
        "title": title, "icon": icon, "theme": theme,
        "items": menu_items_of(&doc),
    }))
}

/// DAM 派生菜单文档（None 表示非 dam: 引用）。
async fn get_dam_menu_page_json(menu_name: &str) -> PortalResult<Option<Value>> {
    let Some((domain, application, module)) = parse_dam_menu_ref(menu_name) else {
        return Ok(None);
    };
    if application.is_empty() {
        return Err(PortalError::bad_request(format!("DAM 菜单引用需至少包含 domain/application：{menu_name}")));
    }
    if module.is_empty() {
        // 应用级：列出该 app 下所有模块各成一组
        let modules = list_modules(Some(&domain), Some(&application)).await?;
        let mut groups = Vec::new();
        for (i, m) in modules.iter().enumerate() {
            groups.push(build_dam_module_group(&m.domain, &m.application, &m.id, Some(m), i).await?);
        }
        return Ok(Some(json!({
            "version": 1, "source": "dam", "domain": domain, "application": application, "modules": groups
        })));
    }
    // 模块级：单组
    let modules = list_modules(Some(&domain), Some(&application)).await?;
    let reg_mod = modules.iter().find(|m| m.id == module);
    let group = build_dam_module_group(&domain, &application, &module, reg_mod, 0).await?;
    Ok(Some(json!({
        "version": 1, "source": "dam", "domain": domain, "application": application, "modules": [group]
    })))
}

/// 内部读取实现（带递归深度保护，避免 menu→module→menu 循环）。
/// 返回 boxed future：async fn 自递归（经 build_dam_module_group）需要装箱。
fn get_menu_page_json_inner(
    menu_name: &str,
    depth: u8,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = PortalResult<Value>> + Send + '_>> {
    Box::pin(async move {
        if depth == 0 {
            // 顶层才尝试 DAM 派生；递归读引用菜单时只走文件，避免无限递归
            if let Some(doc) = get_dam_menu_page_json(menu_name).await? {
                return Ok(doc);
            }
        }
        let rel = parse_menu_ref(menu_name)?;
        let path = data_path(rel);
        match read_json::<serde_json::Value>(&path).await {
            Ok(v) => Ok(v),
            Err(PortalError::NotFound(_)) => Err(PortalError::not_found(format!("菜单数据不存在：{menu_name}"))),
            Err(e) => Err(e),
        }
    })
}

/// 读取菜单 JSON 文档（DAM 派生优先，回退文件）。
pub async fn get_menu_page_json(menu_name: &str) -> PortalResult<serde_json::Value> {
    get_menu_page_json_inner(menu_name, 0).await
}
