//! 版本历史数据模型
//!
//! 定义与 `cmx_plugin_versions` 表对应的结构体：
//! - `VersionRecord`: 查询结果记录
//! - `VersionCreateParams`: 创建/写入参数
//! - `VersionUpdateParams`: 更新参数（所有字段可选）

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 版本历史查询结果记录
///
/// 字段与 `cmx_plugin_versions` 表 20 列一一对应。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionRecord {
    /// 主键ID
    pub id: String,
    /// 关联插件ID
    pub plugin_id: String,
    /// 版本号
    pub version: String,
    /// 该版本的安装路径
    pub install_path: String,
    /// 该版本的 WASM 路径
    pub wasm_path: String,
    /// 是否当前版本
    pub is_current: bool,
    /// 安装时间
    pub installed_at: DateTime<Utc>,
    /// 卸载时间
    pub uninstalled_at: Option<DateTime<Utc>>,
    /// 插件ZIP包来源地址
    pub zip_source_url: Option<String>,
    /// 插件来源类型: local/url/registry
    pub zip_source_type: Option<String>,
    /// 插件类型: wasm/rhai
    pub plugin_type: Option<String>,
    /// 源码路径
    pub source_path: Option<String>,
    /// 市场版本来源 ID。
    pub marketplace_source_id: Option<String>,
    /// 创建时间
    pub create_time: DateTime<Utc>,
    /// 更新时间
    pub update_time: DateTime<Utc>,
    /// 归档标志
    pub archived: i32,
    /// 创建人ID
    pub create_by: Option<String>,
    /// 创建人名称
    pub create_name: Option<String>,
    /// 更新人ID
    pub update_by: Option<String>,
    /// 更新人名称
    pub update_name: Option<String>,
}

/// 版本历史创建参数（用于 INSERT / UPSERT 操作）
///
/// 字段与 `cmx_plugin_versions` 表 20 列一一对应。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionCreateParams {
    /// 主键ID
    pub id: String,
    /// 关联插件ID
    pub plugin_id: String,
    /// 版本号
    pub version: String,

    /// 安装路径
    pub install_path: String,
    /// WASM 路径
    pub wasm_path: String,
    /// 是否当前版本
    pub is_current: bool,
    /// 安装时间
    pub installed_at: DateTime<Utc>,
    /// 卸载时间
    pub uninstalled_at: Option<DateTime<Utc>>,
    /// 插件ZIP包来源地址
    pub zip_source_url: Option<String>,
    /// 插件来源类型
    pub zip_source_type: Option<String>,
    /// 插件类型
    pub plugin_type: Option<String>,
    /// 源码路径
    pub source_path: Option<String>,

    /// 构建类型 debug/release
    pub build_type: String,

    /// 市场版本来源 ID。
    pub marketplace_source_id: Option<String>,

    /// 创建时间
    pub create_time: DateTime<Utc>,
    /// 更新时间
    pub update_time: DateTime<Utc>,
    /// 归档标志
    pub archived: i32,
    /// 创建人ID
    pub create_by: Option<String>,
    /// 创建人名称
    pub create_name: Option<String>,
    /// 更新人ID
    pub update_by: Option<String>,
    /// 更新人名称
    pub update_name: Option<String>,
}

/// 版本历史更新参数（用于 UPDATE 操作，所有字段可选）
///
/// 更新条件为 WHERE id = ?。
#[derive(Debug, Clone, Default)]
pub struct VersionUpdateParams {
    /// 安装路径
    pub install_path: Option<String>,
    /// WASM路径
    pub wasm_path: Option<String>,

    /// 是否当前版本
    pub is_current: Option<bool>,
    /// 卸载时间
    pub uninstalled_at: Option<DateTime<Utc>>,
    /// 更新时间
    pub update_time: Option<DateTime<Utc>>,
    /// 创建人ID
    pub create_by: Option<String>,
    /// 创建人名称
    pub create_name: Option<String>,
    /// 更新人ID
    pub update_by: Option<String>,
    /// 更新人名称
    pub update_name: Option<String>,
}
