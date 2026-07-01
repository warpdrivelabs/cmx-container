# WASM 身份上下文传递与用户查询宿主函数 实施方案

> 评审通过版本。实施时按 Task 1→6 顺序推进，每 Task 提交后验证。

## 调研结论（关键事实）

1. **auth_context 阻塞点**：`SVRContext.auth_context`（cmx-core/context.rs:186）标了 `#[serde(skip)]`（:185，带 fixme"临时解决序列化问题，升级后要去掉"）。`AuthContext` 本身已 derive `Serialize/Deserialize`，可正常往返 MsgPack。移除 skip 后 auth_context 随 `FunctionInput` 进入插件。

2. **宿主函数机制**：Extism + `HostFunctionProvider` trait 模式，单指针 MsgPack 约定，回调运行在 spawn_blocking 线程，用 `Handle::current().block_on` 执行异步查询。错误约定：返回 `{success:false, error}` 结构体，不抛 trap。

3. **关键约束**：宿主函数回调在 spawn_blocking 线程，无法自动获取"当前调用插件的用户"上下文。因此"当前用户"走 auth_context 透传（同步零开销），"任意用户"走宿主函数（显式传 user_id）。

4. **IAM 能力现状**：`UserAuthQuery.get_user_by_id` 返回含 password_hash 的 `UserAuthData`（敏感）；`UserService.get_effective_permissions(user_id)` 最丰富；`PermissionChecker.has_permission/has_role` 已有缓存+熔断；**无批量按 ID 查用户 API**。

## 设计决策

| 决策点 | 选择 |
|---|---|
| auth_context 传递 | 移除 `#[serde(skip)]` |
| 当前/任意用户 | 两者都支持（当前走 ctx，任意走宿主函数）|
| 批量支持 | 支持，WHERE id = ANY($1) |
| 宿主函数范围 | 用户详情 + 角色权限 + has_permission + has_role |

## 新增 5 个宿主函数（cmx:iam namespace）

| 函数 | 入参 | 返回 | 委托 | 性能 |
|---|---|---|---|---|
| `get_user_details` | user_id | Option<WasmUserDetails> | UserAuthQuery.get_user_by_id + 脱敏 | 单次 DB |
| `get_users_details` | user_ids | Vec<WasmUserDetails> | 批量 WHERE id=ANY($1) + 脱敏 | 1 次 DB |
| `get_user_effective_permissions` | user_id | Option<WasmEffectivePermissions> | UserService.get_effective_permissions | 多次 DB |
| `has_permission` | user_id, code | bool | IamChecker.has_permission | 命中缓存 0 次 DB |
| `has_role` | user_id, code | bool | IamChecker.has_role | 命中缓存 0 次 DB |

## WASM 类型设计（cmx-core/wasm_types/iam.rs）

- `WasmUserDetails`（脱敏，无 password_hash）
- `WasmEffectivePermissions`（roles/permissions 用 Vec<String> 轻量 code）
- `IamRequest`(enum) / `IamResponse`(扁平字段，`#[serde(default)]`)

## 关键改动点

1. cmx-core/context.rs:184-185 移除 skip
2. cmx-core/wasm_types/iam.rs 新增类型
3. cmx-iam/host_functions.rs IamHostFunctions provider（持有 IamChecker + DB，含脱敏映射）
4. cmx-iam/user/service 新增 get_users_by_ids（WHERE id=ANY($1)）
5. cmx-plugin-sdk/host_calls.rs 5 个封装方法
6. web-server/config/runtime.rs init_runtime 注册第 5 个 provider
7. cmx-plugin-demo 演示用例

## 任务分解

- Task 1：auth_context 透传（移除 skip + 排查）
- Task 2：新增 WASM 类型（cmx-core/wasm_types/iam.rs + re-export）
- Task 3：IAM 层批量查询 + IamHostFunctions provider
- Task 4：插件 SDK 封装（host_calls + HostFunctions trait）
- Task 5：装配 + 演示（init_runtime 注册 + cmx-plugin-demo）
- Task 6：验证（cargo check + clippy --tests + demo 测试）

## 验收标准
- [ ] 插件可读 input.context.auth_context 获取当前调用者
- [ ] 5 个宿主函数可用
- [ ] 批量查询无 N+1
- [ ] WasmUserDetails 不含 password_hash
- [ ] cargo check --workspace 通过，clippy --workspace --tests 零 warning
- [ ] cmx-plugin-demo 提供可运行演示
