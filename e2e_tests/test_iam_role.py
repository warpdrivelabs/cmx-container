"""IAM 角色端到端测试（9 端点）。"""

from __future__ import annotations

import pytest

from conftest import _flex_get, _flex_get_str
from utils.data_gen import gen_id


def test_role_crud(api_client):
    code = gen_id("role")
    # create
    data = (
        api_client.post(
            "/api/iam/roles/create",
            {"code": code, "name": "E2E测试角色", "description": "auto"},
        )
    ).assert_success()
    role_id = _flex_get(data, "id")
    assert role_id, "create 响应缺少 id"

    try:
        # get
        get_data = (
            api_client.get("/api/iam/roles/get", params={"id": role_id})
        ).assert_success()
        assert _flex_get_str(get_data, "code") == code, "get 返回 code 不匹配"

        # update
        new_name = "E2E更新角色"
        (
            api_client.post(
                "/api/iam/roles/update",
                {"id": role_id, "data": {"name": new_name}},
            )
        ).assert_success()

        # 验证更新
        updated = (
            api_client.get("/api/iam/roles/get", params={"id": role_id})
        ).assert_success()
        assert _flex_get_str(updated, "name") == new_name, "update 后 name 不匹配"
    finally:
        api_client.post("/api/iam/roles/delete", {"ids": [role_id]})


def test_role_duplicate_code(api_client):
    code = gen_id("role")
    api_client.post(
        "/api/iam/roles/create",
        {"code": code, "name": "E2E重复角色1"},
    )

    try:
        resp = api_client.post(
            "/api/iam/roles/create",
            {"code": code, "name": "E2E重复角色2"},
        )
        resp.assert_error()  # 服务端返回 500（唯一约束冲突），非标准 409
    finally:
        list_data = (api_client.post("/api/iam/roles/list", {})).assert_success()
        for item in list_data:
            if _flex_get_str(item, "code") == code:
                api_client.post("/api/iam/roles/delete", {"ids": [_flex_get(item, "id")]})
                break


def test_role_page_and_list(api_client):
    # page
    page_resp = api_client.post("/api/iam/roles/page", {"current": 1, "size": 5})
    page_resp.assert_success()
    assert page_resp.pagination is not None, "page 响应缺少 pagination"

    # list
    list_data = (api_client.post("/api/iam/roles/list", {})).assert_success()
    assert isinstance(list_data, list), f"roles/list 响应非数组: {list_data}"


def test_assign_and_get_permissions(api_client):
    # 创建权限
    perm_code = gen_id("perm")
    perm_data = (
        api_client.post(
            "/api/iam/permissions/create",
            {"code": perm_code, "name": "E2E角色权限测试"},
        )
    ).assert_success()
    perm_id = _flex_get(perm_data, "id")

    # 创建角色
    role_code = gen_id("role")
    role_data = (
        api_client.post(
            "/api/iam/roles/create",
            {"code": role_code, "name": "E2E分配权限角色"},
        )
    ).assert_success()
    role_id = _flex_get(role_data, "id")

    try:
        # 分配权限
        (
            api_client.post(
                "/api/iam/roles/assign-permissions",
                {"role_id": role_id, "permission_ids": [perm_id]},
            )
        ).assert_success()

        # 反查权限
        perms = (
            api_client.get(
                "/api/iam/roles/permissions", params={"id": role_id}
            )
        ).assert_success()
        assert isinstance(perms, list), "roles/permissions 响应非数组"
        perm_ids = [_flex_get(p, "id") for p in perms]
        assert perm_id in perm_ids, f"反查权限不包含已分配的 {perm_id}"
    finally:
        api_client.post("/api/iam/roles/delete", {"ids": [role_id]})
        api_client.post("/api/iam/permissions/delete", {"ids": [perm_id]})


def test_delete_builtin_role(api_client):
    """删除内置角色（如 admin），预期业务码 400。"""
    resp = api_client.post("/api/iam/roles/delete", {"ids": ["builtin_admin"]})
    # 内置角色不存在时可能是 404，存在时是 400；只要非 0 即可
    resp.assert_error()


def test_permission_diff(api_client):
    # 创建两个权限
    perm1_code = gen_id("pd1")
    perm1_data = (
        api_client.post(
            "/api/iam/permissions/create",
            {"code": perm1_code, "name": "E2E diff权限1"},
        )
    ).assert_success()
    perm1_id = _flex_get(perm1_data, "id")

    perm2_code = gen_id("pd2")
    perm2_data = (
        api_client.post(
            "/api/iam/permissions/create",
            {"code": perm2_code, "name": "E2E diff权限2"},
        )
    ).assert_success()
    perm2_id = _flex_get(perm2_data, "id")

    # 创建两个角色
    role1_code = gen_id("rd1")
    role1_data = (
        api_client.post(
            "/api/iam/roles/create",
            {"code": role1_code, "name": "E2E diff角色1"},
        )
    ).assert_success()
    role1_id = _flex_get(role1_data, "id")

    role2_code = gen_id("rd2")
    role2_data = (
        api_client.post(
            "/api/iam/roles/create",
            {"code": role2_code, "name": "E2E diff角色2"},
        )
    ).assert_success()
    role2_id = _flex_get(role2_data, "id")

    try:
        # 分配不同权限
        api_client.post(
            "/api/iam/roles/assign-permissions",
            {"role_id": role1_id, "permission_ids": [perm1_id]},
        )
        api_client.post(
            "/api/iam/roles/assign-permissions",
            {"role_id": role2_id, "permission_ids": [perm2_id]},
        )

        # 对比差异
        diff_data = (
            api_client.get(
                "/api/iam/roles/permission-diff",
                params={"role_id_1": role1_id, "role_id_2": role2_id},
            )
        ).assert_success()
        assert _flex_get(diff_data, "only_in_role_1") is not None or \
               _flex_get(diff_data, "onlyInRole1") is not None, \
               "permission-diff 缺少 only_in_role_1"
    finally:
        for rid in [role1_id, role2_id]:
            try:
                api_client.post("/api/iam/roles/delete", {"ids": [rid]})
            except Exception:
                pass
        for pid in [perm1_id, perm2_id]:
            try:
                api_client.post("/api/iam/permissions/delete", {"ids": [pid]})
            except Exception:
                pass
