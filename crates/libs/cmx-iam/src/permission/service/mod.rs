//! 权限服务层模块
//!
//! [`crate::service_traits::PermissionService`] 的默认实现 `PermissionServiceImpl`，
//! 以及插件权限导入/清理的固有 API。
//!
//! 本模块按职责拆分为多个子模块，结构体定义、构造器、`AuditHelper` 实现与
//! `PermissionService` trait 的薄委托实现保留在此处，各功能方法的 `impl` 块分散到子模块：
//!
//! - [`helpers`]：DataSet 提取、默认过滤注入、权限树构建
//! - [`txn`]：事务内查询 helper（作用域/受影响角色/父 meta/后代/使用检查/is_leaf 重算）
//! - [`import`]：插件权限导入/清理固有方法 + ZIP 解析 + `PermissionDefinition`/`PermissionFile`
//! - [`crud`]：权限创建/查询/更新/删除
//! - [`query`]：分页/列表查询、权限树、使用统计
//!
//! Rust 要求一个类型对同一 trait 只能有一个 `impl`，因此本文件集中委派，
//! 实现逻辑分散在各子模块的 `impl PermissionServiceImpl` 固有方法块中。
//! trait 委托调用同名固有方法时，固有方法优先于 trait 方法解析，故不会递归。

use std::sync::Arc;

use async_trait::async_trait;
use cmx_core::SVRContext;
use cmx_core::model::iam::{Permission, PermissionTreeNode};
use cmx_database::DatabaseManager;
use cmx_traits::error::TraitError;
use modql::filter::ListOptions;

use crate::audit_helper::AuditHelper;
use crate::config::IamConfig;
use crate::iam_checker::IamChecker;
use crate::permission::{PermissionFilter, PermissionForCreate, PermissionForUpdate};
use crate::service_traits::{PermissionService, PermissionUsageStat};

// 插件权限文件解析结构体（供外部 re-export）
pub use import::{PermissionDefinition, PermissionFile};

mod crud;
mod helpers;
mod import;
mod query;
mod txn;

/// 权限服务实现。
pub struct PermissionServiceImpl {
    /// 数据库管理器。
    mm: Arc<DatabaseManager>,
    /// 认证库 `db_id`。
    db_id: String,
    /// IAM 配置（预留：用于权限缓存 TTL 等扩展）。
    #[allow(dead_code)]
    config: IamConfig,
    /// 审计日志记录器（可选）。
    audit: Option<Arc<dyn cmx_audit::AuditLogger>>,
    /// IAM 权限校验器（可选，用于精准缓存失效）。
    iam_checker: Option<Arc<IamChecker>>,
}

impl PermissionServiceImpl {
    /// 构造函数。
    ///
    /// # Arguments
    ///
    /// * `mm` - 数据库管理器。
    /// * `config` - IAM 配置，用于确定认证库 `db_id`。
    ///
    /// # Returns
    ///
    /// 返回 `PermissionServiceImpl` 实例，未设置审计记录器。
    pub async fn new(mm: Arc<DatabaseManager>, config: IamConfig) -> Self {
        let db_id = match &config.auth_db_id {
            Some(id) => id.clone(),
            None => mm.get_default_db_id().await,
        };
        Self {
            mm,
            db_id,
            config,
            audit: None,
            iam_checker: None,
        }
    }

    /// 设置审计日志记录器（Builder 模式）。
    pub fn with_audit(mut self, audit: Arc<dyn cmx_audit::AuditLogger>) -> Self {
        self.audit = Some(audit);
        self
    }

    /// 设置 IAM 权限校验器（Builder 模式）。
    ///
    /// 注入后，导入/清理权限操作完成时会触发精准缓存失效。
    pub fn with_iam_checker(mut self, checker: Arc<IamChecker>) -> Self {
        self.iam_checker = Some(checker);
        self
    }
}

impl AuditHelper for PermissionServiceImpl {
    fn audit_logger(&self) -> Option<&Arc<dyn cmx_audit::AuditLogger>> {
        self.audit.as_ref()
    }
}

/// `PermissionService` 的唯一实现。
///
/// 各方法体委托给按职责拆分到子模块（[`crud`] / [`query`]）中的固有方法。
/// 委托调用同名固有方法时，Rust 的固有方法优先级保证解析到子模块实现，
/// 不会回调本 trait 方法（无递归）。
#[async_trait]
impl PermissionService for PermissionServiceImpl {
    async fn create_permission(
        &self,
        svr_ctx: &SVRContext,
        data: PermissionForCreate,
    ) -> Result<Permission, TraitError> {
        self.create_permission(svr_ctx, data).await
    }

    async fn get_permission(&self, permission_id: &str) -> Result<Permission, TraitError> {
        self.get_permission(permission_id).await
    }

    async fn update_permission(
        &self,
        svr_ctx: &SVRContext,
        permission_id: &str,
        data: PermissionForUpdate,
    ) -> Result<Permission, TraitError> {
        self.update_permission(svr_ctx, permission_id, data).await
    }

    async fn delete_permission(
        &self,
        svr_ctx: &SVRContext,
        permission_ids: &[String],
    ) -> Result<crate::permission::DeletePermissionOutcome, TraitError> {
        self.delete_permission(svr_ctx, permission_ids).await
    }

    async fn page_permissions(
        &self,
        filters: Option<Vec<PermissionFilter>>,
        list_options: ListOptions,
    ) -> Result<(Vec<Permission>, i64), TraitError> {
        self.page_permissions(filters, list_options).await
    }

    async fn list_permissions(
        &self,
        filters: Option<Vec<PermissionFilter>>,
        list_options: Option<ListOptions>,
    ) -> Result<Vec<Permission>, TraitError> {
        self.list_permissions(filters, list_options).await
    }

    async fn get_permission_tree(
        &self,
        domain_code: Option<&str>,
        app_code: Option<&str>,
        module_code: Option<&str>,
    ) -> Result<Vec<PermissionTreeNode>, TraitError> {
        self.get_permission_tree(domain_code, app_code, module_code)
            .await
    }

    async fn get_permission_usage_stat(&self) -> Result<Vec<PermissionUsageStat>, TraitError> {
        self.get_permission_usage_stat().await
    }
}
