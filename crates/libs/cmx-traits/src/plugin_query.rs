//! 插件查询 trait 定义
//!
//! 定义跨模块的插件状态查询接口，cmx-plugin 的 PluginManager 将实现此 trait，
//! cmx-service 等模块通过此 trait 查询插件信息而无需直接依赖 cmx-plugin。

use std::path::PathBuf;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::TraitError;

/// 插件快照信息
///
/// 跨模块传递的插件元数据轻量子集，从 PluginInfo 转换而来。
/// 仅包含其他模块需要的核心字段，避免暴露 cmx-plugin 的内部类型。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginSnapshot {
    /// 插件唯一标识
    pub plugin_id: String,

    /// 插件名称
    pub name: String,

    /// 插件版本
    pub version: String,

    /// 插件状态（"installed", "activated", "deactivated", "error"）
    pub status: String,

    /// 安装路径（绝对路径）
    pub install_path: String,

    /// WASM 文件路径（相对于安装路径）
    pub wasm_path: Option<String>,

    /// 插件类型（如 "wasm", "rhai"）
    pub plugin_type: String,

    /// 域编码
    pub domain_code: String,

    /// 应用编码
    pub application_code: String,

    /// 模块编码
    pub module_code: String,

    /// 源码路径（从 manifest.json 读取，相对于 install_path）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
}

/// 插件筛选条件
///
/// 用于查询插件列表时的过滤条件定义。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginFilter {
    /// 按状态筛选
    pub status: Option<String>,

    /// 按名称筛选（模糊匹配）
    pub name: Option<String>,

    /// 按域编码筛选
    pub domain_code: Option<String>,

    /// 按应用编码筛选
    pub application_code: Option<String>,

    /// 按模块编码筛选
    pub module_code: Option<String>,

    /// 按应用ID筛选
    pub app_id: Option<String>,
}

/// 插件查询 trait
///
/// 供 cmx-service 等模块使用，用于查询插件信息。
/// cmx-plugin 的 PluginManager 实现此 trait，实现跨模块解耦。
#[async_trait]
pub trait PluginQuery: Send + Sync {
    /// 根据插件ID查询插件快照
    ///
    /// # 参数
    ///
    /// * `plugin_id` - 插件唯一标识
    ///
    /// # 返回值
    ///
    /// 返回插件快照，如果插件不存在则返回 None。
    async fn get_plugin(&self, plugin_id: &str) -> Result<Option<PluginSnapshot>, TraitError>;

    /// 检查插件是否已安装
    ///
    /// 插件已安装表示插件已被下载并注册到系统中，
    /// 但不一定已激活（可能处于 deactivated 状态）。
    ///
    /// # 参数
    ///
    /// * `plugin_id` - 插件唯一标识
    ///
    /// # 返回值
    ///
    /// - `Ok(true)` - 插件已安装
    /// - `Ok(false)` - 插件未安装
    /// - `Err(_)` - 查询过程中发生错误
    async fn is_installed(&self, plugin_id: &str) -> Result<bool, TraitError>;

    /// 检查插件是否已激活
    ///
    /// # 参数
    ///
    /// * `plugin_id` - 插件唯一标识
    async fn is_active(&self, plugin_id: &str) -> Result<bool, TraitError>;

    /// 获取插件的 WASM 文件绝对路径
    ///
    /// 通过拼接 install_path 和 wasm_path 生成绝对路径。
    ///
    /// # 参数
    ///
    /// * `plugin_id` - 插件唯一标识
    ///
    /// # 错误
    ///
    /// 插件不存在或未配置 WASM 路径时返回错误。
    async fn get_wasm_path(&self, plugin_id: &str) -> Result<PathBuf, TraitError>;

    /// 列出所有已激活的插件快照
    async fn list_active_plugins(&self) -> Result<Vec<PluginSnapshot>, TraitError>;

    /// 根据筛选条件查询插件列表
    ///
    /// # 参数
    ///
    /// * `filter` - 筛选条件
    async fn list_plugins(&self, filter: &PluginFilter) -> Result<Vec<PluginSnapshot>, TraitError>;
}
