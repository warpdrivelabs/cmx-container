//! 远程定义导入器集合(Remote 实现)。
//!
//! 把结构化定义序列化为 ZIP 后发送到远程中心,远程接收端解压 → 解析 → 调用 Local 实现入库。
//!
//! # 传输方式(按服务键 per-key,见 `center_client.services.{key}.transport`)
//!
//! - `grpc`:经 gRPC(`ResourceDataClient` → `CmxResourceDataService`)传输,需配 `discovery`
//!   服务名(gRPC 经全局 RPC 客户端按服务名路由)
//! - `http`(缺省,取全局 `default_transport`):经 HTTP multipart form-data 传输(POST 到各中心
//!   `/import` 端点),定位用 `url` 静态基址或 `discovery` 服务发现选例
//!
//! 不同数据类别(menu/perm/form)可混用传输——两路传输对 importer 透明:
//! `RemoteImporterContext::send` 内部按该键的生效 transport 分发,
//! 各 Remote importer 只调 `ctx.send(category, request)`,不感知传输细节。
//!
//! 与 Local 实现的关系:调用方(ModuleInstallService)持有 trait 对象,
//! Local/Remote 切换时调用代码完全一致(透明)。
//!
//! 详见方案文档:`20260703_cmx-plugin_模块资源导入导出统一抽象方案.md`

pub mod form;
pub mod menu;
pub mod permission;
pub mod table;

pub use form::RemoteFormDefinitionImporter;
pub use menu::RemoteMenuDefinitionImporter;
pub use permission::RemotePermissionDefinitionImporter;
pub use table::RemoteTableDefinitionImporter;

use std::time::Duration;

use crate::center_client::config::CenterClientConfig;
use crate::center_client::types::DataCategory;
use crate::error::{PluginError, PluginResult};
use cmx_traits::auth::context_scope;
use cmx_traits::error::TraitError;
use cmx_traits::resource::{
    ResourceDataImportRequest, ResourceDataImportResult, ResourceDataListResult,
};

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

/// 出站服务凭证（服务级 API Key，统一走 `X-API-Key`）。
#[derive(Clone, Debug)]
pub struct Credential {
    /// 载荷（API Key 明文，`cmx_sk_xxx`）。
    pub value: String,
}

/// 远程导入器共享上下文(服务定位配置 + 双传输通道)。
#[derive(Clone)]
pub struct RemoteImporterContext {
    /// 服务中心客户端配置(services 单表 + default_transport)
    config: CenterClientConfig,
    /// HTTP 客户端(transport=http 的键使用;无条件构造——reqwest 惰性连接,grpc-only 场景零开销)
    http_client: Option<reqwest::Client>,
    /// 本服务对外的服务级凭证（cmx_sk_xxx），出站请求统一注入。
    outgoing_credential: Option<Credential>,
}

impl RemoteImporterContext {
    /// 创建远程导入器上下文。
    ///
    /// HTTP 客户端无条件构造(per-key 传输下 http/grpc 键可混存,构造期不预判哪些键走哪路;
    /// reqwest 连接惰性建立,纯 gRPC 键不产生任何连接开销)。
    pub fn new(config: CenterClientConfig) -> Self {
        let timeout = Duration::from_millis(config.timeout_ms);
        let http_client = Some(
            reqwest::Client::builder()
                .timeout(timeout)
                .build()
                .unwrap_or_else(|e| {
                    tracing::error!(error = %e, "构建 reqwest Client 失败,降级默认客户端");
                    reqwest::Client::new()
                }),
        );
        Self {
            config,
            http_client,
            outgoing_credential: None,
        }
    }

    /// 注入本服务对外的服务级凭证（来源：`[service_auth].outgoing_api_key`）。
    ///
    /// 构造后所有出站 HTTP 请求都会自动携带三层鉴权 header（服务身份 +
    /// 委托用户 + 追踪）。委托用户与追踪信息从 task_local 读取。
    pub fn with_credential(mut self, cred: Credential) -> Self {
        self.outgoing_credential = Some(cred);
        self
    }

    /// 统一给 reqwest 请求打上三层鉴权 header。
    fn apply_auth_headers(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        let mut req = req;
        // ① 服务身份层：X-API-Key（服务 key 不占用 Authorization，保持 Bearer 专用于 JWT）
        if let Some(cred) = &self.outgoing_credential {
            req = req.header("X-API-Key", &cred.value);
        }
        // ② 委托用户层：从 task_local 取当前请求的原始终端用户 JWT
        if let Some(user_jwt) = context_scope::current_original_token() {
            req = req.header("X-Delegated-User-Token", format!("Bearer {user_jwt}"));
        }
        // ③ 追踪层：请求 ID
        if let Some(request_id) = context_scope::current_request_id() {
            req = req.header("X-Request-Id", request_id);
        }
        req
    }

