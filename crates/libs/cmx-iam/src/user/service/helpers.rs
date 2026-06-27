//! 用户服务内部辅助方法
//!
//! 提供 `UserServiceImpl` 的 DataSet 提取（User / Role / UserRoleAssignment）
//! 与默认过滤注入等纯数据映射工具，供各功能子模块复用。
//! 这些方法以 `pub(super)` 暴露给 [`super`] 的其他子模块，不构成对外 API。

use modql::filter::{OpValInt64, OpValsInt64};

use crate::error::IamError;
use crate::user::service::UserServiceImpl;
use crate::service_traits::UserRoleAssignment;
use crate::user::UserFilter;
use cmx_core::model::iam::{Role, User};

impl UserServiceImpl {
    /// 从 DataSet 第一行提取单个 `User`。
    ///
    /// # Arguments
    ///
    /// * `dataset` - 数据库查询返回的数据集。
    ///
    /// # Returns
    ///
    /// 成功时返回 `User`。
    ///
    /// # Errors
    ///
    /// * `IamError::UserNotFound` - 数据集为空。
    /// * `IamError::Business` - 反序列化失败。
    pub(super) fn extract_user(
        dataset: cmx_core::model::data::dataset::DataSet,
    ) -> Result<User, IamError> {
        let schema = dataset.schema.as_ref();
        let row = dataset
            .iter()
            .next()
            .ok_or_else(|| IamError::UserNotFound("记录不存在".to_string()))?;
        let json_val = row.to_json_value(schema);
        serde_json::from_value::<User>(json_val)
            .map_err(|e| IamError::Business(format!("用户反序列化失败: {e}")))
    }

    /// 从 DataSet 提取 `User` 列表（跳过反序列化失败的行）。
    pub(super) fn extract_users(dataset: cmx_core::model::data::dataset::DataSet) -> Vec<User> {
        let schema = dataset.schema.as_ref();
        dataset
            .iter()
            .filter_map(|row| {
                let json_val = row.to_json_value(schema);
                serde_json::from_value::<User>(json_val).ok()
            })
            .collect()
    }

    /// 从 DataSet 提取 `Role` 列表（跳过反序列化失败的行）。
    pub(super) fn extract_roles(dataset: cmx_core::model::data::dataset::DataSet) -> Vec<Role> {
        let schema = dataset.schema.as_ref();
        dataset
            .iter()
            .filter_map(|row| {
                let json_val = row.to_json_value(schema);
                serde_json::from_value::<Role>(json_val).ok()
            })
            .collect()
    }

    /// 从 DataSet 提取 `UserRoleAssignment` 列表（JOIN 结果）。
    ///
    /// 期望列：id / user_id / role_id / code / name / effective_from /
    /// effective_until / reason / source / status / revoked_by / revoked_at / create_time。
    pub(super) fn extract_assignments(
        dataset: cmx_core::model::data::dataset::DataSet,
    ) -> Vec<UserRoleAssignment> {
        let schema = dataset.schema.as_ref();
        dataset
            .iter()
            .filter_map(|row| {
                Some(UserRoleAssignment {
                    id: row.get_by_name_as(schema, "id")?,
                    user_id: row.get_by_name_as(schema, "user_id")?,
                    role_id: row.get_by_name_as(schema, "role_id")?,
                    role_code: row.get_by_name_as(schema, "code").unwrap_or_default(),
                    role_name: row.get_by_name_as(schema, "name").unwrap_or_default(),
                    effective_from: row
                        .get_by_name_as::<chrono::DateTime<chrono::Utc>>(schema, "effective_from")?,
                    effective_until: row.get_by_name_as::<chrono::DateTime<chrono::Utc>>(
                        schema,
                        "effective_until",
                    )?,
                    reason: row.get_by_name_as(schema, "reason"),
                    source: row
                        .get_by_name_as(schema, "source")
                        .unwrap_or_else(|| "manual".to_string()),
                    status: row.get_by_name_as::<i64>(schema, "status").unwrap_or(1),
                    revoked_by: row.get_by_name_as(schema, "revoked_by"),
                    revoked_at: row
                        .get_by_name_as::<chrono::DateTime<chrono::Utc>>(schema, "revoked_at"),
                    create_time: row
                        .get_by_name_as::<chrono::DateTime<chrono::Utc>>(schema, "create_time")
                        .unwrap_or_else(chrono::Utc::now),
                })
            })
            .collect()
    }

    /// 构造带 `archived = 0` 默认过滤的 `UserFilter`。
    ///
    /// 当 `filter.archived` 未设置时注入默认值，确保默认只查询未归档记录。
    pub(super) fn with_default_archived(mut filter: UserFilter) -> UserFilter {
        if filter.archived.is_none() {
            filter.archived = Some(OpValsInt64(vec![OpValInt64::Eq(0)]));
        }
        filter
    }
}
