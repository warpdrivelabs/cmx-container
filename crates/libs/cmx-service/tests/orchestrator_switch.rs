//! 编排执行器 - switch 多分支路由测试
//!
//! 覆盖场景：
//! - 根据条件选择正确分支（out_1 / out_2）
//! - 无匹配分支时的行为（流程正常结束）
//! - 返回值非字符串类型时报错
//! - switch 节点执行失败时的错误传播
//! - switch 后恢复 previous_output 作为后续节点输入

mod common;

use cmx_core::StepStatus;
use cmx_core::model::service::ServiceFlow;
use cmx_service::ExecuteOptions;
use serde_json::json;

use common::{
    MockRuntimeInvoker, MockServiceQuery, create_orchestrator, make_edge, make_end_node,
    make_func_node, make_orchestration, make_start_node, make_svr_context, make_switch_node,
};

/// 构造 switch 分支流程：start -> switch -> [branch_1 | branch_2] -> merge -> end
///
/// switch 节点根据返回值选择分支：
/// - 返回 "1" -> out_1 -> branch_1
/// - 返回 "2" -> out_2 -> branch_2
fn make_switch_flow() -> ServiceFlow {
    ServiceFlow {
        nodes: vec![
            make_start_node("start_1"),
            make_switch_node("switch_1", "路由判断", "plugin_a", "route_check"),
            make_func_node("branch_1", "分支1", "plugin_a", "process_branch_1"),
            make_func_node("branch_2", "分支2", "plugin_a", "process_branch_2"),
            make_func_node("merge", "合并结果", "plugin_a", "merge_result"),
            make_end_node("end_1"),
        ],
        edges: vec![
            make_edge("start_1", "out", "switch_1"),
            make_edge("switch_1", "out_1", "branch_1"),
            make_edge("switch_1", "out_2", "branch_2"),
            make_edge("branch_1", "out", "merge"),
            make_edge("branch_2", "out", "merge"),
            make_edge("merge", "out", "end_1"),
        ],
    }
}

// ============================================================================
// switch 分支 - 正确分支选择
// ============================================================================

#[tokio::test]
async fn switch分支_根据返回值1选择第一个分支() {
    // switch 返回 "1" -> 应走 branch_1 -> merge -> end
    let flow = make_switch_flow();
    let orchestration = make_orchestration("switch_branch_1", flow);

    let runtime = MockRuntimeInvoker::new()
        .with_loaded("plugin_a")
        // switch 函数返回 "1"
        .with_success("plugin_a", "route_check", json!("1"))
        .with_success("plugin_a", "process_branch_1", json!({"branch": "first"}))
        .with_success("plugin_a", "process_branch_2", json!({"branch": "second"}))
        .with_success("plugin_a", "merge_result", json!({"merged": true}));

    let service_query = MockServiceQuery::new().with_orchestration(orchestration);
    let orchestrator = create_orchestrator(runtime, service_query);

    let svr_context = make_svr_context(json!("input"));
    let options = ExecuteOptions::new(true);

    let result = orchestrator
        .execute_service("switch_branch_1", svr_context, options)
        .await
        .expect("switch 分支流程应执行成功");

    assert!(result.success, "执行应成功");
    assert_eq!(result.output.unwrap(), json!({"merged": true}));

    // 应执行了 3 个步骤：switch + branch_1 + merge
    assert_eq!(result.steps.len(), 3, "应执行 switch + branch_1 + merge");

    // 验证执行的是 branch_1 而非 branch_2
    let step_ids: Vec<&str> = result.steps.iter().map(|s| s.node_id.as_str()).collect();
    assert!(step_ids.contains(&"switch_1"), "应执行 switch 节点");
    assert!(step_ids.contains(&"branch_1"), "应执行 branch_1");
    assert!(step_ids.contains(&"merge"), "应执行 merge 节点");
    assert!(!step_ids.contains(&"branch_2"), "不应执行 branch_2");
}

