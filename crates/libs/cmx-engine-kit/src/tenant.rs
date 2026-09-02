//! 请求级租户上下文（多租户 db-per-tenant）。
//!
//! 镜像平台 `cmx-traits::auth::context_scope` 的 `task_local!` 模式：认证中间件在请求入口建
//! scope，请求生命周期内任意 `.await` 点都能无参读当前租户/用户/角色，无需层层透传。
//!
//! ⚠️ **task_local 不跨 `tokio::spawn`**：后台任务（webhook worker / timer poller）读不到，
//! 须显式捕获租户。故 poller 遍历运行时缓存、webhook emit 只在 handler（有 scope）内做。
//!
//! **单租户零回归**：无 scope（未装认证中间件 / 后台任务 / 默认部署）时 `current_tenant()`
//! 回退 [`DEFAULT_TENANT`]，其 db_id 映射到各引擎的默认库（如 flow 的 `fico-db`、
//! rule 的 `rule_pg`）——行为完全等价多租户改造前的单库形态。
//!
//! 收编自 cmx-flow-app / cmx-rule-app 的同源副本（两仓代码体一致，flow 版含全部文档注释
//! 与测试；本文件为唯一真源，两仓 `src/tenant.rs` 保留为 re-export shim）。

use tokio::task_local;

/// 默认租户名（无租户上下文时的回退；其 db_id 映射到各引擎默认库，保单租户零回归）。
pub const DEFAULT_TENANT: &str = "default";

task_local! {
    /// 当前请求的租户上下文。仅在认证中间件 [`scope`] 作用域内有值。
    static TENANT: TenantCtx;
}

/// 请求级租户上下文快照（认证中间件一次性填充，请求内只读）。
#[derive(Debug, Clone)]
pub struct TenantCtx {
    /// 租户标识（决定用哪个租户库）。
    pub tenant: String,
    /// 当前用户 id（JWT sub；可空——auth off 时无）。授权比对（assignee/initiator）用此。
    pub user: Option<String>,
    /// 当前用户名（JWT `username` claim；可空——旧令牌/第三方精简令牌无）。留痕/审计
    /// 展示用 [`current_display_user`] 取「用户名优先、id 兜底」，勿拿 user 直接当姓名。
    pub username: Option<String>,
    /// 当前用户昵称（JWT `nickname` claim；可空——旧令牌未签发该 claim）。展示名首选：
    /// [`current_display_nickname`] 供审批留痕/快照列取「昵称优先、username 兜底」。
    pub nickname: Option<String>,
    /// 当前用户角色（JWT roles；可空）。
    pub roles: Vec<String>,
    /// 调用方业务系统标识（技术债 003：API Key 结构化声明；None = legacy key 未声明系统，
    /// 归属过滤/命名空间校验一律放行——两阶段迁移口径，存量共享 key 零破坏）。
    pub system: Option<String>,
    /// 委托令牌 fail-close 标记（技术债 004/003-3）：请求带了 `X-Delegated-User-Token`
    /// 但验签失败。涉及身份的动作端点（发起/取消/跳转等）须拒绝；纯查询可降级放行。
    pub delegation_failed: bool,
    /// 结构化 key 声明的流程定义白名单（技术债 003；空 = 不限——legacy key 的过渡态语义）。
    /// 发起端点据此校验 definitionKey 归属（精确全等）。
    pub allowed_definition_keys: Vec<String>,
}

impl TenantCtx {
    /// 用租户名构建（其余字段空/false）。
    pub fn new(tenant: impl Into<String>) -> Self {
        Self {
            tenant: tenant.into(),
            user: None,
            username: None,
            nickname: None,
            roles: Vec::new(),
            system: None,
            delegation_failed: false,
            allowed_definition_keys: Vec::new(),
        }
    }
    pub fn with_user(mut self, user: Option<String>) -> Self {
        self.user = user;
        self
    }
    /// 携带调用方系统标识（003 结构化 key 声明；链路贯通到实例归属/查询过滤）。
    pub fn with_system(mut self, system: Option<String>) -> Self {
        self.system = system;
        self
    }
    /// 携带流程定义白名单（003；空 = 不限）。
    pub fn with_allowed_definition_keys(mut self, keys: Vec<String>) -> Self {
        self.allowed_definition_keys = keys;
        self
    }
    pub fn with_username(mut self, username: Option<String>) -> Self {
        self.username = username;
        self
    }
    pub fn with_nickname(mut self, nickname: Option<String>) -> Self {
        self.nickname = nickname;
        self
    }
    pub fn with_roles(mut self, roles: Vec<String>) -> Self {
        self.roles = roles;
        self
    }
}

/// 在给定租户上下文的作用域内执行 future（认证中间件在请求入口调用）。
pub async fn scope<F, R>(ctx: TenantCtx, fut: F) -> R
where
    F: std::future::Future<Output = R>,
{
    TENANT.scope(ctx, fut).await
}

