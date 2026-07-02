//! 流程图导航器
//!
//! 负责在 ServiceFlow 中查找节点、边和事务框数据库ID等导航操作。
//! 封装 DAG（有向无环图）的遍历逻辑，提供简洁的查询接口。

use cmx_core::model::service::{ServiceEdge, ServiceFlow, ServiceNode};
use tracing::debug;

/// 流程导航器
///
/// 封装 ServiceFlow 的查询操作，提供节点查找、边查找和事务框信息解析。
/// 使用生命周期引用避免数据拷贝，提升性能。
pub struct FlowNavigator<'a> {
    /// 流程定义引用（来自 ServiceOrchestration.flow）
    flow: &'a ServiceFlow,
}

impl<'a> FlowNavigator<'a> {
    /// 创建流程导航器
    ///
    /// # 参数
    /// * `flow` - 流程定义引用（包含 nodes 和 edges）
    pub fn new(flow: &'a ServiceFlow) -> Self {
        Self { flow }
    }

    /// 根据 ID 查找节点
    ///
    /// 在 nodes 数组中线性查找匹配 ID 的节点。
    ///
    /// # 参数
    /// * `node_id` - 节点ID（对应 Flow JSON 中的 node.id）
    ///
    /// # 返回值
    /// 找到返回节点引用，否则返回 None
    pub fn find_node(&self, node_id: &str) -> Option<&ServiceNode> {
        // 线性查找：节点数量通常较少（<100），性能可接受
        self.flow.nodes.iter().find(|n| n.id == node_id)
    }

    /// 查找开始节点
    ///
    /// 查找节点类型为 skylake-start 的节点，作为编排执行的入口点。
    /// 每个 Flow JSON 必须有且仅有一个 start 节点。
    ///
    /// # 返回值
    /// 找到返回开始节点引用，否则返回 None（Flow 配置错误）
    pub fn find_start_node(&self) -> Option<&ServiceNode> {
        // 按节点类型查找：skylake-start 是约定的开始节点类型
        self.flow
            .nodes
            .iter()
            .find(|n| n.node_type == "skylake-start")
    }

    /// 查找从指定节点出发、匹配源端口的下一条边
    ///
    /// 在 edges 数组中查找匹配源节点ID和源端口ID的边。
    /// 用于确定执行流程的下一个节点。
    ///
    /// # 参数
    /// * `source_node_id` - 源节点ID（当前执行的节点）
    /// * `source_port` - 源端口ID
    ///   - 普通节点（func/start）：固定为 "out"
    ///   - 分支节点（switch）：根据返回值动态确定，如 "out_1"、"out_2"
    ///
    /// # 返回值
    /// 找到返回边引用，否则返回 None（流程结束或配置错误）
    pub fn find_next_edge(&self, source_node_id: &str, source_port: &str) -> Option<&ServiceEdge> {
        // 边匹配条件：源节点ID + 源端口ID 同时匹配
        self.flow
            .edges
            .iter()
            .find(|e| e.source_node_id == source_node_id && e.source_port_id == source_port)
    }

