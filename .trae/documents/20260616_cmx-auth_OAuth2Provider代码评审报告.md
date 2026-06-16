# 第三方 OAuth2 Provider 对接 — 代码评审报告（第四轮）

> 评审日期：2026-06-16 | 评审范围：cmx-auth 第三方 OAuth2 Provider Client 功能
> 参照文档：`20260615_cmx-auth_第三方OAuth2Provider对接方案.md` v6 + spec/tasks/checklist
> 前轮状态：第三轮 3 项问题已修复，测试缺失项暂不处理

---

## 一、已修复确认（第三轮 3 项）

| 编号 | 问题 | 修复确认 |
|------|------|----------|
| N-7 | `create_user_from_oauth2` SQL 拼接注入风险 | ✅ 改用 `query_sql_with_json`/`execute_sql_with_json` 参数化查询，`ON CONFLICT (user_id, role_id) DO NOTHING` 正确处理重复关联 |
| N-8 | `BindingRequired` 变体语义不明确 | ✅ 添加注释说明"语义已从'需要前端绑定'变更为'未注册错误'" |
| N-10 | `link` handler 暴露 `provider_user_id` | ✅ `OAuth2(_)` 统一映射为"该第三方账号已被其他用户绑定"，不再泄露敏感信息 |

---

## 二、新发现问题

### N-11: `link` handler 的 `OAuth2(_)` 错误映射过于宽泛

**文件**: [oauth2_provider_handler.rs](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/libs/cmx-api/src/handlers/auth/oauth2_provider_handler.rs#L207-L212)

```rust
match e {
    cmx_traits::AuthError::OAuth2ProviderNotFound(_) => Error::BadRequest("Provider 不存在".to_string()),
    // N-10 修复：脱敏处理，不暴露 provider_user_id
    cmx_traits::AuthError::OAuth2(_) => Error::BadRequest("该第三方账号已被其他用户绑定".to_string()),
    other => Error::InternalError(other.to_string()),
}
```

**问题**: `link_oauth2_account` 内部可能产生多种 `OAuth2(_)` 错误，但 handler 将所有 `OAuth2(_)` 统一映射为"该第三方账号已被其他用户绑定"，导致误导：

| 实际错误来源 | 错误变体 | 前端展示（错误） |
|--------------|----------|------------------|
| 重复绑定检查 | `OAuth2("该 google 账号已被其他用户绑定")` | "该第三方账号已被其他用户绑定" ✅ 正确 |
| Provider 服务不可达 | `OAuth2ProviderUnavailable(...)` | 不匹配此 arm，走 other |
| Token 交换失败 | `OAuth2ProviderTokenError(...)` | 不匹配此 arm，走 other |
| 用户信息获取失败 | `OAuth2ProviderUserInfoError(...)` | 不匹配此 arm，走 other |
| State 不匹配 | `OAuth2("State 中的 provider 与请求不匹配")` | "该第三方账号已被其他用户绑定" ❌ 误导 |

注意：`OAuth2ProviderUnavailable`、`OAuth2ProviderTokenError`、`OAuth2ProviderUserInfoError` 是独立变体（非 `OAuth2(_)`），会落入 `other` 分支返回 `InternalError`。但 `OAuth2(...)` 变体本身也被用于 state 不匹配等场景，此时映射为"已被其他用户绑定"是错误的。

**修复方案**: 将"已被其他用户绑定"的错误改用专用错误变体（如新增 `OAuth2AccountAlreadyBound`），或在 handler 中更精细地匹配：

```rust
match e {
    cmx_traits::AuthError::OAuth2ProviderNotFound(_) => Error::BadRequest("Provider 不存在".to_string()),
    cmx_traits::AuthError::OAuth2ProviderUnavailable(_) => Error::BadRequest("Provider 服务不可用".to_string()),
    cmx_traits::AuthError::OAuth2ProviderTokenError(_) => Error::BadRequest("Provider 授权失败".to_string()),
    cmx_traits::AuthError::OAuth2(msg) if msg.contains("已被其他用户绑定") => {
        Error::BadRequest(msg)
    },
    other => Error::InternalError(other.to_string()),
}
```

---

## 三、安全审查

| 检查项 | 状态 | 说明 |
|--------|------|------|
| State Lua 原子消费 | ✅ | 防止 CSRF 重放 |
| 回调授权码 Lua 原子消费 | ✅ | 30s TTL，防止重放 |
| Google ID Token JWKS 签名验证 | ✅ | RS256 + iss/aud/exp 校验 |
| auto_link_by_email 要求 email_verified | ✅ | 未验证时返回 `OAuth2EmailNotVerified` |
| redirect_uri 服务端配置 | ✅ | 前端不可覆盖 |
| 第三方 Token 不持久化 | ✅ | 仅内存中使用 |
| 解绑检查最后一个绑定 | ✅ | 无密码且无其他绑定时拒绝 |
| client_secret 不暴露给前端 | ✅ | 仅服务端 Token 交换使用 |
| 回调 Token 不通过 URL 传递 | ✅ | 使用授权码模式 |
| 回调错误信息脱敏 | ✅ | `sanitize_oauth2_error` 映射 |
| frontend_callback_url 配置校验 | ✅ | 缺失时返回错误 |
| SQL 注入防护 | ✅ | 已全部改为参数化查询 |

---

## 四、修复优先级排序

| 优先级 | 编号 | 修复内容 | 工作量 |
|--------|------|----------|--------|
| P2 | N-11 | `link` handler 错误映射精细化 | 小 |

---

## 五、结论

前三轮共 25 项问题已全部修复（测试缺失项暂不处理）。核心功能链路完整，安全设计到位，SQL 注入风险已消除。

当前仅剩 **1 个 P2 问题**（N-11 错误映射过于宽泛），属于用户体验层面的优化，不影响功能正确性和安全性。修复后即可达到发布标准。
