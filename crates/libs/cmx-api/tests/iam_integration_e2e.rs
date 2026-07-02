//! 跨模块联动测试：权限 → 角色 → 用户 三层关联一致性。
//!
//! 流程：创建权限 P → 创建角色 R → R 分配 P → 反查角色权限含 P →
//!      创建用户 U → U 分配 R → 反查用户角色含 R → 清理。

mod common;

use common::{client, flex_get, gen_id, get, get_str, post_json, wait_for_server};
use serde_json::json;

#[tokio::test]
async fn test_full_integration() {
    wait_for_server().await;
    let client = client();

    // 1. 创建权限 P
    let perm = post_json(
        &client,
        "/api/iam/permissions/create",
        &json!({ "code": gen_id("e2e_int_perm"), "name": "集成权限" }),
        None,
    )
    .await
    .assert_success();
    let perm_id = flex_get(&perm, "id")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .expect("权限缺少 id");

    // 2. 创建角色 R
    let role = post_json(
        &client,
        "/api/iam/roles/create",
        &json!({ "code": gen_id("e2e_int_role"), "name": "集成角色" }),
        None,
    )
    .await
    .assert_success();
    let role_id = flex_get(&role, "id")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .expect("角色缺少 id");

    // 3. R 分配 P
    post_json(
        &client,
        "/api/iam/roles/assign-permissions",
        &json!({ "role_id": role_id, "permission_ids": [perm_id] }),
        None,
    )
    .await
    .assert_success();

    // 4. 反查角色权限含 P
    let role_perms = get(
        &client,
        "/api/iam/roles/permissions",
        Some(&[("id", role_id.as_str())]),
        None,
    )
    .await
    .assert_success();
    let has_perm = role_perms
        .as_array()
        .map(|arr| {
            arr.iter()
                .any(|p| get_str(p, "id").as_deref() == Some(perm_id.as_str()))
        })
        .unwrap_or(false);
    assert!(has_perm, "角色权限反查未包含 P");

    // 5. 创建用户 U
    let username = gen_id("e2e_int_user");
    post_json(
        &client,
        "/api/iam/users/create",
        &json!({
            "username": username,
            "password": format!("E2e@{}", gen_id("pw")),
            "status": 1,
        }),
        None,
    )
    .await
    .assert_success();

    // 6. U 分配 R
    post_json(
        &client,
        "/api/iam/users/assign-roles",
        &json!({ "username": username, "role_ids": [role_id] }),
        None,
    )
    .await
    .assert_success();

    // 7. 反查用户角色含 R
    let user_roles = get(
        &client,
        "/api/iam/users/roles",
        Some(&[("username", username.as_str())]),
        None,
    )
    .await
    .assert_success();
    let has_role = user_roles
        .as_array()
        .map(|arr| {
            arr.iter()
                .any(|r| get_str(r, "id").as_deref() == Some(role_id.as_str()))
        })
        .unwrap_or(false);
    assert!(has_role, "用户角色反查未包含 R");

    // 8. 清理：删除用户、角色、权限
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
    let _ = post_json(
        &client,
        "/api/iam/permissions/delete",
        &json!({ "ids": [perm_id] }),
        None,
    )
    .await;
}
