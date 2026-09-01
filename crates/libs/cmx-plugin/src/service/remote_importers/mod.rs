//! 远程定义导入器集合(Remote 实现)。
//!
//! 把结构化定义序列化为 ZIP 后发送到远程中心,远程接收端解压 → 解析 → 调用 Local 实现入库。
//!
//! # 传输方式(按服务键 per-key,见 `[service_rpc.services.{key}.transport`)
//!
//! - `http`(缺省,取全局 `default_transport`):经 HTTP multipart form-data 传输(POST 到各中心
//!   统一导入端点 `/api/plugin/data/import`),定位用 `url` 静态基址或 `discovery` 服务发现选例;
//! - `grpc`:经 gRPC(`ResourceDataClient` → `CmxResourceDataService`)传输,需配 `discovery`
//!   服务名(gRPC 经全局 RPC 客户端按服务名路由)。**需启用本 crate 的 `grpc` feature**
//!   (默认不启用——五引擎消费链不背 volo 依赖树)。
//!
//! 传输 / 定位 / 鉴权链(`X-API-Key` + `X-Delegated-User-Token` + `X-Request-Id`) / 超时 /
//! 重试 / 熔断全部由 `cmx-service-rpc` 基座承担;本模块只保留**接收端旧信封方言**
//! (`{code:200,message}`)的自解析(标准 ApiResp 信封的解包在基座 `call_api`)。
//!
//! 与 Local 实现的关系:调用方(ModuleInstallService)持有 trait 对象,
//! Local/Remote 切换时调用代码完全一致(透明)。
//!
//! 详见方案文档:`20260703_cmx-plugin_模块资源导入导出统一抽象方案.md`

pub mod form;
pub mod menu;
pub mod packer;
pub mod permission;
pub mod table;
pub mod types;

pub use form::RemoteFormDefinitionImporter;
pub use menu::RemoteMenuDefinitionImporter;
pub use permission::RemotePermissionDefinitionImporter;
pub use table::RemoteTableDefinitionImporter;
pub use types::DataCategory;

use std::sync::Arc;

use crate::error::{PluginError, PluginResult};
use cmx_service_rpc::{RpcRequest, ServiceRpcConfig, ServiceRpcError, ServiceRpcHandle, TransportKind};
use cmx_traits::error::TraitError;
use cmx_traits::resource::{
    ResourceDataImportRequest, ResourceDataImportResult, ResourceDataListResult,
};

/// 统一导入端点(所有类别共用,接收端按 multipart 的 `category` 字段路由)。
const HTTP_IMPORT_PATH: &str = "/api/plugin/data/import";

/// 统一导出端点(导入端点的伴生查询)。
const HTTP_LIST_PATH: &str = "/api/plugin/data/list";

/// 将 [`PluginError`] 结构化映射为 [`TraitError`],保留远程/网络类别(避免全部坍缩为 Business 字符串)。
///
/// 映射规则:`CenterData` → [`TraitError::RemoteCenter`],
/// `Network`/`Timeout` → [`TraitError::Rpc`],其余 → [`TraitError::Business`]。
pub(crate) fn plugin_err_to_trait(e: PluginError) -> TraitError {
    match e {
        PluginError::CenterData(msg) => TraitError::RemoteCenter(msg),
        PluginError::Network(msg) | PluginError::Timeout(msg) => TraitError::Rpc(msg),
        other => TraitError::Business(other.to_string()),
    }
}

impl Default for RemoteImporterContext {
    fn default() -> Self {
        Self::new()
    }
}

/// 基座错误 → 插件错误(网络类保真,业务类进 CenterData)。
fn rpc_err_to_plugin(e: ServiceRpcError) -> PluginError {
    match e {
        ServiceRpcError::Timeout { key, timeout_ms } => {
            PluginError::Timeout(format!("服务 {key} 调用超时({timeout_ms}ms)"))
        }
        ServiceRpcError::Unavailable { key, cause } => {
            PluginError::Network(format!("服务 {key} 不可达: {cause}"))
        }
        other => PluginError::CenterData(other.to_string()),
    }
}

/// 远程导入器共享上下文(服务间统一调用基座句柄)。
#[derive(Clone)]
pub struct RemoteImporterContext {
    /// 基座句柄(目录 / 传输 / 鉴权 / 熔断)。
    rpc: Arc<ServiceRpcHandle>,
}

impl RemoteImporterContext {
    /// 创建远程导入器上下文。
    ///
    /// 优先取全局基座句柄(`init_infra` 已初始化);未初始化场景(单测 / 特殊装配)回退
    /// 现场 load 配置构造——出站鉴权(`[service_auth].outgoing_api_key`)由基座统一注入,
    /// 不再需要调用方手工传凭证。
    pub fn new() -> Self {
        let rpc = cmx_service_rpc::global_arc()
            .unwrap_or_else(|| Arc::new(ServiceRpcHandle::new(ServiceRpcConfig::load())));
        Self { rpc }
    }

