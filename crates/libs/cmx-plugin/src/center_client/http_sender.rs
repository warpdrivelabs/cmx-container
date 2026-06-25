//! HTTP 服务中心 Sender 实现。
//!
//! 支持两种模式：
//! - `http_url`：直接使用配置的 URL 发送
//! - `http_discovery`：通过服务发现获取实例地址后发送
//!
//! 请求格式为 multipart form-data，与接收端 `import_handler` 对齐。

use std::time::Duration;

use async_trait::async_trait;
use rand::seq::SliceRandom;
use tracing::{info, warn};

use cmx_registry_config::registry::ServiceInstance;
use cmx_registry_config::GlobalServiceInstanceCache;

use super::config::CenterClientConfig;
use super::sender::{CenterError, ServiceCenterSender};
use super::types::{CenterCleanupRequest, CenterSendRequest, CenterResponse, DataCategory};

/// HTTP 服务中心 Sender。
pub struct HttpServiceCenterSender {
    http_client: reqwest::Client,
    config: CenterClientConfig,
}

impl HttpServiceCenterSender {
    /// 创建新的 HTTP Sender。
    ///
    /// 构建失败时返回 `CenterError::Config`（如 TLS 配置错误等罕见场景）。
    pub fn new(config: CenterClientConfig) -> Self {
        let timeout = Duration::from_millis(config.timeout_ms);
        // reqwest::Client::build 仅在系统级配置异常时失败（如 TLS 后端不可用），
        // 此处降级为无超时客户端以保证初始化不中断。
        let http_client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .unwrap_or_else(|e| {
                tracing::error!(
                    error = %e,
                    "构建 reqwest Client 失败，降级为默认客户端（无超时）"
                );
                reqwest::Client::new()
            });
        Self { http_client, config }
    }

    /// 解析导入端点 URL。
    async fn resolve_import_url(&self, category: DataCategory) -> Result<String, CenterError> {
        match self.config.mode.as_str() {
            "http_url" => {
                let urls = self.config.resolve_urls();
                urls.get(&category).cloned().ok_or_else(|| {
                    CenterError::Config(format!("{} URL 未配置", category.center_name()))
                })
            }
            "http_discovery" => {
                let service_name = self.config.discovery.get_service_name(category).ok_or_else(|| {
                    CenterError::Config(format!("{} 服务名未配置", category.center_name()))
                })?;
                self.resolve_url_from_discovery(service_name, category, false)
            }
            other => Err(CenterError::Config(format!(
                "不支持的 HTTP 模式: {}",
                other
            ))),
        }
    }

    /// 解析清理端点 URL。
    async fn resolve_cleanup_url(&self, category: DataCategory) -> Result<String, CenterError> {
        match self.config.mode.as_str() {
            "http_url" => {
                let urls = self.config.resolve_urls();
                let import_url = urls.get(&category).cloned().ok_or_else(|| {
                    CenterError::Config(format!("{} URL 未配置", category.center_name()))
                })?;
                // 将 import URL 末尾的 /import 替换为 /cleanup
                // 仅替换末尾，避免误替换路径中其他 /import 片段
                if let Some(stripped) = import_url.strip_suffix("/import") {
                    Ok(format!("{stripped}/cleanup"))
                } else {
                    // URL 不以 /import 结尾，直接追加 /cleanup
                    // 这种情况通常意味着用户配置了自定义端点
                    tracing::warn!(
                        url = %import_url,
                        "import URL 不以 /import 结尾，直接追加 /cleanup"
                    );
                    Ok(format!("{import_url}/cleanup"))
                }
            }
            "http_discovery" => {
                let service_name = self.config.discovery.get_service_name(category).ok_or_else(|| {
                    CenterError::Config(format!("{} 服务名未配置", category.center_name()))
                })?;
                self.resolve_url_from_discovery(service_name, category, true)
            }
            other => Err(CenterError::Config(format!(
                "不支持的 HTTP 模式: {}",
                other
            ))),
        }
    }

    /// 通过服务发现解析 URL。
    fn resolve_url_from_discovery(
        &self,
        service_name: &str,
        category: DataCategory,
        is_cleanup: bool,
    ) -> Result<String, CenterError> {
        let cache = GlobalServiceInstanceCache::get();
        let instances = cache.get(service_name).filter(|v| !v.is_empty()).ok_or_else(|| {
            CenterError::Unavailable {
                center: category.center_name().to_string(),
                url: service_name.to_string(),
            }
        })?;

        // 过滤健康实例
        let healthy: Vec<&ServiceInstance> = instances.iter().filter(|i| i.healthy).collect();
        let pool: Vec<&ServiceInstance> = if healthy.is_empty() {
            warn!(
                service_name = %service_name,
                "无健康实例，使用全部实例作为回退"
            );
            instances.iter().collect()
        } else {
            healthy
        };

        // 随机选择一个实例（pool 非空已由前面检查保证）
        let instance = pool
            .choose(&mut rand::thread_rng())
            .ok_or_else(|| CenterError::Unavailable {
                center: category.center_name().to_string(),
                url: service_name.to_string(),
            })?;

        // 优先 metadata["http_port"]，回退 instance.port
        let port = instance
            .metadata
            .get("http_port")
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(instance.port);

        let path = category_to_http_path(category, is_cleanup)?;
        Ok(format!("http://{}:{}{}", instance.ip, port, path))
    }
}

