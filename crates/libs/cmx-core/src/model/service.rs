use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceDefinition {
    pub id: String,
    pub service_key: String,
    pub service_name: String,
    pub description: String,
    pub plugin_id: String,
    pub status: i32,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceOrchestration {
    pub name: String,
    pub code: String,
    pub description: String,
    pub flow: ServiceFlow,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceFlow {
    pub nodes: Vec<ServiceNode>,
    pub edges: Vec<ServiceEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceNode {
    pub id: String,
    #[serde(rename = "type")]
    pub node_type: String,
    pub parent: Option<String>,
    pub meta: NodeMeta,
    pub data: NodeData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceEdge {
    pub source_node_id: String,
    pub source_port_id: String,
    pub target_node_id: String,
    pub target_port_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeMeta {
    pub z_index: i32,
    pub size: NodeSize,
    pub position: NodePosition,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeSize {
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodePosition {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeData {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_meta: Option<NodeNodeMeta>,
    #[serde(default)]
    pub inputs: Vec<NodeIO>,
    #[serde(default)]
    pub outputs: Vec<NodeIO>,
    #[serde(default)]
    pub options: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeNodeMeta {
    pub plugin_id: String,
    pub plugin_name: String,
    pub plugin_version: String,
    pub function_name: String,
    /// 事务节点关联的数据库ID
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeIO {
    pub key: String,
    #[serde(rename = "type")]
    pub io_type: String,
    pub description: String,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SVRContext {
    pub initial_input: String,
    pub headers: HashMap<String, String>,
}

impl SVRContext {
    pub fn new(initial_input: String, headers: HashMap<String, String>) -> Self {
        Self { initial_input, headers }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionInput {
    pub input: String,
    pub context: SVRContext,
    /// 事务ID（仅在事务框内执行时设置）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub txn_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionOutput {
    pub result: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInfo {
    pub id: String,
    pub service_key: String,
    pub service_name: String,
    pub description: String,
    pub plugin_id: String,
    pub status: i32,
    pub version: String,
    pub config: String,
}

impl From<ServiceDefinition> for ServiceInfo {
    fn from(def: ServiceDefinition) -> Self {
        Self {
            id: def.id,
            service_key: def.service_key,
            service_name: def.service_name,
            description: def.description,
            plugin_id: def.plugin_id,
            status: def.status,
            version: def.version,
            config: String::new(),
        }
    }
}
