use crate::handlers::PluginCore;
use crate::host::MockHostFunctions;
use crate::tests::{make_input, make_input_with_steps};
use cmx_plugin_sdk::{CacheResponse, DbResponse};

#[test]
fn test_route_check() {
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
fn test_route_check_default() {
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
fn test_branch_process() {
    let mut mock = MockHostFunctions::new();
    mock.expect_log_info()
        .returning(|_| Ok(()));
    let core = PluginCore::new(mock);
    let input = make_input(serde_json::json!({"branch": "2"}));
    let result = core.branch_process(&input).unwrap();
    let parsed: serde_json::Value = serde_json::from_value(result.result).unwrap_or_default();
    assert_eq!(parsed["branch"], "2");
}

#[test]
fn test_merge_result() {
    let mut mock = MockHostFunctions::new();
    mock.expect_log_info()
        .returning(|_| Ok(()));
    let core = PluginCore::new(mock);
    let input = make_input_with_steps(
        serde_json::json!("test_data"),
        vec![("branch_process", serde_json::json!({"branch": "1"}))],
    );
    let result = core.merge_result(&input).unwrap();
    let parsed: serde_json::Value = serde_json::from_value(result.result).unwrap_or_default();
    assert_eq!(parsed["merged"], true);
}

#[test]
fn test_tx_create_order() {
    let mut mock = MockHostFunctions::new();
    mock.expect_log_info()
        .returning(|_| Ok(()));
    mock.expect_db_execute()
        .returning(|_| {
            Ok(DbResponse {
                success: true,
                dataset: None,
                affected_rows: Some(1),
                txn_id: None,
                error: None,
            })
        });
    let core = PluginCore::new(mock);
    let input = make_input(serde_json::json!({
        "customer_name": "Alice",
        "product_name": "Widget",
        "quantity": 10,
        "unit_price": 99.9
    }));
    let result = core.tx_create_order(&input).unwrap();
    let parsed: serde_json::Value = serde_json::from_value(result.result).unwrap_or_default();
    assert_eq!(parsed["operation"], "tx_create_order");
}

#[test]
fn test_tx_update_stock() {
    let mut mock = MockHostFunctions::new();
    mock.expect_log_info()
        .returning(|_| Ok(()));
    mock.expect_db_execute()
        .returning(|_| {
            Ok(DbResponse {
                success: true,
                dataset: None,
                affected_rows: Some(1),
                txn_id: None,
                error: None,
            })
        });
    let core = PluginCore::new(mock);
    let input = make_input(serde_json::json!({
        "product_name": "Widget",
        "quantity": 5
    }));
    let result = core.tx_update_stock(&input).unwrap();
    let parsed: serde_json::Value = serde_json::from_value(result.result).unwrap_or_default();
    assert_eq!(parsed["operation"], "tx_update_stock");
}

#[test]
fn test_final_process() {
    let mut mock = MockHostFunctions::new();
    mock.expect_log_info()
        .returning(|_| Ok(()));
    mock.expect_cache_set()
        .returning(|_, _, _| {
            Ok(CacheResponse {
                success: true,
                value: None,
                exists: None,
                error: None,
            })
        });
    let core = PluginCore::new(mock);
    let input = make_input_with_steps(
        serde_json::json!("test_data"),
        vec![
            ("merge_result", serde_json::json!({"merged": true})),
            ("tx_create_order", serde_json::json!({"operation": "tx_create_order"})),
            ("tx_update_stock", serde_json::json!({"operation": "tx_update_stock"})),
        ],
    );
    let result = core.final_process(&input).unwrap();
    let parsed: serde_json::Value = serde_json::from_value(result.result).unwrap_or_default();
    assert_eq!(parsed["final"], true);
    assert_eq!(parsed["message"], "服务编排执行完成");
}
