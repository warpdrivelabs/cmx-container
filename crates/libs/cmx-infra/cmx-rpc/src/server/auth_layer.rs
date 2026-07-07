//! gRPC 服务端鉴权。
//!
//! 提供 [`AuthVerifier`] 与 [`verify_request`] 辅助函数，供各领域 server impl
//! 在方法入口统一鉴权。验证逻辑复用 [`cmx_traits::auth::AuthService`]，支持
//! 服务级 API Key 与用户 JWT 双通道。
//!
//! # 验证策略
//!
//! 1. **服务身份（主认证，必备）**：从 `X-API-Key: <key>` 提取凭证，走 API Key 通道
//!    验证。服务级 key 不应放入 `Authorization: Bearer`（Bearer 专用于用户 JWT）。
//!    缺失或验证失败返回 `Status::unauthenticated`。
//! 2. **委托用户（on-behalf-of，可选增强）**：若携带
//!    `X-Delegated-User-Token: Bearer <jwt>`，验证之。**失败时仅 warn 并回落服务身份**
//!    （委托是增强而非必需，保证 M2M 调用链不因委托 token 失效而中断）。
//! 3. **合并规则**：优先委托用户上下文，回落服务身份。
//!
//! # 用法
//!
//! server 方法入口：
//!
//! ```ignore
//! async fn import_resource_data(
//!     &self,
//!     req: volo_grpc::Request<ImportResourceDataRequest>,
//! ) -> Result<volo_grpc::Response<...>, volo_grpc::Status> {
//!     let auth_ctx = verify_request(req.metadata(), &self.verifier).await?;
//!     // ... 业务逻辑，可进入 task_local scope
//! }
//! ```

use std::sync::Arc;

use cmx_core::AuthContext;
use cmx_traits::auth::AuthService;
use tracing::warn;

/// 鉴权所需依赖。
///
/// 注入到 [`crate::bundle::ServerDeps`]，由各领域 server impl 在构造时按需持有。
#[derive(Clone)]
pub struct AuthVerifier {
    /// 认证服务（验证 API Key / JWT）。
    pub auth_service: Arc<dyn AuthService>,
}

impl AuthVerifier {
    /// 创建鉴权器。
    pub fn new(auth_service: Arc<dyn AuthService>) -> Self {
        Self { auth_service }
    }
}

/// 鉴权结果。
pub struct VerifiedAuth {
    /// 最终生效的 `AuthContext`（委托用户优先，回落服务身份）。
    pub context: AuthContext,
    /// 是否来自委托用户（true 表示有有效委托 JWT）。
    pub is_delegated: bool,
    /// 入站携带的委托用户 JWT 原文（用于链式调用透传；`None` 表示无委托 token）。
    pub original_user_token: Option<String>,
    /// 入站请求 ID（从 `X-Request-Id` metadata 提取，用于链路追踪）。
    pub request_id: Option<String>,
}

/// 从 gRPC metadata 提取服务身份凭证（`X-API-Key`）。
///
/// 服务级 key 严格走 `X-API-Key`；`Authorization: Bearer` 专用于终端用户 JWT，
/// 不在此处提取（终端用户身份经 `X-Delegated-User-Token` 或本地 mw_auth 处理）。
fn extract_credential(meta: &volo_grpc::metadata::MetadataMap) -> Option<String> {
    let v = meta.get("x-api-key")?;
    let s = v.to_str().ok()?;
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

/// 验证服务身份凭证（API Key），返回 `AuthContext`。
async fn verify_credential(
    verifier: &AuthVerifier,
    api_key: &str,
) -> Result<AuthContext, cmx_traits::auth::AuthError> {
    verifier.auth_service.validate_api_key(api_key).await
}

/// 提取并验证委托用户 JWT（on-behalf-of）。
///
/// 成功时返回（用户 AuthContext, JWT 原文）；失败返回 `None` 并 warn（不阻断 M2M）。
/// 返回 JWT 原文是为了让 server 能把它透传到 task_local，供链式跨服务调用复用。
async fn try_extract_delegated_user(
    verifier: &AuthVerifier,
    meta: &volo_grpc::metadata::MetadataMap,
) -> Option<(AuthContext, String)> {
    let raw = meta.get("x-delegated-user-token")?;
    let s = raw.to_str().ok()?;
    let jwt = s
        .strip_prefix("Bearer ")
        .or_else(|| s.strip_prefix("bearer "))?;
    match verifier.auth_service.validate_token(jwt).await {
        Ok(ctx) => Some((ctx, jwt.to_string())),
        Err(e) => {
            warn!(
                target: "cmx_rpc",
                error = %e,
                "委托用户 JWT 验证失败，回落服务身份"
            );
            None
        }
    }
}

/// 验证 gRPC 请求的鉴权信息。
///
/// server 方法入口调用。验证失败返回 `Status::unauthenticated`；成功返回
/// [`VerifiedAuth`]（含最终生效的 `AuthContext`）。
///
/// # Errors
///
/// - 缺少服务身份凭证：`unauthenticated("缺少服务凭证")`。
/// - 服务身份凭证无效：`unauthenticated("服务凭证无效: ...")`。
pub async fn verify_request(
    meta: &volo_grpc::metadata::MetadataMap,
    verifier: &AuthVerifier,
) -> Result<VerifiedAuth, volo_grpc::Status> {
    // ① 提取并验证服务身份（主认证，必备）
    let cred = extract_credential(meta).ok_or_else(|| {
        warn!(target: "cmx_rpc", "gRPC 请求缺少服务凭证");
        volo_grpc::Status::unauthenticated("缺少服务凭证")
    })?;
    let svc_ctx = verify_credential(verifier, &cred).await.map_err(|e| {
        warn!(target: "cmx_rpc", error = %e, "gRPC 服务凭证无效");
        volo_grpc::Status::unauthenticated(format!("服务凭证无效: {e}"))
    })?;

    // ② 可选：委托用户 JWT（on-behalf-of）
    let delegated = try_extract_delegated_user(verifier, meta).await;

    // ③ 提取追踪信息（X-Request-Id）
    let request_id = meta
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    // ④ 合并：优先委托用户，回落服务身份
    if let Some((user_ctx, jwt)) = delegated {
        Ok(VerifiedAuth {
            context: AuthContext {
                // 标记来源：服务身份委托用户身份发起
                auth_method: Some("delegated_by_api_key".to_string()),
                ..user_ctx
            },
            is_delegated: true,
            original_user_token: Some(jwt),
            request_id,
        })
    } else {
        Ok(VerifiedAuth {
            context: svc_ctx,
            is_delegated: false,
            original_user_token: None,
            request_id,
        })
    }
}
