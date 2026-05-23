//! 升级服务模块
//!
//! 处理插件升级流程，提供完整的插件版本升级功能。

use std::sync::Arc;

use crate::domain::plugin::PluginSource;
use serde::{Deserialize, Serialize};

/// 升级请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpgradeRequest {
    /// 插件ID
    pub plugin_id: String,
    /// 新版本来源
    pub source: PluginSource,
    /// 版本约束
    #[serde(default)]
    pub version_constraint: Option<String>,
    /// 是否强制升级（忽略版本检查）
    pub force: bool,
    /// 操作者
    pub operator: Option<String>,
    /// 构建类型 debug release
    pub  build_type : Option<String>,
    /// 市场版本来源 ID，关联 `cmx_marketplace_plugin_version.id`。
    pub marketplace_source_id: Option<String>,
    /// 应用ID
    #[serde(default)]
    pub app_id: Option<String>,
    /// 是否发送事件通知（管控接口调用时设为 false）
    #[serde(default = "default_true")]
    pub send_event: bool,
}

fn default_true() -> bool {
    true
}

/// 升级响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpgradeResponse {
    /// 插件ID
    pub plugin_id: String,
    /// 旧版本
    pub old_version: String,
    /// 新版本
    pub new_version: String,
    /// 是否成功
    pub success: bool,
    /// 消息
    pub message: String,
}

/// 升级服务
#[derive(Clone)]
pub struct UpgradeService {
    executor: Arc<crate::service::executor::PluginOperationExecutor>,
}

impl UpgradeService {
    /// 创建新的升级服务
    pub fn new(executor: Arc<crate::service::executor::PluginOperationExecutor>) -> Self {
        Self { executor }
    }

    /// 执行升级操作
    pub async fn upgrade(&self, request: UpgradeRequest) -> crate::error::PluginResult<UpgradeResponse> {
        self.executor.execute_upgrade(request).await
    }
}