/// 根据 category 和操作类型解析 HTTP 路径。
///
/// 当前仅支持 Perm（权限中心），其他 category 返回错误。
/// 后续实现其他中心时在此扩展。
fn category_to_http_path(category: DataCategory, is_cleanup: bool) -> Result<&'static str, CenterError> {
    match (category, is_cleanup) {
        (DataCategory::Perm, false) => Ok("/api/iam/permissions/import"),
        (DataCategory::Perm, true) => Ok("/api/iam/permissions/cleanup"),
        // 其他中心的端点待后续实现
        _ => Err(CenterError::Config(format!(
            "{} 的 HTTP 端点尚未实现",
            category.center_name()
        ))),
    }
}

#[async_trait]
impl ServiceCenterSender for HttpServiceCenterSender {
    async fn send_data(
        &self,
        request: CenterSendRequest,
    ) -> Result<CenterResponse, CenterError> {
        let url = self.resolve_import_url(request.category).await?;
        let category = request.category;

        info!(
            target: "cmx_plugin_center",
            category = %category.center_name(),
            url = %url,
            plugin_id = %request.plugin_id,
            zip_size = request.zip_data.len(),
            "HTTP 发送数据到服务中心"
        );

        let part = reqwest::multipart::Part::bytes(request.zip_data.clone())
            .file_name(request.zip_file_name.clone())
            .mime_str("application/zip")
            .map_err(|e| CenterError::PackError(format!("构建 multipart 失败: {e}")))?;

        let form = reqwest::multipart::Form::new()
            .part("file", part)
            .text("category", category.as_str().to_string())
            .text("domain_code", request.domain_code.clone())
            .text("application_code", request.application_code.clone())
            .text("module_code", request.module_code.clone())
            .text("plugin_id", request.plugin_id.clone())
            .text("app_id", request.app_id.clone())
            .text("version", request.version.clone());

        let resp = self
            .http_client
            .post(&url)
            .multipart(form)
            .send()
            .await
            .map_err(|e| CenterError::Network(format!("HTTP 请求失败: {e}")))?;

        let status = resp.status();
        let body = resp
            .text()
            .await
            .map_err(|e| CenterError::Network(format!("读取响应失败: {e}")))?;

        if !status.is_success() {
            return Err(CenterError::CallFailed {
                center: category.center_name().to_string(),
                message: format!("HTTP {}: {}", status, body),
            });
        }

        // 解析响应 JSON
        let api_resp: serde_json::Value = serde_json::from_str(&body).map_err(|e| {
            CenterError::Network(format!("响应 JSON 解析失败: {e}, body: {body}"))
        })?;

        let success = api_resp
            .get("code")
            .and_then(|v| v.as_u64())
            .map(|c| c == 200)
            .unwrap_or(false);

        let message = api_resp
            .get("msg")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        if !success {
            return Err(CenterError::CallFailed {
                center: category.center_name().to_string(),
                message,
            });
        }

        Ok(CenterResponse {
            success: true,
            message,
            center_id: None,
        })
    }

    async fn cleanup_data(
        &self,
        request: CenterCleanupRequest,
    ) -> Result<CenterResponse, CenterError> {
        let url = self.resolve_cleanup_url(request.category).await?;
        let category = request.category;

        info!(
            target: "cmx_plugin_center",
            category = %category.center_name(),
            url = %url,
            plugin_id = %request.plugin_id,
            "HTTP 清理服务中心数据"
        );

        let payload = serde_json::json!({
            "category": category.as_str(),
            "domain_code": request.domain_code,
            "application_code": request.application_code,
            "module_code": request.module_code,
            "plugin_id": request.plugin_id,
            "app_id": request.app_id,
        });

        let resp = self
            .http_client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| CenterError::Network(format!("HTTP 请求失败: {e}")))?;

        let status = resp.status();
        let body = resp
            .text()
            .await
            .map_err(|e| CenterError::Network(format!("读取响应失败: {e}")))?;

        if !status.is_success() {
            return Err(CenterError::CallFailed {
                center: category.center_name().to_string(),
                message: format!("HTTP {}: {}", status, body),
            });
        }

        let api_resp: serde_json::Value = serde_json::from_str(&body).map_err(|e| {
            CenterError::Network(format!("响应 JSON 解析失败: {e}, body: {body}"))
        })?;

        let success = api_resp
            .get("code")
            .and_then(|v| v.as_u64())
            .map(|c| c == 200)
            .unwrap_or(false);

        let message = api_resp
            .get("msg")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        if !success {
            return Err(CenterError::CallFailed {
                center: category.center_name().to_string(),
                message,
            });
        }

        Ok(CenterResponse {
            success: true,
            message,
            center_id: None,
        })
    }
}
