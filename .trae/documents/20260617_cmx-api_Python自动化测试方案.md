# cmx-api 认证与 IAM 接口 Python 自动化测试方案

> 模块：`cmx-api`（auth + iam）
> 创建日期：2026-06-17
> 目标：基于 Python + pytest 重写认证与 IAM 全量接口自动化测试，覆盖 69 个端点，执行「测试 → 整理问题 → 修复 → 重测」闭环。

---

## 一、摘要

上一轮 Rust E2E 测试仅覆盖 33 个端点中的 27 个用例，且接口已发生重大变化（新增 API Key 管理、OAuth2 客户端管理、临时角色授权、权限规则、权限审计等模块，端点总数从 33 增至 69）。本次方案使用 Python + pytest 重写全量测试，代码放置于项目根目录独立文件夹 `e2e_tests/`，与 Rust 代码完全解耦。

**测试范围（69 个端点）：**
- Auth 基础认证：8 个
- OAuth2 授权码流：3 个
- OAuth2 第三方 Provider：6 个
- API Key 管理：4 个
- OAuth2 客户端管理：4 个
- IAM 权限：8 个
- IAM 角色：12 个
- IAM 用户：15 个
- IAM 权限规则：9 个

**不在本次范围：** 第三方 OAuth2 Provider 重定向/回调（需真实第三方凭据与浏览器跳转，仅测 list + 错误场景）。

---

## 二、现状分析

### 2.1 接口变化对比

| 模块 | 旧端点数 | 新端点数 | 新增内容 |
|---|---|---|---|
| Auth 基础 | 8 | 8 | 无变化 |
| OAuth2 授权码流 | 3 | 3 | 无变化 |
| OAuth2 Provider | 1 | 6 | +authorize/callback/exchange/link/unlink |
| API Key 管理 | 0 | 4 | 全新模块 |
| OAuth2 客户端管理 | 0 | 4 | 全新模块 |
| IAM 权限 | 7 | 8 | +usage-stat 审计 |
| IAM 角色 | 8 | 12 | +tree/children/move/permission-diff |
| IAM 用户 | 8 | 15 | +临时角色(5端点)+effective-permissions |
| IAM 权限规则 | 0 | 9 | 全新模块 |
| **合计** | **33** | **69** | **+36 端点** |

### 2.2 现有 Rust 测试

- 位置：`crates/libs/cmx-api/tests/`（5 个测试二进制 + common 模块）
- 覆盖：27 个用例，仅覆盖旧 33 端点
- 问题：无法覆盖新增端点；Rust 编译慢，迭代效率低

### 2.3 Python 测试基础设施

- 项目中 **无** 任何 Python 测试基础设施
- 需从零搭建：目录结构、依赖管理、共享 fixtures

### 2.4 服务配置

- Base URL：`http://127.0.0.1:8080`（环境变量 `CMX_BASE_URL` 可覆盖）
- API Key：`cmx_sk_dev_A1b2C3d4E5f6G7h8I9j0K1l2M3n4O5p6`（环境变量 `CMX_API_KEY` 可覆盖）
- 数据库：`postgresql://postgres:postgres@192.168.137.80:5432/cmx`
- Redis：`redis://192.168.137.95:32496/13`

---

## 三、决策

| 决策项 | 选择 | 理由 |
|---|---|---|
| 测试语言 | Python 3.10+ | 迭代快、生态丰富、无需编译 |
| 测试框架 | pytest | 行业标准、fixture 机制强大、插件丰富 |
| HTTP 客户端 | httpx | 支持异步、API 现代、类型提示完善 |
| 目录位置 | 项目根目录 `e2e_tests/` | 与 Rust 代码完全解耦 |
| 依赖管理 | `requirements.txt` | 轻量，无需 poetry/conda |
| 鉴权策略 | API Key + Bearer Token 双模式 | 复用上一轮验证的鉴权方案 |
| 报告格式 | pytest-html + Markdown 问题报告 | HTML 可视化 + Markdown 便于修复追踪 |

---

## 四、方案设计

### 4.1 目录结构

```
e2e_tests/
├── requirements.txt          # Python 依赖
├── conftest.py               # 全局 pytest fixtures（client、auth、bootstrap）
├── config.py                 # 配置读取（base_url、api_key、超时等）
├── README.md                 # 运行说明（用户明确要求时才创建）
│
├── test_auth_basic.py        # Auth 基础认证 8 端点
├── test_auth_oauth2_flow.py  # OAuth2 授权码流 3 端点
├── test_auth_oauth2_provider.py  # OAuth2 Provider 6 端点
├── test_auth_api_key.py      # API Key 管理 4 端点
├── test_auth_oauth2_client.py    # OAuth2 客户端管理 4 端点
│
├── test_iam_permission.py    # IAM 权限 8 端点
├── test_iam_role.py          # IAM 角色 12 端点
├── test_iam_user.py          # IAM 用户 15 端点
├── test_iam_rule.py          # IAM 权限规则 9 端点
├── test_iam_integration.py   # 跨模块联动集成测试
│
└── utils/
    ├── __init__.py
    ├── http_client.py         # 封装 httpx 客户端 + 统一鉴权 + 响应断言
    ├── data_gen.py            # 唯一测试数据生成
    └── assertions.py          # 自定义断言（assert_api_success / assert_api_error）
```

