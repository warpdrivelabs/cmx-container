//! IAM 权限模块端到端测试（7 端点）。

mod common;

use common::{client, flex_get, gen_id, get, get_str, post_json};
use serde_json::json;

#[tokio::test]
async fn test_permission_crud() {
    let client = client();
    let code = gen_id("e2e_perm");
    let name = "E2E测试权限";

    // create
    let created = post_json(
        &client,
        "/api/iam/permissions/create",
        &json!({ "code": code, "name": name, "description": "auto" }),
        None,
    )
    .await
    .assert_success();
    let id = flex_get(&created, "id")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .expect("create 响应缺少 id");
    assert_eq!(get_str(&created, "code").as_deref(), Some(code.as_str()));
    assert_eq!(get_str(&created, "name").as_deref(), Some(name));

    // get
    let got = get(
        &client,
        "/api/iam/permissions/get",
        Some(&[("id", id.as_str())]),
        None,
    )
    .await
    .assert_success();
    assert_eq!(get_str(&got, "id").as_deref(), Some(id.as_str()));

    // update
    let new_name = "改名后权限";
    let updated = post_json(
        &client,
        "/api/iam/permissions/update",
        &json!({ "id": id, "data": { "name": new_name } }),
        None,
    )
    .await
    .assert_success();
    assert_eq!(
        get_str(&updated, "name").as_deref(),
        Some(new_name)
    );

    // delete（软删除：archived 置 1）
    post_json(
        &client,
        "/api/iam/permissions/delete",
        &json!({ "ids": [id] }),
        None,
    )
    .await
    .assert_success();

    // delete 后 get 仍返回记录，但 archived=1（软删除标记）
    let after = get(
        &client,
        "/api/iam/permissions/get",
        Some(&[("id", id.as_str())]),
        None,
    )
    .await
    .assert_success();
    let archived = after.get("archived").and_then(|v| v.as_i64());
    assert_eq!(archived, Some(1), "软删除后 archived 应为 1，实际 {archived:?}");
}

#[tokio::test]
async fn test_permission_duplicate_code() {
    let client = client();
    let code = gen_id("e2e_dup_perm");
    let body = json!({ "code": code, "name": "重复权限" });

    let created = post_json(&client, "/api/iam/permissions/create", &body, None)
        .await
        .assert_success();
    let id = flex_get(&created, "id")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_default();

    // 再次创建相同 code 应失败（业务唯一约束）
    let dup = post_json(&client, "/api/iam/permissions/create", &body, None).await;
    dup.assert_error(None);

    // 清理（按 id 删除）
    if !id.is_empty() {
        let _ = post_json(
            &client,
            "/api/iam/permissions/delete",
            &json!({ "ids": [id] }),
            None,
        )
        .await;
    }
}

#[tokio::test]
async fn test_permission_page() {
    let client = client();
    let res = post_json(
        &client,
        "/api/iam/permissions/page",
        &json!({ "current": 1, "size": 5 }),
        None,
    )
    .await
    .assert_success();
    // data 应为数组
    assert!(res.as_array().is_some(), "page data 非数组: {res}");
}

#[tokio::test]
async fn test_permission_page_pagination_meta() {
    let client = client();
    let res = post_json(
        &client,
        "/api/iam/permissions/page",
        &json!({ "current": 1, "size": 5 }),
        None,
    )
    .await;
    assert_eq!(res.code, 0, "page 接口失败: {}", res.msg);
    assert!(
        res.pagination.is_some(),
        "page 响应缺少 pagination 字段; msg={}",
        res.msg
    );
}

#[tokio::test]
async fn test_permission_list() {
    let client = client();
    let res = post_json(&client, "/api/iam/permissions/list", &json!({}), None)
        .await
        .assert_success();
    assert!(res.as_array().is_some(), "list data 非数组: {res}");
}

#[tokio::test]
async fn test_permission_tree() {
    let client = client();
    let res = get(&client, "/api/iam/permissions/tree", None, None)
        .await
        .assert_success();
    assert!(res.as_array().is_some(), "tree data 非数组: {res}");
}
