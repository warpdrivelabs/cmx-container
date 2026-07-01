//! IAM 服务初始化
//!
//! 初始化 cmx-iam 各服务实例并注入 CmxAppState。
//! UserAuthQueryImpl 在此创建，共享给 AuthServiceImpl 使用。

use std::collections::HashMap;
use std::sync::Arc;

use cmx_api::app_state::IamState;
use cmx_api::middleware::GlobalPermissionConfig;
use cmx_database::get_default_db_manager;
use cmx_iam::config::IamConfig;
use cmx_iam::iam_checker::IamChecker;
use cmx_iam::permission::{PermissionServiceImpl, PluginDataImporterImpl};
use cmx_iam::role::RoleServiceImpl;
use cmx_iam::role_group::RoleGroupServiceImpl;
use cmx_iam::rule::{ExclusionRuleServiceImpl, RuleEnforcerImpl};
use cmx_iam::user::UserServiceImpl;
use cmx_iam::user_auth_query_impl::UserAuthQueryImpl;
use cmx_traits::auth::UserAuthQuery;
use cmx_traits::plugin::PluginDataImporter;
use tracing::{info, warn};

use crate::error::Result;

/// 创建 IAM 服务（含 UserAuthQueryImpl）
///
/// 返回 `(IamState, Arc<dyn UserAuthQuery>, IamConfig, Option<Arc<dyn PluginDataImporter>>)`，
/// 其中：
/// - `UserAuthQuery` 供 AuthServiceImpl 共享使用；
/// - `IamConfig` 供 finalize_iam_state 使用，避免重复解析配置；
/// - `PluginDataImporter` 供 HTTP 端点和 gRPC 服务端统一调用权限导入/清理逻辑，
///   仅当 `PermissionServiceImpl` 成功创建时返回 `Some`。
///
/// # 参数
/// * `audit_logger` - 审计日志器，注入到各 IAM Service（RuleEnforcer、ExclusionRuleService、
///   RoleService、PermissionService、RoleGroupService、UserService）
pub async fn init_iam_services(
    audit_logger: Arc<dyn cmx_audit::AuditLogger>,
) -> Result<(
    Arc<IamState>,
    Arc<dyn UserAuthQuery>,
    IamConfig,
    Option<Arc<dyn PluginDataImporter>>,
)> {
    // 1. 加载 IAM 配置
    let iam_config = load_iam_config();

    // 2. 获取 DatabaseManager
    let mm = get_default_db_manager();

    // 3. 创建 UserAuthQueryImpl（共享给 AuthServiceImpl）
    let user_auth_query_impl = UserAuthQueryImpl::new(mm.clone(), &iam_config).await
        .map_err(|e| crate::error::Error::ServerSetup(format!("UserAuthQueryImpl 初始化失败: {}", e)))?;
    let user_auth_query: Arc<dyn UserAuthQuery> = Arc::new(user_auth_query_impl);

    // 4. 创建各 Service 实现（UserServiceImpl 需要 auth_service，由调用方在创建后设置）
    // 注意：UserServiceImpl::new 需要 Arc<dyn AuthService>，这里先占位，由调用方创建后再构建
    // 改为两阶段初始化：先返回 user_auth_query，IamState 在 auth_service 创建后再组装

    // 4.1 创建规则校验引擎和规则服务
    let rule_enforcer: Arc<cmx_iam::rule::RuleEnforcerImpl> = Arc::new(
        RuleEnforcerImpl::new(mm.clone(), iam_config.clone()).await,
    );
    let rule_enforcer_dyn: Arc<dyn cmx_iam::rule::RuleEnforcer> = rule_enforcer.clone();

    let rule_service: Arc<dyn cmx_iam::rule::service::ExclusionRuleService> = Arc::new(
        ExclusionRuleServiceImpl::new(mm.clone(), iam_config.clone()).await
            .with_audit(audit_logger.clone()),
    );

    let permission_checker_impl = IamChecker::new(mm.clone(), iam_config.clone()).await;
    let permission_checker: Arc<dyn cmx_traits::iam::PermissionChecker> =
        Arc::new(permission_checker_impl.clone());
    let iam_checker_arc: Arc<cmx_iam::IamChecker> = Arc::new(permission_checker_impl);

    let role_service: Arc<dyn cmx_iam::service_traits::RoleService> = Arc::new(
        RoleServiceImpl::new(mm.clone(), iam_config.clone()).await
            .with_rule_enforcer(rule_enforcer_dyn.clone())
            .with_permission_checker(iam_checker_arc.clone())
            .with_audit(audit_logger.clone()),
    );

    // 创建 PermissionServiceImpl 时保留具体类型 Arc<PermissionServiceImpl>，
    // 用于构造 PluginDataImporterImpl（固有方法 import_permissions/cleanup_permissions
    // 不在 trait 上，必须通过具体类型调用）。
    let permission_service_impl = Arc::new(
        PermissionServiceImpl::new(mm.clone(), iam_config.clone()).await
            .with_audit(audit_logger.clone())
            .with_iam_checker(iam_checker_arc.clone()),
    );
    let permission_service: Arc<dyn cmx_iam::service_traits::PermissionService> =
        permission_service_impl.clone();

    // 构造插件数据导入器（HTTP 端点和 gRPC 服务端共用）
    let plugin_data_importer: Arc<dyn PluginDataImporter> =
        Arc::new(PluginDataImporterImpl::new(permission_service_impl));

    let role_group_service: Arc<dyn cmx_iam::service_traits::RoleGroupService> = Arc::new(
        RoleGroupServiceImpl::new(mm.clone(), iam_config.clone()).await
            .with_audit(audit_logger.clone()),
    );

    // 5. 初始化全局权限校验器
    if let Err(e) = GlobalPermissionConfig::initialize_checker(permission_checker.clone()) {
        warn!("全局权限校验器初始化失败: {}", e);
    }

    // 6. 初始化全局权限映射
    let permission_map = load_permission_map();
    if !permission_map.is_empty()
        && let Err(e) = GlobalPermissionConfig::initialize(permission_map) {
            warn!("全局权限映射初始化失败: {}", e);
        }

    info!("IAM 基础服务初始化完成（等待 AuthService 注入 UserService）");

    // 返回部分初始化的组件，user_service 需要等 auth_service 创建后再构建
    Ok((
        Arc::new(IamState {
            user_service: Arc::new(PlaceholderUserService),
            role_service,
            role_group_service,
            permission_service,
            rule_service: Some(rule_service),
            permission_checker,
            iam_checker: Some(iam_checker_arc.clone()),
            user_auth_query: user_auth_query.clone(),
        }),
        user_auth_query,
        iam_config,
        Some(plugin_data_importer),
    ))
}

