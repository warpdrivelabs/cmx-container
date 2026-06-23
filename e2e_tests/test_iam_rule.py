"""IAM 互斥规则端到端测试（9 端点）。"""

from __future__ import annotations

import pytest

from conftest import _flex_get, _flex_get_str
from utils.data_gen import gen_id


def test_rule_crud(api_client):
    # 先创建两个权限用于互斥规则
    perm1_code = gen_id("rperm1")
    perm1_data = (
        api_client.post(
            "/api/iam/permissions/create",
            {"code": perm1_code, "name": "E2E规则主权限"},
        )
    ).assert_success()
    perm1_id = _flex_get(perm1_data, "id")

    perm2_code = gen_id("rperm2")
    perm2_data = (
        api_client.post(
            "/api/iam/permissions/create",
            {"code": perm2_code, "name": "E2E规则互斥权限"},
        )
    ).assert_success()
    perm2_id = _flex_get(perm2_data, "id")

    code = gen_id("rule")
    # create
    data = (
        api_client.post(
            "/api/iam/exclusion-rules/create",
            {
                "code": code,
                "name": "E2E测试规则",
                "subject_type": "permission",
                "primary_subject_id": perm1_id,
                "excluded_subject_ids": [perm2_id],
                "violation_message": "互斥冲突",
                "priority": 100,
                "description": "E2E测试",
            },
        )
    ).assert_success()
    rule_id = _flex_get(data, "id")
    assert rule_id, "create 响应缺少 id"

    try:
        # get
        get_data = (
            api_client.get(f"/api/iam/exclusion-rules/get/{rule_id}")
        ).assert_success()
        rule_info = _flex_get(get_data, "rule") or get_data
        assert _flex_get_str(rule_info, "code") == code, "get 返回 code 不匹配"

        # update
        new_name = "E2E更新规则"
        (
            api_client.post(
                f"/api/iam/exclusion-rules/update/{rule_id}",
                {"name": new_name},
            )
        ).assert_success()

        # 验证更新
        updated = (
            api_client.get(f"/api/iam/exclusion-rules/get/{rule_id}")
        ).assert_success()
        updated_rule = _flex_get(updated, "rule") or updated
        assert _flex_get_str(updated_rule, "name") == new_name, "update 后 name 不匹配"
    finally:
        api_client.post(f"/api/iam/exclusion-rules/delete/{rule_id}")
        api_client.post("/api/iam/permissions/delete", {"ids": [perm1_id, perm2_id]})


def test_rule_page(api_client):
    data = (
        api_client.post("/api/iam/exclusion-rules/page", {"current": 1, "size": 5})
    ).assert_success()
    # 分页响应可能包含 rules 数组和 total
    assert data is not None, "page 响应为空"


def test_rule_toggle_status(api_client):
    # 先创建权限用于规则
    perm1_code = gen_id("tgperm1")
    perm1_data = (
        api_client.post(
            "/api/iam/permissions/create",
            {"code": perm1_code, "name": "E2E切换主权限"},
        )
    ).assert_success()
    perm1_id = _flex_get(perm1_data, "id")

    perm2_code = gen_id("tgperm2")
    perm2_data = (
        api_client.post(
            "/api/iam/permissions/create",
            {"code": perm2_code, "name": "E2E切换互斥权限"},
        )
    ).assert_success()
    perm2_id = _flex_get(perm2_data, "id")

    code = gen_id("toggle")
    data = (
        api_client.post(
            "/api/iam/exclusion-rules/create",
            {
                "code": code,
                "name": "E2E切换状态规则",
                "subject_type": "permission",
                "primary_subject_id": perm1_id,
                "excluded_subject_ids": [perm2_id],
                "priority": 50,
            },
        )
    ).assert_success()
    rule_id = _flex_get(data, "id")

    try:
        # 禁用
        (
            api_client.post(
                "/api/iam/exclusion-rules/toggle-status",
                {"rule_id": rule_id, "status": 0},
            )
        ).assert_success()

        # 启用
        (
            api_client.post(
                "/api/iam/exclusion-rules/toggle-status",
                {"rule_id": rule_id, "status": 1},
            )
        ).assert_success()
    finally:
        api_client.post(f"/api/iam/exclusion-rules/delete/{rule_id}")
        api_client.post("/api/iam/permissions/delete", {"ids": [perm1_id, perm2_id]})


