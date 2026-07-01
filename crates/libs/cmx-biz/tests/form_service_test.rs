//! Form Service CRUD 集成测试(需真实 PG + cmx_form 表)
mod common;

use common::{ensure_tables, setup_db_manager};
use cmx_biz::form::{FormFilter, FormForCreate, FormForUpdate, FormService};
use modql::filter::{ListOptions, OpValsString};
use serde_json::Value as JsonValue;

/// 将 DataSet 序列化为 JSON 数组以便断言
fn dataset_to_json(ds: &cmx_core::model::data::dataset::DataSet) -> serde_json::Value {
    serde_json::to_value(ds).expect("DataSet 应可序列化")
}

/// 从 DataSet 的 JSON 中提取首行指定字符串字段
fn first_row_field(json: &serde_json::Value, field: &str) -> Option<String> {
    json.get("rows")
        .and_then(|r| r.as_array())
        .and_then(|rows| rows.first())
        .and_then(|row| row.get(field))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

#[tokio::test]
async fn test_form_crud_lifecycle() {
    let mm = setup_db_manager().await;
    ensure_tables(&mm).await;
    let db_id = "test_db";
    let test_code = "test_form:tdd_crud";

    // 0. 清理可能的历史残留(按 code 查 id 再删)
    let existing = FormService::list(
        &mm,
        db_id,
        Some(vec![FormFilter {
            code: Some(OpValsString::from(test_code)),
            ..Default::default()
        }]),
        None,
    )
    .await
    .expect("清理前查询应成功");
    let json = dataset_to_json(&existing);
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
            let _ = FormService::delete(&mm, db_id, ids).await;
        }
    }

    // 1. CREATE
    let create_dto = FormForCreate {
        code: test_code.to_string(),
        name: "TDD测试表单".to_string(),
        description: Some("测试用".to_string()),
        definition: Some(serde_json::json!({"fields": []})),
        domain_code: "TEST".to_string(),
        application_code: "TAPP".to_string(),
        module_code: "TMOD".to_string(),
    };
    let created = FormService::create(&mm, db_id, create_dto)
        .await
        .expect("创建应成功");
    let created_json = dataset_to_json(&created);
    let id = first_row_field(&created_json, "id").expect("应返回 id");

    // 2. GET
    let got = FormService::get(&mm, db_id, &id).await.expect("查询应成功");
    assert!(got.row_count() >= 1, "应查到记录");

    // 3. UPDATE
    let update_dto = FormForUpdate {
        name: Some("TDD测试表单-改".to_string()),
        ..Default::default()
    };
    let _ = FormService::update(
        &mm,
        db_id,
        JsonValue::String(id.clone()),
        update_dto,
    )
    .await
    .expect("更新应成功");
    let updated = FormService::get(&mm, db_id, &id).await.expect("更新后查询");
    let updated_json = dataset_to_json(&updated);
    let updated_name = first_row_field(&updated_json, "name").expect("应返回 name");
    assert_eq!(updated_name, "TDD测试表单-改", "名称应已更新");

    // 4. LIST(filter by module_code)
    let list = FormService::list(
        &mm,
        db_id,
        Some(vec![FormFilter {
            module_code: Some(OpValsString::from("TMOD")),
            ..Default::default()
        }]),
        None,
    )
    .await
    .expect("列表应成功");
    assert!(list.row_count() >= 1, "应至少有1条记录");

    // 5. PAGE
    let list_options = ListOptions {
        limit: Some(10),
        offset: None,
        order_bys: None,
    };
    let (page_ds, total) = FormService::page(
        &mm,
        db_id,
        Some(vec![FormFilter {
            module_code: Some(OpValsString::from("TMOD")),
            ..Default::default()
        }]),
        list_options,
    )
    .await
    .expect("分页应成功");
    assert!(total >= 1, "分页 total 应 >=1, 实际 {}", total);
    assert!(!page_ds.is_empty(), "分页结果应非空");

    // 6. DELETE
    let _ = FormService::delete(&mm, db_id, vec![JsonValue::String(id)])
        .await
        .expect("删除应成功");

    // 验证删除后查不到
    let after_delete = FormService::list(
        &mm,
        db_id,
        Some(vec![FormFilter {
            code: Some(OpValsString::from(test_code)),
            ..Default::default()
        }]),
        None,
    )
    .await
    .expect("删除后查询应成功");
    assert_eq!(after_delete.row_count(), 0, "删除后应查不到记录");

    mm.shutdown().await.ok();
}