/// 用 auth_service 完成 IamState 的最终组装
///
/// 替换 IamState 中的占位 user_service 为真实的 UserServiceImpl。
///
/// # 参数
/// * `audit_logger` - 审计日志器，注入到 UserServiceImpl
pub async fn finalize_iam_state(
    iam_state: &Arc<IamState>,
    auth_service: Arc<dyn cmx_traits::auth::AuthService>,
    iam_config: IamConfig,
    audit_logger: Arc<dyn cmx_audit::AuditLogger>,
) -> Result<Arc<IamState>> {
    let mm = get_default_db_manager();

    // 重新创建 RuleEnforcer（用于 UserServiceImpl 注入）
    let rule_enforcer: Arc<dyn cmx_iam::rule::RuleEnforcer> = Arc::new(
        RuleEnforcerImpl::new(mm.clone(), iam_config).await,
    );

    let user_service: Arc<dyn cmx_iam::service_traits::UserService> = Arc::new(
        UserServiceImpl::new(mm.clone(), auth_service, IamConfig::default()).await
            .with_rule_enforcer(rule_enforcer)
            .with_permission_checker(iam_state.permission_checker_clone())
            .with_audit(audit_logger),
    );

    let finalized = Arc::new(IamState {
        user_service,
        role_service: iam_state.role_service.clone(),
        role_group_service: iam_state.role_group_service.clone(),
        permission_service: iam_state.permission_service.clone(),
        rule_service: iam_state.rule_service.clone(),
        permission_checker: iam_state.permission_checker.clone(),
        iam_checker: iam_state.iam_checker.clone(),
        user_auth_query: iam_state.user_auth_query.clone(),
    });

    info!("IAM 服务最终组装完成");
    Ok(finalized)
}