def test_rule_items_add(api_client):
    # 创建权限和规则
    perm1_code = gen_id("iperm1")
    perm1_data = (
        api_client.post(
            "/api/iam/permissions/create",
            {"code": perm1_code, "name": "E2E主权限"},
        )
    ).assert_success()
    perm1_id = _flex_get(perm1_data, "id")

    perm2_code = gen_id("iperm2")
    perm2_data = (
        api_client.post(
            "/api/iam/permissions/create",
            {"code": perm2_code, "name": "E2E初始互斥权限"},
        )
    ).assert_success()
    perm2_id = _flex_get(perm2_data, "id")

    perm3_code = gen_id("iperm3")
    perm3_data = (
        api_client.post(
            "/api/iam/permissions/create",
            {"code": perm3_code, "name": "E2E追加互斥权限"},
        )
    ).assert_success()
    perm3_id = _flex_get(perm3_data, "id")

    code = gen_id("item_rule")
    # 创建带初始互斥对象的规则（perm2 为初始互斥）
    data = (
        api_client.post(
            "/api/iam/exclusion-rules/create",
            {
                "code": code,
                "name": "E2E规则项测试",
                "subject_type": "permission",
                "primary_subject_id": perm1_id,
                "excluded_subject_ids": [perm2_id],
                "priority": 10,
            },
        )
    ).assert_success()
    rule_id = _flex_get(data, "id")

    try:
        # 添加互斥对象（perm3）
        (
            api_client.post(
                "/api/iam/exclusion-rules/items/add",
                {
                    "rule_id": rule_id,
                    "subject_ids": [perm3_id],
                },
            )
        ).assert_success()
    finally:
        api_client.post(f"/api/iam/exclusion-rules/delete/{rule_id}")
        api_client.post("/api/iam/permissions/delete", {"ids": [perm1_id, perm2_id, perm3_id]})


def test_rule_items_remove(api_client):
    # 创建权限和规则（含规则项）
    perm1_code = gen_id("rperm1")
    perm1_data = (
        api_client.post(
            "/api/iam/permissions/create",
            {"code": perm1_code, "name": "E2E移除主权限"},
        )
    ).assert_success()
    perm1_id = _flex_get(perm1_data, "id")

    perm2_code = gen_id("rperm2")
    perm2_data = (
        api_client.post(
            "/api/iam/permissions/create",
            {"code": perm2_code, "name": "E2E移除互斥权限"},
        )
    ).assert_success()
    perm2_id = _flex_get(perm2_data, "id")

    code = gen_id("rm_rule")
    data = (
        api_client.post(
            "/api/iam/exclusion-rules/create",
            {
                "code": code,
                "name": "E2E移除项规则",
                "subject_type": "permission",
                "primary_subject_id": perm1_id,
                "excluded_subject_ids": [perm2_id],
                "priority": 10,
            },
        )
    ).assert_success()
    rule_id = _flex_get(data, "id")

    try:
        # 获取规则详情，找到 item_id
        detail = (
            api_client.get(f"/api/iam/exclusion-rules/get/{rule_id}")
        ).assert_success()
        items = _flex_get(detail, "items") or []
        if items:
            item_id = _flex_get(items[0], "id")
            if item_id:
                (
                    api_client.post(
                        "/api/iam/exclusion-rules/items/remove",
                        {"rule_id": rule_id, "item_ids": [item_id]},
                    )
                ).assert_success()
    finally:
        api_client.post(f"/api/iam/exclusion-rules/delete/{rule_id}")
        api_client.post("/api/iam/permissions/delete", {"ids": [perm1_id, perm2_id]})


def test_rule_validate(api_client):
    # 创建权限和互斥规则
    perm1_code = gen_id("vp1")
    perm1_data = (
        api_client.post(
            "/api/iam/permissions/create",
            {"code": perm1_code, "name": "E2E校验主权限"},
        )
    ).assert_success()
    perm1_id = _flex_get(perm1_data, "id")

    perm2_code = gen_id("vp2")
    perm2_data = (
        api_client.post(
            "/api/iam/permissions/create",
            {"code": perm2_code, "name": "E2E校验互斥权限"},
        )
    ).assert_success()
    perm2_id = _flex_get(perm2_data, "id")

    code = gen_id("val_rule")
    (
        api_client.post(
            "/api/iam/exclusion-rules/create",
            {
                "code": code,
                "name": "E2E校验规则",
                "subject_type": "permission",
                "primary_subject_id": perm1_id,
                "excluded_subject_ids": [perm2_id],
                "priority": 100,
            },
        )
    ).assert_success()

    try:
        # 校验：同时包含主权限和互斥权限应不通过
        data = (
            api_client.post(
                "/api/iam/exclusion-rules/validate",
                {"permission_ids": [perm1_id, perm2_id]},
            )
        ).assert_success()
        # passed 应为 False（互斥冲突）
        passed = _flex_get(data, "passed")
        assert passed is False, f"互斥权限校验应不通过，实际 passed={passed}"
    finally:
        # 清理规则和权限
        list_resp = api_client.post("/api/iam/exclusion-rules/page", {"current": 1, "size": 100})
        if list_resp.code == 0 and list_resp.data:
            rules = _flex_get(list_resp.data, "rules") or list_resp.data
            for r in (rules if isinstance(rules, list) else []):
                if _flex_get_str(r, "code") == code:
                    api_client.post(f"/api/iam/exclusion-rules/delete/{_flex_get(r, 'id')}")
                    break
        for pid in [perm1_id, perm2_id]:
            try:
                api_client.post("/api/iam/permissions/delete", {"ids": [pid]})
            except Exception:
                pass
