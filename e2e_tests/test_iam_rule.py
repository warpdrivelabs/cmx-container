"""IAM 权限规则端到端测试（9 端点）。"""

from __future__ import annotations

import pytest

from conftest import _flex_get, _flex_get_str
from utils.data_gen import gen_id


def test_rule_crud(api_client):
    # 先创建权限用于规则项
    perm_code = gen_id("rperm")
    perm_data = (
        api_client.post(
            "/api/iam/permissions/create",
            {"code": perm_code, "name": "E2E规则权限"},
        )
    ).assert_success()
    perm_id = _flex_get(perm_data, "id")

    code = gen_id("rule")
    # create
    data = (
        api_client.post(
            "/api/iam/permission-rules/create",
            {
                "code": code,
                "name": "E2E测试规则",
                "rule_type": "mutual_exclusion",
                "violation_message": "互斥冲突",
                "priority": 100,
                "description": "E2E测试",
                "items": [{"group_seq": 1, "permission_id": perm_id}],
            },
        )
    ).assert_success()
    rule_id = _flex_get(data, "id")
    assert rule_id, "create 响应缺少 id"

    try:
        # get
        get_data = (
            api_client.get(f"/api/iam/permission-rules/get/{rule_id}")
        ).assert_success()
        rule_info = _flex_get(get_data, "rule") or get_data
        assert _flex_get_str(rule_info, "code") == code, "get 返回 code 不匹配"

        # update
        new_name = "E2E更新规则"
        (
            api_client.post(
                f"/api/iam/permission-rules/update/{rule_id}",
                {"name": new_name},
            )
        ).assert_success()

        # 验证更新
        updated = (
            api_client.get(f"/api/iam/permission-rules/get/{rule_id}")
        ).assert_success()
        updated_rule = _flex_get(updated, "rule") or updated
        assert _flex_get_str(updated_rule, "name") == new_name, "update 后 name 不匹配"
    finally:
        api_client.post(f"/api/iam/permission-rules/delete/{rule_id}")
        api_client.post("/api/iam/permissions/delete", {"ids": [perm_id]})


def test_rule_page(api_client):
    data = (
        api_client.post("/api/iam/permission-rules/page", {"current": 1, "size": 5})
    ).assert_success()
    # 分页响应可能包含 rules 数组和 total
    assert data is not None, "page 响应为空"


def test_rule_toggle_status(api_client):
    # 创建规则
    code = gen_id("toggle")
    data = (
        api_client.post(
            "/api/iam/permission-rules/create",
            {
                "code": code,
                "name": "E2E切换状态规则",
                "rule_type": "mutual_exclusion",
                "priority": 50,
            },
        )
    ).assert_success()
    rule_id = _flex_get(data, "id")

    try:
        # 禁用
        (
            api_client.post(
                "/api/iam/permission-rules/toggle-status",
                {"rule_id": rule_id, "status": 0},
            )
        ).assert_success()

        # 启用
        (
            api_client.post(
                "/api/iam/permission-rules/toggle-status",
                {"rule_id": rule_id, "status": 1},
            )
        ).assert_success()
    finally:
        api_client.post(f"/api/iam/permission-rules/delete/{rule_id}")


def test_rule_items_add(api_client):
    # 创建权限和规则
    perm_code = gen_id("iperm")
    perm_data = (
        api_client.post(
            "/api/iam/permissions/create",
            {"code": perm_code, "name": "E2E规则项权限"},
        )
    ).assert_success()
    perm_id = _flex_get(perm_data, "id")

    code = gen_id("item_rule")
    data = (
        api_client.post(
            "/api/iam/permission-rules/create",
            {
                "code": code,
                "name": "E2E规则项测试",
                "rule_type": "mutual_exclusion",
                "priority": 10,
            },
        )
    ).assert_success()
    rule_id = _flex_get(data, "id")

    try:
        # 添加规则项
        (
            api_client.post(
                "/api/iam/permission-rules/items/add",
                {
                    "rule_id": rule_id,
                    "items": [{"group_seq": 1, "permission_id": perm_id}],
                },
            )
        ).assert_success()
    finally:
        api_client.post(f"/api/iam/permission-rules/delete/{rule_id}")
        api_client.post("/api/iam/permissions/delete", {"ids": [perm_id]})


