//! 用户服务层模块
//!
//! [`crate::service_traits::UserService`] 的默认实现 `UserServiceImpl`。
//!
//! 本模块按职责拆分为多个子模块，结构体定义、构造器、`AuditHelper` 实现与
//! `UserService` trait 的薄委托实现保留在此处，各功能方法的 `impl` 块分散到子模块：
//!
//! - [`helpers`]：DataSet 提取（User / Role / UserRoleAssignment）与默认过滤注入
//! - [`crud`]：用户创建/查询/更新/删除
//! - [`query`]：分页/列表查询、用户角色查询、临时授权查询
//! - [`roles`]：永久角色授权（全量替换）
//! - [`temp_roles`]：临时角色授权生命周期 + 有效权限聚合查询
//!
//! Rust 要求一个类型对同一 trait 只能有一个 `impl`，因此本文件集中委派，
//! 实现逻辑分散在各子模块的 `impl UserServiceImpl` 固有方法块中。
//! trait 委托调用同名固有方法时，固有方法优先于 trait 方法解析，故不会递归。

use std::sync::Arc;

use async_trait::async_trait;
use cmx_core::SVRContext;
use cmx_core::model::iam::{Role, User};
use cmx_database::DatabaseManager;
use cmx_traits::auth::AuthService;
use cmx_traits::error::TraitError;
use modql::filter::ListOptions;

use crate::audit_helper::AuditHelper;
use crate::config::IamConfig;
use crate::rule::RuleEnforcer;
use crate::service_traits::{
    EffectivePermissionsResponse, TempAssignmentStatusFilter, UserRoleAssignment, UserService,
};
use crate::user::{UserFilter, UserForCreate, UserForUpdate};

mod crud;
mod helpers;
mod query;
mod roles;
mod temp_roles;

/// 用户服务实现。
pub struct UserServiceImpl {
    /// 数据库管理器。
    mm: Arc<DatabaseManager>,

    /// 认证服务（用于密码哈希）。
    auth: Arc<dyn AuthService>,

    /// 认证库 `db_id`。
    db_id: String,

    /// IAM 配置。
    config: IamConfig,

    /// 审计日志记录器（可选，通过 `with_audit` 注入）。
    audit: Option<Arc<dyn cmx_audit::AuditLogger>>,

    /// 规则校验引擎（可选，用于 SoD 校验）
    rule_enforcer: Option<Arc<dyn RuleEnforcer>>,

    /// 权限校验器引用（可选，用于缓存失效）
    permission_checker: Option<Arc<crate::iam_checker::IamChecker>>,
}

impl UserServiceImpl {
    /// 构造函数
    pub async fn new(
        mm: Arc<DatabaseManager>,
        auth: Arc<dyn AuthService>,
        config: IamConfig,
    ) -> Self {
        let db_id = match &config.auth_db_id {
            Some(id) => id.clone(),
            None => mm.get_default_db_id().await,
        };
        Self {
            mm,
            auth,
            db_id,
            config,
            audit: None,
            rule_enforcer: None,
            permission_checker: None,
        }
    }

    /// 设置审计日志记录器（Builder 模式）
    pub fn with_audit(mut self, audit: Arc<dyn cmx_audit::AuditLogger>) -> Self {
        self.audit = Some(audit);
        self
    }

    /// 设置规则校验引擎（Builder 模式）
    pub fn with_rule_enforcer(mut self, enforcer: Arc<dyn RuleEnforcer>) -> Self {
        self.rule_enforcer = Some(enforcer);
        self
    }

    /// 设置权限校验器（Builder 模式，用于缓存失效）
    pub fn with_permission_checker(mut self, checker: Arc<crate::iam_checker::IamChecker>) -> Self {
        self.permission_checker = Some(checker);
        self
    }
}

impl AuditHelper for UserServiceImpl {
    fn audit_logger(&self) -> Option<&Arc<dyn cmx_audit::AuditLogger>> {
        self.audit.as_ref()
    }
}

