//! gRPC 客户端出站鉴权 header 注入。
//!
//! 提供辅助函数 [`apply_auth_metadata`]，供各领域客户端在发起调用前把三层凭证
//! （服务身份 + 委托用户 + 追踪）统一注入到 `volo_grpc::Request` 的 metadata。
//!
//! # 三层凭证
//!
//! 1. **服务身份**：`X-API-Key: <cmx_sk_xxx>`（本服务静态 key）。
//! 2. **委托用户**（on-behalf-of）：`X-Delegated-User-Token: Bearer <jwt>`，
//!    从 task_local 读取当前请求的原始终端用户 token（若有）。
//! 3. **追踪**：`X-Request-Id`，从 task_local 读取。
//!
//! # Header 语义约定
//!
//! - `Authorization: Bearer <jwt>` 严格只承载终端用户 JWT（遵循 OAuth2 Bearer 语义）。
//! - 服务级 API Key（`cmx_sk_` 前缀）**只走 `X-API-Key`**，不放入 `Authorization`，
//!   避免语义混淆并让接收端按 header 直接区分认证通道。
//!
//! # 设计说明
//!
//! 采用「辅助函数 + 显式调用」而非 motore `Layer`，原因：客户端经
//! `CmxServiceOrchestratorClientBuilder` 构建，注入 `layer_inner` 会改变其类型参数，
//! 与领域 client 全局单例的类型擦除冲突。辅助函数方案保持 client 类型不变，
//! 各 `call_*` / `import_*` 方法在构造 `Request` 时一行调用即可。
//!
//! # 用法
//!
//! ```ignore
//! let mut req = volo_grpc::Request::new(ExecuteServiceRequest { ... });
//! apply_auth_metadata(&mut req, &self.service_key);
//! client.execute_service(req).await
//! ```

use cmx_traits::auth::context_scope;
use volo_grpc::Request;

/// 把三层鉴权 header 注入到出站 gRPC 请求的 metadata。
///
/// # Arguments
///
/// - `req`：待发送的 gRPC 请求（已构造好消息体）。
/// - `service_key`：本服务的静态 key（`cmx_sk_xxx`），作为服务身份凭证，注入 `X-API-Key`。
pub fn apply_auth_metadata<T>(req: &mut Request<T>, service_key: &str) {
    let meta = req.metadata_mut();

    // ① 服务身份：X-API-Key: <cmx_sk_xxx>（不占用 Authorization，保持 Bearer 专用于 JWT）
    if !service_key.is_empty()
        && let Ok(v) = volo_grpc::metadata::MetadataValue::from_str(service_key) {
            meta.insert("x-api-key", v);
        }

    // ② 委托用户：X-Delegated-User-Token: Bearer <jwt>（从 task_local 取）
    if let Some(user_jwt) = context_scope::current_original_token() {
        let val = format!("Bearer {user_jwt}");
        if let Ok(v) = volo_grpc::metadata::MetadataValue::from_str(&val) {
            meta.insert("x-delegated-user-token", v);
        }
    }

    // ③ 追踪：X-Request-Id（从 task_local 取）
    if let Some(request_id) = context_scope::current_request_id()
        && let Ok(v) = volo_grpc::metadata::MetadataValue::from_str(&request_id) {
            meta.insert("x-request-id", v);
        }
}

