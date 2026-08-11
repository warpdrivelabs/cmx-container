//! menu-pages JSON（前端真源格式）-> MenuDefinition（后端结构体）适配层。
//!
//! 字段映射要点：
//! - 顶层 {version, items:[...]} -> 取 items 数组（version 丢弃）
//! - 节点 id -> code；caption -> name；name 字段（前端短名）丢弃
//! - permissionId -> fun_code（null -> None）
//! - icon 直传
//! - definition JSONB 组装 caption/workspace/dialogspace/expanded/type/name
//!   （与 .agents/skills/menu-generator/gen_menu_migration.mjs 逻辑一致，
//!   前端读 definition.caption/definition.dialogspace 等字段渲染菜单）
//! - expanded/dirty 是前端运行时态，但 expanded 仍入 definition（与迁移脚本一致）
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

/// 组装 definition JSONB，与 gen_menu_migration.mjs 的 definition 生成逻辑一致。
///
/// 收集 caption/workspace/dialogspace/expanded/type/name 六个字段（值为 null 的跳过），
/// 全部缺失时返回 None。
fn build_definition(v: &serde_json::Value) -> Option<serde_json::Value> {
    let mut def = serde_json::Map::new();
    // 与前端菜单渲染约定的 6 个 key 集合：caption 标题 / workspace 工作区标识 /
    // dialogspace 对话框标识 / expanded 是否展开 / type 菜单类型 / name 前端短名
    for key in &["caption", "workspace", "dialogspace", "expanded", "type", "name"] {
        // 只收集非 null 的字段（与迁移脚本行为对齐——null 表示"无配置"而非"空串"）
        if let Some(val) = v.get(*key)
            && !val.is_null()
        {
            def.insert((*key).to_string(), val.clone());
        }
    }
    // 6 个 key 全缺失时返回 None（让 MenuDefinition.definition = None，区分"有但全空"和"无 definition"）
    if def.is_empty() {
        None
    } else {
        Some(serde_json::Value::Object(def))
    }
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
        visible: v["visible"].as_i64().unwrap_or(1) as i32,
        open_type: 0,
        fun_code: v
            .get("permissionId")
            .and_then(|x| if x.is_null() { None } else { x.as_str() })
            .map(String::from),
        definition: build_definition(v),
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
