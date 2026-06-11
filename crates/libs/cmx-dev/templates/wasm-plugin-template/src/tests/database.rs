use crate::handlers::PluginCore;
use crate::host::MockHostFunctions;
use crate::tests::make_input;
use cmx_plugin_sdk::DbResponse;

#[test]
fn test_query_orders() {
    let mut mock = MockHostFunctions::new();
    mock.expect_db_query()
        .returning(|_| {
            Ok(DbResponse {
                success: true,
                dataset: None,
                affected_rows: None,
                txn_id: None,
                error: None,
            })
        });
    let core = PluginCore::new(mock);
    let input = make_input(serde_json::json!({"customer_name": "Alice"}));
    let result = core.query_orders(&input).unwrap();
    let parsed: serde_json::Value = serde_json::from_value(result.result).unwrap_or_default();
    assert_eq!(parsed["success"], true);
}

#[test]
fn test_create_order() {
    let mut mock = MockHostFunctions::new();
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
    mock.expect_log_info()
        .returning(|_| Ok(()));
    let core = PluginCore::new(mock);
    let input = make_input(serde_json::json!({
        "customer_name": "Alice",
        "product_name": "Widget",
        "quantity": 10,
        "unit_price": 99.9
    }));
    let result = core.create_order(&input).unwrap();
    let parsed: serde_json::Value = serde_json::from_value(result.result).unwrap_or_default();
    assert_eq!(parsed["success"], true);
    assert_eq!(parsed["affected_rows"], 1);
}

#[test]
fn test_update_order() {
    let mut mock = MockHostFunctions::new();
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
        "order_id": "ORD-001",
        "status": "shipped"
    }));
    let result = core.update_order(&input).unwrap();
    let parsed: serde_json::Value = serde_json::from_value(result.result).unwrap_or_default();
    assert_eq!(parsed["success"], true);
}

#[test]
fn test_delete_order() {
    let mut mock = MockHostFunctions::new();
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
    mock.expect_log_info()
        .returning(|_| Ok(()));
    let core = PluginCore::new(mock);
    let input = make_input(serde_json::json!("ORD-001"));
    let result = core.delete_order(&input).unwrap();
    let parsed: serde_json::Value = serde_json::from_value(result.result).unwrap_or_default();
    assert_eq!(parsed["success"], true);
}
