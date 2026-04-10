use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 服务定义 — 对应 cmx_service_define 表
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceDefinition {
    /// 主键ID
    pub id: String,
    /// 服务唯一标识（来自 JSON 的 code 字段）
    pub service_key: String,
    /// 服务名称
    pub service_name: String,
    /// 服务描述
    pub description: String,
    /// 所属插件ID
    pub plugin_id: String,
    /// 状态：0-禁用，1-启用
    pub status: i32,
    /// 服务版本
    pub version: String,
    /// 服务编排配置
    pub config: Option<String>,
}

/// 服务编排定义 — 从 服务.json 解析
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceOrchestration {
    /// 编排名称
    pub name: String,
    /// 服务key（唯一标识）
    pub code: String,
    /// 描述信息
    pub description: String,
    /// 流程定义
    pub flow: ServiceFlow,
    ///原始json字符
    #[serde(skip)]
    pub source_str: String,
}

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
    ///
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
    // #[serde(skip_serializing_if = "Option::is_none")]
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

/// 服务调用上下文 — 在函数间传递
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SVRContext {
    /// 初始调用入参（API 请求传入的参数）
    pub initial_input: String,
    /// HTTP 请求头信息
    pub headers: HashMap<String, String>,
    /// 各步骤执行结果的缓存（步骤ID -> 输出）
    #[serde(default)]
    pub step_outputs: HashMap<String, String>,
}

impl SVRContext {
    /// 创建新的服务调用上下文
    ///
    /// # 参数
    /// * `initial_input` - 初始调用入参
    /// * `headers` - HTTP 请求头
    pub fn new(initial_input: String, headers: HashMap<String, String>) -> Self {
        Self {
            initial_input,
            headers,
            step_outputs: HashMap::new(),
        }
    }

    /// 添加步骤输出
    ///
    /// # 参数
    /// * `step_id` - 步骤ID（节点ID）
    /// * `output` - 步骤输出结果
    pub fn add_step_output(&mut self, step_id: String, output: String) {
        self.step_outputs.insert(step_id, output);
    }

    /// 获取步骤输出
    ///
    /// # 参数
    /// * `step_id` - 步骤ID
    ///
    /// # 返回值
    /// 返回步骤输出结果的引用
    pub fn get_step_output(&self, step_id: &str) -> Option<&String> {
        self.step_outputs.get(step_id)
    }
}

/// 函数输入结构体 — 固定入参格式
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionInput {
    /// 当前步骤输入数据（前序步骤输出或初始输入）
    pub input: String,
    /// 服务调用上下文
    pub context: SVRContext,
    /// 事务ID（仅在事务框内执行时设置）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub txn_id: Option<String>,
}

/// 函数输出结构体 — 固定出参格式
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionOutput {
    /// 函数执行结果
    pub result: String,
}

/// 服务运行时信息 — 内存缓存用
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInfo {
    /// 主键ID
    pub id: String,
    /// 服务唯一标识
    pub service_key: String,
    /// 服务名称
    pub service_name: String,
    /// 服务描述
    pub description: String,
    /// 所属插件ID
    pub plugin_id: String,
    /// 状态：0-禁用，1-启用
    pub status: i32,
    /// 当前版本号
    pub version: String,
    /// 编排配置 JSON
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