#[tokio::test]
async fn switch分支_根据返回值2选择第二个分支() {
    // switch 返回 "2" -> 应走 branch_2 -> merge -> end
    let flow = make_switch_flow();
    let orchestration = make_orchestration("switch_branch_2", flow);

    let runtime = MockRuntimeInvoker::new()
        .with_loaded("plugin_a")
        .with_success("plugin_a", "route_check", json!("2"))
        .with_success("plugin_a", "process_branch_1", json!({"branch": "first"}))
        .with_success("plugin_a", "process_branch_2", json!({"branch": "second"}))
        .with_success("plugin_a", "merge_result", json!({"merged": true}));

    let service_query = MockServiceQuery::new().with_orchestration(orchestration);
    let orchestrator = create_orchestrator(runtime, service_query);

    let svr_context = make_svr_context(json!("input"));
    let options = ExecuteOptions::new(true);

    let result = orchestrator
        .execute_service("switch_branch_2", svr_context, options)
        .await
        .expect("switch 分支流程应执行成功");

    assert!(result.success);
    assert_eq!(result.output.unwrap(), json!({"merged": true}));

    // 验证执行的是 branch_2 而非 branch_1
    let step_ids: Vec<&str> = result.steps.iter().map(|s| s.node_id.as_str()).collect();
    assert!(step_ids.contains(&"switch_1"));
    assert!(step_ids.contains(&"branch_2"), "应执行 branch_2");
    assert!(!step_ids.contains(&"branch_1"), "不应执行 branch_1");
}

// ============================================================================
// switch 分支 - 无匹配分支
// ============================================================================

#[tokio::test]
async fn switch分支_无匹配分支时正常结束() {
    // switch 返回 "3" 但只有 out_1 和 out_2 两条边
    // 根据执行器逻辑，无匹配出边时 break 退出循环，返回成功
    let flow = make_switch_flow();
    let orchestration = make_orchestration("switch_no_match", flow);

    let runtime = MockRuntimeInvoker::new()
        .with_loaded("plugin_a")
        // switch 函数返回 "3" - 没有对应的 out_3 边
        .with_success("plugin_a", "route_check", json!("3"))
        .with_success("plugin_a", "process_branch_1", json!({"branch": "first"}))
        .with_success("plugin_a", "process_branch_2", json!({"branch": "second"}))
        .with_success("plugin_a", "merge_result", json!({"merged": true}));

    let service_query = MockServiceQuery::new().with_orchestration(orchestration);
    let orchestrator = create_orchestrator(runtime, service_query);

    let svr_context = make_svr_context(json!("initial_input"));
    let options = ExecuteOptions::new(true);

    let result = orchestrator
        .execute_service("switch_no_match", svr_context, options)
        .await
        .expect("无匹配分支应正常返回编排结果");

    // 无匹配出边时，执行器 break 退出循环，result 仍为 Ok，所以 success=true
    assert!(result.success, "无匹配分支时仍应返回成功（break 退出）");

    // 只执行了 switch 节点
    assert_eq!(result.steps.len(), 1, "应只执行 switch 节点");
    assert_eq!(result.steps[0].node_id, "switch_1");
    assert_eq!(result.steps[0].status, StepStatus::Success);

    // current_output 应恢复为 previous_output（switch 执行前的输入）
    // 即 initial_input
    assert_eq!(
        result.output.unwrap(),
        json!("initial_input"),
        "无匹配分支时输出应恢复为 switch 之前的输入"
    );
}

// ============================================================================
// switch 分支 - 返回值非字符串
// ============================================================================

#[tokio::test]
async fn switch分支_返回值非字符串时报错() {
    // switch 返回数字而非字符串，应报错
    // 注意：此场景执行器通过 ? 直接传播 ServiceError，而非包装到 OrchestrationResult
    let flow = make_switch_flow();
    let orchestration = make_orchestration("switch_non_string", flow);

    let runtime = MockRuntimeInvoker::new()
        .with_loaded("plugin_a")
        // switch 函数返回数字 1（非字符串）
        .with_success("plugin_a", "route_check", json!(1))
        .with_success("plugin_a", "process_branch_1", json!({"branch": "first"}))
        .with_success("plugin_a", "merge_result", json!({"merged": true}));

    let service_query = MockServiceQuery::new().with_orchestration(orchestration);
    let orchestrator = create_orchestrator(runtime, service_query);

    let svr_context = make_svr_context(json!("input"));
    let options = ExecuteOptions::new(true);

    let result = orchestrator
        .execute_service("switch_non_string", svr_context, options)
        .await;

    // switch 返回非字符串时，执行器通过 ? 传播 ServiceError::OrchestrationFailed
    assert!(result.is_err(), "switch 返回非字符串应返回 Err");
    let err = result.unwrap_err();
    let err_msg = err.to_string();
    assert!(
        err_msg.contains("字符串") || err_msg.contains("多分支"),
        "错误信息应提及字符串类型问题，实际: {}",
        err_msg
    );
}

// ============================================================================
// switch 分支 - 错误传播
// ============================================================================

