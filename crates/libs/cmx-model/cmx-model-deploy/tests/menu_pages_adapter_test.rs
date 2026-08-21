use cmx_model_deploy::menu_pages_adapter::parse_menu_pages_file;
use serde_json::json;

#[test]
fn test_parses_items_and_maps_fields() {
    let raw = json!({
        "version": 1,
        "items": [
            {
                "id": "root1",
                "name": "short_name_ignored",
                "caption": "根节点1",
                "permissionId": "perm.root1",
                "icon": "tabler-outline/home",
                "expanded": true,
                "workspace": { "content": { "caption": "视图1" } },
                "children": [
                    { "id": "c1", "caption": "子1", "icon": "add" },
                    { "id": "c2", "caption": "子2", "children": [
                        { "id": "g1", "caption": "孙1" }
                    ]}
                ]
            }
        ]
    })
    .to_string();

    let defs = parse_menu_pages_file(&raw, "fi", "cmxfico", "gl").unwrap();
    assert_eq!(defs.len(), 4); // root1 + c1 + c2 + g1

    let root1 = defs.iter().find(|d| d.code == "root1").unwrap();
    assert_eq!(root1.name, "根节点1"); // caption → name
    assert_eq!(root1.fun_code, Some("perm.root1".to_string()));
    assert_eq!(root1.icon, Some("tabler-outline/home".to_string()));
    assert_eq!(root1.domain_code, "fi");
    assert_eq!(root1.application_code, "cmxfico");
    assert_eq!(root1.module_code, "gl");
    assert_eq!(root1.parent_code, None);
    assert_eq!(root1.visible, 1);
    assert_eq!(root1.open_type, 0);
    assert!(root1.definition.is_some()); // workspace 透传（definition 平铺 6 key，workspace 为其一）
    assert_eq!(
        root1.definition.as_ref().unwrap()["workspace"]["content"]["caption"],
        "视图1"
    );

    let c1 = defs.iter().find(|d| d.code == "c1").unwrap();
    assert_eq!(c1.parent_code, Some("root1".to_string()));

    let g1 = defs.iter().find(|d| d.code == "g1").unwrap();
    assert_eq!(g1.parent_code, Some("c2".to_string()));
}

#[test]
fn test_handles_null_permission_id() {
    let raw = json!({
        "items": [{ "id": "x", "caption": "X", "permissionId": null }]
    })
    .to_string();
    let defs = parse_menu_pages_file(&raw, "fi", "cmxfico", "gl").unwrap();
    assert_eq!(defs[0].fun_code, None);
}

#[test]
fn test_missing_items_returns_error() {
    let raw = r#"{ "version": 1 }"#;
    let result = parse_menu_pages_file(raw, "fi", "cmxfico", "gl");
    assert!(result.is_err());
}

#[test]
fn test_empty_items_returns_empty() {
    let raw = r#"{ "version": 1, "items": [] }"#;
    let defs = parse_menu_pages_file(raw, "fi", "cmxfico", "gl").unwrap();
    assert!(defs.is_empty());
}

#[test]
fn test_sort_order_assigned_by_index() {
    let raw = json!({
        "items": [
            { "id": "first", "caption": "F", "children": [
                { "id": "c_a", "caption": "A" },
                { "id": "c_b", "caption": "B" }
            ]}
        ]
    })
    .to_string();
    let defs = parse_menu_pages_file(&raw, "fi", "cmxfico", "gl").unwrap();
    let first = defs.iter().find(|d| d.code == "first").unwrap();
    assert_eq!(first.sort_order, 0);
    let c_a = defs.iter().find(|d| d.code == "c_a").unwrap();
    assert_eq!(c_a.sort_order, 0);
    let c_b = defs.iter().find(|d| d.code == "c_b").unwrap();
    assert_eq!(c_b.sort_order, 1);
}
