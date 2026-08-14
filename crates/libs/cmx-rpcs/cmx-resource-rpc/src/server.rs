//! 资源数据管理 gRPC 服务端实现。
//!
//! 实现 [`CmxResourceDataService`] trait，桥接 gRPC 请求到 [`ResourceDataImporter`]。
//! `data_importer` 为可选依赖，未配置时 import/cleanup 返回失败响应。

use std::sync::Arc;

use cmx_rpc_gen::cmx::cmx_resource_data_service::cmx_resource_data_service::cmx as resource_data_proto;
use cmx_traits::resource::{
    ResourceDataCategory, ResourceDataCleanupRequest, ResourceDataImportRequest,
    ResourceDataImporter,
};
use tracing::instrument;

use cmx_rpc::{AuthVerifier, VerifiedAuth, verify_request};

/// [`resource_data_proto::CmxResourceDataService`] 的 gRPC 服务端实现。
#[derive(Clone)]
pub struct CmxResourceDataServerImpl {
    /// 资源数据导入器（可选，未配置时 import/cleanup 返回错误）
    data_importer: Option<Arc<dyn ResourceDataImporter>>,
    /// 鉴权器（`None` 表示不启用 gRPC 鉴权）。
    auth_verifier: Option<AuthVerifier>,
}

impl CmxResourceDataServerImpl {
    /// 创建新的资源数据管理 gRPC 服务端。
    pub fn new(data_importer: Option<Arc<dyn ResourceDataImporter>>) -> Self {
        Self {
            data_importer,
            auth_verifier: None,
        }
    }

    /// 设置鉴权器（由 Bundle 在 `build_server` 时按需注入）。
    pub fn with_auth_verifier(mut self, verifier: AuthVerifier) -> Self {
        self.auth_verifier = Some(verifier);
        self
    }

    /// 统一鉴权入口。未配置 verifier 时直接返回 None（兼容无鉴权场景）。
    async fn auth(
        &self,
        meta: &volo_grpc::metadata::MetadataMap,
    ) -> Result<Option<VerifiedAuth>, volo_grpc::Status> {
        match &self.auth_verifier {
            Some(v) => verify_request(meta, v).await.map(Some),
            None => Ok(None),
        }
    }
}

