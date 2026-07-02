//! Auth 模块端到端测试。
//!
//! 覆盖：health / login / validate / refresh / heartbeat / change-password /
//! logout / revoke-all，以及 OAuth2 客户端授权码流（条件执行）。

mod common;

use common::{bootstrap_user, client, flex_get, gen_id, get, get_str, post_json, wait_for_server};
use serde_json::json;

#[tokio::test]
async fn test_health() {
    wait_for_server().await;
    let client = client();
    let res = get(&client, "/api/auth/health", None, None)
        .await
        .assert_success();
    // data 含 redis / jwt_keys / status
    assert!(
        res.get("redis").is_some() || res.get("status").is_some(),
        "health 响应缺少 redis/status 字段: {res}"
    );
}

#[tokio::test]
async fn test_login_success() {
    wait_for_server().await;
    let user = bootstrap_user().await;
    let client = client();
    let data = post_json(
        &client,
        "/api/auth/login",
        &json!({ "username": user.username, "password": user.password }),
        None,
    )
    .await
    .assert_success();
    let access = flex_get(&data, "access_token")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .expect("缺少 access_token");
    let refresh = flex_get(&data, "refresh_token")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .expect("缺少 refresh_token");
    assert!(!access.is_empty(), "access_token 为空");
    assert!(!refresh.is_empty(), "refresh_token 为空");
    // 注：每次登录会签发新的 jti/session，token 内容不同（属正常行为），不比较一致性
}

#[tokio::test]
async fn test_login_wrong_password() {
    wait_for_server().await;
    let user = bootstrap_user().await;
    let client = client();
    let res = post_json(
        &client,
        "/api/auth/login",
        &json!({ "username": user.username, "password": "WrongPass!9999" }),
        None,
    )
    .await;
    // 登录失败：业务码非 0（通常 401）
    res.assert_error(None);
}

#[tokio::test]
async fn test_validate_token() {
    wait_for_server().await;
    let user = bootstrap_user().await;
    let client = client();
    let data = post_json(
        &client,
        "/api/auth/validate",
        &json!({ "token": user.access_token }),
        None,
    )
    .await
    .assert_success();
    assert_eq!(
        get_str(&data, "username").as_deref(),
        Some(user.username.as_str()),
        "validate 返回用户名不匹配"
    );
    assert!(
        flex_get(&data, "roles")
            .map(|v| v.is_array())
            .unwrap_or(false),
        "validate 缺少 roles 数组"
    );
}

#[tokio::test]
async fn test_refresh_token() {
    wait_for_server().await;
    let user = bootstrap_user().await;
    let client = client();
    let data = post_json(
        &client,
        "/api/auth/refresh",
        &json!({ "refresh_token": user.refresh_token }),
        None,
    )
    .await
    .assert_success();
    let new_access = flex_get(&data, "access_token")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .expect("refresh 缺少 access_token");
    // refresh 后通常签发新的 access token（可能与原 token 不同，因为 jti 不同）
    assert!(!new_access.is_empty(), "refresh 后 access_token 为空");
}

#[tokio::test]
async fn test_heartbeat() {
    wait_for_server().await;
    let user = bootstrap_user().await;
    let client = client();
    post_json(
        &client,
        "/api/auth/heartbeat",
        &json!({}),
        Some(&user.access_token),
    )
    .await
    .assert_success();
}

#[tokio::test]
async fn test_change_password() {
    wait_for_server().await;
    let user = bootstrap_user().await;
    let client = client();
    let new_password = format!("NewE2e@{}", gen_id("pw"));

    post_json(
        &client,
        "/api/auth/change-password",
        &json!({ "old_password": user.password, "new_password": new_password }),
        Some(&user.access_token),
    )
    .await
    .assert_success();

    // 旧密码登录应失败
    let old_login = post_json(
        &client,
        "/api/auth/login",
        &json!({ "username": user.username, "password": user.password }),
        None,
    )
    .await;
    old_login.assert_error(None);

    // 新密码登录应成功
    post_json(
        &client,
        "/api/auth/login",
        &json!({ "username": user.username, "password": new_password }),
        None,
    )
    .await
    .assert_success();
}

#[tokio::test]
async fn test_logout_invalidates_token() {
    wait_for_server().await;
    let user = bootstrap_user().await;
    let client = client();

    // logout（撤销 access token）
    post_json(
        &client,
        "/api/auth/logout",
        &json!({ "token": user.access_token }),
        Some(&user.access_token),
    )
    .await
    .assert_success();

    // logout 后 validate 该 token 应失败
    let after = post_json(
        &client,
        "/api/auth/validate",
        &json!({ "token": user.access_token }),
        None,
    )
    .await;
    after.assert_error(None);
}

#[tokio::test]
async fn test_revoke_all_forbidden_for_normal_user() {
    wait_for_server().await;
    let user = bootstrap_user().await;
    let client = client();
    // 普通用户无 system:auth:kick 权限，预期 403（业务码或 HTTP 403）
    let res = post_json(
        &client,
        "/api/auth/revoke-all",
        &json!({ "user_id": "anyone" }),
        Some(&user.access_token),
    )
    .await;
    res.assert_error(None);
}

#[tokio::test]
async fn test_oauth2_providers_list() {
    wait_for_server().await;
    let client = client();
    let res = get(&client, "/api/auth/oauth2/providers", None, None)
        .await
        .assert_success();
    assert!(res.is_array(), "providers 响应非数组: {res}");
}

#[tokio::test]
async fn test_oauth2_authorize_invalid_client() {
    wait_for_server().await;
    let client = client();
    let res = get(
        &client,
        "/api/auth/oauth2/authorize",
        Some(&[
            ("client_id", "invalid_nonexistent_client"),
            ("redirect_uri", "http://localhost/cb"),
            ("state", "xyz"),
            ("response_type", "code"),
        ]),
        None,
    )
    .await;
    // 客户端不存在，预期业务错误（通常 400）
    res.assert_error(None);
}
