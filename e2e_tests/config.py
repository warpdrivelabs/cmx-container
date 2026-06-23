"""E2E 测试配置。"""

import os

BASE_URL = os.getenv("CMX_BASE_URL", "http://127.0.0.1:8080")
API_KEY = os.getenv("CMX_API_KEY", "cmx_sk_dev_A1b2C3d4E5f6G7h8I9j0K1l2M3n4O5p6")
TIMEOUT = 30  # 秒
