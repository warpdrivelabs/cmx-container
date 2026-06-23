"""唯一测试数据生成。"""

from __future__ import annotations

import uuid


def gen_id(prefix: str) -> str:
    """生成带前缀的唯一字符串，如 e2e_perm_a1b2c3d4。"""
    short = uuid.uuid4().hex[:8]
    return f"e2e_{prefix}_{short}"


def gen_password() -> str:
    """生成符合策略的测试密码，如 E2e@a1b2c3d4。"""
    short = uuid.uuid4().hex[:8]
    return f"E2e@{short}"
