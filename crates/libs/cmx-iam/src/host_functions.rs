//! WASM 宿主函数 — IAM 用户/权限查询
//!
//! 为 WASM 插件提供用户详情、角色权限、权限校验能力的宿主函数。
//! 封装 `PermissionChecker`（含缓存+熔断）与 `UserAuthQuery`，通过 MsgPack 格式传递参数和结果。
//!
//! # 安全设计
//!
//! 所有返回给 WASM 的用户信息均经过脱敏：`UserAuthData` → [`WasmUserDetails`] 映射时
//! 显式丢弃 `password_hash`、`last_login_ip` 等敏感字段，编译期保证不跨 WASM 边界泄露。
//!
//! # 命名空间
//!
//! 注册为 `cmx:iam` 命名空间，单一入口函数 `iam_query` 接收 [`IamRequest`] enum，
//! 按变体分发，避免注册多个 Extism host function（每个 host function 都有注册开销）。

use std::sync::Arc;

use cmx_core::model::cell::DataValue;
use cmx_core::wasm_types::{IamRequest, IamResponse, WasmEffectivePermissions, WasmUserDetails};
use cmx_database::DatabaseManager;
use cmx_traits::auth::UserAuthQuery;
use cmx_traits::error::HostFuncError;
use cmx_traits::iam::PermissionChecker;
use cmx_traits::runtime::{HostFunctionDef, HostFunctionProvider};
use tracing::{debug, warn};

/// 单一入口函数名。
const FN_IAM_QUERY: &str = "iam_query";

/// IAM 宿主函数提供者。
///
/// 向 WASM 运行时注册 `cmx:iam` 命名空间的用户/权限查询宿主函数。
/// 持有权限校验器（含缓存+熔断）与用户查询 trait 对象，委托执行实际查询。
pub struct IamHostFunctions {
    /// 权限校验器（IamChecker，含 has_permission/has_role/get_user_permissions/get_user_role_codes）。
    checker: Arc<dyn PermissionChecker>,
    /// 用户认证数据查询（get_user_by_id，单用户详情）。
    user_query: Arc<dyn UserAuthQuery>,
    /// 数据库管理器（批量用户查询，WHERE id = ANY($1)）。
    db_manager: Arc<DatabaseManager>,
    /// 认证库 db_id。
    db_id: String,
}

impl IamHostFunctions {
    /// 创建 IAM 宿主函数提供者。
    ///
    /// # Arguments
    ///
    /// * `checker` - 权限校验器（通常是 `IamChecker`，已配置缓存与熔断）。
    /// * `user_query` - 用户认证数据查询（通常是 `UserAuthQueryImpl`）。
    /// * `db_manager` - 数据库管理器（用于批量用户查询）。
    /// * `db_id` - 认证库 db_id。
    pub fn new(
        checker: Arc<dyn PermissionChecker>,
        user_query: Arc<dyn UserAuthQuery>,
        db_manager: Arc<DatabaseManager>,
        db_id: String,
    ) -> Self {
        Self {
            checker,
            user_query,
            db_manager,
            db_id,
        }
    }

    /// 构建错误响应（MsgPack 编码的 `IamResponse{success:false}`）。
    fn err_response(msg: impl Into<String>) -> Vec<u8> {
        let resp = IamResponse::error(msg);
        rmp_serde::to_vec(&resp).unwrap_or_default()
    }

    /// 入口分发：解析 `IamRequest`，按变体路由到对应 do_* 方法。
    fn do_dispatch(&self, input: Vec<u8>) -> Result<Vec<u8>, HostFuncError> {
        let request: IamRequest = match rmp_serde::from_slice(&input) {
            Ok(r) => r,
            Err(e) => return Ok(Self::err_response(format!("解析请求失败: {e}"))),
        };

        // 宿主函数回调运行在 spawn_blocking 线程，用 block_on 执行异步查询。
        let rt = tokio::runtime::Handle::current();
        let result: Result<IamResponse, String> = rt.block_on(async {
            match request {
                IamRequest::GetUserDetails { user_id } => self.do_get_user_details(&user_id).await,
                IamRequest::GetUsersDetails { user_ids } => {
                    self.do_get_users_details(&user_ids).await
                }
                IamRequest::GetEffectivePermissions { user_id } => {
                    self.do_get_effective_permissions(&user_id).await
                }
                IamRequest::HasPermission { user_id, code } => {
                    self.do_has_permission(&user_id, &code).await
                }
                IamRequest::HasRole { user_id, code } => self.do_has_role(&user_id, &code).await,
            }
        });

        let resp = match result {
            Ok(r) => r,
            Err(e) => IamResponse::error(e),
        };
        Ok(rmp_serde::to_vec(&resp).unwrap_or_default())
    }

    /// 查询单个用户详情（脱敏）。
    async fn do_get_user_details(&self, user_id: &str) -> Result<IamResponse, String> {
        debug!("[cmx:iam] get_user_details: {user_id}");
        let user = self
            .user_query
            .get_user_by_id(user_id)
            .await
            .map_err(|e| format!("查询用户失败: {e}"))?;

        Ok(IamResponse {
            success: true,
            user: user.map(Self::sanitize_user),
            ..Default::default()
        })
    }

