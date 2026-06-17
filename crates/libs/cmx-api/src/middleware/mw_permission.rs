//! 权限校验中间件
//!
//! 基于路由→权限码配置映射进行访问控制。
//! 在 mw_auth 之后执行，从 CmxSvrContext 获取 AuthContext，
//! 检查用户是否拥有当前路由所需权限码。
//!
//! ## 设计原则
//!
//! - **配置驱动**：路由→权限码映射通过 `[iam_permissions]` TOML section 配置
//! - **白名单模式**：未配置映射的路由默认放行
//! - **system:all 短路**：拥有 system:all 权限的用户直接放行
//! - **403 拦截**：无权限返回 403 Forbidden

use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use tracing::{debug, warn};

use crate::middleware::CmxSvrContext;

/// 全局路由→权限码映射表
///
/// 启动时由 `GlobalPermissionConfig::initialize` 注入，
/// 内容来自 TOML `[iam_permissions]` section。
static GLOBAL_PERMISSION_MAP: OnceLock<RwLock<HashMap<String, String>>> = OnceLock::new();

/// 全局权限校验服务实例（用于动态权限查询）
static GLOBAL_PERMISSION_CHECKER: OnceLock<Arc<dyn cmx_traits::iam::PermissionChecker>> =
    OnceLock::new();

/// 全局权限配置管理器
pub struct GlobalPermissionConfig;

impl GlobalPermissionConfig {
    /// 初始化全局路由→权限码映射
    ///
    /// 从 TOML `[iam_permissions]` section 加载路由前缀→权限码映射。
    /// 必须在应用启动早期调用。
    pub fn initialize(permission_map: HashMap<String, String>) -> Result<(), String> {
        debug!(count = permission_map.len(), "权限映射初始化");
        GLOBAL_PERMISSION_MAP
            .set(RwLock::new(permission_map))
            .map_err(|_| "全局权限映射已初始化".to_string())
    }

    /// 初始化全局权限校验器
    pub fn initialize_checker(
        checker: Arc<dyn cmx_traits::iam::PermissionChecker>,
    ) -> Result<(), String> {
        GLOBAL_PERMISSION_CHECKER
            .set(checker)
            .map_err(|_| "全局权限校验器已初始化".to_string())
    }

    /// 获取全局权限校验器
    pub fn get_checker() -> Option<&'static Arc<dyn cmx_traits::iam::PermissionChecker>> {
        GLOBAL_PERMISSION_CHECKER.get()
    }

    /// 查找路径所需权限码
    ///
    /// 按最长前缀匹配：遍历所有映射规则，找到匹配当前路径的最长前缀。
    /// 未配置映射的路径返回 None（默认放行）。
    pub fn find_required_permission(path: &str) -> Option<String> {
        GLOBAL_PERMISSION_MAP
            .get()
            .and_then(|rw| rw.read().ok())
            .and_then(|map| {
                let mut best_match: Option<(&String, &String)> = None;
                for (prefix, perm) in map.iter() {
                    if path.starts_with(prefix) {
                        match &best_match {
                            Some((best_prefix, _)) if prefix.len() > best_prefix.len() => {
                                best_match = Some((prefix, perm));
                            }
                            None => {
                                best_match = Some((prefix, perm));
                            }
                            _ => {}
                        }
                    }
                }
                best_match.map(|(_, perm)| perm.clone())
            })
    }

    /// 运行时更新权限映射（支持热更新）
    pub fn update_permission_map(new_map: HashMap<String, String>) -> Result<(), String> {
        if let Some(rw) = GLOBAL_PERMISSION_MAP.get() {
            let mut guard = rw
                .write()
                .map_err(|e| format!("获取写锁失败: {e}"))?;
            *guard = new_map;
            Ok(())
        } else {
            Err("全局权限映射未初始化".to_string())
        }
    }
}

/// 权限校验中间件
///
/// 需在 `mw_auth` 之后注册，确保 CmxSvrContext 中已注入 AuthContext。
/// 执行流程：
/// 1. 查找当前路由所需权限码（最长前缀匹配）
/// 2. 未配置映射 → 放行
/// 3. 从 AuthContext.permissions 检查是否包含所需权限码
/// 4. system:all 短路放行
/// 5. 无权限 → 403
pub async fn mw_permission(req: Request<Body>, next: Next) -> Result<Response, StatusCode> {
    let path = req.uri().path().to_string();

    // 1. 查找所需权限码
    let required_permission = match GlobalPermissionConfig::find_required_permission(&path) {
        Some(perm) => perm,
        None => {
            // 未配置映射的路由默认放行
            debug!(path = %path, "无权限映射，放行");
            return Ok(next.run(req).await);
        }
    };

    // 2. 获取 AuthContext
    let auth_ctx = req
        .extensions()
        .get::<CmxSvrContext>()
        .and_then(|ctx| ctx.0.auth_context.as_ref());

    let auth_ctx = match auth_ctx {
        Some(ctx) => ctx,
        None => {
            warn!(path = %path, "AuthContext 缺失，权限校验拒绝");
            return Err(StatusCode::FORBIDDEN);
        }
    };

    // 3. system:all 短路放行
    if auth_ctx.has_permission("system:all") {
        debug!(path = %path, user_id = %auth_ctx.user_id, "system:all 短路放行");
        return Ok(next.run(req).await);
    }

    // 4. 检查具体权限码
    if auth_ctx.has_permission(&required_permission) {
        debug!(
            path = %path,
            user_id = %auth_ctx.user_id,
            permission = %required_permission,
            "权限校验通过"
        );
        return Ok(next.run(req).await);
    }

    // 5. 无权限 → 403
    warn!(
        path = %path,
        user_id = %auth_ctx.user_id,
        required = %required_permission,
        "权限不足，拒绝访问"
    );
    Err(StatusCode::FORBIDDEN)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_required_permission_exact_match() {
        let mut map = HashMap::new();
        map.insert("/api/iam/users".to_string(), "user:read".to_string());
        map.insert("/api/iam/roles".to_string(), "role:read".to_string());

        let _ = GlobalPermissionConfig::initialize(map);

        assert_eq!(
            GlobalPermissionConfig::find_required_permission("/api/iam/users"),
            Some("user:read".to_string())
        );
        assert_eq!(
            GlobalPermissionConfig::find_required_permission("/api/iam/roles"),
            Some("role:read".to_string())
        );
    }

    #[test]
    fn test_find_required_permission_prefix_match() {
        let mut map = HashMap::new();
        map.insert("/api/iam/users".to_string(), "user:read".to_string());

        let _ = GlobalPermissionConfig::initialize(map);

        // 子路径应匹配
        assert_eq!(
            GlobalPermissionConfig::find_required_permission("/api/iam/users/123"),
            Some("user:read".to_string())
        );
        // 不相关路径不匹配
        assert_eq!(
            GlobalPermissionConfig::find_required_permission("/api/iam/roles"),
            None
        );
    }

    #[test]
    fn test_find_required_permission_longest_prefix() {
        let mut map = HashMap::new();
        map.insert("/api/iam".to_string(), "iam:access".to_string());
        map.insert("/api/iam/users".to_string(), "user:read".to_string());

        let _ = GlobalPermissionConfig::initialize(map);

        // /api/iam/users 应匹配更长的前缀 user:read
        assert_eq!(
            GlobalPermissionConfig::find_required_permission("/api/iam/users"),
            Some("user:read".to_string())
        );
        // /api/iam/roles 应匹配较短的前缀 iam:access
        assert_eq!(
            GlobalPermissionConfig::find_required_permission("/api/iam/roles"),
            Some("iam:access".to_string())
        );
    }
}
