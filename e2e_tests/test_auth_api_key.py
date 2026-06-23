"""API Key 管理端到端测试（4 端点）。"""

from __future__ import annotations

import time

import pytest

from conftest import _flex_get, _flex_get_str
from utils.data_gen import gen_id


def test_create_api_key(api_client):
    data = (
        api_client.post(
            "/api/auth/api-keys/create",
            {
                "service_name": gen_id("svc"),
                "scopes": ["read", "write"],
                "description": "E2E测试API Key",
            },
        )
    ).assert_success()
    assert _flex_get_str(data, "api_key"), "创建 API Key 缺少 api_key 明文"
    assert _flex_get_str(data, "key_prefix"), "创建 API Key 缺少 key_prefix"

    # 清理
    key_id = _flex_get(data, "id")
    if key_id:
        api_client.post("/api/auth/api-keys/delete", {"id": key_id})


def test_list_api_keys(api_client):
    # 创建与上一个测试间隔 1s，避免 key_prefix 时间戳碰撞
    time.sleep(1)
    create_data = (
        api_client.post(
            "/api/auth/api-keys/create",
            {
                "service_name": gen_id("svc"),
                "scopes": ["read"],
                "description": "E2E list测试",
            },
        )
    ).assert_success()
    key_id = _flex_get(create_data, "id")

    try:
        data = (api_client.get("/api/auth/api-keys/list")).assert_success()
        assert isinstance(data, list), f"api-keys/list 响应非数组: {data}"
        # list 不应包含 api_key 明文
        for item in data:
            assert "api_key" not in item or item.get("api_key") is None, (
                "api-keys/list 不应返回 api_key 明文"
            )
    finally:
        if key_id:
            api_client.post("/api/auth/api-keys/delete", {"id": key_id})


def test_toggle_api_key_status(api_client):
    time.sleep(1)
    create_data = (
        api_client.post(
            "/api/auth/api-keys/create",
            {
                "service_name": gen_id("svc"),
                "scopes": ["read"],
                "description": "E2E toggle测试",
            },
        )
    ).assert_success()
    key_id = _flex_get(create_data, "id")

    try:
        # 禁用
        (
            api_client.post(
                "/api/auth/api-keys/toggle-status",
                {"id": key_id, "status": 0},
            )
        ).assert_success()

        # 启用
        (
            api_client.post(
                "/api/auth/api-keys/toggle-status",
                {"id": key_id, "status": 1},
            )
        ).assert_success()
    finally:
        if key_id:
            api_client.post("/api/auth/api-keys/delete", {"id": key_id})


def test_delete_api_key(api_client):
    time.sleep(1)
    create_data = (
        api_client.post(
            "/api/auth/api-keys/create",
            {
                "service_name": gen_id("svc"),
                "scopes": ["read"],
                "description": "E2E delete测试",
            },
        )
    ).assert_success()
    key_id = _flex_get(create_data, "id")

    # 删除
    (api_client.post("/api/auth/api-keys/delete", {"id": key_id})).assert_success()

    # 删除后 list 不应包含该 key
    list_data = (api_client.get("/api/auth/api-keys/list")).assert_success()
    ids = [_flex_get(item, "id") for item in list_data]
    assert key_id not in ids, "删除后 list 仍包含该 API Key"
