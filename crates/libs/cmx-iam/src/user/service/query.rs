//! 用户查询
//!
//! 实现 [`crate::service_traits::UserService`] 的分页/列表查询、用户角色查询，
//! 以及临时授权的查询方法（用户维度、角色维度）。

use cmx_core::model::cell::DataValue;
use cmx_core::model::iam::{Role, User};
use cmx_database::crud::GenericCrudService;
use cmx_traits::error::TraitError;
use modql::filter::ListOptions;
use tracing::debug;

use crate::error::IamError;
use crate::service_traits::{TempAssignmentStatusFilter, UserRoleAssignment};
use crate::user::service::UserServiceImpl;
use crate::user::{UserBmc, UserFilter};

impl UserServiceImpl {
    /// 分页查询用户（[`crate::service_traits::UserService::page_users`] 的实现）。
    ///
    /// 默认附加 `archived = 0` 过滤。
    pub(super) async fn page_users(
        &self,
        filters: Option<Vec<UserFilter>>,
        list_options: ListOptions,
    ) -> Result<(Vec<User>, i64), TraitError> {
        debug!("{:<12} - UserServiceImpl::page_users", "IAM");

        // 对每个 filter 组注入默认 archived = 0（filters=None 时构造默认 filter，确保归档数据不泄露）
        let filters = Some(match filters {
            Some(fs) => fs
                .into_iter()
                .map(Self::with_default_archived)
                .collect::<Vec<_>>(),
            None => vec![Self::with_default_archived(UserFilter::default())],
        });

        let (dataset, total) = GenericCrudService::<UserBmc, UserFilter>::page(
            &self.mm,
            &self.db_id,
            None,
            filters,
            list_options,
        )
        .await
        .map_err(|e| TraitError::from(IamError::Crud(e)))?;

        let users = Self::extract_users(dataset);
        Ok((users, total))
    }

    /// 列表查询用户（[`crate::service_traits::UserService::list_users`] 的实现）。
    ///
    /// 默认附加 `archived = 0` 过滤，返回所有匹配记录（不分页）。
    pub(super) async fn list_users(
        &self,
        filters: Option<Vec<UserFilter>>,
        list_options: Option<ListOptions>,
    ) -> Result<Vec<User>, TraitError> {
        debug!("{:<12} - UserServiceImpl::list_users", "IAM");

        // 对每个 filter 组注入默认 archived = 0（filters=None 时构造默认 filter，确保归档数据不泄露）
        let filters = Some(match filters {
            Some(fs) => fs
                .into_iter()
                .map(Self::with_default_archived)
                .collect::<Vec<_>>(),
            None => vec![Self::with_default_archived(UserFilter::default())],
        });

        let dataset = GenericCrudService::<UserBmc, UserFilter>::list(
            &self.mm,
            &self.db_id,
            None,
            filters,
            list_options,
        )
        .await
        .map_err(|e| TraitError::from(IamError::Crud(e)))?;

        Ok(Self::extract_users(dataset))
    }