/// `UserService` 的唯一实现。
///
/// 各方法体委托给按职责拆分到子模块（[`crud`] / [`query`] / [`roles`] / [`temp_roles`]）
/// 中的固有方法。委托调用同名固有方法时，Rust 的固有方法优先级保证解析到子模块实现，
/// 不会回调本 trait 方法（无递归）。
#[async_trait]
impl UserService for UserServiceImpl {
    async fn create_user(
        &self,
        svr_ctx: &SVRContext,
        data: UserForCreate,
    ) -> Result<User, TraitError> {
        self.create_user(svr_ctx, data).await
    }

    async fn get_user(&self, username: &str) -> Result<User, TraitError> {
        self.get_user(username).await
    }

    async fn update_user(
        &self,
        svr_ctx: &SVRContext,
        user_id: &str,
        data: UserForUpdate,
    ) -> Result<User, TraitError> {
        self.update_user(svr_ctx, user_id, data).await
    }

    async fn delete_user(
        &self,
        svr_ctx: &SVRContext,
        user_ids: &[String],
    ) -> Result<(), TraitError> {
        self.delete_user(svr_ctx, user_ids).await
    }

    async fn page_users(
        &self,
        filters: Option<Vec<UserFilter>>,
        list_options: ListOptions,
    ) -> Result<(Vec<User>, i64), TraitError> {
        self.page_users(filters, list_options).await
    }

    async fn list_users(
        &self,
        filters: Option<Vec<UserFilter>>,
        list_options: Option<ListOptions>,
    ) -> Result<Vec<User>, TraitError> {
        self.list_users(filters, list_options).await
    }

    async fn assign_roles(
        &self,
        svr_ctx: &SVRContext,
        username: &str,
        role_ids: &[String],
    ) -> Result<(), TraitError> {
        self.assign_roles(svr_ctx, username, role_ids).await
    }

    async fn get_user_roles(&self, username: &str) -> Result<Vec<Role>, TraitError> {
        self.get_user_roles(username).await
    }

    async fn assign_temp_role(
        &self,
        svr_ctx: &SVRContext,
        user_id: &str,
        role_id: &str,
        effective_from: chrono::DateTime<chrono::Utc>,
        effective_until: chrono::DateTime<chrono::Utc>,
        reason: Option<&str>,
        source: &str,
    ) -> Result<UserRoleAssignment, TraitError> {
        self.assign_temp_role(
            svr_ctx,
            user_id,
            role_id,
            effective_from,
            effective_until,
            reason,
            source,
        )
        .await
    }

    async fn revoke_temp_role(
        &self,
        svr_ctx: &SVRContext,
        assignment_id: &str,
        reason: Option<&str>,
    ) -> Result<(), TraitError> {
        self.revoke_temp_role(svr_ctx, assignment_id, reason).await
    }

    async fn revoke_temp_roles_batch(
        &self,
        svr_ctx: &SVRContext,
        assignment_ids: &[String],
        reason: Option<&str>,
    ) -> Result<u64, TraitError> {
        self.revoke_temp_roles_batch(svr_ctx, assignment_ids, reason)
            .await
    }

    async fn extend_temp_role(
        &self,
        svr_ctx: &SVRContext,
        assignment_id: &str,
        new_effective_until: chrono::DateTime<chrono::Utc>,
        reason: Option<&str>,
    ) -> Result<(), TraitError> {
        self.extend_temp_role(svr_ctx, assignment_id, new_effective_until, reason)
            .await
    }

    async fn get_user_temp_assignments(
        &self,
        user_id: &str,
        status_filter: TempAssignmentStatusFilter,
    ) -> Result<Vec<UserRoleAssignment>, TraitError> {
        self.get_user_temp_assignments(user_id, status_filter).await
    }

    async fn get_role_temp_assigned_users(
        &self,
        role_id: &str,
        status_filter: TempAssignmentStatusFilter,
    ) -> Result<Vec<UserRoleAssignment>, TraitError> {
        self.get_role_temp_assigned_users(role_id, status_filter)
            .await
    }

    async fn get_effective_permissions(
        &self,
        user_id: &str,
    ) -> Result<EffectivePermissionsResponse, TraitError> {
        self.get_effective_permissions(user_id).await
    }
}
