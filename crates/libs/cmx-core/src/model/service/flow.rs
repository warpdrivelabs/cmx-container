//! 流程定义模块
//!
//! 包含服务编排中的流程结构：节点、边、元数据等。

use serde::{Deserialize, Serialize};

/// 流程定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceFlow {
    /// 节点列表
    pub nodes: Vec<ServiceNode>,
    /// 边列表
    pub edges: Vec<ServiceEdge>,
}

/// 流程节点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceNode {
    /// 节点ID
    pub id: String,
    /// 节点类型（skylake-start / skylake-end / skylake-transaction / skylake-switch / skylake-func）
    #[serde(rename = "type")]
    pub node_type: String,
    /// 父节点ID（事务框内的节点指向事务框节点ID）
    pub parent: Option<String>,
    /// 节点元数据（位置、大小）
    pub meta: NodeMeta,
    /// 节点数据（名称、函数信息等）
    pub data: Option<NodeData>,
}

/// 流程边
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceEdge {
    /// 源节点ID
    #[serde(rename = "sourceNodeID")]
    pub source_node_id: String,
    /// 源端口ID（out / out_{value}）
    #[serde(rename = "sourcePortID")]
    pub source_port_id: String,
    /// 目标节点ID
    #[serde(rename = "targetNodeID")]
    pub target_node_id: String,
    /// 目标端口ID（in）
    #[serde(rename = "targetPortID")]
    pub target_port_id: String,
}

/// 节点元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeMeta {
    /// 层级顺序
    #[serde(default)]
    pub z_index: i32,
    /// 节点尺寸
    pub size: NodeSize,
    /// 节点位置
    pub position: NodePosition,
}

/// 节点尺寸
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeSize {
    /// 宽度
    pub width: i32,
    /// 高度
    pub height: i32,
}

/// 节点位置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodePosition {
    /// X坐标
    pub x: f64,
    /// Y坐标
    pub y: f64,
}

/// 节点数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeData {
    /// 节点名称
    pub name: String,
    /// 函数元信息
    #[serde(rename = "nodeMeta")]
    pub node_meta: Option<NodeNodeMeta>,
    /// 输入参数列表
    #[serde(default)]
    pub inputs: Vec<NodeIO>,
    /// 输出参数列表
    #[serde(default)]
    pub outputs: Vec<NodeIO>,
    /// 分支选项（仅 skylake-switch 节点有）
    #[serde(default)]
    pub options: Option<Vec<String>>,
}

/// 函数元信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeNodeMeta {
    #[serde(rename = "pluginId")]
    /// 插件ID
    pub plugin_id: String,
    /// 插件名称
    #[serde(rename = "pluginName")]
    pub plugin_name: String,
    /// 插件版本
    #[serde(default)]
    #[serde(rename = "pluginVersion")]
    pub plugin_version: String,
    /// 函数名称
    #[serde(rename = "functionName")]
    pub function_name: String,
    /// 事务节点关联的数据库ID
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "databaseId")]
    pub database_id: Option<String>,
}

/// 节点输入输出参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeIO {
    /// 参数键名
    pub key: String,
    /// 参数类型
    #[serde(rename = "type")]
    pub io_type: String,
    /// 参数描述
    pub description: String,
    /// 是否必填
    #[serde(default)]
    pub required: bool,
}
