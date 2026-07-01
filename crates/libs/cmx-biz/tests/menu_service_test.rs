//! Menu Service CRUD + 树形字段计算集成测试(需真实 PG + cmx_menu 表)
mod common;

use common::{ensure_tables, setup_db_manager};
use cmx_biz::menu::{MenuFilter, MenuForCreate, MenuService};
use modql::filter::OpValsString;
use serde_json::Value as JsonValue;

fn first_row_field(json: &JsonValue, field: &str) -> Option<String> {
    let val = json
        .get("rows")
        .and_then(|r| r.as_array())
        .and_then(|rows| rows.first())
        .and_then(|row| row.get(field))?;
    // 字段可能是字符串或数字，统一转 String
    val.as_str()
        .map(|s| s.to_string())
        .or_else(|| val.as_i64().map(|n| n.to_string()))
        .or_else(|| val.as_f64().map(|n| n.to_string()))
}

fn dataset_to_json(ds: &cmx_core::model::data::dataset::DataSet) -> JsonValue {
    serde_json::to_value(ds).expect("DataSet 应可序列化")
}

/// 按 code 查 id 并删除(清理辅助)
async fn cleanup_by_codes(mm: &cmx_database::DatabaseManager, db_id: &str, codes: &[&str]) {
    for code in codes {
        let existing = MenuService::list(
            mm,
            db_id,
            Some(vec![MenuFilter {
                code: Some(OpValsString::from(*code)),
                ..Default::default()
            }]),
            None,
        )
        .await
        .ok();
        if let Some(ds) = existing {
            let json = dataset_to_json(&ds);
            if let Some(rows) = json.get("rows").and_then(|v| v.as_array()) {
                let ids: Vec<JsonValue> = rows
                    .iter()
                    .filter_map(|r| {
                        r.get("id")
                            .and_then(|v| v.as_str())
                            .map(|s| JsonValue::String(s.to_string()))
                    })
                    .collect();
                if !ids.is_empty() {
                    let _ = MenuService::delete(mm, db_id, ids).await;
                }
            }
        }
    }
}

#[tokio::test]
async fn test_menu_root_create_calculates_tree_fields() {
    let mm = setup_db_manager().await;
    ensure_tables(&mm).await;
    let db_id = "test_db";
    let root_code = "test_menu:tdd_root";

    cleanup_by_codes(&mm, db_id, &[root_code]).await;

    // 创建根菜单(无 parent_id)
    let root = MenuForCreate {
        code: root_code.to_string(),
        name: "根菜单".to_string(),
        parent_id: None,
        path: Some("/root".to_string()),
        icon: None,
        component: None,
        sort_order: 0,
        visible: 1,
        extension: None,
        domain_code: "TEST".to_string(),
        application_code: "TAPP".to_string(),
        module_code: "TMOD".to_string(),
    };
    let created = MenuService::create(&mm, db_id, root)
        .await
        .expect("根菜单创建应成功");
    let json = dataset_to_json(&created);

    // 根菜单: level=1, is_leaf=1, full_path=/code
    let level = first_row_field(&json, "level").expect("应返回 level");
    assert_eq!(level, "1", "根菜单 level 应为 1");
    let is_leaf = first_row_field(&json, "is_leaf").expect("应返回 is_leaf");
    assert_eq!(is_leaf, "1", "根菜单 is_leaf 应为 1");
    let full_path = first_row_field(&json, "full_path").expect("应返回 full_path");
    assert_eq!(
        full_path,
        format!("/{}", root_code),
        "根菜单 full_path 应为 /code"
    );

    let root_id = first_row_field(&json, "id").expect("应返回 id");

    // 清理
    let _ = MenuService::delete(
        &mm,
        db_id,
        vec![JsonValue::String(root_id)],
    )
    .await;
    mm.shutdown().await.ok();
}

#[tokio::test]
async fn test_menu_child_create_inherits_parent_path() {
    let mm = setup_db_manager().await;
    ensure_tables(&mm).await;
    let db_id = "test_db";
    let root_code = "test_menu:tdd_parent";
    let child_code = "test_menu:tdd_child";

    cleanup_by_codes(&mm, db_id, &[root_code, child_code]).await;

    // 1. 创建父
    let root = MenuForCreate {
        code: root_code.to_string(),
        name: "父菜单".to_string(),
        parent_id: None,
        path: None,
        icon: None,
        component: None,
        sort_order: 0,
        visible: 1,
        extension: None,
        domain_code: "TEST".to_string(),
        application_code: "TAPP".to_string(),
        module_code: "TMOD".to_string(),
    };
    let root_ds = MenuService::create(&mm, db_id, root)
        .await
        .expect("父菜单创建应成功");
    let root_id = first_row_field(&dataset_to_json(&root_ds), "id").expect("父 id");

    // 2. 创建子(指定 parent_id)
    let child = MenuForCreate {
        code: child_code.to_string(),
        name: "子菜单".to_string(),
        parent_id: Some(root_id.clone()),
        path: None,
        icon: None,
        component: None,
        sort_order: 0,
        visible: 1,
        extension: None,
        domain_code: "TEST".to_string(),
        application_code: "TAPP".to_string(),
        module_code: "TMOD".to_string(),
    };
    let child_ds = MenuService::create(&mm, db_id, child)
        .await
        .expect("子菜单创建应成功");
    let child_json = dataset_to_json(&child_ds);

    // 子菜单: level=2, full_path=/root_code/child_code
    let level = first_row_field(&child_json, "level").expect("子 level");
    assert_eq!(level, "2", "子菜单 level 应为 2");
    let full_path = first_row_field(&child_json, "full_path").expect("子 full_path");
    assert_eq!(
        full_path,
        format!("/{}/{}", root_code, child_code),
        "子菜单 full_path 应继承父路径"
    );
    let parent_code = first_row_field(&child_json, "parent_code").expect("子 parent_code");
    assert_eq!(parent_code, root_code, "子菜单 parent_code 应为父 code");

    // 3. 验证父节点 is_leaf 变为 0
    let parent_after = MenuService::get(&mm, db_id, &root_id)
        .await
        .expect("查询父节点");
    let parent_is_leaf =
        first_row_field(&dataset_to_json(&parent_after), "is_leaf").expect("父 is_leaf");
    assert_eq!(
        parent_is_leaf, "0",
        "有子节点后父菜单 is_leaf 应为 0"
    );

    // 清理(先删子再删父)
    let child_id = first_row_field(&child_json, "id").expect("子 id");
    let _ = MenuService::delete(&mm, db_id, vec![JsonValue::String(child_id)]).await;
    let _ = MenuService::delete(&mm, db_id, vec![JsonValue::String(root_id)]).await;
    mm.shutdown().await.ok();
}

#[test]
fn test_menu_filter_deserialize() {
    let json = r#"{"full_path": {"$startsWith": "/gl:"}}"#;
    let f: MenuFilter = serde_json::from_str(json).expect("反序列化应成功");
    assert!(f.full_path.is_some(), "full_path 应被解析");
}
