use crate::host_traits::MockHostFunctions;
use crate::models::*;
use crate::core::PluginCore;
use cmx_plugin_sdk::{FunctionInput, SVRContext, DbResponse, CacheResponse};
use std::collections::HashMap;

fn make_input(input_value: serde_json::Value) -> FunctionInput {
    FunctionInput {
        input: input_value,
        context: SVRContext::new(
            serde_json::Value::Null,
            HashMap::new(),
            chrono::Utc::now(),
            "test-request-id".to_string(),
        ),
        binary_data: HashMap::new(),
    }
}

fn make_input_with_context(input_value: serde_json::Value) -> FunctionInput {
    let mut context = SVRContext::new(
        serde_json::Value::Null,
        HashMap::new(),
        chrono::Utc::now(),
        "test-request-id".to_string(),
    );
    context.add_step_output("branch_1_func".to_string(), serde_json::json!({"branch": "1"}));
    context.add_step_output("merge_func".to_string(), serde_json::json!({"merged": true}));
    FunctionInput {
        input: input_value,
        context,
        binary_data: HashMap::new(),
    }
}

#[test]
fn test_count_vowels() {
    let mock = MockHostFunctions::new();
    let core = PluginCore::new(mock);
    let input = make_input(serde_json::json!("hello world"));
    let result = core.count_vowels(&input).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&result.result.to_string()).unwrap_or_default();
    assert_eq!(parsed["count"], 3);
}

#[test]
fn test_demo_log() {
    let mut mock = MockHostFunctions::new();
    mock.expect_log_info().returning(|_| Ok(()));
    mock.expect_log_error().returning(|_| Ok(()));
    mock.expect_log_debug().returning(|_| Ok(()));
    mock.expect_log_warn().returning(|_| Ok(()));
    let core = PluginCore::new(mock);
    let input = make_input(serde_json::json!(null));
    let result = core.demo_log(&input).unwrap();
    let parsed: DemoResponse = serde_json::from_value(result.result).unwrap_or_default();
    assert_eq!(parsed.message, "日志记录完成");
    assert_eq!(parsed.total, 4);
}

#[test]
fn test_demo_cache() {
    let mut mock = MockHostFunctions::new();
    mock.expect_cache_set()
        .returning(|_, _, _| Ok(CacheResponse { success: true, value: None, exists: None, error: None }));
    mock.expect_cache_get()
        .returning(|_| Ok(CacheResponse { success: true, value: Some(serde_json::json!("100")), exists: Some(true), error: None }));
    mock.expect_log_info()
        .returning(|_| Ok(()));
    let core = PluginCore::new(mock);
    let input = make_input(serde_json::json!({"name": "test", "count": 100}));
    let result = core.demo_cache(&input).unwrap();
    let parsed: DemoResponse = serde_json::from_value(result.result).unwrap_or_default();
    assert_eq!(parsed.total, 100);
}

#[test]
fn test_demo_database() {
    let mut mock = MockHostFunctions::new();
    mock.expect_db_query()
        .returning(|_| Ok(DbResponse {
            success: true,
            dataset: None,
            affected_rows: None,
            txn_id: None,
            error: None,
        }));
    mock.expect_log_info()
        .returning(|_| Ok(()));
    let core = PluginCore::new(mock);
    let input = make_input(serde_json::json!({"name": "test", "count": 1}));
    let result = core.demo_database(&input).unwrap();
    let parsed: DemoResponse = serde_json::from_value(result.result).unwrap_or_default();
    assert!(parsed.message.contains("数据库查询成功"));
}

#[test]
fn test_route_check_default() {
    let mut mock = MockHostFunctions::new();
    mock.expect_log_info()
        .returning(|_| Ok(()));
    let core = PluginCore::new(mock);
    let input = make_input(serde_json::json!({"route": "2"}));
    let result = core.route_check(&input).unwrap();
    let parsed: serde_json::Value = serde_json::from_value(result.result).unwrap_or_default();
    assert_eq!(parsed, "2");
}

#[test]
fn test_route_check_unknown() {
    let mut mock = MockHostFunctions::new();
    mock.expect_log_info()
        .returning(|_| Ok(()));
    let core = PluginCore::new(mock);
    let input = make_input(serde_json::json!({"route": "unknown"}));
    let result = core.route_check(&input).unwrap();
    let parsed: serde_json::Value = serde_json::from_value(result.result).unwrap_or_default();
    assert_eq!(parsed, "1");
}

#[test]
fn test_branch_1_process() {
    let mut mock = MockHostFunctions::new();
    mock.expect_log_info()
        .returning(|_| Ok(()));
    let core = PluginCore::new(mock);
    let input = make_input(serde_json::json!("test_data"));
    let result = core.branch_1_process(&input).unwrap();
    let parsed: serde_json::Value = serde_json::from_value(result.result).unwrap_or_default();
    assert_eq!(parsed["branch"], "1");
}

#[test]
fn test_merge_result() {
    let mut mock = MockHostFunctions::new();
    mock.expect_log_info()
        .returning(|_| Ok(()));
    let core = PluginCore::new(mock);
    let input = make_input_with_context(serde_json::json!("test_data"));
    let result = core.merge_result(&input).unwrap();
    let parsed: serde_json::Value = serde_json::from_value(result.result).unwrap_or_default();
    assert_eq!(parsed["merged"], true);
}