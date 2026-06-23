"""封装 httpx 同步客户端 + 统一鉴权 + 响应解析。"""

from __future__ import annotations

from typing import Any

import httpx

from config import API_KEY, BASE_URL, TIMEOUT
from utils.assertions import ApiResponse


class ApiClient:
    """同步 HTTP 客户端，自动处理鉴权与响应解析。"""

    def __init__(self) -> None:
        self._client = httpx.Client(timeout=TIMEOUT)

    def close(self) -> None:
        self._client.close()

    def __enter__(self) -> ApiClient:
        return self

    def __exit__(self, *args: object) -> None:
        self.close()

    # ── 公开方法 ──────────────────────────────────

    def post(
        self,
        path: str,
        body: dict | None = None,
        *,
        token: str | None = None,
    ) -> ApiResponse:
        """发送 JSON POST 请求。"""
        url = f"{BASE_URL}{path}"
        headers = self._auth_headers(token)
        resp = self._client.post(url, json=body, headers=headers)
        return self._parse(resp)

    def get(
        self,
        path: str,
        params: dict | None = None,
        *,
        token: str | None = None,
    ) -> ApiResponse:
        """发送 GET 请求。"""
        url = f"{BASE_URL}{path}"
        headers = self._auth_headers(token)
        resp = self._client.get(url, params=params, headers=headers)
        return self._parse(resp)

    def delete(
        self,
        path: str,
        body: dict | None = None,
        *,
        token: str | None = None,
    ) -> ApiResponse:
        """发送 DELETE 请求。"""
        url = f"{BASE_URL}{path}"
        headers = self._auth_headers(token)
        resp = self._client.request("DELETE", url, json=body, headers=headers)
        return self._parse(resp)

    # ── 内部方法 ──────────────────────────────────

    @staticmethod
    def _auth_headers(token: str | None) -> dict[str, str]:
        """构造鉴权头：有 Bearer Token 用 Token，否则附加 API Key。"""
        headers: dict[str, str] = {}
        if token:
            headers["Authorization"] = f"Bearer {token}"
        else:
            headers["X-API-Key"] = API_KEY
        return headers

    @staticmethod
    def _parse(resp: httpx.Response) -> ApiResponse:
        """解析 HTTP 响应为 ApiResponse。"""
        status_code = resp.status_code
        try:
            raw = resp.json()
        except Exception:
            raw = {}

        code = raw.get("code", status_code)
        if isinstance(code, str):
            try:
                code = int(code)
            except ValueError:
                code = status_code
        msg = raw.get("msg", "")
        data = raw.get("data")
        pagination = raw.get("pagination")

        return ApiResponse(
            status_code=status_code,
            code=code,
            msg=str(msg),
            data=data,
            pagination=pagination,
            raw=raw,
        )
