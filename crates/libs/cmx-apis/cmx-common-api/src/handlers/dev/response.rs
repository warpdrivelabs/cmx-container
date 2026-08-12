//! 开发工具 API 响应结构体

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// 模板信息
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TemplateInfo {
    /// 模板名称
    pub name: String,
    /// 模板路径
    pub path: String,
    /// 修改时间
    pub modified_time: Option<DateTime<Utc>>,
    /// 文件大小（字节）
    pub file_size: Option<u64>,
}

/// 创建项目响应
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateProjectResponse {
    /// 状态码 (0=成功, -1=失败)
    pub code: i32,
    /// 消息
    pub message: Option<String>,
    /// 项目URL (code-server打开链接)
    pub project_url: Option<String>,
}