### 4.2 依赖（`requirements.txt`）

```
# HTTP 客户端
httpx>=0.27,<1.0
# 测试框架
pytest>=8.0,<9.0
pytest-asyncio>=0.23,<1.0
# HTML 报告
pytest-html>=4.0,<5.0
# 唯一 ID 生成
uuid-utils>=0.8,<1.0
```

### 4.3 核心模块设计

#### 4.3.1 `config.py` — 配置

```python
import os

BASE_URL = os.getenv("CMX_BASE_URL", "http://127.0.0.1:8080")
API_KEY = os.getenv("CMX_API_KEY", "cmx_sk_dev_A1b2C3d4E5f6G7h8I9j0K1l2M3n4O5p6")
TIMEOUT = 30  # 秒
```

#### 4.3.2 `utils/http_client.py` — HTTP 客户端封装

核心类 `ApiClient`：
- 构造时创建 `httpx.AsyncClient`（超时 30s）
- `post(path, body, token=None)` — 发送 JSON POST，无 token 时附加 `X-API-Key`
- `get(path, params=None, token=None)` — 发送 GET，无 token 时附加 `X-API-Key`
- `delete(path, body=None, token=None)` — 发送 DELETE
- 统一响应解析：返回 `ApiResponse(code, msg, data, pagination, status_code)`
- `assert_success()` / `assert_error(expected_code=None)` 断言方法

#### 4.3.3 `utils/data_gen.py` — 数据生成

- `gen_id(prefix: str) -> str`：生成 `e2e_{prefix}_{uuid_short8}`
- `gen_password() -> str`：生成 `E2e@{uuid_short8}` 格式密码

#### 4.3.4 `conftest.py` — 全局 Fixtures

```python
@pytest.fixture(scope="session")
async def api_client():
    """会话级 HTTP 客户端，测试结束自动关闭。"""
    async with ApiClient() as client:
        yield client

@pytest.fixture(scope="session")
async def wait_server(api_client):
    """等待服务就绪（轮询 /api/auth/health）。"""
    ...

@pytest.fixture
async def test_user(api_client, wait_server):
    """创建唯一测试用户并登录，返回 (username, password, access_token, refresh_token)。
    测试结束自动清理（delete 用户）。"""
    ...
```

### 4.4 测试用例设计（按模块）

#### 4.4.1 `test_auth_basic.py` — Auth 基础认证（8 端点）

| 测试函数 | 覆盖端点 | 断言要点 |
|---|---|---|
| `test_health` | `GET /api/auth/health` | code=0，data 含 redis/jwt_keys/status |
| `test_login_success` | `POST /api/auth/login` | code=0，access_token/refresh_token 非空 |
| `test_login_wrong_password` | `POST /api/auth/login` | 业务码非 0 |
| `test_validate_token` | `POST /api/auth/validate` | code=0，username 匹配，roles/permissions 为数组 |
| `test_refresh_token` | `POST /api/auth/refresh` | code=0，新 access_token 非空 |
| `test_heartbeat` | `POST /api/auth/heartbeat` | code=0（需 Bearer Token） |
| `test_change_password` | `POST /api/auth/change-password` | code=0；旧密码登录失败、新密码登录成功 |
| `test_logout_invalidates_token` | `POST /api/auth/logout` | logout 后 validate 该 token 返回业务错误 |
| `test_revoke_all_forbidden` | `POST /api/auth/revoke-all` | 普通用户无 system:auth:kick → 业务码 403 |

#### 4.4.2 `test_auth_oauth2_flow.py` — OAuth2 授权码流（3 端点）

| 测试函数 | 覆盖端点 | 断言要点 |
|---|---|---|
| `test_oauth2_providers_list` | `GET /api/auth/oauth2/providers` | code=0，data 为数组 |
| `test_oauth2_authorize_invalid_client` | `GET /api/auth/oauth2/authorize` | client_id 无效 → 业务码非 0 |
| `test_oauth2_full_flow` | authorize→login→token | 条件执行：需 DB 有 OAuth2 客户端；无则 skip |

#### 4.4.3 `test_auth_oauth2_provider.py` — OAuth2 第三方 Provider（6 端点）

