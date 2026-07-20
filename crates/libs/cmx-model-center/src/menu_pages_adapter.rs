//! menu-pages JSON（前端真源格式）→ MenuDefinition（后端结构体）适配层。
//!
//! 字段映射要点：
//! - 顶层 {version, items:[...]} → 取 items 数组（version 丢弃）
//! - 节点 id → code；caption → name；name 字段（前端短名）丢弃
//! - permissionId → fun_code（null → None）
//! - icon 直传；workspace 整体作为 definition JSONB 透传
//! - expanded/dirty 是前端运行时态，丢弃
//! - children 递归 flatten，parent_code 由父节点 code 注入
//! - sort_order 按数组下标（每个父节点下从 0 重新计数）

use anyhow::{anyhow, Result};
use cmx_core::model::module::MenuDefinition;

/// 解析 menu-pages JSON，返回扁平化 MenuDefinition 列表
pub fn parse_menu_pages_file(
    raw: &str,
    domain: &str,
    app: &str,
    module: &str,
) -> Result<Vec<MenuDefinition>> {
    let parsed: serde_json::Value =
        serde_json::from_str(raw).map_err(|e| anyhow!("menu-pages JSON 解析失败: {e}"))?;
    let roots = parsed
        .get("items")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow!("menu-pages JSON 缺少 items 数组"))?;
    let mut out = Vec::with_capacity(roots.len() * 4);
    for (sort_idx, root) in roots.iter().enumerate() {
        flatten_node(root, None, domain, app, module, sort_idx as i32, &mut out);
    }
    Ok(out)
}

fn flatten_node(
    v: &serde_json::Value,
    parent_code: Option<&str>,
    domain: &str,
    app: &str,
    module: &str,
    sort_order: i32,
    out: &mut Vec<MenuDefinition>,
) {
    let code = v
        .get("id")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let def = MenuDefinition {
        code: code.clone(),
        name: v
            .get("caption")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string(),
        parent_code: parent_code.map(String::from),
        description: None,
        path: None,
        icon: v.get("icon").and_then(|x| x.as_str()).map(String::from),
        component: None,
        sort_order,
        visible: 1,
        open_type: 0,
        fun_code: v
            .get("permissionId")
            .and_then(|x| if x.is_null() { None } else { x.as_str() })
            .map(String::from),
        definition: v.get("workspace").cloned(), // 整体 JSONB 透传
        ext_attributes: None,
        children: vec![],
        domain_code: domain.to_string(),
        application_code: app.to_string(),
        module_code: module.to_string(),
    };
    out.push(def);

    if let Some(children) = v.get("children").and_then(|c| c.as_array()) {
        for (idx, child) in children.iter().enumerate() {
            flatten_node(child, Some(&code), domain, app, module, idx as i32, out);
        }
    }
}
