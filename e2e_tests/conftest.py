"""全局 pytest fixtures。"""

from __future__ import annotations

import sys
import os
import time
from dataclasses import dataclass

import pytest

# 确保 e2e_tests/ 在 sys.path 中，使 `from config import ...` 可用
sys.path.insert(0, os.path.dirname(__file__))

from config import BASE_URL
from utils.data_gen import gen_id, gen_password
from utils.http_client import ApiClient


@dataclass
class TestUser:
    """测试用户信息。"""

    username: str
    password: str
    access_token: str
    refresh_token: str
    user_id: str | None = None


# ── Session 级 fixtures ──────────────────────────────


@pytest.fixture(scope="session")
def api_client():
    """会话级 HTTP 客户端，测试结束自动关闭。"""
    with ApiClient() as client:
        yield client


@pytest.fixture(scope="session", autouse=True)
def wait_server(api_client: ApiClient):
    """等待服务就绪（轮询 /api/auth/health，最多 120 秒）。"""
    for i in range(60):
        try:
            resp = api_client.get("/api/auth/health")
            if resp.code == 0:
                return
        except Exception:
            pass
        if i == 0:
            print("等待服务启动...")
        time.sleep(2)
    pytest.exit("服务在 120s 内未就绪")


# ── 函数级 fixtures ──────────────────────────────────


@pytest.fixture
def test_user(api_client: ApiClient) -> TestUser:
    """创建唯一测试用户并登录，返回 TestUser。测试结束自动清理。"""
    username = gen_id("user")
    password = gen_password()

    # 创建用户
    create_body = {
        "username": username,
        "password": password,
        "nickname": "E2E测试",
        "status": 1,
    }
    create_resp = api_client.post("/api/iam/users/create", create_body)
    data = create_resp.assert_success()
    user_id = _flex_get(data, "id")

    # 登录
    login_body = {"username": username, "password": password}
    login_resp = api_client.post("/api/auth/login", login_body)
    login_data = login_resp.assert_success()
    access_token = _flex_get_str(login_data, "access_token")
    refresh_token = _flex_get_str(login_data, "refresh_token")

    user = TestUser(
        username=username,
        password=password,
        access_token=access_token,
        refresh_token=refresh_token,
        user_id=user_id,
    )

    yield user

    # 清理：删除测试用户
    try:
        api_client.post("/api/iam/users/delete", {"ids": [user_id]})
    except Exception:
        pass


# ── 辅助函数 ────────────────────────────────────────


def _flex_get(obj: dict | list | None, key: str):
    """从 JSON 对象中按 key 取值，兼容 snake_case / camelCase。"""
    if not isinstance(obj, dict):
        return None
    if key in obj:
        return obj[key]
    # snake_case -> camelCase
    camel = _snake_to_camel(key)
    if camel != key and camel in obj:
        return obj[camel]
    # camelCase -> snake_case
    snake = _camel_to_snake(key)
    if snake != key and snake in obj:
        return obj[snake]
    return None


def _flex_get_str(obj: dict | list | None, key: str) -> str:
    """从 JSON 对象中按 key 取字符串值，兼容 snake_case / camelCase。"""
    val = _flex_get(obj, key)
    if isinstance(val, str):
        return val
    return ""


def _snake_to_camel(s: str) -> str:
    out = []
    up = False
    for ch in s:
        if ch == "_":
            up = True
        elif up:
            out.append(ch.upper())
            up = False
        else:
            out.append(ch)
    return "".join(out)


def _camel_to_snake(s: str) -> str:
    out = []
    for ch in s:
        if ch.isupper():
            if out:
                out.append("_")
            out.append(ch.lower())
        else:
            out.append(ch)
    return "".join(out)
