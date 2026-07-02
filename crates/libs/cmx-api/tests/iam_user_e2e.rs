//! IAM 用户模块端到端测试（8 端点）。
//!
//! 注意：用户 GET 用 `?username=` 查询（非 id）。

mod common;

use common::{client, flex_get, gen_id, get, get_str, post_json};
use serde_json::json;

/// 创建一个唯一测试用户，返回其 username。
async fn create_test_user(client: &reqwest::Client, suffix: &str) -> String {
    let username = gen_id(&format!("e2e_{suffix}"));
    let body = json!({
        "username": username,
        "password": format!("E2e@{}", gen_id("pw")),
        "nickname": "用户测试",
        "status": 1,
    });
    post_json(client, "/api/iam/users/create", &body, None)
        .await
        .assert_success();
    username
}

#[tokio::test]
async fn test_user_crud() {
    let client = client();
    let username = create_test_user(&client, "crud").await;

    // get by username
    let got = get(
        &client,
        "/api/iam/users/get",
        Some(&[("username", username.as_str())]),
        None,
    )
    .await
    .assert_success();
    assert_eq!(
        get_str(&got, "username").as_deref(),
        Some(username.as_str())
    );
    let id = flex_get(&got, "id")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .expect("用户缺少 id");

    // update
    let new_nickname = "改昵称后";
    let updated = post_json(
        &client,
        "/api/iam/users/update",
        &json!({ "id": id, "data": { "nickname": new_nickname } }),
        None,
    )
    .await
    .assert_success();
    assert_eq!(get_str(&updated, "nickname").as_deref(), Some(new_nickname));

    // delete
    post_json(
        &client,
        "/api/iam/users/delete",
        &json!({ "ids": [id] }),
        None,
    )
    .await
    .assert_success();

    // delete 后 get 应失败
    let after = get(
        &client,
        "/api/iam/users/get",
        Some(&[("username", username.as_str())]),
        None,
    )
    .await;
    after.assert_error(None);
}

#[tokio::test]
async fn test_user_duplicate_username() {
    let client = client();
    let username = create_test_user(&client, "dup").await;
    let body = json!({
        "username": username,
        "password": "E2e@dup12345",
    });
    let dup = post_json(&client, "/api/iam/users/create", &body, None).await;
    dup.assert_error(None);
}

#[tokio::test]
async fn test_user_page_and_list() {
    let client = client();

    let page_res = post_json(
        &client,
        "/api/iam/users/page",
        &json!({ "current": 1, "size": 5 }),
        None,
    )
    .await;
    assert_eq!(page_res.code, 0, "user page 失败: {}", page_res.msg);
    assert!(page_res.pagination.is_some(), "user page 缺少 pagination");

    let list_res = post_json(&client, "/api/iam/users/list", &json!({}), None)
        .await
        .assert_success();
    assert!(list_res.as_array().is_some(), "user list data 非数组");
}

#[tokio::test]
async fn test_assign_and_get_roles() {
    let client = client();

    // 准备：创建角色 + 用户
    let role = post_json(
        &client,
        "/api/iam/roles/create",
        &json!({ "code": gen_id("e2e_ur_role"), "name": "用户角色" }),
        None,
    )
    .await
    .assert_success();
    let role_id = flex_get(&role, "id")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .expect("角色缺少 id");

    let username = create_test_user(&client, "ar").await;

    // assign-roles
    post_json(
        &client,
        "/api/iam/users/assign-roles",
        &json!({ "username": username, "role_ids": [role_id] }),
        None,
    )
    .await
    .assert_success();

    // get-roles 反查应包含 role_id
    let roles = get(
        &client,
        "/api/iam/users/roles",
        Some(&[("username", username.as_str())]),
        None,
    )
    .await
    .assert_success();
    let arr = roles.as_array().expect("用户角色非数组");
    let found = arr
        .iter()
        .any(|r| get_str(r, "id").as_deref() == Some(role_id.as_str()));
    assert!(found, "用户角色反查未包含已分配角色 {role_id}");

    // 清理用户与角色
    let user = get(
        &client,
        "/api/iam/users/get",
        Some(&[("username", username.as_str())]),
        None,
    )
    .await
    .assert_success();
    if let Some(uid) = flex_get(&user, "id").and_then(|v| v.as_str().map(|s| s.to_string())) {
        let _ = post_json(
            &client,
            "/api/iam/users/delete",
            &json!({ "ids": [uid] }),
            None,
        )
        .await;
    }
    let _ = post_json(
        &client,
        "/api/iam/roles/delete",
        &json!({ "ids": [role_id] }),
        None,
    )
    .await;
}
