"""IAM 用户端到端测试（15 端点）。"""

from __future__ import annotations

from datetime import datetime, timedelta, timezone

import pytest

from conftest import TestUser, _flex_get, _flex_get_str
from utils.data_gen import gen_id, gen_password


def test_user_crud(api_client):
    username = gen_id("user")
    password = gen_password()

    # create
    data = (
        api_client.post(
            "/api/iam/users/create",
            {"username": username, "password": password, "nickname": "E2E测试", "status": 1},
        )
    ).assert_success()
    user_id = _flex_get(data, "id")

    try:
        # get (by username)
        get_data = (
            api_client.get("/api/iam/users/get", params={"username": username})
        ).assert_success()
        assert _flex_get_str(get_data, "username") == username, "get 返回 username 不匹配"

        # update
        new_nickname = "E2E更新昵称"
        (
            api_client.post(
                "/api/iam/users/update",
                {"id": user_id, "data": {"nickname": new_nickname}},
            )
        ).assert_success()

        # 验证更新
        updated = (
            api_client.get("/api/iam/users/get", params={"username": username})
        ).assert_success()
        assert _flex_get_str(updated, "nickname") == new_nickname, "update 后 nickname 不匹配"
    finally:
        api_client.post("/api/iam/users/delete", {"ids": [user_id]})


def test_user_duplicate_username(api_client):
    username = gen_id("user")
    password = gen_password()
    api_client.post(
        "/api/iam/users/create",
        {"username": username, "password": password, "nickname": "E2E重复1", "status": 1},
    )

    try:
        resp = api_client.post(
            "/api/iam/users/create",
            {"username": username, "password": password, "nickname": "E2E重复2", "status": 1},
        )
        resp.assert_error()  # 服务端返回 500（唯一约束冲突），非标准 409
    finally:
        # 清理
        list_data = (api_client.post("/api/iam/users/list", {})).assert_success()
        for item in list_data:
            if _flex_get_str(item, "username") == username:
                api_client.post("/api/iam/users/delete", {"ids": [_flex_get(item, "id")]})
                break


def test_user_page_and_list(api_client):
    # page
    page_resp = api_client.post("/api/iam/users/page", {"current": 1, "size": 5})
    page_resp.assert_success()
    assert page_resp.pagination is not None, "page 响应缺少 pagination"

    # list
    list_data = (api_client.post("/api/iam/users/list", {})).assert_success()
    assert isinstance(list_data, list), f"users/list 响应非数组: {list_data}"


def test_assign_and_get_roles(api_client):
    # 创建角色
    role_code = gen_id("role")
    role_data = (
        api_client.post(
            "/api/iam/roles/create",
            {"code": role_code, "name": "E2E用户角色测试"},
        )
    ).assert_success()
    role_id = _flex_get(role_data, "id")

    # 创建用户
    username = gen_id("user")
    password = gen_password()
    user_data = (
        api_client.post(
            "/api/iam/users/create",
            {"username": username, "password": password, "nickname": "E2E角色分配", "status": 1},
        )
    ).assert_success()
    user_id = _flex_get(user_data, "id")

    try:
        # 分配角色
        (
            api_client.post(
                "/api/iam/users/assign-roles",
                {"username": username, "role_ids": [role_id]},
            )
        ).assert_success()

        # 反查角色
        roles = (
            api_client.get("/api/iam/users/roles", params={"username": username})
        ).assert_success()
        assert isinstance(roles, list), "users/roles 响应非数组"
        role_ids = [_flex_get(r, "id") for r in roles]
        assert role_id in role_ids, f"反查角色不包含已分配的 {role_id}"
    finally:
        api_client.post("/api/iam/users/delete", {"ids": [user_id]})
        api_client.post("/api/iam/roles/delete", {"ids": [role_id]})


def test_assign_temp_role(api_client):
    # 创建角色
    role_code = gen_id("temp_role")
    role_data = (
        api_client.post(
            "/api/iam/roles/create",
            {"code": role_code, "name": "E2E临时角色"},
        )
    ).assert_success()
    role_id = _flex_get(role_data, "id")

    # 创建用户
    username = gen_id("user")
    password = gen_password()
    user_data = (
        api_client.post(
            "/api/iam/users/create",
            {"username": username, "password": password, "nickname": "E2E临时角色用户", "status": 1},
        )
    ).assert_success()
    user_id = _flex_get(user_data, "id")

    try:
        now = datetime.now(timezone.utc)
        effective_from = now.isoformat()
        effective_until = (now + timedelta(hours=1)).isoformat()

        data = (
            api_client.post(
                "/api/iam/users/assign-temp-role",
                {
                    "user_id": user_id,
                    "role_id": role_id,
                    "effective_from": effective_from,
                    "effective_until": effective_until,
                    "reason": "E2E测试",
                },
            )
        ).assert_success()
        assert _flex_get(data, "effective_from") is not None or \
               _flex_get(data, "effectiveFrom") is not None, \
               "assign-temp-role 响应缺少 effective_from"
    finally:
        api_client.post("/api/iam/users/delete", {"ids": [user_id]})
        api_client.post("/api/iam/roles/delete", {"ids": [role_id]})


