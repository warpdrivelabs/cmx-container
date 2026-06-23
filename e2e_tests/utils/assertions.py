"""统一 API 响应模型与断言。"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any

import pytest


@dataclass
class ApiResponse:
    """解析后的 API 响应。"""

    status_code: int
    code: int
    msg: str
    data: Any = None
    pagination: dict | None = None
    raw: dict = field(default_factory=dict)

    # ── 断言方法 ──────────────────────────────────

    def assert_success(self) -> Any:
        """断言业务码为 0，返回 data（允许 data 为 null）。"""
        assert self.code == 0, (
            f"期望成功(code=0)，实际 code={self.code} "
            f"msg={self.msg} status={self.status_code}"
        )
        return self.data

    def assert_error(self, expected_code: int | None = None) -> Any:
        """断言业务码非 0。如指定 expected_code 则同时校验相等。"""
        assert self.code != 0, (
            f"期望失败，实际成功；status={self.status_code} msg={self.msg}"
        )
        if expected_code is not None:
            assert self.code == expected_code, (
                f"期望业务码 {expected_code}，实际 {self.code} "
                f"msg={self.msg} status={self.status_code}"
            )
        return self.data


# ── 便捷断言函数 ──────────────────────────────────


def assert_api_success(resp: ApiResponse) -> Any:
    """快捷方式：断言成功并返回 data。"""
    return resp.assert_success()


def assert_api_error(resp: ApiResponse, expected_code: int | None = None) -> Any:
    """快捷方式：断言失败并返回 data。"""
    return resp.assert_error(expected_code)