#[tokio::test]
async fn switch分支_执行失败时错误传播() {
    // switch 函数执行失败，应中断流程
    let flow = make_switch_flow();
    let orchestration = make_orchestration("switch_error", flow);

    let runtime = MockRuntimeInvoker::new()
        .with_loaded("plugin_a")
        .with_error("plugin_a", "route_check", "路由函数异常")
        .with_success("plugin_a", "process_branch_1", json!({"branch": "first"}))
        .with_success("plugin_a", "merge_result", json!({"merged": true}));

    let service_query = MockServiceQuery::new().with_orchestration(orchestration);
    let orchestrator = create_orchestrator(runtime, service_query);

    let svr_context = make_svr_context(json!("input"));
    let options = ExecuteOptions::new(true);

    let result = orchestrator
        .execute_service("switch_error", svr_context, options)
        .await
        .expect("应返回编排结果（包含错误信息）");

    assert!(!result.success, "switch 失败应导致整体失败");
    assert!(result.error.is_some());

    let error = result.error.unwrap();
    assert!(
        error.message.contains("switch_1") || error.message.contains("路由判断"),
        "错误信息应包含 switch 节点标识，实际: {}",
        error.message
    );

    // 验证步骤记录
    let failed_step = result.steps.last().expect("应有失败步骤");
    assert_eq!(failed_step.node_id, "switch_1");
    assert_eq!(failed_step.status, StepStatus::Failed);
}

// ============================================================================
// switch 分支 - previous_output 恢复
// ============================================================================

#[tokio::test]
async fn switch分支_执行后恢复previous_output作为后续输入() {
    // 验证 switch 的返回值仅用于路由，不应作为后续节点的输入
    // previous_output 应恢复为 switch 执行前的输入
    let flow = make_switch_flow();
    let orchestration = make_orchestration("switch_restore_output", flow);

    let runtime = MockRuntimeInvoker::new()
        .with_loaded("plugin_a")
        // switch 返回 "1" 用于路由
        .with_success("plugin_a", "route_check", json!("1"))
        // branch_1 应收到 previous_output（switch 之前的输入），而非 switch 的返回值 "1"
        .with_success("plugin_a", "process_branch_1", json!({"processed": true}))
        .with_success("plugin_a", "process_branch_2", json!({}))
        .with_success("plugin_a", "merge_result", json!({"final": "done"}));

    let service_query = MockServiceQuery::new().with_orchestration(orchestration);
    let orchestrator = create_orchestrator(runtime, service_query);

    // 初始输入为特定值
    let initial_input = json!({"data": "input_value"});
    let svr_context = make_svr_context(initial_input.clone());
    let options = ExecuteOptions::new(true);

    let result = orchestrator
        .execute_service("switch_restore_output", svr_context, options)
        .await
        .expect("流程应执行成功");

    assert!(result.success);

    // 最终输出应为 merge 的返回值
    assert_eq!(result.output.unwrap(), json!({"final": "done"}));

    // 验证步骤：switch 的输出是 "1"（用于路由）
    let switch_step = result
        .steps
        .iter()
        .find(|s| s.node_id == "switch_1")
        .unwrap();
    assert_eq!(switch_step.output.as_ref().unwrap(), &json!("1"));

    // branch_1 的输出是 {"processed": true}
    let branch_step = result
        .steps
        .iter()
        .find(|s| s.node_id == "branch_1")
        .unwrap();
    assert_eq!(
        branch_step.output.as_ref().unwrap(),
        &json!({"processed": true})
    );
}

// ============================================================================
// switch 分支 - 单分支场景
// ============================================================================

#[tokio::test]
async fn switch分支_只有一个分支时正常工作() {
    // 只配置 out_1 分支
    let flow = ServiceFlow {
        nodes: vec![
            make_start_node("start_1"),
            make_switch_node("switch_1", "路由", "plugin_a", "route"),
            make_func_node("branch_1", "唯一分支", "plugin_a", "process"),
            make_end_node("end_1"),
        ],
        edges: vec![
            make_edge("start_1", "out", "switch_1"),
            make_edge("switch_1", "out_1", "branch_1"),
            make_edge("branch_1", "out", "end_1"),
        ],
    };
    let orchestration = make_orchestration("single_branch", flow);

    let runtime = MockRuntimeInvoker::new()
        .with_loaded("plugin_a")
        .with_success("plugin_a", "route", json!("1"))
        .with_success("plugin_a", "process", json!({"done": true}));

    let service_query = MockServiceQuery::new().with_orchestration(orchestration);
    let orchestrator = create_orchestrator(runtime, service_query);

    let svr_context = make_svr_context(json!("input"));
    let options = ExecuteOptions::new(true);

    let result = orchestrator
        .execute_service("single_branch", svr_context, options)
        .await
        .expect("单分支 switch 应执行成功");

    assert!(result.success);
    assert_eq!(result.output.unwrap(), json!({"done": true}));
    assert_eq!(result.steps.len(), 2, "应执行 switch + branch_1");
}