    /// 解析指定数据类别对应的服务名(discovery 定位 / grpc 传输使用,查 `services.{key}.discovery`)。
    pub fn resolve_service_name(&self, category: DataCategory) -> PluginResult<String> {
        self.config
            .services
            .get(category.as_str())
            .and_then(|e| e.discovery.as_deref())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                PluginError::CenterData(format!(
                    "未配置 {} 的服务名(center_client.services.{}.discovery)",
                    category.center_name(),
                    category.as_str()
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
        match self.config.transport_of(category.as_str()) {
            "grpc" => self.send_via_grpc(category, request).await,
            _ => self.send_via_http(category, request).await,
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
        match self.config.transport_of(category.as_str()) {
            "grpc" => self.list_via_grpc(category, request).await,
            _ => self.list_via_http(category, request).await,
        }
    }

    /// gRPC 查询:经 `cmx_resource_rpc::resource_data_client()` 调用远程 `ListResourceData`。
    async fn list_via_grpc(
        &self,
        category: DataCategory,
        request: ResourceDataImportRequest,
    ) -> PluginResult<ResourceDataListResult> {
        if !cmx_rpc::global::GlobalRpcClient::is_initialized() {
            return Err(PluginError::CenterData(format!(
                "RPC 未初始化,无法远程导出 {} (transport=grpc 需启用 [rpc])",
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

    /// HTTP 查询:GET 到各中心 `/api/plugin/data/list` 端点,返回 JSON。
    async fn list_via_http(
        &self,
        category: DataCategory,
        request: ResourceDataImportRequest,
    ) -> PluginResult<ResourceDataListResult> {
        let http_client = self.http_client.as_ref().ok_or_else(|| {
            PluginError::CenterData("HTTP 客户端未初始化".to_string())
        })?;

        let url = self.resolve_http_url_for_list(category).await?;
        tracing::info!(
            category = category.as_str(),
            %url,
            "HTTP 远程导出"
        );

        // GET 查询参数
        let resp = self
            .apply_auth_headers(http_client.get(&url).query(&[
                ("category", request.category.as_str()),
                ("domain_code", request.domain_code.as_str()),
                ("application_code", request.application_code.as_str()),
                ("module_code", request.module_code.as_str()),
            ]))
            .send()
            .await
            .map_err(|e| {
                PluginError::CenterData(format!(
                    "HTTP 查询 {} 中心失败 ({}): {e}",
                    category.center_name(),
                    url
                ))
            })?;

        let status = resp.status();
        let body = resp
            .text()
            .await
            .map_err(|e| PluginError::CenterData(format!("读取 HTTP 响应体失败: {e}")))?;
        if !status.is_success() {
            return Err(PluginError::CenterData(format!(
                "{} 中心返回 HTTP {}: {}",
                category.center_name(),
                status,
                body
            )));
        }

        // 解析 ApiResp { code, data: { json_data: "..." } } 响应
        let json: serde_json::Value = serde_json::from_str(&body).map_err(|e| {
            PluginError::CenterData(format!(
                "解析 {} 中心响应 JSON 失败: {e}",
                category.center_name()
            ))
        })?;
        let code = json.get("code").and_then(|v| v.as_i64()).unwrap_or(0);
        if code != 200 {
            let msg = json
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("未知错误");
            return Err(PluginError::CenterData(format!(
                "{} 中心业务失败: {msg}",
                category.center_name()
            )));
        }
        let data = json.get("data").cloned().unwrap_or_default();
        // 接收端返回 { json_data: "base64或JSON字符串" } 或直接是 JSON 数组
        // 这里兼容两种格式:优先取 json_data 字段,否则把 data 本身作为 JSON
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

    /// 解析 HTTP list 端点 URL(import 端点的 /import 替换为 /list)。
    async fn resolve_http_url_for_list(&self, category: DataCategory) -> PluginResult<String> {
        let import_url = self.resolve_http_url(category).await?;
        // 把末尾 /import 替换为 /list;不以 /import 结尾则追加 /list
        if import_url.ends_with("/import") {
            Ok(format!("{}list", &import_url[..import_url.len() - 6]))
        } else {
            Ok(format!("{import_url}/list"))
        }
    }

    /// gRPC 传输:经 `cmx_resource_rpc::resource_data_client()` 调用远程 `CmxResourceDataService`。
    async fn send_via_grpc(
        &self,
        category: DataCategory,
        request: ResourceDataImportRequest,
    ) -> PluginResult<ResourceDataImportResult> {
        if !cmx_rpc::global::GlobalRpcClient::is_initialized() {
            return Err(PluginError::CenterData(format!(
                "RPC 未初始化,无法远程导入 {} (transport=grpc 需启用 [rpc])",
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

    /// HTTP 传输:经 multipart form-data POST 到统一导入端点 `/api/plugin/data/import`。
    async fn send_via_http(
        &self,
        category: DataCategory,
        request: ResourceDataImportRequest,
    ) -> PluginResult<ResourceDataImportResult> {
        let http_client = self.http_client.as_ref().ok_or_else(|| {
            PluginError::CenterData("HTTP 客户端未初始化".to_string())
        })?;

        let url = self.resolve_http_url(category).await?;
        tracing::info!(
            category = category.as_str(),
            %url,
            "HTTP 远程导入"
        );

        // 构造 multipart form-data(对齐接收端 import_handler 的字段契约)
        let part = reqwest::multipart::Part::bytes(request.zip_data.clone())
            .file_name(format!("{}.zip", category.dir_name()))
            .mime_str("application/zip")
            .map_err(|e| PluginError::CenterData(format!("构造 multipart part 失败: {e}")))?;
        let form = reqwest::multipart::Form::new()
            .part("file", part)
            .text("category", request.category.as_str().to_string())
            .text("domain_code", request.domain_code.clone())
            .text("application_code", request.application_code.clone())
            .text("module_code", request.module_code.clone())
            .text("plugin_id", request.plugin_id.clone())
            .text("app_id", request.app_id.clone())
            .text("version", request.version.clone());

        let resp = self
            .apply_auth_headers(http_client.post(&url).multipart(form))
            .send()
            .await
            .map_err(|e| {
                PluginError::CenterData(format!(
                    "HTTP 调用 {} 中心失败 ({}): {e}",
                    category.center_name(),
                    url
                ))
            })?;

        let status = resp.status();
        let body = resp
            .text()
            .await
            .map_err(|e| PluginError::CenterData(format!("读取 HTTP 响应体失败: {e}")))?;
        if !status.is_success() {
            return Err(PluginError::CenterData(format!(
                "{} 中心返回 HTTP {}: {}",
                category.center_name(),
                status,
                body
            )));
        }

        // 解析响应(对接收端 ApiResp<ImportResultDto> 的 JSON)
        parse_http_response(&body, category)
    }

    /// 解析 HTTP 端点 URL(定位 per-key:该键 url 静态基址或 discovery 服务发现选例)。
    ///
    /// - `services.{category}.url`:静态基址 + 统一导入端点路径(值不含路径——旧「完整端点」
    ///   写法会在 config 加载时告警)
    /// - `services.{category}.discovery`:复用 `center_client::upstream` 的共享选例核
    ///   (healthy 过滤 + 随机 + `http_port` 元数据优先)解析实例基址,再拼统一端点路径
    async fn resolve_http_url(&self, category: DataCategory) -> PluginResult<String> {
        let entry = self.config.services.get(category.as_str());
        // 静态基址优先(该键配了 url)。
        if let Some(base) = entry
            .and_then(|e| e.url.as_deref())
            .map(|s| s.trim().trim_end_matches('/'))
            .filter(|s| !s.is_empty())
        {
            return Ok(format!("{base}{}", category_to_http_import_path(category)));
        }

        // 服务发现定位:经共享选例核解析实例基址(与反代同一套负载均衡语义)。
        let service_name = self.resolve_service_name(category)?;
        let base = crate::center_client::upstream::resolve_service_base(&service_name).ok_or_else(
            || {
                PluginError::CenterData(format!(
                    "服务发现未找到 {} 的可用实例 (service={})",
                    category.center_name(),
                    service_name
                ))
            },
        )?;
        Ok(format!("{base}{}", category_to_http_import_path(category)))
    }
}

/// 解析 HTTP 接收端响应(ApiResp<ImportResultDto> JSON)。
fn parse_http_response(
    body: &str,
    category: DataCategory,
) -> PluginResult<ResourceDataImportResult> {
    let json: serde_json::Value = serde_json::from_str(body).map_err(|e| {
        PluginError::CenterData(format!(
            "解析 {} 中心响应 JSON 失败: {e} (body={body})",
            category.center_name()
        ))
    })?;
    let code = json.get("code").and_then(|v| v.as_i64()).unwrap_or(0);
    if code != 200 {
        let msg = json
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("未知错误");
        return Err(PluginError::CenterData(format!(
            "{} 中心业务失败: {msg}",
            category.center_name()
        )));
    }
    let data = json.get("data").cloned().unwrap_or_default();
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

/// 各 category 的 HTTP import 端点路径(http 传输拼接用)。
///
/// 所有类别统一走通用端点 `/api/plugin/data/import`,由接收端按 multipart 的 `category` 字段路由。
fn category_to_http_import_path(_category: DataCategory) -> &'static str {
    "/api/plugin/data/import"
}
