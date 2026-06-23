"""跨模块联动集成测试。"""

from __future__ import annotations

from datetime import datetime, timedelta, timezone

import pytest

from conftest import _flex_get, _flex_get_str
from utils.data_gen import gen_id, gen_password


def test_full_integration(api_client):
    """权限 → 角色 → 分配 → 规则 → 用户 → 临时角色 → effective-permissions → permission-diff → 清理。"""

    # 1. 创建权限 P1, P2
    p1_code = gen_id("int_p1")
    p1_data = (
        api_client.post(
            "/api/iam/permissions/create",
            {"code": p1_code, "name": "集成权限1"},
        )
    ).assert_success()
    p1_id = _flex_get(p1_data, "id")

    p2_code = gen_id("int_p2")
    p2_data = (
        api_client.post(
            "/api/iam/permissions/create",
            {"code": p2_code, "name": "集成权限2"},
        )
    ).assert_success()
    p2_id = _flex_get(p2_data, "id")

    try:
        # 2. 创建角色 R1, R2
        r1_code = gen_id("int_r1")
        r1_data = (
            api_client.post(
                "/api/iam/roles/create",
                {"code": r1_code, "name": "集成角色1"},
            )
        ).assert_success()
        r1_id = _flex_get(r1_data, "id")

        r2_code = gen_id("int_r2")
        r2_data = (
            api_client.post(
                "/api/iam/roles/create",
                {"code": r2_code, "name": "集成角色2"},
            )
        ).assert_success()
        r2_id = _flex_get(r2_data, "id")

        try:
            # 3. R1 分配 P1, R2 分配 P2
            api_client.post(
                "/api/iam/roles/assign-permissions",
                {"role_id": r1_id, "permission_ids": [p1_id]},
            )
            api_client.post(
                "/api/iam/roles/assign-permissions",
                {"role_id": r2_id, "permission_ids": [p2_id]},
            )

            # 4. 验证角色权限反查
            r1_perms = (
                api_client.get("/api/iam/roles/permissions", params={"id": r1_id})
            ).assert_success()
            assert isinstance(r1_perms, list)
            assert p1_id in [_flex_get(p, "id") for p in r1_perms], "R1 反查缺少 P1"

            # 5. 创建互斥规则
            rule_code = gen_id("int_rule")
            rule_data = (
                api_client.post(
                    "/api/iam/permission-rules/create",
                    {
                        "code": rule_code,
                        "name": "集成互斥规则",
                        "rule_type": "mutual_exclusion",
                        "priority": 100,
                        "items": [
                            {"group_seq": 1, "permission_id": p1_id},
                            {"group_seq": 2, "permission_id": p2_id},
                        ],
                    },
                )
            ).assert_success()
            rule_id = _flex_get(rule_data, "id")

            try:
                # 6. 创建用户 U
                username = gen_id("int_user")
                password = gen_password()
                user_data = (
                    api_client.post(
                        "/api/iam/users/create",
                        {"username": username, "password": password, "nickname": "集成用户", "status": 1},
                    )
                ).assert_success()
                user_id = _flex_get(user_data, "id")

                try:
                    # 7. U 分配 R1
                    api_client.post(
                        "/api/iam/users/assign-roles",
                        {"username": username, "role_ids": [r1_id]},
                    )

                    # 8. U 分配临时角色 R2
                    now = datetime.now(timezone.utc)
                    api_client.post(
                        "/api/iam/users/assign-temp-role",
                        {
                            "user_id": user_id,
                            "role_id": r2_id,
                            "effective_from": now.isoformat(),
                            "effective_until": (now + timedelta(hours=1)).isoformat(),
                            "reason": "集成测试临时角色",
                        },
                    )

                    # 9. 验证 effective-permissions
                    eff_data = (
                        api_client.get(
                            "/api/iam/users/effective-permissions",
                            params={"user_id": user_id},
                        )
                    ).assert_success()
                    assert _flex_get(eff_data, "roles") is not None or \
                           _flex_get(eff_data, "permissions") is not None, \
                           "effective-permissions 缺少 roles/permissions"

                    # 10. 验证 permission-diff
                    diff_data = (
                        api_client.get(
                            "/api/iam/roles/permission-diff",
                            params={"role_id_1": r1_id, "role_id_2": r2_id},
                        )
                    ).assert_success()
                    assert _flex_get(diff_data, "only_in_role_1") is not None or \
                           _flex_get(diff_data, "onlyInRole1") is not None, \
                           "permission-diff 缺少 only_in_role_1"

                finally:
                    api_client.post("/api/iam/users/delete", {"ids": [user_id]})
            finally:
                api_client.post(f"/api/iam/permission-rules/delete/{rule_id}")
        finally:
            api_client.post("/api/iam/roles/delete", {"ids": [r1_id]})
            api_client.post("/api/iam/roles/delete", {"ids": [r2_id]})
    finally:
        api_client.post("/api/iam/permissions/delete", {"ids": [p1_id]})
        api_client.post("/api/iam/permissions/delete", {"ids": [p2_id]})
