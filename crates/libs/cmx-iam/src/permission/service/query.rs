//! 权限查询
//!
//! 实现 [`crate::service_traits::PermissionService`] 的分页/列表查询、
//! 权限树构建与权限使用统计方法。

use cmx_core::model::cell::DataValue;
use cmx_core::model::iam::{Permission, PermissionTreeNode};
use cmx_database::crud::GenericCrudService;
use cmx_traits::error::TraitError;
use modql::filter::ListOptions;
use tracing::debug;

use crate::error::IamError;
use crate::permission::service::PermissionServiceImpl;
use crate::permission::{PermissionBmc, PermissionFilter};

impl PermissionServiceImpl {
    /// 分页查询权限（[`crate::service_traits::PermissionService::page_permissions`] 的实现）。
    ///
    /// 默认附加 `archived = 0` 过滤。
    pub(super) async fn page_permissions(
        &self,
        filters: Option<Vec<PermissionFilter>>,
        list_options: ListOptions,
    ) -> Result<(Vec<Permission>, i64), TraitError> {
        debug!("{:<12} - PermissionServiceImpl::page_permissions", "IAM");

        // 对每个 filter 组注入默认 archived = 0（filters=None 时构造默认 filter，确保归档数据不泄露）
        let filters = Some(match filters {
            Some(fs) => fs
                .into_iter()
                .map(Self::with_default_archived)
                .collect::<Vec<_>>(),
            None => vec![Self::with_default_archived(PermissionFilter::default())],
        });

        let (dataset, total) = GenericCrudService::<PermissionBmc, PermissionFilter>::page(
            &self.mm,
            &self.db_id,
            None,
            filters,
            list_options,
        )
        .await
        .map_err(|e| TraitError::from(IamError::Crud(e)))?;

        let permissions = Self::extract_permissions(dataset);
        Ok((permissions, total))
    }

    /// 列表查询权限（[`crate::service_traits::PermissionService::list_permissions`] 的实现）。
    ///
    /// 默认附加 `archived = 0` 过滤，返回所有匹配记录（不分页）。
    pub(super) async fn list_permissions(
        &self,
        filters: Option<Vec<PermissionFilter>>,
        list_options: Option<ListOptions>,
    ) -> Result<Vec<Permission>, TraitError> {
        debug!("{:<12} - PermissionServiceImpl::list_permissions", "IAM");

        // 对每个 filter 组注入默认 archived = 0（filters=None 时构造默认 filter，确保归档数据不泄露）
        let filters = Some(match filters {
            Some(fs) => fs
                .into_iter()
                .map(Self::with_default_archived)
                .collect::<Vec<_>>(),
            None => vec![Self::with_default_archived(PermissionFilter::default())],
        });

        let dataset = GenericCrudService::<PermissionBmc, PermissionFilter>::list(
            &self.mm,
            &self.db_id,
            None,
            filters,
            list_options,
        )
        .await
        .map_err(|e| TraitError::from(IamError::Crud(e)))?;

        Ok(Self::extract_permissions(dataset))
    }

    /// 获取权限树（递归结构，[`crate::service_traits::PermissionService::get_permission_tree`] 的实现）。
    ///
    /// 一次性加载所有有效权限（`archived = 0 AND status = 1`），在内存中按 `parent_id` 递归构建树。
    pub(super) async fn get_permission_tree(
        &self,
        domain_code: Option<&str>,
        app_code: Option<&str>,
        module_code: Option<&str>,
    ) -> Result<Vec<PermissionTreeNode>, TraitError> {
        debug!(
            "{:<12} - PermissionServiceImpl::get_permission_tree - domain: {:?}, app: {:?}, module: {:?}",
            "IAM", domain_code, app_code, module_code
        );

        // 动态构建带过滤条件的 SQL
        let mut sql = String::from(
            "SELECT id, code, name, resource_type, parent_id, sort_order, status, description, \
             domain_code, app_code, module_code, extension \
             FROM cmx_permission WHERE archived = 0 AND status = 1",
        );
        let mut params: Vec<DataValue> = Vec::new();
        let mut param_idx = 1;

        if let Some(dc) = domain_code {
            sql.push_str(&format!(" AND domain_code = ${param_idx}"));
            params.push(DataValue::String(dc.to_string()));
            param_idx += 1;
        }
        if let Some(ac) = app_code {
            sql.push_str(&format!(" AND app_code = ${param_idx}"));
            params.push(DataValue::String(ac.to_string()));
            param_idx += 1;
        }
        if let Some(mc) = module_code {
            sql.push_str(&format!(" AND module_code = ${param_idx}"));
            params.push(DataValue::String(mc.to_string()));
        }
        sql.push_str(" ORDER BY sort_order ASC NULLS LAST, code ASC");

        let dataset = self
            .mm
            .query_sql_with_datavalues(&self.db_id, None, &sql, params, "permission_tree")
            .await
            .map_err(|e| TraitError::from(IamError::Business(format!("查询权限树失败: {e}"))))?;

        let permissions = Self::extract_permissions(dataset);

        Ok(Self::build_tree(permissions))
    }

    /// 统计每个权限被多少角色使用（[`crate::service_traits::PermissionService::get_permission_usage_stat`] 的实现）。
    ///
    /// 按 `role_count` 降序排列。
    pub(super) async fn get_permission_usage_stat(
        &self,
    ) -> Result<Vec<crate::service_traits::PermissionUsageStat>, TraitError> {
        debug!(
            "{:<12} - PermissionServiceImpl::get_permission_usage_stat",
            "IAM"
        );

        let sql = r#"
            SELECT p.id, p.code, p.name,
                   COUNT(DISTINCT rp.role_id) AS role_count,
                   COUNT(DISTINCT ur.user_id) AS user_count,
                   MAX(rp.create_time) AS last_assigned_at
            FROM cmx_permission p
            LEFT JOIN cmx_role_permission rp ON rp.permission_id = p.id AND rp.archived = 0
            LEFT JOIN cmx_user_role ur ON ur.role_id = rp.role_id AND ur.archived = 0
            WHERE p.archived = 0 AND p.status = 1
            GROUP BY p.id, p.code, p.name
            ORDER BY role_count DESC, p.sort_order, p.code
        "#;
        let params: Vec<DataValue> = vec![];
        let dataset = self
            .mm
            .query_sql_with_datavalues(&self.db_id, None, sql, params, "perm_usage_stat")
            .await
            .map_err(|e| {
                TraitError::from(IamError::Business(format!("查询权限使用统计失败: {e}")))
            })?;

        let schema = dataset.schema.as_ref();
        let stats: Vec<crate::service_traits::PermissionUsageStat> = dataset
            .iter()
            .filter_map(|row| {
                Some(crate::service_traits::PermissionUsageStat {
                    permission_id: row.get_by_name_as(schema, "id")?,
                    permission_code: row.get_by_name_as(schema, "code")?,
                    permission_name: row.get_by_name_as(schema, "name")?,
                    role_count: row.get_by_name_as::<i64>(schema, "role_count").unwrap_or(0) as u32,
                    user_count: row.get_by_name_as::<i64>(schema, "user_count").unwrap_or(0) as u32,
                    last_assigned_at: row.get_by_name_as::<chrono::DateTime<chrono::Utc>>(
                        schema,
                        "last_assigned_at",
                    ),
                })
            })
            .collect();

        Ok(stats)
    }
}
