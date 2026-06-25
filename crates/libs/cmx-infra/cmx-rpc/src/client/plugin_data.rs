//! 插件数据管理 gRPC 客户端 + Bundle + 领域全局访问器。
//!
//! 基于 volo-grpc 的 [`PluginDataClient`] trait 实现，通过注册中心缓存发现服务实例。
//!
//! # 领域全局
//!
//! 访问：[`plugin_data_client()`] → `&'static Arc<dyn PluginDataClient>`
//!
//! # 重试策略
//!
//! `import_plugin_data` / `cleanup_plugin_data` **不走 [`super::retry::with_retry`]**：
//! 传输 ZIP 二进制大包（默认上限 4MB），重试需保证下游导入幂等。当前服务端按 upsert
//! 语义实现，理论上幂等，但：(1) 大包重试放大带宽与下游负载；(2) 4MB 上限下网络抖动
//! 概率高，盲目重试易雪崩；(3) import 由插件安装流程驱动，失败可由上层重试整个安装任务。
//! 故本期不启用 RPC 层重试，失败立即返回。路线：未来若引入幂等 token + 分片上传，可启用有限重试。

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use async_trait::async_trait;
use cmx_rpc_gen::cmx::cmx_plugin_data_service::cmx_plugin_data_service::cmx as plugin_data_proto;
use cmx_traits::plugin::{
    PluginDataCleanupRequest, PluginDataImportRequest, PluginDataImportResult,
};
use cmx_traits::rpc::{PluginDataClient, RpcError};
use tokio::sync::RwLock;
use tracing::instrument;

use super::infra::GrpcInfrastructure;
use crate::bundle::{RpcServiceBundle, ServerDeps, ServerRegistration};

// ==================== 领域全局访问器 ====================

static PLUGIN_DATA_CLIENT: OnceLock<Arc<dyn PluginDataClient>> = OnceLock::new();

pub(crate) fn set_client(c: Arc<dyn PluginDataClient>) -> Result<(), ()> {
    PLUGIN_DATA_CLIENT.set(c).map_err(|_| ())
}

/// 获取插件数据管理 RPC 客户端（须先通过 [`crate::factory::init_rpc_clients`] 初始化）。
///
/// # Panics
///
/// 未初始化时 panic。先用 [`crate::global::GlobalRpcClient::is_initialized`] 守卫。
pub fn plugin_data_client() -> &'static Arc<dyn PluginDataClient> {
    PLUGIN_DATA_CLIENT
        .get()
        .expect("plugin_data client not initialized")
}

// ==================== 客户端实现 ====================

/// 插件数据管理 gRPC 客户端。
pub struct PluginDataGrpcClient {
    /// 共享基础设施（服务发现、Discover 缓存、超时/重试配置）
    infra: Arc<GrpcInfrastructure>,
    /// gRPC 客户端缓存（service_name → client）
    clients: RwLock<HashMap<String, plugin_data_proto::CmxPluginDataServiceClient>>,
}

impl PluginDataGrpcClient {
    /// 创建新的插件数据管理 gRPC 客户端。
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
    ) -> Result<plugin_data_proto::CmxPluginDataServiceClient, RpcError> {
        // 快查：读锁检查缓存
        if let Some(c) = self.clients.read().await.get(service_name) {
            return Ok(c.clone());
        }

        // 慢路径：获取共享 discover + 构建 client
        let discover = self.infra.get_or_create_discover(service_name).await?;
        let client = plugin_data_proto::CmxPluginDataServiceClientBuilder::new(service_name)
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
impl PluginDataClient for PluginDataGrpcClient {
    #[instrument(target = "cmx_rpc", skip(self, request), fields(service_name = %service_name, category = ?request.category))]
    async fn import_plugin_data(
        &self,
        service_name: &str,
        request: PluginDataImportRequest,
    ) -> Result<PluginDataImportResult, RpcError> {
        let client = self.get_client(service_name).await?;

        let category_str = request.category.as_str();
        let proto_req = plugin_data_proto::ImportPluginDataRequest {
            category: category_str.into(),
            domain_code: request.domain_code.clone().into(),
            application_code: request.application_code.clone().into(),
            module_code: request.module_code.clone().into(),
            plugin_id: request.plugin_id.clone().into(),
            app_id: request.app_id.clone().into(),
            version: request.version.clone().into(),
            zip_data: request.zip_data.clone().into(),
        };

        match client.import_plugin_data(proto_req).await {
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
                    "RPC import_plugin_data 完成"
                );
                Ok(PluginDataImportResult {
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
                    "RPC import_plugin_data 失败"
                );
                Err(RpcError::RpcCallFailed(e.to_string()))
            }
        }
    }

    #[instrument(target = "cmx_rpc", skip(self, request), fields(service_name = %service_name, category = ?request.category))]
    async fn cleanup_plugin_data(
        &self,
        service_name: &str,
        request: PluginDataCleanupRequest,
    ) -> Result<PluginDataImportResult, RpcError> {
        let client = self.get_client(service_name).await?;

        let category_str = request.category.as_str();
        let proto_req = plugin_data_proto::CleanupPluginDataRequest {
            category: category_str.into(),
            domain_code: request.domain_code.clone().into(),
            application_code: request.application_code.clone().into(),
            module_code: request.module_code.clone().into(),
            plugin_id: request.plugin_id.clone().into(),
            app_id: request.app_id.clone().into(),
        };

        match client.cleanup_plugin_data(proto_req).await {
            Ok(resp) => {
                let resp = resp.into_inner();
                tracing::info!(
                    target: "cmx_rpc",
                    service_name = %service_name,
                    category = category_str,
                    success = resp.success,
                    deleted = resp.deleted_count,
                    "RPC cleanup_plugin_data 完成"
                );
                Ok(PluginDataImportResult {
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
                    "RPC cleanup_plugin_data 失败"
                );
                Err(RpcError::RpcCallFailed(e.to_string()))
            }
        }
    }
}

// ==================== Bundle ====================

/// 插件数据管理领域 Bundle。
pub struct PluginDataBundle;

impl RpcServiceBundle for PluginDataBundle {
    fn name(&self) -> &'static str {
        "plugin_data"
    }

    fn init_client(&self, infra: Arc<GrpcInfrastructure>) {
        set_client(Arc::new(PluginDataGrpcClient::new(infra)))
            .expect("plugin_data client already initialized");
    }

    fn build_server(&self, deps: &ServerDeps) -> ServerRegistration {
        let data_importer = deps.data_importer.clone();
        ServerRegistration::new(move |server| {
            let impl_ =
                crate::server::plugin_data::CmxPluginDataServerImpl::new(data_importer);
            let svc = volo_grpc::server::ServiceBuilder::new(
                plugin_data_proto::CmxPluginDataServiceServer::new(impl_),
            )
            .build::<
                plugin_data_proto::CmxPluginDataServiceRequestRecv,
                plugin_data_proto::CmxPluginDataServiceResponseSend,
            >();
            server.add_service(svc)
        })
    }
}