| 测试函数 | 覆盖端点 | 断言要点 |
|---|---|---|
| `test_providers_list` | `GET /api/auth/oauth2/providers` | code=0，数组 |
| `test_provider_authorize_invalid` | `GET /api/auth/oauth2/provider/{invalid}/authorize` | 无效 provider → 业务码非 0 |
| `test_provider_callback_missing_code` | `GET /api/auth/oauth2/{provider}/callback` | 缺 code → 业务码非 0 |
| `test_provider_exchange_invalid_code` | `POST /api/auth/oauth2/provider/exchange` | 无效 code → 业务码非 0 |
| `test_provider_link_requires_auth` | `POST /api/auth/oauth2/provider/{p}/link` | 无 Bearer Token → 401 |
| `test_provider_unlink_requires_auth` | `DELETE /api/auth/oauth2/provider/{p}/unlink` | 无 Bearer Token → 401 |

#### 4.4.4 `test_auth_api_key.py` — API Key 管理（4 端点）

| 测试函数 | 覆盖端点 | 断言要点 |
|---|---|---|
| `test_create_api_key` | `POST /api/auth/api-keys/create` | code=0，返回含 api_key 明文 + key_prefix |
| `test_list_api_keys` | `GET /api/auth/api-keys/list` | code=0，数组，不含 api_key 明文 |
| `test_toggle_api_key_status` | `POST /api/auth/api-keys/toggle-status` | code=0 |
| `test_delete_api_key` | `POST /api/auth/api-keys/delete` | code=0；删除后 list 不含该 key |

#### 4.4.5 `test_auth_oauth2_client.py` — OAuth2 客户端管理（4 端点）

| 测试函数 | 覆盖端点 | 断言要点 |
|---|---|---|
| `test_create_oauth2_client` | `POST /api/auth/oauth2-clients/create` | code=0，返回 client_id/client_secret |
| `test_list_oauth2_clients` | `GET /api/auth/oauth2-clients/list` | code=0，数组 |
| `test_update_oauth2_client` | `POST /api/auth/oauth2-clients/update` | code=0；更新后 get 验证字段变更 |
| `test_delete_oauth2_client` | `POST /api/auth/oauth2-clients/delete` | code=0；删除后 list 不含该 client |

#### 4.4.6 `test_iam_permission.py` — IAM 权限（8 端点）

| 测试函数 | 覆盖端点 | 断言要点 |
|---|---|---|
| `test_permission_crud` | create→get→update→delete | 字段一致性校验 |
| `test_permission_duplicate_code` | create 重复 code | 业务码 409 |
| `test_permission_page` | `POST /api/iam/permissions/page` | 分页元数据 |
| `test_permission_list` | `POST /api/iam/permissions/list` | 数组 |
| `test_permission_tree` | `GET /api/iam/permissions/tree` | 树结构含 children |
| `test_permission_usage_stat` | `GET /api/iam/permissions/usage-stat` | code=0，数组含 role_count/user_count |

#### 4.4.7 `test_iam_role.py` — IAM 角色（12 端点）

| 测试函数 | 覆盖端点 | 断言要点 |
|---|---|---|
| `test_role_crud` | create→get→update→delete | 字段一致性 |
| `test_role_duplicate_code` | create 重复 code | 业务码 409 |
| `test_role_page_and_list` | page + list | 分页 + 数组 |
| `test_assign_and_get_permissions` | assign-permissions → get-permissions | 反查一致 |
| `test_delete_builtin_role` | delete 内置角色 | 业务码 400 |
| `test_permission_diff` | `GET /api/iam/roles/permission-diff` | 返回 only_in_1/only_in_2/common |

#### 4.4.8 `test_iam_user.py` — IAM 用户（15 端点）

| 测试函数 | 覆盖端点 | 断言要点 |
|---|---|---|
| `test_user_crud` | create→get→update→delete | 字段一致性 |
| `test_user_duplicate_username` | create 重复 username | 业务码 409 |
| `test_user_page_and_list` | page + list | 分页 + 数组 |
| `test_assign_and_get_roles` | assign-roles → get-roles | 反查一致 |
| `test_assign_temp_role` | `POST /api/iam/users/assign-temp-role` | code=0，返回含 effective_from/until |
| `test_revoke_temp_role` | `POST /api/iam/users/revoke-temp-role` | code=0 |
| `test_revoke_temp_roles_batch` | `POST /api/iam/users/revoke-temp-roles-batch` | code=0，返回 affected |
| `test_extend_temp_role` | `POST /api/iam/users/extend-temp-role` | code=0 |
| `test_get_temp_assignments` | `GET /api/iam/users/temp-assignments` | code=0，数组 |
| `test_effective_permissions` | `GET /api/iam/users/effective-permissions` | code=0，含 roles/permissions |

#### 4.4.9 `test_iam_rule.py` — IAM 权限规则（9 端点）

