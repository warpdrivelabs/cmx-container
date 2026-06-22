//! IAM 角色模块端到端测试（8 端点）。

mod common;

use common::{client, flex_get, gen_id, get, get_str, post_json};
use serde_json::json;

#[tokio::test]
async fn test_role_crud() {
    let client = client();
    let code = gen_id("e2e_role");
    let name = "E2E测试角色";

    // create
    let created = post_json(
        &client,
        "/api/iam/roles/create",
        &json!({ "code": code, "name": name, "description": "auto" }),
        None,
    )
    .await
    .assert_success();
    let id = flex_get(&created, "id")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .expect("create 响应缺少 id");
    assert_eq!(get_str(&created, "code").as_deref(), Some(code.as_str()));

    // get
    let got = get(
        &client,
        "/api/iam/roles/get",
        Some(&[("id", id.as_str())]),
        None,
    )
    .await
    .assert_success();
    assert_eq!(get_str(&got, "id").as_deref(), Some(id.as_str()));

    // update
    let new_name = "改名后角色";
    let updated = post_json(
        &client,
        "/api/iam/roles/update",
        &json!({ "id": id, "data": { "name": new_name } }),
        None,
    )
    .await
    .assert_success();
    assert_eq!(
        get_str(&updated, "name").as_deref(),
        Some(new_name)
    );

    // delete（软删除）
    post_json(&client, "/api/iam/roles/delete", &json!({ "ids": [id] }), None)
        .await
        .assert_success();

    // delete 后 get 仍返回记录，但 archived=1
    let after = get(
        &client,
        "/api/iam/roles/get",
        Some(&[("id", id.as_str())]),
        None,
    )
    .await
    .assert_success();
    let archived = after.get("archived").and_then(|v| v.as_i64());
    assert_eq!(archived, Some(1), "软删除后 archived 应为 1，实际 {archived:?}");
}

#[tokio::test]
async fn test_role_duplicate_code() {
    let client = client();
    let code = gen_id("e2e_dup_role");
    let body = json!({ "code": code, "name": "重复角色" });

    post_json(&client, "/api/iam/roles/create", &body, None)
        .await
        .assert_success();

    let dup = post_json(&client, "/api/iam/roles/create", &body, None).await;
    dup.assert_error(None);
}

#[tokio::test]
async fn test_role_page_and_list() {
    let client = client();

    let page_res = post_json(
        &client,
        "/api/iam/roles/page",
        &json!({ "current": 1, "size": 5 }),
        None,
    )
    .await;
    assert_eq!(page_res.code, 0, "role page 失败: {}", page_res.msg);
    assert!(page_res.pagination.is_some(), "role page 缺少 pagination");

    let list_res = post_json(&client, "/api/iam/roles/list", &json!({}), None)
        .await
        .assert_success();
    assert!(list_res.as_array().is_some(), "role list data 非数组");
}

#[tokio::test]
async fn test_assign_and_get_permissions() {
    let client = client();

    // 准备：创建一个权限 + 一个角色
    let perm = post_json(
        &client,
        "/api/iam/permissions/create",
        &json!({ "code": gen_id("e2e_rp_perm"), "name": "关联权限" }),
        None,
    )
    .await
    .assert_success();
    let perm_id = flex_get(&perm, "id")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .expect("权限缺少 id");

    let role = post_json(
        &client,
        "/api/iam/roles/create",
        &json!({ "code": gen_id("e2e_rp_role"), "name": "关联角色" }),
        None,
    )
    .await
    .assert_success();
    let role_id = flex_get(&role, "id")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .expect("角色缺少 id");

    // assign-permissions
    post_json(
        &client,
        "/api/iam/roles/assign-permissions",
        &json!({ "role_id": role_id, "permission_ids": [perm_id] }),
        None,
    )
    .await
    .assert_success();

    // get-permissions 反查应包含 perm_id
    let perms = get(
        &client,
        "/api/iam/roles/permissions",
        Some(&[("id", role_id.as_str())]),
        None,
    )
    .await
    .assert_success();
    let arr = perms.as_array().expect("角色权限非数组");
    let found = arr.iter().any(|p| get_str(p, "id").as_deref() == Some(perm_id.as_str()));
    assert!(found, "角色权限反查未包含已分配权限 {perm_id}");

    // 清理
    let _ = post_json(&client, "/api/iam/roles/delete", &json!({ "ids": [role_id] }), None).await;
    let _ = post_json(
        &client,
        "/api/iam/permissions/delete",
        &json!({ "ids": [perm_id] }),
        None,
    )
    .await;
}

#[tokio::test]
async fn test_delete_builtin_role() {
    let client = client();
    // 内置角色 code 通常为 admin / user，通过 list 找到一个内置角色的 id
    let list = post_json(&client, "/api/iam/roles/list", &json!({}), None)
        .await
        .assert_success();
    let arr = list.as_array().expect("角色列表非数组");
    // 找一个 code 为 admin 的内置角色
    let builtin = arr.iter().find(|r| {
        matches!(get_str(r, "code").as_deref(), Some("admin"))
    });

    if let Some(b) = builtin {
        let id = flex_get(b, "id")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_default();
        if id.is_empty() {
            eprintln!("跳过内置角色删除测试：未取到 id");
            return;
        }
        let res = post_json(&client, "/api/iam/roles/delete", &json!({ "ids": [id] }), None).await;
        // 内置角色不可删除，预期业务错误
        res.assert_error(None);
    } else {
        eprintln!("跳过内置角色删除测试：未找到 admin 角色");
    }
}