    /// 获取用户已启用的角色列表（按 username 查询，[`crate::service_traits::UserService::get_user_roles`] 的实现）。
    ///
    /// 含 `status = 1` 且 `archived = 0` 过滤。
    pub(super) async fn get_user_roles(&self, username: &str) -> Result<Vec<Role>, TraitError> {
        debug!(
            "{:<12} - UserServiceImpl::get_user_roles - username: {}",
            "IAM", username
        );

        let sql = r#"
            SELECT r.id, r.code, r.name, r.data_scope, r.sort_order, r.status, r.description,
                   r.archived, r.create_time, r.update_time,
                   r.create_by, r.create_name, r.update_by, r.update_name
            FROM cmx_role r
            INNER JOIN cmx_user_role ur ON ur.role_id = r.id
            INNER JOIN cmx_user u ON u.id = ur.user_id
            WHERE u.username = $1 AND ur.archived = 0 AND r.archived = 0 AND r.status = 1
              AND u.archived = 0
        "#;
        let params = vec![DataValue::String(username.to_string())];

        let dataset = self
            .mm
            .query_sql_with_datavalues(&self.db_id, None, sql, params, "user_roles")
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("查询用户角色失败: {e}"))))?;

        Ok(Self::extract_roles(dataset))
    }

    /// 查询用户的临时授权列表（[`crate::service_traits::UserService::get_user_temp_assignments`] 的实现）。
    pub(super) async fn get_user_temp_assignments(
        &self,
        user_id: &str,
        status_filter: TempAssignmentStatusFilter,
    ) -> Result<Vec<UserRoleAssignment>, TraitError> {
        debug!(
            "{:<12} - UserServiceImpl::get_user_temp_assignments - user: {}, filter: {:?}",
            "IAM", user_id, status_filter
        );

        let mut where_clause = String::from("a.user_id = $1 AND a.archived = 0 AND r.archived = 0");
        match status_filter {
            TempAssignmentStatusFilter::All => {}
            TempAssignmentStatusFilter::Active => {
                where_clause.push_str(
                    " AND a.status = 1 AND NOW() BETWEEN a.effective_from AND a.effective_until",
                );
            }
            TempAssignmentStatusFilter::Expired => {
                where_clause.push_str(" AND a.status = 1 AND a.effective_until < NOW()");
            }
            TempAssignmentStatusFilter::Revoked => {
                where_clause.push_str(" AND a.status = 0");
            }
        }

        let sql = format!(
            r#"
            SELECT a.id, a.user_id, a.role_id, a.effective_from, a.effective_until,
                   a.reason, a.source, a.status, a.revoked_by, a.revoked_at, a.create_time,
                   r.code, r.name,
                   u.username, u.nickname
            FROM cmx_user_role_assignment a
            INNER JOIN cmx_role r ON r.id = a.role_id
            LEFT JOIN cmx_user u ON u.id = a.user_id AND u.archived = 0
            WHERE {where_clause}
            ORDER BY a.create_time DESC
            "#
        );
        let params = vec![DataValue::String(user_id.to_string())];

        let dataset = self
            .mm
            .query_sql_with_datavalues(&self.db_id, None, &sql, params, "user_temp_assignments")
            .await
            .map_err(|e| {
                TraitError::from(IamError::Business(format!("查询用户临时授权失败: {e}")))
            })?;

        Ok(Self::extract_assignments(dataset))
    }

    /// 查询角色被授权的用户列表（临时授权，[`crate::service_traits::UserService::get_role_temp_assigned_users`] 的实现）。
    pub(super) async fn get_role_temp_assigned_users(
        &self,
        role_id: &str,
        status_filter: TempAssignmentStatusFilter,
    ) -> Result<Vec<UserRoleAssignment>, TraitError> {
        debug!(
            "{:<12} - UserServiceImpl::get_role_temp_assigned_users - role: {}, filter: {:?}",
            "IAM", role_id, status_filter
        );

        let mut where_clause = String::from("a.role_id = $1 AND a.archived = 0 AND r.archived = 0");
        match status_filter {
            TempAssignmentStatusFilter::All => {}
            TempAssignmentStatusFilter::Active => {
                where_clause.push_str(
                    " AND a.status = 1 AND NOW() BETWEEN a.effective_from AND a.effective_until",
                );
            }
            TempAssignmentStatusFilter::Expired => {
                where_clause.push_str(" AND a.status = 1 AND a.effective_until < NOW()");
            }
            TempAssignmentStatusFilter::Revoked => {
                where_clause.push_str(" AND a.status = 0");
            }
        }

        let sql = format!(
            r#"
            SELECT a.id, a.user_id, a.role_id, a.effective_from, a.effective_until,
                   a.reason, a.source, a.status, a.revoked_by, a.revoked_at, a.create_time,
                   r.code, r.name,
                   u.username, u.nickname
            FROM cmx_user_role_assignment a
            INNER JOIN cmx_role r ON r.id = a.role_id
            LEFT JOIN cmx_user u ON u.id = a.user_id AND u.archived = 0
            WHERE {where_clause}
            ORDER BY a.create_time DESC
            "#
        );
        let params = vec![DataValue::String(role_id.to_string())];

        let dataset = self
            .mm
            .query_sql_with_datavalues(&self.db_id, None, &sql, params, "role_temp_users")
            .await
            .map_err(|e| {
                TraitError::from(IamError::Business(format!("查询角色临时用户失败: {e}")))
            })?;

        Ok(Self::extract_assignments(dataset))
    }
}