| 测试函数 | 覆盖端点 | 断言要点 |
|---|---|---|
| `test_rule_crud` | create→get→update→delete | 字段一致性 |
| `test_rule_page` | `POST /api/iam/permission-rules/page` | 分页元数据 |
| `test_rule_toggle_status` | `POST /api/iam/permission-rules/toggle-status` | 启用/禁用切换 |
| `test_rule_items_add` | `POST /api/iam/permission-rules/items/add` | code=0 |
| `test_rule_items_remove` | `POST /api/iam/permission-rules/items/remove` | code=0 |
| `test_rule_validate` | `POST /api/iam/permission-rules/validate` | 返回 passed + violations |

#### 4.4.10 `test_iam_integration.py` — 跨模块联动

单一测试函数 `test_full_integration`，顺序执行：
1. 创建权限 P → 2. 创建角色 R → 3. R 分配 P → 4. 创建权限规则 → 5. 创建用户 U → 6. U 分配 R（含临时角色） → 7. 验证 effective-permissions → 8. 验证 permission-diff → 9. 清理

### 4.5 鉴权策略

沿用上一轮验证的双模式鉴权：

```python
def _apply_auth(headers: dict, token: str | None) -> dict:
    if token:
        headers["Authorization"] = f"Bearer {token}"
    else:
        headers["X-API-Key"] = API_KEY
    return headers
```

- 无需用户上下文的接口（CRUD、list、page 等）：使用 API Key
- 需用户上下文的接口（heartbeat、change-password、revoke-all、provider link/unlink）：使用 Bearer Token

### 4.6 测试数据隔离

- 每个用例用 `gen_id()` 生成唯一数据（如 `e2e_perm_a1b2c3d4`）
- 测试结束在 fixture teardown 中 delete 清理
- 不依赖/不影响现有数据

### 4.7 运行方式

```bash
# 安装依赖
cd e2e_tests && pip install -r requirements.txt

# 运行全部测试
pytest -v --html=report.html

# 运行单个模块
pytest test_auth_basic.py -v

# 运行指定用例
pytest test_iam_role.py::test_role_tree -v
```

---

## 五、测试 → 修复 → 重测 闭环流程

```
┌─────────────────────────────────────────────────────────┐
│ 1. 确认服务就绪：curl /api/auth/health → healthy        │
├─────────────────────────────────────────────────────────┤
│ 2. 运行测试：pytest -v --html=report.html               │
├─────────────────────────────────────────────────────────┤
│ 3. 收集结果：解析 pytest 输出 + report.html              │
│    将失败接口写入：.trae/documents/e2e_issues_report.md   │
├─────────────────────────────────────────────────────────┤
│ 4. 修复源码：定位 handler/service 层，最小化修改          │
├─────────────────────────────────────────────────────────┤
│ 5. 重测失败项：pytest --lf -v                            │
├─────────────────────────────────────────────────────────┤
│ 6. 全绿则更新报告为「已解决」，结束；否则回到步骤 3       │
└─────────────────────────────────────────────────────────┘
```

---

## 六、假设与风险

| # | 假设/风险 | 应对 |
|---|---|---|
| 1 | Python 3.10+ 已安装 | 运行前检查 `python3 --version` |
| 2 | `web-server` 已在 8080 端口运行 | `wait_server` fixture 轮询 health 端点 |
| 3 | OAuth2 完整授权码流依赖 DB 种子客户端 | 条件执行，无数据则 `pytest.skip()` |
| 4 | 第三方 Provider 重定向需真实凭据 | 仅测 list + 错误场景，不测完整重定向 |
| 5 | 临时角色涉及时间计算，可能与服务器时区差异 | 使用 UTC 时间，容忍 ±5s 偏差 |
| 6 | 权限规则 validate 依赖已有权限数据 | 测试内先创建权限，再验证规则 |

---

## 七、验证步骤

1. **依赖安装**：`pip install -r e2e_tests/requirements.txt` 成功
2. **服务可达**：`curl http://127.0.0.1:8080/api/auth/health` 返回 healthy
3. **单模块测试**：`pytest e2e_tests/test_auth_basic.py -v` 全绿
4. **全量测试**：`pytest e2e_tests/ -v --html=report.html` 全绿
5. **报告产出**：`.trae/documents/e2e_issues_report.md` 标记「全部已解决」
6. **回归**：修复后重跑全量，确认无回归

---

## 八、实施步骤（供执行阶段 Follow）

1. 创建 `e2e_tests/` 目录结构
2. 编写 `requirements.txt`
3. 编写 `config.py` + `utils/` 三个工具模块
4. 编写 `conftest.py`（全局 fixtures）
5. 按 4.4 节顺序编写 10 个测试文件
6. 运行全量测试，收集失败结果
7. 修复源码，重测，循环直至全绿
8. 生成最终问题报告