    /// 解析事务框节点的数据库ID
    ///
    /// 从事务框节点的元信息中获取 database_id，如果未指定则使用默认值。
    /// 事务框可以指定独立的数据库连接，用于多数据源场景。
    ///
    /// # 参数
    /// * `txn_node_id` - 事务框节点ID（node_type = skylake-transaction）
    /// * `default_db_id` - 默认数据库ID（事务框未指定时使用）
    ///
    /// # 返回值
    /// 数据库ID字符串
    ///
    /// # 数据库ID解析优先级
    /// 1. 事务框节点的 node.data.node_meta.database_id
    /// 2. 默认数据库ID（default_db_id）
    pub fn resolve_transaction_db_id(&self, txn_node_id: &str, default_db_id: &str) -> String {
        // 查找事务框节点：ID 匹配 + 类型为 skylake-transaction
        if let Some(txn_node) = self
            .flow
            .nodes
            .iter()
            .find(|n| n.id == txn_node_id && n.node_type == "skylake-transaction")
        {
            // 从节点元数据中提取 database_id
            // 链式调用：data -> node_meta -> database_id
            txn_node
                .data
                .as_ref()
                .and_then(|d| d.node_meta.as_ref())
                .and_then(|m| m.database_id.clone())
                .unwrap_or_else(|| {
                    // 未指定 database_id，使用默认值并记录日志
                    debug!(
                        "事务框节点未指定 database_id，使用默认值: txn_node_id={}, default={}",
                        txn_node_id, default_db_id
                    );
                    default_db_id.to_string()
                })
        } else {
            // 未找到事务框节点（配置错误），使用默认值
            debug!(
                "未找到事务框节点，使用默认数据库ID: txn_node_id={}, default={}",
                txn_node_id, default_db_id
            );
            default_db_id.to_string()
        }
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use cmx_core::model::service::{
        NodeData, NodeMeta, NodeNodeMeta, NodePosition, NodeSize, ServiceEdge, ServiceFlow,
        ServiceNode,
    };

    /// 构造测试用节点元数据（位置/尺寸）
    fn make_meta() -> NodeMeta {
        NodeMeta {
            z_index: 1,
            size: NodeSize {
                width: 100,
                height: 50,
            },
            position: NodePosition { x: 0.0, y: 0.0 },
        }
    }

    /// 构造一个简单的开始节点
    fn make_start_node(id: &str) -> ServiceNode {
        ServiceNode {
            id: id.to_string(),
            node_type: "skylake-start".to_string(),
            parent: None,
            meta: make_meta(),
            data: Some(NodeData {
                name: "开始".to_string(),
                node_meta: None,
                inputs: serde_json::Value::Array(vec![]),
                outputs: serde_json::Value::Array(vec![]),
                options: None,
            }),
        }
    }

    /// 构造一个函数节点
    fn make_func_node(id: &str, name: &str, plugin_id: &str, function_name: &str) -> ServiceNode {
        ServiceNode {
            id: id.to_string(),
            node_type: "skylake-func".to_string(),
            parent: None,
            meta: make_meta(),
            data: Some(NodeData {
                name: name.to_string(),
                node_meta: Some(NodeNodeMeta {
                    plugin_id: plugin_id.to_string(),
                    plugin_name: plugin_id.to_string(),
                    plugin_version: "1.0.0".to_string(),
                    function_name: function_name.to_string(),
                    database_id: None,
                }),
                inputs: serde_json::Value::Array(vec![]),
                outputs: serde_json::Value::Array(vec![]),
                options: None,
            }),
        }
    }

    /// 构造一个事务框节点（可指定 database_id）
    fn make_txn_node(id: &str, name: &str, database_id: Option<&str>) -> ServiceNode {
        ServiceNode {
            id: id.to_string(),
            node_type: "skylake-transaction".to_string(),
            parent: None,
            meta: make_meta(),
            data: Some(NodeData {
                name: name.to_string(),
                node_meta: Some(NodeNodeMeta {
                    plugin_id: String::new(),
                    plugin_name: String::new(),
                    plugin_version: String::new(),
                    function_name: String::new(),
                    database_id: database_id.map(|s| s.to_string()),
                }),
                inputs: serde_json::Value::Array(vec![]),
                outputs: serde_json::Value::Array(vec![]),
                options: None,
            }),
        }
    }

    /// 构造一条边
    fn make_edge(source: &str, source_port: &str, target: &str) -> ServiceEdge {
        ServiceEdge {
            source_node_id: source.to_string(),
            source_port_id: source_port.to_string(),
            target_node_id: target.to_string(),
            target_port_id: "in".to_string(),
        }
    }

    /// 构造线性流程：start -> func1 -> func2 -> end
    fn make_linear_flow() -> ServiceFlow {
        ServiceFlow {
            nodes: vec![
                make_start_node("start_1"),
                make_func_node("func_1", "步骤1", "plugin_a", "func1"),
                make_func_node("func_2", "步骤2", "plugin_a", "func2"),
                ServiceNode {
                    id: "end_1".to_string(),
                    node_type: "skylake-end".to_string(),
                    parent: None,
                    meta: make_meta(),
                    data: Some(NodeData {
                        name: "结束".to_string(),
                        node_meta: None,
                        inputs: serde_json::Value::Array(vec![]),
                        outputs: serde_json::Value::Array(vec![]),
                        options: None,
                    }),
                },
            ],
            edges: vec![
                make_edge("start_1", "out", "func_1"),
                make_edge("func_1", "out", "func_2"),
                make_edge("func_2", "out", "end_1"),
            ],
        }
    }

    // ==================== find_node 测试 ====================

    #[test]
    fn find_node_应返回存在的节点() {
        let flow = make_linear_flow();
        let navigator = FlowNavigator::new(&flow);

        let node = navigator.find_node("func_1");
        assert!(node.is_some(), "应找到 func_1 节点");
        assert_eq!(node.unwrap().id, "func_1");
        assert_eq!(node.unwrap().node_type, "skylake-func");
    }

    #[test]
    fn find_node_对不存在的id应返回none() {
        let flow = make_linear_flow();
        let navigator = FlowNavigator::new(&flow);

        assert!(navigator.find_node("not_exists").is_none());
        assert!(navigator.find_node("").is_none());
    }

    // ==================== find_start_node 测试 ====================

    #[test]
    fn find_start_node_应返回开始节点() {
        let flow = make_linear_flow();
        let navigator = FlowNavigator::new(&flow);

        let start = navigator.find_start_node();
        assert!(start.is_some(), "应找到开始节点");
        assert_eq!(start.unwrap().id, "start_1");
        assert_eq!(start.unwrap().node_type, "skylake-start");
    }

    #[test]
    fn find_start_node_无开始节点时返回none() {
        // 流程中只有 func 节点，没有 start 节点
        let flow = ServiceFlow {
            nodes: vec![make_func_node("func_1", "步骤1", "p", "f")],
            edges: vec![],
        };
        let navigator = FlowNavigator::new(&flow);

        assert!(navigator.find_start_node().is_none());
    }

    #[test]
    fn find_start_node_多个开始节点时返回第一个() {
        // 异常配置：存在多个 start 节点，应返回第一个
        let flow = ServiceFlow {
            nodes: vec![make_start_node("start_2"), make_start_node("start_1")],
            edges: vec![],
        };
        let navigator = FlowNavigator::new(&flow);

        let start = navigator.find_start_node();
        assert!(start.is_some());
        assert_eq!(start.unwrap().id, "start_2");
    }

    // ==================== find_next_edge 测试 ====================

    #[test]
    fn find_next_edge_普通节点应找到out端口边() {
        let flow = make_linear_flow();
        let navigator = FlowNavigator::new(&flow);

        let edge = navigator.find_next_edge("func_1", "out");
        assert!(edge.is_some());
        assert_eq!(edge.unwrap().source_node_id, "func_1");
        assert_eq!(edge.unwrap().source_port_id, "out");
        assert_eq!(edge.unwrap().target_node_id, "func_2");
    }

    #[test]
    fn find_next_edge_开始节点应找到out端口边() {
        let flow = make_linear_flow();
        let navigator = FlowNavigator::new(&flow);

        let edge = navigator.find_next_edge("start_1", "out");
        assert!(edge.is_some());
        assert_eq!(edge.unwrap().target_node_id, "func_1");
    }

    #[test]
    fn find_next_edge_端口不匹配时返回none() {
        let flow = make_linear_flow();
        let navigator = FlowNavigator::new(&flow);

        // 普通节点没有 out_1 端口（那是 switch 节点的端口）
        assert!(navigator.find_next_edge("func_1", "out_1").is_none());
    }

    #[test]
    fn find_next_edge_节点不存在时返回none() {
        let flow = make_linear_flow();
        let navigator = FlowNavigator::new(&flow);

        assert!(navigator.find_next_edge("not_exists", "out").is_none());
    }

    #[test]
    fn find_next_edge_switch节点的分支端口() {
        // 构造带 switch 节点的流程
        let flow = ServiceFlow {
            nodes: vec![
                make_start_node("start_1"),
                make_func_node("switch_1", "路由", "p", "route"),
                make_func_node("branch_a", "分支A", "p", "a"),
                make_func_node("branch_b", "分支B", "p", "b"),
            ],
            edges: vec![
                make_edge("start_1", "out", "switch_1"),
                make_edge("switch_1", "out_1", "branch_a"),
                make_edge("switch_1", "out_2", "branch_b"),
            ],
        };
        let navigator = FlowNavigator::new(&flow);

        // out_1 端口应指向 branch_a
        let edge_a = navigator.find_next_edge("switch_1", "out_1");
        assert!(edge_a.is_some());
        assert_eq!(edge_a.unwrap().target_node_id, "branch_a");

        // out_2 端口应指向 branch_b
        let edge_b = navigator.find_next_edge("switch_1", "out_2");
        assert!(edge_b.is_some());
        assert_eq!(edge_b.unwrap().target_node_id, "branch_b");

        // 不存在的 out_3 端口应返回 None
        assert!(navigator.find_next_edge("switch_1", "out_3").is_none());
    }

    // ==================== resolve_transaction_db_id 测试 ====================

    #[test]
    fn resolve_transaction_db_id_节点指定database_id时返回指定值() {
        let flow = ServiceFlow {
            nodes: vec![make_txn_node("txn_1", "事务框1", Some("db_secondary"))],
            edges: vec![],
        };
        let navigator = FlowNavigator::new(&flow);

        let db_id = navigator.resolve_transaction_db_id("txn_1", "db_default");
        assert_eq!(db_id, "db_secondary");
    }

    #[test]
    fn resolve_transaction_db_id_节点未指定database_id时返回默认值() {
        let flow = ServiceFlow {
            nodes: vec![make_txn_node("txn_1", "事务框1", None)],
            edges: vec![],
        };
        let navigator = FlowNavigator::new(&flow);

        let db_id = navigator.resolve_transaction_db_id("txn_1", "db_default");
        assert_eq!(db_id, "db_default");
    }

    #[test]
    fn resolve_transaction_db_id_事务框节点不存在时返回默认值() {
        let flow = ServiceFlow {
            nodes: vec![make_func_node("func_1", "步骤1", "p", "f")],
            edges: vec![],
        };
        let navigator = FlowNavigator::new(&flow);

        // txn_not_exist 不在节点列表中
        let db_id = navigator.resolve_transaction_db_id("txn_not_exist", "db_default");
        assert_eq!(db_id, "db_default");
    }

    #[test]
    fn resolve_transaction_db_id_节点id匹配但类型不是事务框时返回默认值() {
        // 节点 ID 匹配但类型是 func 而非 transaction
        let flow = ServiceFlow {
            nodes: vec![make_func_node("txn_1", "伪装的事务框", "p", "f")],
            edges: vec![],
        };
        let navigator = FlowNavigator::new(&flow);

        let db_id = navigator.resolve_transaction_db_id("txn_1", "db_default");
        assert_eq!(db_id, "db_default");
    }

    #[test]
    fn resolve_transaction_db_id_事务框节点缺少data时返回默认值() {
        let flow = ServiceFlow {
            nodes: vec![ServiceNode {
                id: "txn_1".to_string(),
                node_type: "skylake-transaction".to_string(),
                parent: None,
                meta: make_meta(),
                data: None, // 缺少 data
            }],
            edges: vec![],
        };
        let navigator = FlowNavigator::new(&flow);

        let db_id = navigator.resolve_transaction_db_id("txn_1", "db_default");
        assert_eq!(db_id, "db_default");
    }
}
