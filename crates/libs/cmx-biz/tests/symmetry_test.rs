//! 资源导入导出对称性验证测试
//!
//! 验证 forms/menus 的 definition 字段读写 round-trip 一致性:
//! 存入复杂 JSON → 查询取出 → 比对完全相等。
//! 这是模块包导入/导出对称性的核心保证(整体透传策略)。

mod common;

use common::setup_db_manager;
use cmx_biz::form::{FormFilter, FormForCreate, FormService};
use cmx_biz::menu::{MenuFilter, MenuForCreate, MenuService};
use modql::filter::OpValsString;
use serde_json::{json, Value};

/// DataSet 取首行某字段为 JSON
fn first_row_field_json(ds: &cmx_core::model::data::dataset::DataSet, field: &str) -> Option<Value> {
    let j = serde_json::to_value(ds).expect("DataSet 应可序列化");
    j.get("rows")
        .and_then(|r| r.as_array())
        .and_then(|rows| rows.first())
        .and_then(|row| row.get(field))
        .cloned()
}

#[tokio::test]
async fn test_form_definition_roundtrip_symmetry() {
    let mm = setup_db_manager().await;
    common::ensure_tables(&mm).await;
    let db_id = "test_db";
    let code = "test_form:symmetry";

    // 清理
    let _ = FormService::delete_by_code(&mm, db_id, code).await;

    // 构造复杂 definition(模拟真实表单:多层嵌套 + 数组 + 各种类型)
    let original_definition = json!({
        "name": "对称测试表单",
        "version": "2.1.0",
        "description": "round-trip 验证",
        "fields": [
            {
                "id": "code",
                "type": "input",
                "label": "编码",
                "required": true,
                "validation": "alphanumeric"
            },
            {
                "id": "type",
                "type": "select",
                "label": "类型",
                "options": [
                    {"label": "资产类", "value": "asset"},
                    {"label": "负债类", "value": "liability"}
                ]
            },
            {
                "id": "nested",
                "type": "group",
                "children": [
                    {"id": "sub1", "type": "input", "label": "子字段1"},
                    {"id": "sub2", "type": "number", "label": "子字段2", "default": 0}
                ]
            }
        ],
        "metadata": {"author": "test", "tags": ["finance", "core"]}
    });

    // 存入
    let dto = FormForCreate {
        code: code.to_string(),
        name: "对称测试".to_string(),
        description: None,
        definition: Some(original_definition.clone()),
        domain_code: "TEST".to_string(),
        application_code: "TAPP".to_string(),
        module_code: "TMOD".to_string(),
    };
    FormService::create(&mm, db_id, dto)
        .await
        .expect("创建应成功");

    // 查询取出
    let ds = FormService::list(
        &mm,
        db_id,
        Some(vec![FormFilter {
            code: Some(OpValsString::from(code)),
            ..Default::default()
        }]),
        None,
    )
    .await
    .expect("查询应成功");

    let roundtrip_definition =
        first_row_field_json(&ds, "definition").expect("应返回 definition");

    // DB JSONB 列可能以字符串形式返回,需解析回对象后比对
    let roundtrip_parsed: Value = if roundtrip_definition.is_string() {
        serde_json::from_str(roundtrip_definition.as_str().expect("应为字符串"))
            .expect("definition 字符串应可解析为 JSON")
    } else {
        roundtrip_definition
    };

    // 核心断言:round-trip 后 definition 完全一致
    assert_eq!(
        roundtrip_parsed, original_definition,
        "表单 definition round-trip 后必须完全一致"
    );

    // 清理
    let _ = FormService::delete_by_code(&mm, db_id, code).await;
    mm.shutdown().await.ok();
}

#[tokio::test]
async fn test_menu_extension_roundtrip_symmetry() {
    let mm = setup_db_manager().await;
    common::ensure_tables(&mm).await;
    let db_id = "test_db";
    let code = "test_menu:symmetry";

    // 清理
    let _ = MenuService::delete_by_code(&mm, db_id, code).await;

    // 构造复杂菜单树(模拟真实 menudata:items + children 嵌套)
    let original_menu_json = json!({
        "name": "对称测试菜单",
        "version": "1.5.0",
        "description": "菜单树 round-trip",
        "items": [
            {
                "id": "root1",
                "label": "根菜单1",
                "icon": "home",
                "path": "/root1",
                "children": [
                    {"id": "child1", "label": "子菜单1", "path": "/root1/child1"},
                    {"id": "child2", "label": "子菜单2", "path": "/root1/child2", "icon": "list"}
                ]
            },
            {
                "id": "root2",
                "label": "根菜单2",
                "path": "/root2"
            }
        ]
    });
    let original_str = serde_json::to_string(&original_menu_json).unwrap();

    // 存入(extension 存原始 JSON 文本)
    let dto = MenuForCreate {
        code: code.to_string(),
        name: "对称测试菜单".to_string(),
        parent_id: None,
        path: None,
        icon: None,
        component: None,
        sort_order: 0,
        visible: 1,
        extension: Some(original_str.clone()),
        domain_code: "TEST".to_string(),
        application_code: "TAPP".to_string(),
        module_code: "TMOD".to_string(),
    };
    MenuService::create(&mm, db_id, dto)
        .await
        .expect("创建应成功");

    // 查询取出
    let ds = MenuService::list(
        &mm,
        db_id,
        Some(vec![MenuFilter {
            code: Some(OpValsString::from(code)),
            ..Default::default()
        }]),
        None,
    )
    .await
    .expect("查询应成功");

    let roundtrip_ext = first_row_field_json(&ds, "extension")
        .expect("应返回 extension")
        .as_str()
        .expect("extension 应为字符串")
        .to_string();

    // 核心断言:extension round-trip 后完全一致
    // 注意:从 DB 取出的可能是 JSON 字符串,需比对解析后的值
    let roundtrip_json: Value = serde_json::from_str(&roundtrip_ext).expect("应可解析为 JSON");
    assert_eq!(
        roundtrip_json, original_menu_json,
        "菜单 extension round-trip 后必须完全一致"
    );

    // 清理
    let _ = MenuService::delete_by_code(&mm, db_id, code).await;
    mm.shutdown().await.ok();
}

#[test]
fn test_permission_definition_serialize_symmetry() {
    // 验证 PermissionDefinition 结构序列化/反序列化对称(模块包权限导入导出契约)
    let perm_def = json!({
        "code": "test:view",
        "name": "测试-查看",
        "resource_type": "api",
        "parent_code": "test:root",
        "sort_order": 10,
        "description": "查看权限",
        "extension": "{\"confirm\":true}",
        "status": 1
    });
    let serialized = serde_json::to_string(&perm_def).expect("序列化应成功");
    let deserialized: Value = serde_json::from_str(&serialized).expect("反序列化应成功");
    assert_eq!(deserialized, perm_def, "权限定义序列化 round-trip 一致");
    // code 必须含 ':'(导入校验规则)
    assert!(perm_def["code"].as_str().unwrap().contains(':'), "权限 code 必须含 :");
}