impl resource_data_proto::CmxResourceDataService for CmxResourceDataServerImpl {
    #[instrument(
        target = "cmx_rpc",
        skip(self, req),
        name = "grpc_import_resource_data"
    )]
    fn import_resource_data(
        &self,
        req: volo_grpc::Request<resource_data_proto::ImportResourceDataRequest>,
    ) -> impl std::future::Future<
        Output = Result<
            volo_grpc::Response<resource_data_proto::ImportResourceDataResponse>,
            volo_grpc::Status,
        >,
    > + Send {
        let data_importer = self.data_importer.clone();
        let auth_self = self.clone();
        async move {
            // 鉴权（在 into_inner 前从 metadata 读取）
            let verified = auth_self.auth(req.metadata()).await?;
            let req = req.into_inner();

            let (auth_ctx, user_token, request_id) = match verified {
                Some(v) => (Some(v.context), v.original_user_token, v.request_id),
                None => (None, None, None),
            };
            // 建立 task_local scope，使 importer 内部可通过 current_auth() 获取调用者身份；
            // 跨服务出站调用据此透传委托用户。
            cmx_traits::auth::context_scope::scope_full(
                auth_ctx,
                user_token,
                request_id.unwrap_or_default(),
                None,
                async {
                    let Some(importer) = data_importer else {
                        let response = resource_data_proto::ImportResourceDataResponse {
                            success: false,
                            message: "data_importer 未配置".into(),
                            created_count: 0,
                            updated_count: 0,
                            deleted_count: 0,
                        };
                        return Ok(volo_grpc::Response::new(response));
                    };

                    let Some(category) = ResourceDataCategory::parse_from_str(&req.category) else {
                        let response = resource_data_proto::ImportResourceDataResponse {
                            success: false,
                            message: format!(
                                "无效的数据类别: {}（有效值: menu/perm/form/flow）",
                                req.category
                            )
                            .into(),
                            created_count: 0,
                            updated_count: 0,
                            deleted_count: 0,
                        };
                        return Ok(volo_grpc::Response::new(response));
                    };

                    // 校验必填字段(domain_code/application_code/module_code 所有类别都需要)
                    if req.domain_code.is_empty()
                        || req.application_code.is_empty()
                        || req.module_code.is_empty()
                    {
                        let response = resource_data_proto::ImportResourceDataResponse {
                            success: false,
                            message: "domain_code/application_code/module_code 不能为空".into(),
                            created_count: 0,
                            updated_count: 0,
                            deleted_count: 0,
                        };
                        return Ok(volo_grpc::Response::new(response));
                    }
                    // plugin_id/app_id/version 仅 Perm(插件权限导入)场景需要;
                    // Form/Menu/Table(模块资源导入)无插件上下文,允许为空。
                    if matches!(category, ResourceDataCategory::Perm)
                        && (req.plugin_id.is_empty()
                            || req.app_id.is_empty()
                            || req.version.is_empty())
                    {
                        let response = resource_data_proto::ImportResourceDataResponse {
                            success: false,
                            message: "Perm 类别导入需要 plugin_id/app_id/version 非空".into(),
                            created_count: 0,
                            updated_count: 0,
                            deleted_count: 0,
                        };
                        return Ok(volo_grpc::Response::new(response));
                    }

                    let request = ResourceDataImportRequest {
                        category,
                        domain_code: req.domain_code.to_string(),
                        application_code: req.application_code.to_string(),
                        module_code: req.module_code.to_string(),
                        plugin_id: req.plugin_id.to_string(),
                        app_id: req.app_id.to_string(),
                        version: req.version.to_string(),
                        zip_data: req.zip_data.to_vec(),
                    };

                    match importer.import_data(request).await {
                        Ok(result) => {
                            let response = resource_data_proto::ImportResourceDataResponse {
                                success: result.success,
                                message: result.message.into(),
                                created_count: result.created_count,
                                updated_count: result.updated_count,
                                deleted_count: result.deleted_count,
                            };
                            Ok(volo_grpc::Response::new(response))
                        }
                        Err(e) => {
                            tracing::error!(
                                target = "cmx_rpc",
                                error = %e,
                                category = %req.category,
                                domain = %req.domain_code,
                                app = %req.application_code,
                                module = %req.module_code,
                                plugin_id = %req.plugin_id,
                                "资源数据导入失败"
                            );
                            let response = resource_data_proto::ImportResourceDataResponse {
                                success: false,
                                message: e.to_string().into(),
                                created_count: 0,
                                updated_count: 0,
                                deleted_count: 0,
                            };
                            Ok(volo_grpc::Response::new(response))
                        }
                    }
                },
            )
            .await
        }
    }

    #[instrument(
        target = "cmx_rpc",
        skip(self, req),
        name = "grpc_cleanup_resource_data"
    )]
    fn cleanup_resource_data(
        &self,
        req: volo_grpc::Request<resource_data_proto::CleanupResourceDataRequest>,
    ) -> impl std::future::Future<
        Output = Result<
            volo_grpc::Response<resource_data_proto::ImportResourceDataResponse>,
            volo_grpc::Status,
        >,
    > + Send {
        let data_importer = self.data_importer.clone();
        let auth_self = self.clone();
        async move {
            // 鉴权（在 into_inner 前从 metadata 读取）
            let verified = auth_self.auth(req.metadata()).await?;
            let req = req.into_inner();

            let (auth_ctx, user_token, request_id) = match verified {
                Some(v) => (Some(v.context), v.original_user_token, v.request_id),
                None => (None, None, None),
            };
            cmx_traits::auth::context_scope::scope_full(
                auth_ctx,
                user_token,
                request_id.unwrap_or_default(),
                None,
                async {
                    let Some(importer) = data_importer else {
                        let response = resource_data_proto::ImportResourceDataResponse {
                            success: false,
                            message: "data_importer 未配置".into(),
                            created_count: 0,
                            updated_count: 0,
                            deleted_count: 0,
                        };
                        return Ok(volo_grpc::Response::new(response));
                    };

                    let Some(category) = ResourceDataCategory::parse_from_str(&req.category) else {
                        let response = resource_data_proto::ImportResourceDataResponse {
                            success: false,
                            message: format!("无效的数据类别: {}", req.category).into(),
                            created_count: 0,
                            updated_count: 0,
                            deleted_count: 0,
                        };
                        return Ok(volo_grpc::Response::new(response));
                    };

                    let request = ResourceDataCleanupRequest {
                        category,
                        domain_code: req.domain_code.to_string(),
                        application_code: req.application_code.to_string(),
                        module_code: req.module_code.to_string(),
                        plugin_id: req.plugin_id.to_string(),
                        app_id: req.app_id.to_string(),
                    };

                    match importer.cleanup_data(request).await {
                        Ok(result) => {
                            let response = resource_data_proto::ImportResourceDataResponse {
                                success: result.success,
                                message: result.message.into(),
                                created_count: result.created_count,
                                updated_count: result.updated_count,
                                deleted_count: result.deleted_count,
                            };
                            Ok(volo_grpc::Response::new(response))
                        }
                        Err(e) => {
                            tracing::error!(
                                target = "cmx_rpc",
                                error = %e,
                                category = %req.category,
                                domain = %req.domain_code,
                                app = %req.application_code,
                                module = %req.module_code,
                                plugin_id = %req.plugin_id,
                                "资源数据清理失败"
                            );
                            let response = resource_data_proto::ImportResourceDataResponse {
                                success: false,
                                message: e.to_string().into(),
                                created_count: 0,
                                updated_count: 0,
                                deleted_count: 0,
                            };
                            Ok(volo_grpc::Response::new(response))
                        }
                    }
                },
            )
            .await
        }
    }

    #[instrument(target = "cmx_rpc", skip(self, req), name = "grpc_list_resource_data")]
    fn list_resource_data(
        &self,
        req: volo_grpc::Request<resource_data_proto::ListResourceDataRequest>,
    ) -> impl std::future::Future<
        Output = Result<
            volo_grpc::Response<resource_data_proto::ListResourceDataResponse>,
            volo_grpc::Status,
        >,
    > + Send {
        let data_importer = self.data_importer.clone();
        let auth_self = self.clone();
        async move {
            // 鉴权（在 into_inner 前从 metadata 读取）
            let verified = auth_self.auth(req.metadata()).await?;
            let req = req.into_inner();

            let (auth_ctx, user_token, request_id) = match verified {
                Some(v) => (Some(v.context), v.original_user_token, v.request_id),
                None => (None, None, None),
            };
            cmx_traits::auth::context_scope::scope_full(
                auth_ctx,
                user_token,
                request_id.unwrap_or_default(),
                None,
                async {
                    let Some(importer) = data_importer else {
                        let response = resource_data_proto::ListResourceDataResponse {
                            success: false,
                            message: "data_importer 未配置".into(),
                            json_data: Vec::new().into(),
                        };
                        return Ok(volo_grpc::Response::new(response));
                    };

                    let Some(category) = ResourceDataCategory::parse_from_str(&req.category) else {
                        let response = resource_data_proto::ListResourceDataResponse {
                            success: false,
                            message: format!("无效的数据类别: {}", req.category).into(),
                            json_data: Vec::new().into(),
                        };
                        return Ok(volo_grpc::Response::new(response));
                    };

                    if req.module_code.is_empty() {
                        let response = resource_data_proto::ListResourceDataResponse {
                            success: false,
                            message: "module_code 不能为空".into(),
                            json_data: Vec::new().into(),
                        };
                        return Ok(volo_grpc::Response::new(response));
                    }

                    let request = ResourceDataImportRequest {
                        category,
                        domain_code: req.domain_code.to_string(),
                        application_code: req.application_code.to_string(),
                        module_code: req.module_code.to_string(),
                        plugin_id: String::new(),
                        app_id: String::new(),
                        version: String::new(),
                        zip_data: Vec::new(),
                    };

                    match importer.list_data(request).await {
                        Ok(result) => {
                            let response = resource_data_proto::ListResourceDataResponse {
                                success: result.success,
                                message: result.message.into(),
                                json_data: result.json_data.into(),
                            };
                            Ok(volo_grpc::Response::new(response))
                        }
                        Err(e) => {
                            tracing::error!(
                                target = "cmx_rpc",
                                error = %e,
                                category = %req.category,
                                module = %req.module_code,
                                "资源数据查询失败"
                            );
                            let response = resource_data_proto::ListResourceDataResponse {
                                success: false,
                                message: e.to_string().into(),
                                json_data: Vec::new().into(),
                            };
                            Ok(volo_grpc::Response::new(response))
                        }
                    }
                },
            )
            .await
        }
    }
}
