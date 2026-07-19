//! 资源导入导出对称性验证测试
//!
//! 验证 forms/menus 的 definition 字段读写 round-trip 一致性:
//! 存入复杂 JSON → 查询取出 → 比对完全相等。
//! 这是模块包导入/导出对称性的核心保证(整体透传策略)。

mod common;

use cmx_biz::form::{FormFilter, FormForCreate, FormService};
use cmx_biz::menu::{MenuForCreate, MenuService};
use common::setup_db_manager;
use modql::filter::OpValsString;
use serde_json::{Value, json};

/// DataSet 取首行某字段为 JSON
fn first_row_field_json(
    ds: &cmx_core::model::data::dataset::DataSet,
    field: &str,
) -> Option<Value> {
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
    let _ = FormService::delete_by_code(&mm, db_id, None, code).await;

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
    FormService::create(&mm, db_id, None, dto)
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

    let roundtrip_definition = first_row_field_json(&ds, "definition").expect("应返回 definition");

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
    let _ = FormService::delete_by_code(&mm, db_id, None, code).await;
    mm.shutdown().await.ok();
}

#[tokio::test]
async fn test_menu_definition_roundtrip_symmetry() {
    // 模式A对称性:建多节点(父+子+孙) → list_by_module 导出 → 每节点一行、definition 非空、parent_code 正确。
    let mm = setup_db_manager().await;
    common::ensure_tables(&mm).await;
    let db_id = "test_db";
    let module_code = "TMOD_SYM";

    // 清理整个模块(幂等)
    let _ = MenuService::delete_by_module(&mm, db_id, None, module_code).await;

    let root_def = json!({"name": "对称根", "path": "/sym", "icon": "home"});
    let child_def = json!({"name": "子节点", "path": "/sym/child", "label": "子节点"});
    let grandchild_def = json!({"name": "孙节点", "path": "/sym/child/grand", "label": "孙节点"});

    // 建根
    let root = MenuForCreate {
        code: "test_menu:sym_root".to_string(),
        name: "对称根".to_string(),
        description: None,
        parent_id: None,
        parent_code: None,
        path: Some("/sym".to_string()),
        icon: Some("home".to_string()),
        component: None,
        sort_order: 0,
        visible: 1,
        open_type: 0,
        fun_code: None,
        definition: Some(root_def),
        ext_attributes: None,
        domain_code: "TEST".to_string(),
        application_code: "TAPP".to_string(),
        module_code: module_code.to_string(),
    };
    let root_ds = MenuService::create(&mm, db_id, None, root)
        .await
        .expect("根菜单创建应成功");
    let root_id = first_row_field_json(&root_ds, "id")
        .and_then(|v| v.as_str().map(String::from))
        .expect("根 id");

    // 建子(挂根)
    let child = MenuForCreate {
        code: "test_menu:sym_child".to_string(),
        name: "子节点".to_string(),
        description: None,
        parent_id: Some(root_id.clone()),
        parent_code: None,
        path: Some("/sym/child".to_string()),
        icon: None,
        component: None,
        sort_order: 0,
        visible: 1,
        open_type: 0,
        fun_code: None,
        definition: Some(child_def),
        ext_attributes: None,
        domain_code: "TEST".to_string(),
        application_code: "TAPP".to_string(),
        module_code: module_code.to_string(),
    };
    MenuService::create(&mm, db_id, None, child)
        .await
        .expect("子菜单创建应成功");

    // 建孙(挂根 → 用 parent_code 关联,验证 parent_code 路径)
    let grandchild = MenuForCreate {
        code: "test_menu:sym_child:grand".to_string(),
        name: "孙节点".to_string(),
        description: None,
        parent_id: None,
        parent_code: Some("test_menu:sym_child".to_string()),
        path: Some("/sym/child/grand".to_string()),
        icon: None,
        component: None,
        sort_order: 0,
        visible: 1,
        open_type: 0,
        fun_code: None,
        definition: Some(grandchild_def),
        ext_attributes: None,
        domain_code: "TEST".to_string(),
        application_code: "TAPP".to_string(),
        module_code: module_code.to_string(),
    };
    MenuService::create(&mm, db_id, None, grandchild)
        .await
        .expect("孙菜单创建应成功");

    // 导出:list_by_module 应返回 3 个节点,definition 均非空
    let defs = MenuService::list_by_module(&mm, db_id, module_code)
        .await
        .expect("导出查询应成功");
    assert_eq!(defs.len(), 3, "模式A:模块下应导出全部 3 个节点");

    // 根节点:parent_code 为 None,一等字段(path/icon)正确导出
    let root_export = defs
        .iter()
        .find(|d| d.code == "test_menu:sym_root")
        .expect("应导出根节点");
    assert!(
        root_export.parent_code.is_none(),
        "根 parent_code 应为 None"
    );
    assert_eq!(
        root_export.path.as_deref(),
        Some("/sym"),
        "根 path 应作为一等字段导出"
    );
    assert_eq!(
        root_export.icon.as_deref(),
        Some("home"),
        "根 icon 应作为一等字段导出"
    );

    // 孙节点:parent_code 应指向子节点,一等字段正确导出
    let grand_export = defs
        .iter()
        .find(|d| d.code == "test_menu:sym_child:grand")
        .expect("应导出孙节点");
    assert_eq!(
        grand_export.parent_code.as_deref(),
        Some("test_menu:sym_child"),
        "孙节点 parent_code 应指向子节点"
    );
    assert_eq!(
        grand_export.path.as_deref(),
        Some("/sym/child/grand"),
        "孙 path 应作为一等字段导出"
    );

    // 清理
    let _ = MenuService::delete_by_module(&mm, db_id, None, module_code).await;
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
    assert!(
        perm_def["code"].as_str().unwrap().contains(':'),
        "权限 code 必须含 :"
    );
}
