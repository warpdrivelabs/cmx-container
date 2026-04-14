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
        self.flow.nodes.iter().find(|n| n.node_type == "skylake-start")
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
        self.flow.edges.iter().find(|e| {
            e.source_node_id == source_node_id && e.source_port_id == source_port
        })
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
        if let Some(txn_node) = self.flow.nodes.iter().find(|n| {
            n.id == txn_node_id && n.node_type == "skylake-transaction"
        }) {
            // 从节点元数据中提取 database_id
            // 链式调用：data -> node_meta -> database_id
            txn_node.data.as_ref()
                .and_then(|d| d.node_meta.as_ref())
                .and_then(|m| m.database_id.clone())
                .unwrap_or_else(|| {
                    // 未指定 database_id，使用默认值并记录日志
                    debug!("事务框节点未指定 database_id，使用默认值: txn_node_id={}, default={}", txn_node_id, default_db_id);
                    default_db_id.to_string()
                })
        } else {
            // 未找到事务框节点（配置错误），使用默认值
            debug!("未找到事务框节点，使用默认数据库ID: txn_node_id={}, default={}", txn_node_id, default_db_id);
            default_db_id.to_string()
        }
    }
}