    /// 解析指定数据类别对应的服务发现名(gRPC 传输使用,查 `services.{key}.discovery`)。
    pub fn resolve_service_name(&self, category: DataCategory) -> PluginResult<String> {
        let key = category.as_str();
        self.rpc
            .directory()
            .grpc_service_name(key)
            .map(|(service, _group)| service)
            .ok_or_else(|| {
                PluginError::CenterData(format!(
                    "未配置 {} 的服务名([service_rpc.services.{key}].discovery)",
                    category.center_name(),
                ))
            })
    }

    /// 统一发送入口:按该键的生效 transport(per-key)分发到 gRPC 或 HTTP 传输。
    ///
    /// 各 Remote importer 把结构体打包为 ZIP 后构造 `ResourceDataImportRequest`,
    /// 调本方法发送,不感知传输细节。
    pub async fn send(
        &self,
        category: DataCategory,
        request: ResourceDataImportRequest,
    ) -> PluginResult<ResourceDataImportResult> {
        match self.rpc.directory().transport_of(category.as_str()) {
            TransportKind::Grpc => self.send_via_grpc(category, request).await,
            TransportKind::Http => self.send_via_http(category, request).await,
        }
    }

    /// 统一查询(导出)入口:按该键的生效 transport(per-key)分发到 gRPC 或 HTTP 传输。
    ///
    /// 各 Remote importer 的 `list_*` 方法调本方法,获取远程中心的 JSON 定义列表,
    /// 再反序列化为结构体返回。
    pub async fn send_list(
        &self,
        category: DataCategory,
        request: ResourceDataImportRequest,
    ) -> PluginResult<ResourceDataListResult> {
        match self.rpc.directory().transport_of(category.as_str()) {
            TransportKind::Grpc => self.list_via_grpc(category, request).await,
            TransportKind::Http => self.list_via_http(category, request).await,
        }
    }

    /// gRPC 查询:经 `cmx_resource_rpc::resource_data_client()` 调用远程 `ListResourceData`。
    #[cfg(feature = "grpc")]
    async fn list_via_grpc(
        &self,
        category: DataCategory,
        request: ResourceDataImportRequest,
    ) -> PluginResult<ResourceDataListResult> {
        if !cmx_service_rpc::grpc::GlobalRpcClient::is_initialized() {
            return Err(PluginError::CenterData(format!(
                "RPC 未初始化,无法远程导出 {} (transport=grpc 需启用 [service_rpc.server])",
                category.center_name()
            )));
        }
        let service_name = self.resolve_service_name(category)?;
        let client = cmx_resource_rpc::resource_data_client();
        client
            .list_resource_data(&service_name, request)
            .await
            .map_err(|e| {
                PluginError::CenterData(format!(
                    "gRPC远程[{}]导出失败: {e}",
                    category.center_name()
                ))
            })
    }

    /// gRPC 未编译(feature 未启用)时的占位:显式报错而非静默回退。
    #[cfg(not(feature = "grpc"))]
    async fn list_via_grpc(
        &self,
        category: DataCategory,
        _request: ResourceDataImportRequest,
    ) -> PluginResult<ResourceDataListResult> {
        Err(PluginError::CenterData(format!(
            "transport=grpc 但 cmx-plugin 未启用 grpc feature,无法远程导出 {}",
            category.center_name()
        )))
    }

    /// HTTP 查询:GET 到各中心统一导出端点,返回 JSON。
    async fn list_via_http(
        &self,
        category: DataCategory,
        request: ResourceDataImportRequest,
    ) -> PluginResult<ResourceDataListResult> {
        let key = category.as_str().to_string();
        let req = RpcRequest::get(key.clone(), HTTP_LIST_PATH)
            .query("category", request.category.as_str())
            .query("domain_code", request.domain_code.as_str())
            .query("application_code", request.application_code.as_str())
            .query("module_code", request.module_code.as_str());
        let resp = self
            .rpc
            .execute(req)
            .await
            .map_err(|e| annotate(rpc_err_to_plugin(e), category, "导出"))?;

        // 解析接收端旧信封 { code: 200, data: { json_data } }(code!=200 → 业务失败)。
        let code = resp.body.get("code").and_then(|v| v.as_i64()).unwrap_or(0);
        if code != 200 {
            let msg = resp
                .body
                .get("message")
                .or_else(|| resp.body.get("msg"))
                .and_then(|v| v.as_str())
                .unwrap_or("未知错误");
            return Err(PluginError::CenterData(format!(
                "{} 中心业务失败: {msg}",
                category.center_name()
            )));
        }
        let data = resp.body.get("data").cloned().unwrap_or_default();
        // 接收端返回 { json_data: "..." } 或直接是 JSON 数组,兼容两种格式:
        // 优先取 json_data 字段,否则把 data 本身作为 JSON。
        let json_data = if let Some(jd) = data.get("jsonData").or_else(|| data.get("json_data")) {
            match jd {
                serde_json::Value::String(s) => s.as_bytes().to_vec(),
                other => serde_json::to_vec(other).unwrap_or_default(),
            }
        } else {
            serde_json::to_vec(&data).unwrap_or_default()
        };
        Ok(ResourceDataListResult {
            success: true,
            message: format!("HTTP {} 导出完成", category.center_name()),
            json_data,
        })
    }