/// 占位 UserService（在 auth_service 创建前使用）
struct PlaceholderUserService;

#[async_trait::async_trait]
impl cmx_iam::service_traits::UserService for PlaceholderUserService {
    async fn create_user(
        &self,
        _svr_ctx: &cmx_core::SVRContext,
        _data: cmx_iam::user::UserForCreate,
    ) -> std::result::Result<cmx_core::model::iam::User, cmx_traits::error::TraitError> {
        Err(cmx_traits::error::TraitError::Internal("IAM 服务尚未完成初始化".to_string()))
    }

    async fn get_user(
        &self,
        _username: &str,
    ) -> std::result::Result<cmx_core::model::iam::User, cmx_traits::error::TraitError> {
        Err(cmx_traits::error::TraitError::Internal("IAM 服务尚未完成初始化".to_string()))
    }

    async fn update_user(
        &self,
        _svr_ctx: &cmx_core::SVRContext,
        _user_id: &str,
        _data: cmx_iam::user::UserForUpdate,
    ) -> std::result::Result<cmx_core::model::iam::User, cmx_traits::error::TraitError> {
        Err(cmx_traits::error::TraitError::Internal("IAM 服务尚未完成初始化".to_string()))
    }

    async fn delete_user(
        &self,
        _svr_ctx: &cmx_core::SVRContext,
        _user_ids: &[String],
    ) -> std::result::Result<(), cmx_traits::error::TraitError> {
        Err(cmx_traits::error::TraitError::Internal("IAM 服务尚未完成初始化".to_string()))
    }

    async fn page_users(
        &self,
        _filters: Option<Vec<cmx_iam::user::UserFilter>>,
        _list_options: modql::filter::ListOptions,
    ) -> std::result::Result<(Vec<cmx_core::model::iam::User>, i64), cmx_traits::error::TraitError> {
        Err(cmx_traits::error::TraitError::Internal("IAM 服务尚未完成初始化".to_string()))
    }

    async fn list_users(
        &self,
        _filters: Option<Vec<cmx_iam::user::UserFilter>>,
        _list_options: Option<modql::filter::ListOptions>,
    ) -> std::result::Result<Vec<cmx_core::model::iam::User>, cmx_traits::error::TraitError> {
        Err(cmx_traits::error::TraitError::Internal("IAM 服务尚未完成初始化".to_string()))
    }

    async fn assign_roles(
        &self,
        _svr_ctx: &cmx_core::SVRContext,
        _username: &str,
        _role_ids: &[String],
    ) -> std::result::Result<(), cmx_traits::error::TraitError> {
        Err(cmx_traits::error::TraitError::Internal("IAM 服务尚未完成初始化".to_string()))
    }

    async fn get_user_roles(
        &self,
        _username: &str,
    ) -> std::result::Result<Vec<cmx_core::model::iam::Role>, cmx_traits::error::TraitError> {
        Err(cmx_traits::error::TraitError::Internal("IAM 服务尚未完成初始化".to_string()))
    }

    async fn assign_temp_role(
        &self,
        _svr_ctx: &cmx_core::SVRContext,
        _user_id: &str,
        _role_id: &str,
        _effective_from: chrono::DateTime<chrono::Utc>,
        _effective_until: chrono::DateTime<chrono::Utc>,
        _reason: Option<&str>,
        _source: &str,
    ) -> std::result::Result<cmx_iam::service_traits::UserRoleAssignment, cmx_traits::error::TraitError> {
        Err(cmx_traits::error::TraitError::Internal("IAM 服务尚未完成初始化".to_string()))
    }