def test_rule_items_remove(api_client):
    # 创建权限和规则（含规则项）
    perm_code = gen_id("rperm")
    perm_data = (
        api_client.post(
            "/api/iam/permissions/create",
            {"code": perm_code, "name": "E2E移除项权限"},
        )
    ).assert_success()
    perm_id = _flex_get(perm_data, "id")

    code = gen_id("rm_rule")
    data = (
        api_client.post(
            "/api/iam/permission-rules/create",
            {
                "code": code,
                "name": "E2E移除项规则",
                "rule_type": "mutual_exclusion",
                "priority": 10,
                "items": [{"group_seq": 1, "permission_id": perm_id}],
            },
        )
    ).assert_success()
    rule_id = _flex_get(data, "id")

    try:
        # 获取规则详情，找到 item_id
        detail = (
            api_client.get(f"/api/iam/permission-rules/get/{rule_id}")
        ).assert_success()
        items = _flex_get(detail, "items") or []
        if items:
            item_id = _flex_get(items[0], "id")
            if item_id:
                (
                    api_client.post(
                        "/api/iam/permission-rules/items/remove",
                        {"rule_id": rule_id, "item_ids": [item_id]},
                    )
                ).assert_success()
    finally:
        api_client.post(f"/api/iam/permission-rules/delete/{rule_id}")
        api_client.post("/api/iam/permissions/delete", {"ids": [perm_id]})


def test_rule_validate(api_client):
    # 创建权限和互斥规则
    perm1_code = gen_id("vp1")
    perm1_data = (
        api_client.post(
            "/api/iam/permissions/create",
            {"code": perm1_code, "name": "E2E校验权限1"},
        )
    ).assert_success()
    perm1_id = _flex_get(perm1_data, "id")

    perm2_code = gen_id("vp2")
    perm2_data = (
        api_client.post(
            "/api/iam/permissions/create",
            {"code": perm2_code, "name": "E2E校验权限2"},
        )
    ).assert_success()
    perm2_id = _flex_get(perm2_data, "id")

    code = gen_id("val_rule")
    (
        api_client.post(
            "/api/iam/permission-rules/create",
            {
                "code": code,
                "name": "E2E校验规则",
                "rule_type": "mutual_exclusion",
                "priority": 100,
                "items": [
                    {"group_seq": 1, "permission_id": perm1_id},
                    {"group_seq": 2, "permission_id": perm2_id},
                ],
            },
        )
    ).assert_success()

    try:
        # 校验：同时包含两个互斥权限应不通过
        data = (
            api_client.post(
                "/api/iam/permission-rules/validate",
                {"permission_ids": [perm1_id, perm2_id]},
            )
        ).assert_success()
        # passed 应为 False（互斥冲突）
        passed = _flex_get(data, "passed")
        assert passed is False, f"互斥权限校验应不通过，实际 passed={passed}"
    finally:
        # 清理规则和权限
        list_resp = api_client.post("/api/iam/permission-rules/page", {"current": 1, "size": 100})
        if list_resp.code == 0 and list_resp.data:
            rules = _flex_get(list_resp.data, "rules") or list_resp.data
            for r in (rules if isinstance(rules, list) else []):
                if _flex_get_str(r, "code") == code:
                    api_client.post(f"/api/iam/permission-rules/delete/{_flex_get(r, 'id')}")
                    break
        for pid in [perm1_id, perm2_id]:
            try:
                api_client.post("/api/iam/permissions/delete", {"ids": [pid]})
            except Exception:
                pass