/// 当前租户名。无 scope 时回退 [`DEFAULT_TENANT`]（单租户零回归）。
pub fn current_tenant() -> String {
    TENANT
        .try_with(|c| c.tenant.clone())
        .unwrap_or_else(|_| DEFAULT_TENANT.to_string())
}

/// 当前用户 id（无 scope / 未认证时 None）。
pub fn current_user() -> Option<String> {
    TENANT.try_with(|c| c.user.clone()).ok().flatten()
}

/// 留痕/审计展示用操作人名：优先 `username` claim（如 "admin"），无则回退用户 id——
/// 平台 AccessClaims 自带 `username`，旧令牌/第三方精简令牌缺省时退 id 保证不空。
pub fn current_display_user() -> Option<String> {
    TENANT
        .try_with(|c| {
            c.username
                .clone()
                .filter(|s| !s.trim().is_empty())
                .or_else(|| c.user.clone())
        })
        .ok()
        .flatten()
}

/// 昵称优先的展示名：`nickname` claim → `username` claim，均无则 None（不回退 id——
/// 供审批意见 nick_name 快照列等场景，宁缺勿假）。昵称为空/旧令牌未签发时自然落到 username。
pub fn current_display_nickname() -> Option<String> {
    TENANT
        .try_with(|c| {
            c.nickname
                .clone()
                .filter(|s| !s.trim().is_empty())
                .or_else(|| c.username.clone().filter(|s| !s.trim().is_empty()))
        })
        .ok()
        .flatten()
}

/// 当前用户角色（无 scope 时空）。
pub fn current_roles() -> Vec<String> {
    TENANT.try_with(|c| c.roles.clone()).unwrap_or_default()
}

/// 当前调用方系统标识（003 结构化 key 声明；无 scope / legacy key 时 None）。
pub fn current_system() -> Option<String> {
    TENANT.try_with(|c| c.system.clone()).ok().flatten()
}

/// 本请求是否携带了验签失败的委托令牌（004/003-3 fail-close 依据）。
/// 涉及身份的动作端点据此拒绝；纯查询端点可降级放行。
pub fn delegation_failed() -> bool {
    TENANT.try_with(|c| c.delegation_failed).unwrap_or(false)
}

/// 当前请求的结构化 key 流程定义白名单（003；空 = 不限）。
pub fn current_allowed_definition_keys() -> Vec<String> {
    TENANT
        .try_with(|c| c.allowed_definition_keys.clone())
        .unwrap_or_default()
}

/// 是否处于租户 scope 内（认证中间件已建立）。
pub fn in_scope() -> bool {
    TENANT.try_with(|_| ()).is_ok()
}

/// 身份快照 —— 供通用监控 crate [`cmx_web_monitor`] 的 observe 中间件读取当前请求身份。
///
/// 注册方式：引擎 server 启动时 `cmx_web_monitor::set_identity_provider(identity_snapshot)`。
/// observe 夹在认证之后（scope 已建），故这里能读到 tenant/user/roles；无 scope 返 None（记为匿名）。
pub fn identity_snapshot() -> Option<cmx_web_monitor::Identity> {
    if !in_scope() {
        return None;
    }
    Some(cmx_web_monitor::Identity {
        tenant: current_tenant(),
        user: current_user(),
        roles: current_roles(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn no_scope_falls_back_to_default() {
        // 无 scope：回退默认租户（单租户零回归的核心保证）。
        assert_eq!(current_tenant(), DEFAULT_TENANT);
        assert_eq!(current_user(), None);
        assert!(current_roles().is_empty());
        assert!(!in_scope());
    }

    #[tokio::test]
    async fn scope_threads_tenant() {
        let ctx = TenantCtx::new("acme")
            .with_user(Some("u_1".into()))
            .with_username(Some("alice".into()))
            .with_nickname(Some("爱丽丝".into()))
            .with_roles(vec!["approver".into()]);
        scope(ctx, async {
            assert!(in_scope());
            assert_eq!(current_tenant(), "acme");
            assert_eq!(current_user(), Some("u_1".to_string()));
            assert_eq!(current_display_user(), Some("alice".to_string()));
            assert_eq!(current_display_nickname(), Some("爱丽丝".to_string()));
            assert_eq!(current_roles(), vec!["approver".to_string()]);
        })
        .await;
        // 出 scope 后回退默认。
        assert_eq!(current_tenant(), DEFAULT_TENANT);
    }

    #[tokio::test]
    async fn display_fallbacks() {
        // 昵称缺失：current_display_nickname 落到 username；current_display_user 保持 username 优先。
        let ctx = TenantCtx::new("t").with_user(Some("u_2".into())).with_username(Some("bob".into()));
        scope(ctx, async {
            assert_eq!(current_display_nickname(), Some("bob".to_string()));
            assert_eq!(current_display_user(), Some("bob".to_string()));
        })
        .await;
    }
}