    async fn revoke_temp_role(
        &self,
        _svr_ctx: &cmx_core::SVRContext,
        _assignment_id: &str,
        _reason: Option<&str>,
    ) -> std::result::Result<(), cmx_traits::error::TraitError> {
        Err(cmx_traits::error::TraitError::Internal("IAM 服务尚未完成初始化".to_string()))
    }

    async fn revoke_temp_roles_batch(
        &self,
        _svr_ctx: &cmx_core::SVRContext,
        _assignment_ids: &[String],
        _reason: Option<&str>,
    ) -> std::result::Result<u64, cmx_traits::error::TraitError> {
        Err(cmx_traits::error::TraitError::Internal("IAM 服务尚未完成初始化".to_string()))
    }

    async fn extend_temp_role(
        &self,
        _svr_ctx: &cmx_core::SVRContext,
        _assignment_id: &str,
        _new_effective_until: chrono::DateTime<chrono::Utc>,
        _reason: Option<&str>,
    ) -> std::result::Result<(), cmx_traits::error::TraitError> {
        Err(cmx_traits::error::TraitError::Internal("IAM 服务尚未完成初始化".to_string()))
    }

    async fn get_user_temp_assignments(
        &self,
        _user_id: &str,
        _status_filter: cmx_iam::service_traits::TempAssignmentStatusFilter,
    ) -> std::result::Result<Vec<cmx_iam::service_traits::UserRoleAssignment>, cmx_traits::error::TraitError> {
        Err(cmx_traits::error::TraitError::Internal("IAM 服务尚未完成初始化".to_string()))
    }

    async fn get_role_temp_assigned_users(
        &self,
        _role_id: &str,
        _status_filter: cmx_iam::service_traits::TempAssignmentStatusFilter,
    ) -> std::result::Result<Vec<cmx_iam::service_traits::UserRoleAssignment>, cmx_traits::error::TraitError> {
        Err(cmx_traits::error::TraitError::Internal("IAM 服务尚未完成初始化".to_string()))
    }

    async fn get_effective_permissions(
        &self,
        _user_id: &str,
    ) -> std::result::Result<cmx_iam::service_traits::EffectivePermissionsResponse, cmx_traits::error::TraitError> {
        Err(cmx_traits::error::TraitError::Internal("IAM 服务尚未完成初始化".to_string()))
    }
}

/// 从配置文件加载 IamConfig
pub fn load_iam_config() -> IamConfig {
    let config = cmx_utils::ConfigManager::global();
    let mut iam_config = IamConfig::default();

    if let Ok(db_id) = config.get_string("iam.auth_db_id") {
        iam_config.auth_db_id = Some(db_id);
    }
    if let Ok(min_len) = config.get_int("iam.password_min_length") {
        iam_config.password_min_length = min_len as usize;
    }
    if let Ok(codes) = config.get_string("iam.builtin_role_codes") {
        iam_config.builtin_role_codes = codes.split(',').map(|s| s.trim().to_string()).collect();
    }
    if let Ok(ttl) = config.get_int("iam.permission_cache_ttl_secs") {
        iam_config.permission_cache_ttl_secs = ttl as u64;
    }
    // SoD 互斥校验开关：默认开启（见 IamConfig::default），此处允许配置覆盖关闭
    if let Ok(enabled) = config.get_bool("iam.enable_sod_check") {
        iam_config.enable_sod_check = enabled;
    }

    iam_config
}

/// 从配置文件加载路由→权限码映射
///
/// 格式：`[iam_permissions]` TOML section
/// ```toml
/// [iam_permissions]
/// "/api/iam/users" = "user:read"
/// "/api/iam/roles" = "role:read"
/// "/api/iam/permissions" = "permission:read"
/// ```
fn load_permission_map() -> HashMap<String, String> {
    let config = cmx_utils::ConfigManager::global();

    config
        .inner()
        .get_table("iam_permissions")
        .map(|table| {
            table
                .into_iter()
                .filter_map(|(k, v)| {
                    v.clone().into_string().map(|s| (k, s)).ok()
                })
                .collect()
        })
        .unwrap_or_default()
}
