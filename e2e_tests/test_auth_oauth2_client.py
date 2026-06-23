"""OAuth2 客户端管理端到端测试（4 端点）。"""

from __future__ import annotations

import pytest

from conftest import _flex_get, _flex_get_str
from utils.data_gen import gen_id


def test_create_oauth2_client(api_client):
    client_id = gen_id("oauth2")
    data = (
        api_client.post(
            "/api/auth/oauth2-clients/create",
            {
                "client_id": client_id,
                "client_name": "E2E测试客户端",
                "client_secret": "TestSecret12345678",
                "client_type": "confidential",
                "redirect_uris": ["http://localhost/cb"],
                "grant_types": ["authorization_code"],
                "allowed_scopes": ["openid"],
                "pkce_required": True,
                "description": "E2E测试",
            },
        )
    ).assert_success()
    assert _flex_get_str(data, "client_id") == client_id

    # 清理
    api_client.post("/api/auth/oauth2-clients/delete", {"client_id": client_id})


def test_list_oauth2_clients(api_client):
    client_id = gen_id("oauth2")
    api_client.post(
        "/api/auth/oauth2-clients/create",
        {
            "client_id": client_id,
            "client_name": "E2E list测试",
            "client_type": "public",
            "redirect_uris": ["http://localhost/cb"],
            "grant_types": ["authorization_code"],
            "allowed_scopes": ["openid"],
            "pkce_required": True,
        },
    )

    try:
        data = (api_client.get("/api/auth/oauth2-clients/list")).assert_success()
        assert isinstance(data, list), f"oauth2-clients/list 响应非数组: {data}"
    finally:
        api_client.post("/api/auth/oauth2-clients/delete", {"client_id": client_id})


def test_update_oauth2_client(api_client):
    client_id = gen_id("oauth2")
    api_client.post(
        "/api/auth/oauth2-clients/create",
        {
            "client_id": client_id,
            "client_name": "E2E update测试",
            "client_type": "public",
            "redirect_uris": ["http://localhost/cb"],
            "grant_types": ["authorization_code"],
            "allowed_scopes": ["openid"],
            "pkce_required": True,
        },
    )

    try:
        new_name = "E2E更新后名称"
        (
            api_client.post(
                "/api/auth/oauth2-clients/update",
                {
                    "client_id": client_id,
                    "client_name": new_name,
                },
            )
        ).assert_success()

        # 验证更新后名称
        list_data = (api_client.get("/api/auth/oauth2-clients/list")).assert_success()
        found = None
        for item in list_data:
            if _flex_get(item, "client_id") == client_id:
                found = item
                break
        assert found, "更新后 list 中未找到该客户端"
        assert _flex_get_str(found, "client_name") == new_name, "更新后 client_name 不匹配"
    finally:
        api_client.post("/api/auth/oauth2-clients/delete", {"client_id": client_id})


def test_delete_oauth2_client(api_client):
    client_id = gen_id("oauth2")
    api_client.post(
        "/api/auth/oauth2-clients/create",
        {
            "client_id": client_id,
            "client_name": "E2E delete测试",
            "client_type": "public",
            "redirect_uris": ["http://localhost/cb"],
            "grant_types": ["authorization_code"],
            "allowed_scopes": ["openid"],
            "pkce_required": True,
        },
    )

    # 删除
    (
        api_client.post("/api/auth/oauth2-clients/delete", {"client_id": client_id})
    ).assert_success()

    # 删除后 list 不应包含该 client
    list_data = (api_client.get("/api/auth/oauth2-clients/list")).assert_success()
    cids = [_flex_get(item, "client_id") for item in list_data]
    assert client_id not in cids, "删除后 list 仍包含该 OAuth2 客户端"
