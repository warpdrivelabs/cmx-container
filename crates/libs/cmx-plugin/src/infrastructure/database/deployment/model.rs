//! 部署记录数据模型
//!
//! 定义与 `cmx_plugin_deployments` 表对应的结构体：
//! - `DeploymentRecord`: 查询结果记录
//! - `DeploymentCreateParams`: 创建/写入参数
//! - `DeploymentUpdateParams`: 更新参数（所有字段可选）

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 部署记录查询结果
///
/// 字段与 `cmx_plugin_deployments` 表 18 列一一对应。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentRecord {
    /// 主键ID
    pub id: String,
    /// 关联插件ID
    pub plugin_id: String,
    /// 节点标识
    pub node_id: String,
    /// 节点类型: primary/replica/worker
    pub node_type: Option<String>,
    /// 部署的版本
    pub version: String,
    /// 部署状态
    pub status: String,
    /// 进度 (0-100)
    pub progress: i32,
    /// 错误消息
    pub error_message: Option<String>,
    /// 错误详情
    pub error_details: Option<String>,
    /// 插件类型: wasm/rhai (对应列: plugin_type)
    pub plugin_type: Option<String>,
    /// 源码路径 (对应列: source_path)
    pub source_path: Option<String>,
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

/// 部署创建参数（用于 INSERT 操作）
///
/// 字段与 `cmx_plugin_deployments` 表 18 列一一对应。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentCreateParams {
    /// 主键ID
    pub id: String,
    /// 关联插件ID
    pub plugin_id: String,
    /// 节点标识
    pub node_id: String,
    /// 节点类型
    pub node_type: Option<String>,
    /// 部署版本
    pub version: String,
    /// 部署状态
    pub status: String,
    /// 进度
    pub progress: i32,
    /// 归档标志
    pub archived: i32,
    /// 插件类型
    pub plugin_type: Option<String>,
    /// 源码路径
    pub source_path: Option<String>,
    /// 创建人ID
    pub create_by: Option<String>,
    /// 创建人名称
    pub create_name: Option<String>,
    /// 更新人ID
    pub update_by: Option<String>,
    /// 更新人名称
    pub update_name: Option<String>,
}

/// 部署更新参数（用于 UPDATE 操作，所有字段可选）
///
/// 更新条件为 WHERE id = ?。
#[derive(Debug, Clone, Default)]
pub struct DeploymentUpdateParams {
    /// 部署版本
    pub version: Option<String>,
    /// 部署状态
    pub status: Option<String>,
    /// 进度
    pub progress: Option<i32>,
    /// 错误消息
    pub error_message: Option<String>,
    /// 错误详情
    pub error_details: Option<String>,
    /// 归档标志
    pub archived: Option<i32>,
    /// 插件类型
    pub plugin_type: Option<String>,
    /// 源码路径
    pub source_path: Option<String>,
    /// 创建人ID
    pub create_by: Option<String>,
    /// 创建人名称
    pub create_name: Option<String>,
    /// 更新人ID
    pub update_by: Option<String>,
    /// 更新人名称
    pub update_name: Option<String>,
}
