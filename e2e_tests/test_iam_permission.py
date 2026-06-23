"""IAM 权限端到端测试（8 端点）。"""

from __future__ import annotations

import pytest

from conftest import _flex_get, _flex_get_str
from utils.data_gen import gen_id


def test_permission_crud(api_client):
    code = gen_id("perm")
    # create
    data = (
        api_client.post(
            "/api/iam/permissions/create",
            {"code": code, "name": "E2E测试权限", "description": "auto"},
        )
    ).assert_success()
    perm_id = _flex_get(data, "id")
    assert perm_id, "create 响应缺少 id"

    try:
        # get
        get_data = (
            api_client.get("/api/iam/permissions/get", params={"id": perm_id})
        ).assert_success()
        assert _flex_get_str(get_data, "code") == code, "get 返回 code 不匹配"

        # update
        new_name = "E2E更新权限"
        (
            api_client.post(
                "/api/iam/permissions/update",
                {"id": perm_id, "data": {"name": new_name}},
            )
        ).assert_success()

        # 验证更新
        updated = (
            api_client.get("/api/iam/permissions/get", params={"id": perm_id})
        ).assert_success()
        assert _flex_get_str(updated, "name") == new_name, "update 后 name 不匹配"
    finally:
        # delete
        api_client.post("/api/iam/permissions/delete", {"ids": [perm_id]})


def test_permission_duplicate_code(api_client):
    code = gen_id("perm")
    api_client.post(
        "/api/iam/permissions/create",
        {"code": code, "name": "E2E重复测试1"},
    )

    try:
        resp = api_client.post(
            "/api/iam/permissions/create",
            {"code": code, "name": "E2E重复测试2"},
        )
        resp.assert_error()  # 服务端返回 500（唯一约束冲突），非标准 409
    finally:
        # 清理
        list_data = (api_client.post("/api/iam/permissions/list", {})).assert_success()
        for item in list_data:
            if _flex_get_str(item, "code") == code:
                api_client.post("/api/iam/permissions/delete", {"ids": [_flex_get(item, "id")]})
                break


def test_permission_page(api_client):
    data = (
        api_client.post("/api/iam/permissions/page", {"current": 1, "size": 5})
    ).assert_success()
    # 分页响应在 pagination 字段
    resp_raw = (api_client.post("/api/iam/permissions/page", {"current": 1, "size": 5}))
    pagination = resp_raw.pagination
    assert pagination is not None, "page 响应缺少 pagination"


def test_permission_page_pagination_meta(api_client):
    resp = api_client.post("/api/iam/permissions/page", {"current": 1, "size": 5})
    data = resp.assert_success()
    pagination = resp.pagination
    assert pagination is not None, "page 响应缺少 pagination"
    assert "total" in pagination or "totalPages" in pagination, (
        f"pagination 缺少分页元数据: {pagination}"
    )


def test_permission_list(api_client):
    data = (
        api_client.post("/api/iam/permissions/list", {})
    ).assert_success()
    assert isinstance(data, list), f"permissions/list 响应非数组: {data}"


def test_permission_tree(api_client):
    data = (
        api_client.get("/api/iam/permissions/tree")
    ).assert_success()
    assert isinstance(data, list), f"permissions/tree 响应非数组: {data}"


def test_permission_usage_stat(api_client):
    data = (
        api_client.get("/api/iam/permissions/usage-stat")
    ).assert_success()
    assert isinstance(data, list), f"permissions/usage-stat 响应非数组: {data}"
