//! 基础服务中心数据类型定义。

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 数据类别枚举。
///
/// 标识插件安装目录中不同类型的业务数据子目录及对应的目标服务中心。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DataCategory {
    /// 菜单数据 → 门户中心。
    Menu,
    /// 权限数据 → 权限中心。
    Perm,
    /// 表单数据 → 表单中心。
    Form,
    /// 流程定义数据 → 流程中心。
    Flow,
}

impl DataCategory {
    /// 返回数据目录名称。
    pub fn dir_name(&self) -> &str {
        match self {
            Self::Menu => "menudata",
            Self::Perm => "permdata",
            Self::Form => "formdata",
            Self::Flow => "flowdata",
        }
    }

    /// 返回用于 HTTP/gRPC 传输的短标识符。
    ///
    /// 与 `PluginDataCategory::as_str()` 保持一致（`menu`/`perm`/`form`/`flow`），
    /// 用于跨服务传输的 category 字段。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Menu => "menu",
            Self::Perm => "perm",
            Self::Form => "form",
            Self::Flow => "flow",
        }
    }

    /// 返回对应服务中心的中文名称。
    pub fn center_name(&self) -> &str {
        match self {
            Self::Menu => "门户中心",
            Self::Perm => "权限中心",
            Self::Form => "表单中心",
            Self::Flow => "流程中心",
        }
    }

    /// 返回所有数据类别的有序列表。
    pub fn all() -> &'static [DataCategory] {
        &[Self::Menu, Self::Perm, Self::Form, Self::Flow]
    }
}

/// 分发操作的统一入参结构体。
///
/// 从 `PersistResult` 映射而来，作为 `dispatch_install` / `dispatch_cleanup` 的参数。
/// 后续如需扩展参数，只需在此结构体中添加字段。
#[derive(Debug, Clone)]
pub struct DispatchContext {
    /// 插件安装路径。
    pub install_path: PathBuf,
    /// 插件 ID。
    pub plugin_id: String,
    /// 应用 ID。
    pub app_id: String,
    /// 插件版本。
    pub version: String,
    /// 域编码。
    pub domain_code: String,
    /// 应用编码。
    pub application_code: String,
    /// 模块编码。
    pub module_code: String,
}

/// 单个数据类别的分发结果。
#[derive(Debug)]
pub struct CategoryResult {
    /// 数据类别。
    pub category: DataCategory,
    /// 调用结果。
    pub result: Result<CenterResponse, super::sender::CenterError>,
}

/// 整体分发结果汇总。
///
/// 包含所有数据类别的调用结果，提供便捷的汇总查询方法。
#[derive(Debug)]
pub struct DispatchResult {
    /// 各数据类别的分发结果列表。
    pub results: Vec<CategoryResult>,
}

impl DispatchResult {
    /// 检查是否所有中心的调用均成功。
    pub fn is_all_success(&self) -> bool {
        self.results
            .iter()
            .all(|r| r.result.is_ok() && r.result.as_ref().unwrap().success)
    }

    /// 返回所有失败的中心结果列表。
    pub fn failed_categories(&self) -> Vec<&CategoryResult> {
        self.results
            .iter()
            .filter(|r| {
                r.result.is_err() || !r.result.as_ref().map(|resp| resp.success).unwrap_or(false)
            })
            .collect()
    }

    /// 返回所有成功的中心结果列表。
    pub fn success_categories(&self) -> Vec<&CategoryResult> {
        self.results
            .iter()
            .filter(|r| {
                r.result.is_ok() && r.result.as_ref().map(|resp| resp.success).unwrap_or(false)
            })
            .collect()
    }
}

/// 发送数据到服务中心的请求。
///
/// 包含 form-data 所需的元数据字段和 ZIP 文件字节。
pub struct CenterSendRequest {
    /// 插件 ID。
    pub plugin_id: String,
    /// 应用 ID。
    pub app_id: String,
    /// 插件版本。
    pub version: String,
    /// 数据类别。
    pub category: DataCategory,
    /// ZIP 压缩后的数据字节。
    pub zip_data: Vec<u8>,
    /// ZIP 文件名。
    pub zip_file_name: String,
    /// 域编码。
    pub domain_code: String,
    /// 应用编码。
    pub application_code: String,
    /// 模块编码。
    pub module_code: String,
}

/// 清理服务中心数据的请求。
pub struct CenterCleanupRequest {
    /// 插件 ID。
    pub plugin_id: String,
    /// 应用 ID。
    pub app_id: String,
    /// 插件版本。
    pub version: Option<String>,
    /// 数据类别。
    pub category: DataCategory,
    /// 域编码。
    pub domain_code: String,
    /// 应用编码。
    pub application_code: String,
    /// 模块编码。
    pub module_code: String,
}

/// 服务中心响应。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CenterResponse {
    /// 是否成功。
    pub success: bool,
    /// 响应消息。
    pub message: String,
    /// 中心侧的资源 ID（可选）。
    pub center_id: Option<String>,
}
