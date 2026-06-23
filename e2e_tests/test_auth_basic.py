"""Auth 基础认证端到端测试（8 端点）。"""

from __future__ import annotations

import pytest

from conftest import TestUser, _flex_get, _flex_get_str


def test_health(api_client):
    data = (api_client.get("/api/auth/health")).assert_success()
    assert _flex_get(data, "redis") is not None or _flex_get(data, "status") is not None, (
        f"health 响应缺少 redis/status 字段: {data}"
    )


def test_login_success(api_client, test_user: TestUser):
    # test_user fixture 已完成登录，验证 token 非空即可
    assert test_user.access_token, "access_token 为空"
    assert test_user.refresh_token, "refresh_token 为空"


def test_login_wrong_password(api_client, test_user: TestUser):
    resp = api_client.post(
        "/api/auth/login",
        {"username": test_user.username, "password": "WrongPass!9999"},
    )
    resp.assert_error()


def test_validate_token(api_client, test_user: TestUser):
    data = (
        api_client.post(
            "/api/auth/validate",
            {"token": test_user.access_token},
        )
    ).assert_success()
    username = _flex_get_str(data, "username")
    assert username == test_user.username, f"validate 返回用户名不匹配: {username}"
    roles = _flex_get(data, "roles")
    assert isinstance(roles, list), f"validate 缺少 roles 数组: {data}"


def test_refresh_token(api_client, test_user: TestUser):
    data = (
        api_client.post(
            "/api/auth/refresh",
            {"refresh_token": test_user.refresh_token},
        )
    ).assert_success()
    new_access = _flex_get_str(data, "access_token")
    assert new_access, "refresh 后 access_token 为空"


def test_heartbeat(api_client, test_user: TestUser):
    (api_client.post("/api/auth/heartbeat", token=test_user.access_token)).assert_success()


def test_change_password(api_client, test_user: TestUser):
    from utils.data_gen import gen_id, gen_password

    new_password = gen_password()
    (
        api_client.post(
            "/api/auth/change-password",
            {"old_password": test_user.password, "new_password": new_password},
            token=test_user.access_token,
        )
    ).assert_success()

    # 旧密码登录应失败
    old_login = api_client.post(
        "/api/auth/login",
        {"username": test_user.username, "password": test_user.password},
    )
    old_login.assert_error()

    # 新密码登录应成功
    (
        api_client.post(
            "/api/auth/login",
            {"username": test_user.username, "password": new_password},
        )
    ).assert_success()


def test_logout_invalidates_token(api_client, test_user: TestUser):
    # logout
    (
        api_client.post(
            "/api/auth/logout",
            {"token": test_user.access_token},
            token=test_user.access_token,
        )
    ).assert_success()

    # logout 后 validate 该 token 应失败
    after = api_client.post(
        "/api/auth/validate",
        {"token": test_user.access_token},
    )
    after.assert_error()


def test_revoke_all_forbidden(api_client, test_user: TestUser):
    # 普通用户无 system:auth:kick 权限，预期 403
    resp = api_client.post(
        "/api/auth/revoke-all",
        {"user_id": "anyone"},
        token=test_user.access_token,
    )
    resp.assert_error()
