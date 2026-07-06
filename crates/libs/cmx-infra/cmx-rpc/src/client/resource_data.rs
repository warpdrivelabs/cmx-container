//! 资源数据管理 gRPC 客户端 + Bundle + 领域全局访问器。
//!
//! 基于 volo-grpc 的 [`ResourceDataClient`] trait 实现，通过注册中心缓存发现服务实例。
//!
//! # 领域全局
//!
//! 访问：[`resource_data_client()`] → `&'static Arc<dyn ResourceDataClient>`
//!
//! # 重试策略
//!
//! `import_resource_data` / `cleanup_resource_data` **不走 [`super::retry::with_retry`]**：
//! 传输 ZIP 二进制大包（默认上限 4MB），重试需保证下游导入幂等。当前服务端按 upsert
//! 语义实现，理论上幂等，但：(1) 大包重试放大带宽与下游负载；(2) 4MB 上限下网络抖动
//! 概率高，盲目重试易雪崩；(3) import 由插件安装流程驱动，失败可由上层重试整个安装任务。
//! 故本期不启用 RPC 层重试，失败立即返回。路线：未来若引入幂等 token + 分片上传，可启用有限重试。

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use async_trait::async_trait;
use cmx_rpc_gen::cmx::cmx_resource_data_service::cmx_resource_data_service::cmx as resource_data_proto;
use cmx_traits::resource::{
    ResourceDataCleanupRequest, ResourceDataImportRequest, ResourceDataImportResult,
    ResourceDataListResult,
};
use cmx_traits::rpc::{ResourceDataClient, RpcError};
use tokio::sync::RwLock;
use tracing::instrument;

use super::infra::GrpcInfrastructure;
use crate::bundle::{RpcServiceBundle, ServerDeps, ServerRegistration};

// ==================== 领域全局访问器 ====================

static RESOURCE_DATA_CLIENT: OnceLock<Arc<dyn ResourceDataClient>> = OnceLock::new();

pub(crate) fn set_client(c: Arc<dyn ResourceDataClient>) -> Result<(), ()> {
    RESOURCE_DATA_CLIENT.set(c).map_err(|_| ())
}

/// 获取资源数据管理 RPC 客户端（须先通过 [`crate::factory::init_rpc_clients`] 初始化）。
///
/// # Panics
///
/// 未初始化时 panic。先用 [`crate::global::GlobalRpcClient::is_initialized`] 守卫。
pub fn resource_data_client() -> &'static Arc<dyn ResourceDataClient> {
    RESOURCE_DATA_CLIENT
        .get()
        .expect("resource_data client not initialized")
}

// ==================== 客户端实现 ====================

/// 资源数据管理 gRPC 客户端。
pub struct ResourceDataGrpcClient {
    /// 共享基础设施（服务发现、Discover 缓存、超时/重试配置）
    infra: Arc<GrpcInfrastructure>,
    /// gRPC 客户端缓存（service_name → client）
    clients: RwLock<HashMap<String, resource_data_proto::CmxResourceDataServiceClient>>,
}

impl ResourceDataGrpcClient {
    /// 创建新的资源数据管理 gRPC 客户端。
    pub fn new(infra: Arc<GrpcInfrastructure>) -> Self {
        Self {
            infra,
            clients: RwLock::new(HashMap::new()),
        }
    }

    /// 获取或创建指定服务的 gRPC 客户端（double-check locking 防止并发重复创建）。
    #[instrument(target = "cmx_rpc", skip(self), fields(service_name = %service_name))]
    async fn get_client(
        &self,
        service_name: &str,
    ) -> Result<resource_data_proto::CmxResourceDataServiceClient, RpcError> {
        // 快查：读锁检查缓存
        if let Some(c) = self.clients.read().await.get(service_name) {
            return Ok(c.clone());
        }

        // 慢路径：获取共享 discover + 构建 client
        let discover = self.infra.get_or_create_discover(service_name).await?;
        let client = resource_data_proto::CmxResourceDataServiceClientBuilder::new(service_name)
            .discover(discover)
            .rpc_timeout(Some(self.infra.rpc_timeout()))
            .connect_timeout(self.infra.connect_timeout())
            .build();

        // 写锁：double-check 防止并发重复创建
        let mut clients = self.clients.write().await;
        if let Some(c) = clients.get(service_name) {
            return Ok(c.clone());
        }
        clients.insert(service_name.to_string(), client.clone());

        Ok(client)
    }
}