def test_revoke_temp_role(api_client):
    # 创建角色 + 用户 + 临时角色
    role_code = gen_id("rvk_role")
    role_data = (
        api_client.post(
            "/api/iam/roles/create",
            {"code": role_code, "name": "E2E撤销临时角色"},
        )
    ).assert_success()
    role_id = _flex_get(role_data, "id")

    username = gen_id("user")
    password = gen_password()
    user_data = (
        api_client.post(
            "/api/iam/users/create",
            {"username": username, "password": password, "nickname": "E2E撤销临时", "status": 1},
        )
    ).assert_success()
    user_id = _flex_get(user_data, "id")

    try:
        now = datetime.now(timezone.utc)
        assign_data = (
            api_client.post(
                "/api/iam/users/assign-temp-role",
                {
                    "user_id": user_id,
                    "role_id": role_id,
                    "effective_from": now.isoformat(),
                    "effective_until": (now + timedelta(hours=1)).isoformat(),
                    "reason": "E2E撤销测试",
                },
            )
        ).assert_success()
        assignment_id = _flex_get(assign_data, "id")

        # 撤销
        (
            api_client.post(
                "/api/iam/users/revoke-temp-role",
                {"assignment_id": assignment_id, "reason": "E2E撤销"},
            )
        ).assert_success()
    finally:
        api_client.post("/api/iam/users/delete", {"ids": [user_id]})
        api_client.post("/api/iam/roles/delete", {"ids": [role_id]})


def test_revoke_temp_roles_batch(api_client):
    # 创建角色 + 用户 + 两个临时角色
    role_code = gen_id("batch_role")
    role_data = (
        api_client.post(
            "/api/iam/roles/create",
            {"code": role_code, "name": "E2E批量撤销角色"},
        )
    ).assert_success()
    role_id = _flex_get(role_data, "id")

    username = gen_id("user")
    password = gen_password()
    user_data = (
        api_client.post(
            "/api/iam/users/create",
            {"username": username, "password": password, "nickname": "E2E批量撤销", "status": 1},
        )
    ).assert_success()
    user_id = _flex_get(user_data, "id")

    try:
        now = datetime.now(timezone.utc)
        ids = []
        for i in range(2):
            assign_data = (
                api_client.post(
                    "/api/iam/users/assign-temp-role",
                    {
                        "user_id": user_id,
                        "role_id": role_id,
                        "effective_from": now.isoformat(),
                        "effective_until": (now + timedelta(hours=1)).isoformat(),
                        "reason": f"E2E批量{i}",
                    },
                )
            ).assert_success()
            ids.append(_flex_get(assign_data, "id"))

        # 批量撤销
        data = (
            api_client.post(
                "/api/iam/users/revoke-temp-roles-batch",
                {"assignment_ids": ids, "reason": "E2E批量撤销"},
            )
        ).assert_success()
        assert _flex_get(data, "affected") is not None, "revoke-temp-roles-batch 缺少 affected"
    finally:
        api_client.post("/api/iam/users/delete", {"ids": [user_id]})
        api_client.post("/api/iam/roles/delete", {"ids": [role_id]})


def test_extend_temp_role(api_client):
    # 创建角色 + 用户 + 临时角色
    role_code = gen_id("ext_role")
    role_data = (
        api_client.post(
            "/api/iam/roles/create",
            {"code": role_code, "name": "E2E延期临时角色"},
        )
    ).assert_success()
    role_id = _flex_get(role_data, "id")

    username = gen_id("user")
    password = gen_password()
    user_data = (
        api_client.post(
            "/api/iam/users/create",
            {"username": username, "password": password, "nickname": "E2E延期临时", "status": 1},
        )
    ).assert_success()
    user_id = _flex_get(user_data, "id")

    try:
        now = datetime.now(timezone.utc)
        assign_data = (
            api_client.post(
                "/api/iam/users/assign-temp-role",
                {
                    "user_id": user_id,
                    "role_id": role_id,
                    "effective_from": now.isoformat(),
                    "effective_until": (now + timedelta(hours=1)).isoformat(),
                    "reason": "E2E延期测试",
                },
            )
        ).assert_success()
        assignment_id = _flex_get(assign_data, "id")

        # 延期
        new_until = (now + timedelta(hours=2)).isoformat()
        (
            api_client.post(
                "/api/iam/users/extend-temp-role",
                {
                    "assignment_id": assignment_id,
                    "new_effective_until": new_until,
                    "reason": "E2E延期",
                },
            )
        ).assert_success()
    finally:
        api_client.post("/api/iam/users/delete", {"ids": [user_id]})
        api_client.post("/api/iam/roles/delete", {"ids": [role_id]})


def test_get_temp_assignments(api_client):
    data = (
        api_client.get(
            "/api/iam/users/temp-assignments",
            params={"user_id": "any", "status": "all"},
        )
    ).assert_success()
    assert isinstance(data, list), f"temp-assignments 响应非数组: {data}"


def test_effective_permissions(api_client):
    # 创建用户
    username = gen_id("user")
    password = gen_password()
    user_data = (
        api_client.post(
            "/api/iam/users/create",
            {"username": username, "password": password, "nickname": "E2E有效权限", "status": 1},
        )
    ).assert_success()
    user_id = _flex_get(user_data, "id")

    try:
        data = (
            api_client.get(
                "/api/iam/users/effective-permissions",
                params={"user_id": user_id},
            )
        ).assert_success()
        # 应包含 roles 和 permissions 字段
        assert _flex_get(data, "roles") is not None or _flex_get(data, "permissions") is not None, \
            f"effective-permissions 缺少 roles/permissions: {data}"
    finally:
        api_client.post("/api/iam/users/delete", {"ids": [user_id]})
