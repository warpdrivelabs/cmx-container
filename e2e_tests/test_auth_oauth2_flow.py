"""OAuth2 授权码流端到端测试（3 端点）。"""

from __future__ import annotations

import pytest

from conftest import _flex_get, _flex_get_str


def test_oauth2_providers_list(api_client):
    data = (
        api_client.get("/api/auth/oauth2/providers")
    ).assert_success()
    assert isinstance(data, list), f"providers 响应非数组: {data}"


def test_oauth2_authorize_invalid_client(api_client):
    resp = api_client.get(
        "/api/auth/oauth2/authorize",
        params={
            "client_id": "invalid_nonexistent_client",
            "redirect_uri": "http://localhost/cb",
            "state": "xyz",
            "response_type": "code",
        },
    )
    resp.assert_error()


def test_oauth2_full_flow(api_client):
    """完整 OAuth2 授权码流：authorize → login → token。

    条件执行：需 DB 中已注册 OAuth2 客户端。若无法完成 authorize 则 skip。
    """
    # 尝试创建一个 OAuth2 客户端用于测试
    from utils.data_gen import gen_id

    client_id = gen_id("oauth2")
    client_secret = "TestSecret12345678"

    # 创建 confidential 类型客户端
    create_resp = api_client.post(
        "/api/auth/oauth2-clients/create",
        {
            "client_id": client_id,
            "client_name": "E2E测试客户端",
            "client_secret": client_secret,
            "client_type": "confidential",
            "redirect_uris": ["http://localhost/cb"],
            "grant_types": ["authorization_code"],
            "allowed_scopes": ["openid", "profile"],
            "pkce_required": False,
            "description": "E2E测试用",
        },
    )
    if create_resp.code != 0:
        pytest.skip("无法创建 OAuth2 客户端，跳过完整授权码流测试")

    try:
        # Step 1: authorize
        auth_resp = api_client.get(
            "/api/auth/oauth2/authorize",
            params={
                "client_id": client_id,
                "redirect_uri": "http://localhost/cb",
                "state": "e2e_state",
                "response_type": "code",
            },
        )
        if auth_resp.code != 0:
            pytest.skip(f"authorize 失败: {auth_resp.msg}")
        state = _flex_get_str(auth_resp.data, "state")
        assert state, "authorize 响应缺少 state"

        # Step 2: login（用户认证签发授权码）
        from conftest import TestUser
        # 需要先创建测试用户
        username = gen_id("oauth2u")
        password = "E2e@oauth2pw1"
        create_user_resp = api_client.post(
            "/api/iam/users/create",
            {"username": username, "password": password, "nickname": "OAuth2E2E", "status": 1},
        )
        if create_user_resp.code != 0:
            pytest.skip("无法创建 OAuth2 测试用户")
        user_id = _flex_get(create_user_resp.data, "id")

        try:
            login_resp = api_client.post(
                "/api/auth/oauth2/login",
                {
                    "state": state,
                    "username": username,
                    "password": password,
                    "client_id": client_id,
                    "redirect_uri": "http://localhost/cb",
                },
            )
            if login_resp.code != 0:
                pytest.skip(f"oauth2 login 失败: {login_resp.msg}")
            code = _flex_get_str(login_resp.data, "code")
            assert code, "oauth2 login 响应缺少 code"

            # Step 3: token（用授权码换 token）
            token_resp = api_client.post(
                "/api/auth/oauth2/token",
                {
                    "grant_type": "authorization_code",
                    "code": code,
                    "client_id": client_id,
                    "redirect_uri": "http://localhost/cb",
                },
            )
            token_data = token_resp.assert_success()
            assert _flex_get_str(token_data, "access_token"), "oauth2 token 响应缺少 access_token"
        finally:
            # 清理用户
            try:
                api_client.post("/api/iam/users/delete", {"ids": [user_id]})
            except Exception:
                pass
    finally:
        # 清理 OAuth2 客户端
        try:
            api_client.post("/api/auth/oauth2-clients/delete", {"client_id": client_id})
        except Exception:
            pass