    /// gRPC 传输:经 `cmx_resource_rpc::resource_data_client()` 调用远程 `CmxResourceDataService`。
    #[cfg(feature = "grpc")]
    async fn send_via_grpc(
        &self,
        category: DataCategory,
        request: ResourceDataImportRequest,
    ) -> PluginResult<ResourceDataImportResult> {
        if !cmx_service_rpc::grpc::GlobalRpcClient::is_initialized() {
            return Err(PluginError::CenterData(format!(
                "RPC 未初始化,无法远程导入 {} (transport=grpc 需启用 [service_rpc.server])",
                category.center_name()
            )));
        }
        let service_name = self.resolve_service_name(category)?;
        let client = cmx_resource_rpc::resource_data_client();
        client
            .import_resource_data(&service_name, request)
            .await
            .map_err(|e| {
                PluginError::CenterData(format!(
                    "gRPC 远程 {} 导入失败: {e}",
                    category.center_name()
                ))
            })
    }

    /// gRPC 未编译(feature 未启用)时的占位:显式报错而非静默回退。
    #[cfg(not(feature = "grpc"))]
    async fn send_via_grpc(
        &self,
        category: DataCategory,
        _request: ResourceDataImportRequest,
    ) -> PluginResult<ResourceDataImportResult> {
        Err(PluginError::CenterData(format!(
            "transport=grpc 但 cmx-plugin 未启用 grpc feature,无法远程导入 {}",
            category.center_name()
        )))
    }

    /// HTTP 传输:经 multipart form-data POST 到统一导入端点(基座负责定位/鉴权/超时)。
    async fn send_via_http(
        &self,
        category: DataCategory,
        request: ResourceDataImportRequest,
    ) -> PluginResult<ResourceDataImportResult> {
        let key = category.as_str().to_string();
        let parts = vec![
            cmx_service_rpc::FormPart::file(
                "file",
                format!("{}.zip", category.dir_name()),
                "application/zip",
                request.zip_data.clone(),
            ),
            cmx_service_rpc::FormPart::text("category", request.category.as_str()),
            cmx_service_rpc::FormPart::text("domain_code", request.domain_code.clone()),
            cmx_service_rpc::FormPart::text(
                "application_code",
                request.application_code.clone(),
            ),
            cmx_service_rpc::FormPart::text("module_code", request.module_code.clone()),
            cmx_service_rpc::FormPart::text("plugin_id", request.plugin_id.clone()),
            cmx_service_rpc::FormPart::text("app_id", request.app_id.clone()),
            cmx_service_rpc::FormPart::text("version", request.version.clone()),
        ];
        let req = RpcRequest::post(key.clone(), HTTP_IMPORT_PATH).multipart(parts);
        let resp = self
            .rpc
            .execute(req)
            .await
            .map_err(|e| annotate(rpc_err_to_plugin(e), category, "导入"))?;
        parse_http_response(&resp.body, category)
    }
}

/// 错误信息补上下文(中心名 + 动作),保留 Network/Timeout 分类(供 TraitError 映射)。
fn annotate(e: PluginError, category: DataCategory, action: &str) -> PluginError {
    let head = format!("HTTP {action} {} 中心失败", category.center_name());
    match e {
        PluginError::Network(msg) => PluginError::Network(format!("{head}: {msg}")),
        PluginError::Timeout(msg) => PluginError::Timeout(format!("{head}: {msg}")),
        other => other,
    }
}

/// 解析 HTTP 接收端响应(旧信封 ApiResp<ImportResultDto> 的 JSON:`{code:200,message}`)。
fn parse_http_response(
    body: &serde_json::Value,
    category: DataCategory,
) -> PluginResult<ResourceDataImportResult> {
    let code = body.get("code").and_then(|v| v.as_i64()).unwrap_or(0);
    if code != 200 {
        let msg = body
            .get("message")
            .or_else(|| body.get("msg"))
            .and_then(|v| v.as_str())
            .unwrap_or("未知错误");
        return Err(PluginError::CenterData(format!(
            "{} 中心业务失败: {msg}",
            category.center_name()
        )));
    }
    let data = body.get("data").cloned().unwrap_or_default();
    Ok(ResourceDataImportResult {
        success: data
            .get("success")
            .and_then(|v| v.as_bool())
            .unwrap_or(true),
        message: data
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("HTTP 导入完成")
            .to_string(),
        created_count: data
            .get("createdCount")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32,
        updated_count: data
            .get("updatedCount")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32,
        deleted_count: data
            .get("deletedCount")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32,
    })
}
