//! 编排执行器 - 线性流程测试
//!
//! 覆盖场景：
//! - 线性流程多节点按顺序执行成功
//! - 节点执行失败时的错误传播
//! - 链式输出传递（上一个节点的输出作为下一个节点的输入）
//! - include_steps 选项对返回结果的影响

mod common;

use cmx_core::model::service::ServiceFlow;
use cmx_core::StepStatus;
use cmx_service::ExecuteOptions;
use serde_json::json;

use common::{
    create_orchestrator, make_edge, make_end_node, make_func_node, make_orchestration,
    make_start_node, make_svr_context, MockRuntimeInvoker, MockServiceQuery,
};

/// 构造线性流程：start -> func1 -> func2 -> end
fn make_linear_flow() -> ServiceFlow {
    ServiceFlow {
        nodes: vec![
            make_start_node("start_1"),
            make_func_node("func_1", "步骤1", "plugin_a", "process_1"),
            make_func_node("func_2", "步骤2", "plugin_a", "process_2"),
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
// 线性流程 - 成功执行
// ============================================================================

#[tokio::test]
async fn 线性流程_多节点按顺序执行成功() {
    // 流程：start -> func_1 -> func_2 -> end
    // func_1 返回 {"step": 1}, func_2 返回 {"step": 2}
    let flow = make_linear_flow();
    let orchestration = make_orchestration("linear_success", flow);

    let runtime = MockRuntimeInvoker::new()
        .with_loaded("plugin_a")
        .with_success("plugin_a", "process_1", json!({"step": 1}))
        .with_success("plugin_a", "process_2", json!({"step": 2}));

    let service_query = MockServiceQuery::new().with_orchestration(orchestration);
    let orchestrator = create_orchestrator(runtime, service_query);

    let svr_context = make_svr_context(json!({"input": "test"}));
    let options = ExecuteOptions::new(true); // 返回步骤数据

    let result = orchestrator
        .execute_service("linear_success", svr_context, options)
        .await
        .expect("线性流程应执行成功");

    assert!(result.success, "执行应成功");
    assert!(result.output.is_some(), "应有最终输出");
    assert_eq!(result.output.unwrap(), json!({"step": 2}), "最终输出应为 func_2 的返回值");
    assert_eq!(result.steps.len(), 2, "应记录 2 个步骤（func_1 和 func_2）");
    assert!(result.error.is_none(), "不应有错误");
    assert_eq!(result.debug_triggered, Some(false));
}

#[tokio::test]
async fn 线性流程_单节点流程执行成功() {
    // 流程：start -> func_1 -> end
    let flow = ServiceFlow {
        nodes: vec![
            make_start_node("start_1"),
            make_func_node("func_1", "步骤1", "plugin_a", "process_1"),
            make_end_node("end_1"),
        ],
        edges: vec![
            make_edge("start_1", "out", "func_1"),
            make_edge("func_1", "out", "end_1"),
        ],
    };
    let orchestration = make_orchestration("single_node", flow);

    let runtime = MockRuntimeInvoker::new()
        .with_loaded("plugin_a")
        .with_success("plugin_a", "process_1", json!({"result": "done"}));

    let service_query = MockServiceQuery::new().with_orchestration(orchestration);
    let orchestrator = create_orchestrator(runtime, service_query);

    let svr_context = make_svr_context(json!("input_data"));
    let options = ExecuteOptions::new(true);

    let result = orchestrator
        .execute_service("single_node", svr_context, options)
        .await
        .expect("单节点流程应执行成功");

    assert!(result.success);
    assert_eq!(result.output.unwrap(), json!({"result": "done"}));
    assert_eq!(result.steps.len(), 1);
}

#[tokio::test]
async fn 线性流程_链式输出传递() {
    // 验证上一个节点的输出作为下一个节点的输入（通过 mock 验证）
    // 这里通过让 func_2 返回包含 func_1 输出的数据来验证链式传递
    // 由于 mock 不读取输入，这里主要验证执行顺序和最终结果
    let flow = make_linear_flow();
    let orchestration = make_orchestration("chain_output", flow);

    let runtime = MockRuntimeInvoker::new()
        .with_loaded("plugin_a")
        .with_success("plugin_a", "process_1", json!({"value": "first"}))
        .with_success("plugin_a", "process_2", json!({"value": "second"}));

    let service_query = MockServiceQuery::new().with_orchestration(orchestration);
    let orchestrator = create_orchestrator(runtime, service_query);

    let svr_context = make_svr_context(json!({"initial": "data"}));
    let options = ExecuteOptions::new(true);

    let result = orchestrator
        .execute_service("chain_output", svr_context, options)
        .await
        .expect("链式输出传递流程应执行成功");

    // 最终输出应为最后一个节点的返回值
    assert_eq!(result.output.unwrap(), json!({"value": "second"}));

    // 验证步骤顺序
    assert_eq!(result.steps.len(), 2);
    assert_eq!(result.steps[0].node_id, "func_1");
    assert_eq!(result.steps[0].output.clone().unwrap(), json!({"value": "first"}));
    assert_eq!(result.steps[0].status, StepStatus::Success);
    assert_eq!(result.steps[1].node_id, "func_2");
    assert_eq!(result.steps[1].output.clone().unwrap(), json!({"value": "second"}));
    assert_eq!(result.steps[1].status, StepStatus::Success);
}

#[tokio::test]
async fn 线性流程_include_steps为false时不返回步骤数据() {
    // include_steps=false 时，成功执行后 steps 应为空数组
    let flow = make_linear_flow();
    let orchestration = make_orchestration("no_steps", flow);

    let runtime = MockRuntimeInvoker::new()
        .with_loaded("plugin_a")
        .with_success("plugin_a", "process_1", json!({"step": 1}))
        .with_success("plugin_a", "process_2", json!({"step": 2}));

    let service_query = MockServiceQuery::new().with_orchestration(orchestration);
    let orchestrator = create_orchestrator(runtime, service_query);

    let svr_context = make_svr_context(json!("input"));
    let options = ExecuteOptions::new(false); // 不返回步骤数据

    let result = orchestrator
        .execute_service("no_steps", svr_context, options)
        .await
        .expect("流程应执行成功");

    assert!(result.success);
    assert!(result.output.is_some());
    assert_eq!(result.steps.len(), 0, "include_steps=false 时 steps 应为空");
}

// ============================================================================
// 线性流程 - 错误传播
// ============================================================================

#[tokio::test]
async fn 线性流程_节点执行失败时错误传播() {
    // 流程：start -> func_1 -> func_2 -> end
    // func_1 执行失败，应中断流程并返回错误
    let flow = make_linear_flow();
    let orchestration = make_orchestration("error_propagation", flow);

    let runtime = MockRuntimeInvoker::new()
        .with_loaded("plugin_a")
        .with_error("plugin_a", "process_1", "函数执行异常")
        .with_success("plugin_a", "process_2", json!({"step": 2}));

    let service_query = MockServiceQuery::new().with_orchestration(orchestration);
    let orchestrator = create_orchestrator(runtime, service_query);

    let svr_context = make_svr_context(json!("input"));
    let options = ExecuteOptions::new(true);

    let result = orchestrator
        .execute_service("error_propagation", svr_context, options)
        .await
        .expect("即使节点失败，编排结果也应返回（包含错误信息）");

    assert!(!result.success, "执行应失败");
    assert!(result.output.is_none(), "失败时不应有最终输出");
    assert!(result.error.is_some(), "应有错误信息");

    let error = result.error.unwrap();
    assert!(
        error.message.contains("func_1") || error.message.contains("步骤1"),
        "错误信息应包含失败节点标识，实际: {}",
        error.message
    );
    assert!(
        error.message.contains("执行失败"),
        "错误信息应包含'执行失败'，实际: {}",
        error.message
    );
}

#[tokio::test]
async fn 线性流程_第一个节点失败时记录失败步骤() {
    // 验证失败时步骤记录包含失败节点的信息
    let flow = make_linear_flow();
    let orchestration = make_orchestration("first_node_fail", flow);

    let runtime = MockRuntimeInvoker::new()
        .with_loaded("plugin_a")
        .with_error("plugin_a", "process_1", "执行异常");

    let service_query = MockServiceQuery::new().with_orchestration(orchestration);
    let orchestrator = create_orchestrator(runtime, service_query);

    let svr_context = make_svr_context(json!({"input": "data"}));
    let options = ExecuteOptions::new(false); // 即使 include_steps=false，失败时也应返回步骤

    let result = orchestrator
        .execute_service("first_node_fail", svr_context, options)
        .await
        .expect("应返回编排结果");

    assert!(!result.success);

    // 失败时即使 include_steps=false 也应返回步骤数据（便于排错）
    assert!(!result.steps.is_empty(), "失败时应返回步骤数据用于排错");

    // 最后一个步骤应是失败步骤
    let failed_step = result.steps.last().expect("应有失败步骤");
    assert_eq!(failed_step.node_id, "func_1");
    assert_eq!(failed_step.status, StepStatus::Failed);
    assert!(failed_step.error.is_some(), "失败步骤应有错误信息");
    assert!(failed_step.previous_output.is_some(), "失败步骤应记录上一步输出");
}

#[tokio::test]
async fn 线性流程_中间节点失败时后续节点不执行() {
    // 流程：start -> func_1 -> func_2 -> end
    // func_1 成功，func_2 失败
    let flow = ServiceFlow {
        nodes: vec![
            make_start_node("start_1"),
            make_func_node("func_1", "步骤1", "plugin_a", "process_1"),
            make_func_node("func_2", "步骤2", "plugin_a", "process_2"),
            make_end_node("end_1"),
        ],
        edges: vec![
            make_edge("start_1", "out", "func_1"),
            make_edge("func_1", "out", "func_2"),
            make_edge("func_2", "out", "end_1"),
        ],
    };
    let orchestration = make_orchestration("middle_node_fail", flow);

    let runtime = MockRuntimeInvoker::new()
        .with_loaded("plugin_a")
        .with_success("plugin_a", "process_1", json!({"step": 1}))
        .with_error("plugin_a", "process_2", "第二步失败");

    let service_query = MockServiceQuery::new().with_orchestration(orchestration);
    let orchestrator = create_orchestrator(runtime, service_query);

    let svr_context = make_svr_context(json!("input"));
    let options = ExecuteOptions::new(true);

    let result = orchestrator
        .execute_service("middle_node_fail", svr_context, options)
        .await
        .expect("应返回编排结果");

    assert!(!result.success);

    // 应只执行了 func_1（成功）和 func_2（失败），end_1 未执行
    assert_eq!(result.steps.len(), 2, "应只有 2 个步骤记录");

    // 第一个步骤成功
    assert_eq!(result.steps[0].node_id, "func_1");
    assert_eq!(result.steps[0].status, StepStatus::Success);

    // 第二个步骤失败
    assert_eq!(result.steps[1].node_id, "func_2");
    assert_eq!(result.steps[1].status, StepStatus::Failed);
    assert!(result.steps[1].error.is_some());
}

// ============================================================================
// 边界情况
// ============================================================================

#[tokio::test]
async fn 线性流程_服务不存在时返回错误() {
    // service_key 不在 MockServiceQuery 中注册
    let runtime = MockRuntimeInvoker::new();
    let service_query = MockServiceQuery::new();
    let orchestrator = create_orchestrator(runtime, service_query);

    let svr_context = make_svr_context(json!("input"));
    let options = ExecuteOptions::new(true);

    let result = orchestrator
        .execute_service("not_exist_service", svr_context, options)
        .await;

    // 注意：MockServiceQuery.get_service 总是返回 Some(ServiceDefinition)
    // 但 get_orchestration 会返回 None，所以应失败在编排查找阶段
    assert!(result.is_err(), "服务编排不存在应返回 Err");
}

#[tokio::test]
async fn 线性流程_无开始节点时返回错误() {
    // 流程中没有 skylake-start 节点
    let flow = ServiceFlow {
        nodes: vec![
            make_func_node("func_1", "步骤1", "plugin_a", "process_1"),
            make_end_node("end_1"),
        ],
        edges: vec![
            make_edge("func_1", "out", "end_1"),
        ],
    };
    let orchestration = make_orchestration("no_start", flow);

    let runtime = MockRuntimeInvoker::new().with_loaded("plugin_a");
    let service_query = MockServiceQuery::new().with_orchestration(orchestration);
    let orchestrator = create_orchestrator(runtime, service_query);

    let svr_context = make_svr_context(json!("input"));
    let options = ExecuteOptions::new(true);

    let result = orchestrator
        .execute_service("no_start", svr_context, options)
        .await;

    assert!(result.is_err(), "无开始节点应返回 Err");
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("开始节点"),
        "错误信息应提及开始节点，实际: {}",
        err
    );
}
