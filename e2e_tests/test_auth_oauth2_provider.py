"""OAuth2 第三方 Provider 端到端测试（6 端点）。"""

from __future__ import annotations

import pytest

from conftest import _flex_get


def test_providers_list(api_client):
    data = (
        api_client.get("/api/auth/oauth2/providers")
    ).assert_success()
    assert isinstance(data, list), f"providers 响应非数组: {data}"


def test_provider_authorize_invalid(api_client):
    """无效 provider 名称，预期业务错误。"""
    resp = api_client.get("/api/auth/oauth2/provider/nonexistent_provider/authorize")
    resp.assert_error()


def test_provider_callback_missing_code(api_client):
    """回调缺少 code 参数，预期业务错误。"""
    resp = api_client.get(
        "/api/auth/oauth2/provider/nonexistent_provider/callback",
        params={"state": "xyz"},
    )
    # 无论 provider 是否存在，缺少 code 都应报错
    resp.assert_error()


def test_provider_exchange_invalid_code(api_client):
    """用无效的一次性回调授权码换 Token，预期业务错误。"""
    resp = api_client.post(
        "/api/auth/oauth2/provider/exchange",
        {"code": "invalid_exchange_code", "state": "xyz"},
    )
    resp.assert_error()


def test_provider_link_requires_auth(api_client):
    """绑定第三方账号需要 Bearer Token，无 Token 预期 401。"""
    resp = api_client.post(
        "/api/auth/oauth2/provider/nonexistent_provider/link",
        {"code": "any_code"},
    )
    # 无 Bearer Token 时应返回 401 或业务码非 0
    resp.assert_error()


def test_provider_unlink_requires_auth(api_client):
    """解绑第三方账号需要 Bearer Token，无 Token 预期 401。"""
    resp = api_client.delete(
        "/api/auth/oauth2/provider/nonexistent_provider/unlink",
    )
    resp.assert_error()