#[async_trait]
impl ResourceDataClient for ResourceDataGrpcClient {
    #[instrument(target = "cmx_rpc", skip(self, request), fields(service_name = %service_name, category = ?request.category))]
    async fn import_resource_data(
        &self,
        service_name: &str,
        request: ResourceDataImportRequest,
    ) -> Result<ResourceDataImportResult, RpcError> {
        let client = self.get_client(service_name).await?;

        let category_str = request.category.as_str();
        let proto_req = resource_data_proto::ImportResourceDataRequest {
            category: category_str.into(),
            domain_code: request.domain_code.clone().into(),
            application_code: request.application_code.clone().into(),
            module_code: request.module_code.clone().into(),
            plugin_id: request.plugin_id.clone().into(),
            app_id: request.app_id.clone().into(),
            version: request.version.clone().into(),
            zip_data: request.zip_data.clone().into(),
        };

        match client.import_resource_data(proto_req).await {
            Ok(resp) => {
                let resp = resp.into_inner();
                tracing::info!(
                    target: "cmx_rpc",
                    service_name = %service_name,
                    category = category_str,
                    success = resp.success,
                    created = resp.created_count,
                    updated = resp.updated_count,
                    deleted = resp.deleted_count,
                    "RPC import_resource_data 完成"
                );
                Ok(ResourceDataImportResult {
                    success: resp.success,
                    message: resp.message.to_string(),
                    created_count: resp.created_count,
                    updated_count: resp.updated_count,
                    deleted_count: resp.deleted_count,
                })
            }
            Err(e) => {
                tracing::warn!(
                    target: "cmx_rpc",
                    service_name = %service_name,
                    category = category_str,
                    success = false,
                    error = %e,
                    "RPC import_resource_data 失败"
                );
                Err(RpcError::RpcCallFailed(e.to_string()))
            }
        }
    }

    #[instrument(target = "cmx_rpc", skip(self, request), fields(service_name = %service_name, category = ?request.category))]
    async fn cleanup_resource_data(
        &self,
        service_name: &str,
        request: ResourceDataCleanupRequest,
    ) -> Result<ResourceDataImportResult, RpcError> {
        let client = self.get_client(service_name).await?;

        let category_str = request.category.as_str();
        let proto_req = resource_data_proto::CleanupResourceDataRequest {
            category: category_str.into(),
            domain_code: request.domain_code.clone().into(),
            application_code: request.application_code.clone().into(),
            module_code: request.module_code.clone().into(),
            plugin_id: request.plugin_id.clone().into(),
            app_id: request.app_id.clone().into(),
        };

        match client.cleanup_resource_data(proto_req).await {
            Ok(resp) => {
                let resp = resp.into_inner();
                tracing::info!(
                    target: "cmx_rpc",
                    service_name = %service_name,
                    category = category_str,
                    success = resp.success,
                    deleted = resp.deleted_count,
                    "RPC cleanup_resource_data 完成"
                );
                Ok(ResourceDataImportResult {
                    success: resp.success,
                    message: resp.message.to_string(),
                    created_count: resp.created_count,
                    updated_count: resp.updated_count,
                    deleted_count: resp.deleted_count,
                })
            }
            Err(e) => {
                tracing::warn!(
                    target: "cmx_rpc",
                    service_name = %service_name,
                    category = category_str,
                    success = false,
                    error = %e,
                    "RPC cleanup_resource_data 失败"
                );
                Err(RpcError::RpcCallFailed(e.to_string()))
            }
        }
    }

    #[instrument(target = "cmx_rpc", skip(self, request), fields(service_name = %service_name, category = ?request.category))]
    async fn list_resource_data(
        &self,
        service_name: &str,
        request: ResourceDataImportRequest,
    ) -> Result<ResourceDataListResult, RpcError> {
        let client = self.get_client(service_name).await?;

        let category_str = request.category.as_str();
        let proto_req = resource_data_proto::ListResourceDataRequest {
            category: category_str.into(),
            domain_code: request.domain_code.clone().into(),
            application_code: request.application_code.clone().into(),
            module_code: request.module_code.clone().into(),
        };

        match client.list_resource_data(proto_req).await {
            Ok(resp) => {
                let resp = resp.into_inner();
                tracing::info!(
                    target: "cmx_rpc",
                    service_name = %service_name,
                    category = category_str,
                    success = resp.success,
                    json_len = resp.json_data.len(),
                    "RPC list_resource_data 完成"
                );
                Ok(ResourceDataListResult {
                    success: resp.success,
                    message: resp.message.to_string(),
                    json_data: resp.json_data.to_vec(),
                })
            }
            Err(e) => {
                tracing::warn!(
                    target: "cmx_rpc",
                    service_name = %service_name,
                    category = category_str,
                    error = %e,
                    "RPC list_resource_data 失败"
                );
                Err(RpcError::RpcCallFailed(e.to_string()))
            }
        }
    }
}

// ==================== Bundle ====================

/// 资源数据管理领域 Bundle。
pub struct ResourceDataBundle;

impl RpcServiceBundle for ResourceDataBundle {
    fn name(&self) -> &'static str {
        "resource_data"
    }

    fn init_client(&self, infra: Arc<GrpcInfrastructure>) {
        set_client(Arc::new(ResourceDataGrpcClient::new(infra)))
            .expect("resource_data client already initialized");
    }

    fn build_server(&self, deps: &ServerDeps) -> ServerRegistration {
        let data_importer = deps.data_importer.clone();
        ServerRegistration::new(move |server| {
            let impl_ = crate::server::resource_data::CmxResourceDataServerImpl::new(data_importer);
            let svc = volo_grpc::server::ServiceBuilder::new(
                resource_data_proto::CmxResourceDataServiceServer::new(impl_),
            )
            .build::<
                resource_data_proto::CmxResourceDataServiceRequestRecv,
                resource_data_proto::CmxResourceDataServiceResponseSend,
            >();
            server.add_service(svc)
        })
    }
}
