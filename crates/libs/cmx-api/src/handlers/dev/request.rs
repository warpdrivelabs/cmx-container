//! 开发工具 API 请求结构体

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// 创建项目请求
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateProjectRequest {
    /// 插件名称（必填）
    pub name: String,
    /// 插件编码（必填）
    pub id: String,
    /// 保存路径（必填）
    pub path: String,
    /// 描述（可选）
    pub description: Option<String>,
    /// 模板名称（必填）
    pub template: String,
    /// 领域编码（可选）
    pub domain_code: Option<String>,
    /// 应用编码（可选）
    pub application_code: Option<String>,
    /// 模块编码（可选）
    pub module_code: Option<String>,
    /// 数据源ID（可选）
    pub datasource_id: Option<String>,
}
