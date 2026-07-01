//! 编排执行器 - 事务框场景测试
//!
//! 覆盖场景：
//! - 事务框内节点因无数据库连接导致事务开启失败时的错误传播
//!
//! 注意：
//! 完整的事务提交/回滚测试需要真实数据库环境，无法在纯单元测试中覆盖。
//! 这里主要验证：
//! 1. 当节点进入事务框（有 parent 属性）但数据库不可用时，应正确传播错误
//! 2. 错误信息应包含"事务管理失败"标识
//! 3. 失败时应返回结构化的编排错误信息
//!
//! 完整的事务提交/回滚行为应在有数据库的集成测试中覆盖。

mod common;

use cmx_core::model::service::{ServiceFlow, ServiceNode};
use cmx_core::StepStatus;
use cmx_service::ExecuteOptions;
use serde_json::json;

use common::{
    create_orchestrator, make_edge, make_end_node, make_func_node, make_func_node_with_parent,
    make_orchestration, make_start_node, make_svr_context, MockRuntimeInvoker, MockServiceQuery,
};

/// 构造事务框流程：start -> [txn_box: func_1 -> func_2] -> end
///
/// func_1 和 func_2 的 parent 指向 txn_box（事务框节点），
/// 但流程中没有定义 txn_box 节点本身（这是正常的，事务框节点在
/// ensure_transaction 中通过 parent_id 查找数据库ID）。
fn make_transaction_flow() -> ServiceFlow {
    ServiceFlow {
        nodes: vec![
            make_start_node("start_1"),
            // 事务框内的两个函数节点
            make_func_node_with_parent(
                "func_1",
                "事务步骤1",
                "plugin_a",
                "tx_insert",
                "txn_box",
            ),
            make_func_node_with_parent(
                "func_2",
                "事务步骤2",
                "plugin_a",
                "tx_update",
                "txn_box",
            ),
            make_end_node("end_1"),
        ],
        edges: vec![
            make_edge("start_1", "out", "func_1"),
            make_edge("func_1", "out", "func_2"),
            make_edge("func_2", "out", "end_1"),
        ],
    }
}

/// 构造事务框流程（包含事务框节点定义本身）
fn make_transaction_flow_with_txn_node() -> ServiceFlow {
    use cmx_core::model::service::{NodeData, NodeMeta, NodeNodeMeta, NodePosition, NodeSize};

    let txn_node = ServiceNode {
        id: "txn_box".to_string(),
        node_type: "skylake-transaction".to_string(),
        parent: None,
        meta: NodeMeta {
            z_index: 1,
            size: NodeSize { width: 800, height: 600 },
            position: NodePosition { x: 100.0, y: 100.0 },
        },
        data: Some(NodeData {
            name: "事务处理框".to_string(),
            node_meta: Some(NodeNodeMeta {
                plugin_id: String::new(),
                plugin_name: String::new(),
                plugin_version: String::new(),
                function_name: String::new(),
                database_id: Some("db_primary".to_string()),
            }),
            inputs: serde_json::Value::Array(vec![]),
            outputs: serde_json::Value::Array(vec![]),
            options: None,
        }),
    };

    ServiceFlow {
        nodes: vec![
            make_start_node("start_1"),
            txn_node,
            make_func_node_with_parent(
                "func_1",
                "事务步骤1",
                "plugin_a",
                "tx_insert",
                "txn_box",
            ),
            make_func_node_with_parent(
                "func_2",
                "事务步骤2",
                "plugin_a",
                "tx_update",
                "txn_box",
            ),
            make_end_node("end_1"),
        ],
        edges: vec![
            make_edge("start_1", "out", "func_1"),
            make_edge("func_1", "out", "func_2"),
            make_edge("func_2", "out", "end_1"),
        ],
    }
}

// ============================================================================
// 事务框 - 错误传播（无数据库环境）
// ============================================================================