    /// 批量查询用户详情（WHERE id = ANY($1)，无 N+1，脱敏）。
    async fn do_get_users_details(&self, user_ids: &[String]) -> Result<IamResponse, String> {
        debug!("[cmx:iam] get_users_details: {} 个用户", user_ids.len());
        if user_ids.is_empty() {
            return Ok(IamResponse {
                success: true,
                users: vec![],
                ..Default::default()
            });
        }

        // 使用 ANY($1) 数组绑定，单次查询取回所有用户，避免 N+1。
        let sql = "SELECT id, username, password_hash, nickname, email, phone, avatar, \
                   org_id, gender, status, description \
                   FROM cmx_user WHERE id = ANY($1) AND archived = 0";
        let params = vec![DataValue::Array(
            user_ids.iter().map(|s| DataValue::String(s.clone())).collect(),
        )];

        let dataset = self
            .db_manager
            .query_sql_with_datavalues(&self.db_id, None, sql, params, "users_by_ids")
            .await
            .map_err(|e| format!("批量查询用户失败: {e}"))?;

        let schema = dataset.schema.as_ref();
        let users: Vec<WasmUserDetails> = dataset
            .iter()
            .map(|row| WasmUserDetails {
                user_id: row.get_by_name_as(schema, "id").unwrap_or_default(),
                username: row.get_by_name_as(schema, "username").unwrap_or_default(),
                nickname: row.get_by_name_as(schema, "nickname"),
                email: row.get_by_name_as(schema, "email"),
                phone: row.get_by_name_as(schema, "phone"),
                avatar: row.get_by_name_as(schema, "avatar"),
                org_id: row.get_by_name_as(schema, "org_id"),
                status: row.get_by_name_as::<i64>(schema, "status").unwrap_or(1),
                description: row.get_by_name_as(schema, "description"),
            })
            .collect();

        Ok(IamResponse {
            success: true,
            users,
            ..Default::default()
        })
    }

    /// 查询用户有效权限聚合（roles + permissions code 列表）。
    ///
    /// 委托到 PermissionChecker 的 get_user_role_codes / get_user_permissions（含缓存）。
    /// 注意：此处 active_temp_roles 暂不精确统计（需 UserService.get_effective_permissions），
    /// 置 0；如需临时角色统计，插件可单独调用 IAM 业务接口。
    async fn do_get_effective_permissions(&self, user_id: &str) -> Result<IamResponse, String> {
        debug!("[cmx:iam] get_effective_permissions: {user_id}");

        // 并发获取角色与权限（两者无依赖）。
        let (roles, permissions) = tokio::try_join!(
            self.checker.get_user_role_codes(user_id),
            self.checker.get_user_permissions(user_id),
        )
        .map_err(|e| format!("查询有效权限失败: {e}"))?;

        // 用户名从单用户查询补充（若失败则用 user_id 占位，不阻塞权限返回）。
        let username = self
            .user_query
            .get_user_by_id(user_id)
            .await
            .ok()
            .flatten()
            .map(|u| u.username)
            .unwrap_or_else(|| user_id.to_string());

        Ok(IamResponse {
            success: true,
            permissions: Some(WasmEffectivePermissions {
                user_id: user_id.to_string(),
                username,
                roles,
                permissions,
                active_temp_roles: 0,
            }),
            ..Default::default()
        })
    }

    /// 权限校验：用户是否拥有指定权限码（走 IamChecker 缓存+熔断）。
    async fn do_has_permission(&self, user_id: &str, code: &str) -> Result<IamResponse, String> {
        debug!("[cmx:iam] has_permission: user={user_id}, code={code}");
        let allowed = self
            .checker
            .has_permission(user_id, code)
            .await
            .map_err(|e| format!("权限校验失败: {e}"))?;

        Ok(IamResponse {
            success: true,
            allowed: Some(allowed),
            ..Default::default()
        })
    }

    /// 角色判断：用户是否拥有指定角色码（走 IamChecker 缓存+熔断）。
    async fn do_has_role(&self, user_id: &str, code: &str) -> Result<IamResponse, String> {
        debug!("[cmx:iam] has_role: user={user_id}, code={code}");
        let allowed = self
            .checker
            .has_role(user_id, code)
            .await
            .map_err(|e| format!("角色校验失败: {e}"))?;

        Ok(IamResponse {
            success: true,
            allowed: Some(allowed),
            ..Default::default()
        })
    }

    /// 脱敏映射：`UserAuthData` → `WasmUserDetails`。
    ///
    /// **显式丢弃 password_hash、last_login_at、last_login_ip、gender** 等敏感/非必要字段，
    /// 编译期保证不跨 WASM 边界泄露。
    fn sanitize_user(u: cmx_traits::auth::UserAuthData) -> WasmUserDetails {
        WasmUserDetails {
            user_id: u.user_id,
            username: u.username,
            nickname: u.nickname,
            email: u.email,
            phone: u.phone,
            avatar: u.avatar,
            org_id: u.org_id,
            status: u.status,
            description: u.description,
        }
    }
}

impl HostFunctionProvider for IamHostFunctions {
    fn namespace(&self) -> &str {
        "cmx:iam"
    }

    fn functions(&self) -> Vec<HostFunctionDef> {
        vec![HostFunctionDef::msgpack_fn(FN_IAM_QUERY, "cmx:iam")]
    }

    fn call(&self, name: &str, input: Vec<u8>) -> Result<Vec<u8>, HostFuncError> {
        match name {
            FN_IAM_QUERY => self.do_dispatch(input),
            other => {
                warn!("[cmx:iam] 未知函数: {other}");
                Err(HostFuncError::invalid_function(other))
            }
        }
    }
}
