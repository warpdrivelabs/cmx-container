//! 降级服务模块
//!
//! 处理插件降级流程，提供将插件回退到指定旧版本的功能。
//!
//! 降级只是切换版本目录，不涉及文件拷贝。

use std::sync::Arc;

use crate::domain::plugin::PluginSource;
use serde::{Deserialize, Serialize};

/// 降级请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DowngradeRequest {
    /// 插件ID
    pub plugin_id: String,
    /// 目标版本
    pub target_version: String,
    /// 插件来源（可选，用于下载旧版本）
    pub source: Option<PluginSource>,
    /// 操作者
    pub operator: Option<String>,
    /// 应用ID
    #[serde(default)]
    pub app_id: Option<String>,
}

/// 降级响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DowngradeResponse {
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

/// 降级服务
#[derive(Clone)]
pub struct DowngradeService {
    executor: Arc<crate::service::executor::PluginOperationExecutor>,
}

impl DowngradeService {
    /// 创建新的降级服务
    pub fn new(executor: Arc<crate::service::executor::PluginOperationExecutor>) -> Self {
        Self { executor }
    }

    /// 降级插件
    pub async fn downgrade(
        &self,
        request: DowngradeRequest,
    ) -> crate::error::PluginResult<DowngradeResponse> {
        self.executor.execute_downgrade(request).await
    }
}