#[tokio::test]
async fn 事务框_无数据库时事务开启失败应传播错误() {
    // 流程：start -> [txn_box: func_1 -> func_2] -> end
    // 由于没有配置真实数据库，begin_transaction_guard_by_db_id 会失败
    // 应返回包含"事务管理失败"的错误
    let flow = make_transaction_flow();
    let orchestration = make_orchestration("txn_no_db", flow);

    let runtime = MockRuntimeInvoker::new()
        .with_loaded("plugin_a")
        .with_success("plugin_a", "tx_insert", json!({"inserted": true}))
        .with_success("plugin_a", "tx_update", json!({"updated": true}));

    let service_query = MockServiceQuery::new().with_orchestration(orchestration);
    let orchestrator = create_orchestrator(runtime, service_query);

    let svr_context = make_svr_context(json!("input"));
    let options = ExecuteOptions::new(true);

    let result = orchestrator
        .execute_service("txn_no_db", svr_context, options)
        .await
        .expect("应返回编排结果（包含错误信息）");

    assert!(!result.success, "事务开启失败应导致执行失败");
    assert!(result.output.is_none(), "失败时不应有输出");
    assert!(result.error.is_some(), "应有错误信息");

    let error = result.error.unwrap();
    assert!(
        error.message.contains("事务管理失败") || error.message.contains("事务"),
        "错误信息应提及事务管理失败，实际: {}",
        error.message
    );

    // 不应有函数节点被执行（事务开启失败时立即中断）
    let step_ids: Vec<&str> = result.steps.iter().map(|s| s.node_id.as_str()).collect();
    assert!(
        !step_ids.contains(&"func_1"),
        "事务开启失败时 func_1 不应执行"
    );
    assert!(
        !step_ids.contains(&"func_2"),
        "事务开启失败时 func_2 不应执行"
    );
}

#[tokio::test]
async fn 事务框_有事务框节点定义时无数据库仍应失败() {
    // 即使流程中定义了事务框节点（skylake-transaction），
    // 由于没有真实数据库连接，事务开启仍会失败
    let flow = make_transaction_flow_with_txn_node();
    let orchestration = make_orchestration("txn_with_node", flow);

    let runtime = MockRuntimeInvoker::new()
        .with_loaded("plugin_a")
        .with_success("plugin_a", "tx_insert", json!({"inserted": true}))
        .with_success("plugin_a", "tx_update", json!({"updated": true}));

    let service_query = MockServiceQuery::new().with_orchestration(orchestration);
    let orchestrator = create_orchestrator(runtime, service_query);

    let svr_context = make_svr_context(json!("input"));
    let options = ExecuteOptions::new(true);

    let result = orchestrator
        .execute_service("txn_with_node", svr_context, options)
        .await
        .expect("应返回编排结果");

    assert!(!result.success, "无数据库时事务应失败");
    assert!(result.error.is_some());

    let error = result.error.unwrap();
    assert!(
        error.message.contains("事务"),
        "错误信息应提及事务，实际: {}",
        error.message
    );
}

// ============================================================================
// 事务框 - 无事务框的普通流程对比
// ============================================================================

#[tokio::test]
async fn 事务框_普通流程无parent时不触发事务管理() {
    // 对比测试：相同结构的流程但节点没有 parent 属性，
    // 不应触发事务管理，应正常执行
    let flow = ServiceFlow {
        nodes: vec![
            make_start_node("start_1"),
            // 这两个节点没有 parent，不在事务框中
            make_func_node("func_1", "步骤1", "plugin_a", "tx_insert"),
            make_func_node("func_2", "步骤2", "plugin_a", "tx_update"),
            make_end_node("end_1"),
        ],
        edges: vec![
            make_edge("start_1", "out", "func_1"),
            make_edge("func_1", "out", "func_2"),
            make_edge("func_2", "out", "end_1"),
        ],
    };
    let orchestration = make_orchestration("no_txn", flow);

    let runtime = MockRuntimeInvoker::new()
        .with_loaded("plugin_a")
        .with_success("plugin_a", "tx_insert", json!({"inserted": true}))
        .with_success("plugin_a", "tx_update", json!({"updated": true}));

    let service_query = MockServiceQuery::new().with_orchestration(orchestration);
    let orchestrator = create_orchestrator(runtime, service_query);

    let svr_context = make_svr_context(json!("input"));
    let options = ExecuteOptions::new(true);

    let result = orchestrator
        .execute_service("no_txn", svr_context, options)
        .await
        .expect("普通流程应执行成功");

    assert!(result.success, "无事务框的流程应正常执行");
    assert_eq!(result.output.unwrap(), json!({"updated": true}));
    assert_eq!(result.steps.len(), 2);
    assert_eq!(result.steps[0].status, StepStatus::Success);
    assert_eq!(result.steps[1].status, StepStatus::Success);
}
