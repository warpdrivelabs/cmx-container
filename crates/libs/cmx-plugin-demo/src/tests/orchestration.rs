use crate::handlers::PluginCore;
use crate::host::MockHostFunctions;
use crate::tests::{make_input, make_input_with_initial, make_input_with_steps};
use cmx_plugin_sdk::{CacheResponse, DbResponse};

#[test]
fn test_check_order_amount_high_value() {
    let mut mock = MockHostFunctions::new();
    mock.expect_log_info().returning(|_| Ok(()));
    let core = PluginCore::new(mock);
    let input = make_input(serde_json::json!({
        "customer_name": "Alice",
        "product_name": "Enterprise License",
        "quantity": 100,
        "unit_price": 999.0
    }));
    let result = core.check_order_amount(&input).unwrap();
    let parsed: serde_json::Value = serde_json::from_value(result.result).unwrap_or_default();
    assert_eq!(parsed, "high_value");
}

#[test]
fn test_check_order_amount_normal() {
    let mut mock = MockHostFunctions::new();
    mock.expect_log_info().returning(|_| Ok(()));
    let core = PluginCore::new(mock);
    let input = make_input(serde_json::json!({
        "customer_name": "Bob",
        "product_name": "Standard License",
        "quantity": 5,
        "unit_price": 499.0
    }));
    let result = core.check_order_amount(&input).unwrap();
    let parsed: serde_json::Value = serde_json::from_value(result.result).unwrap_or_default();
    assert_eq!(parsed, "normal");
}

#[test]
fn test_tx_create_order() {
    let mut mock = MockHostFunctions::new();
    mock.expect_log_info().returning(|_| Ok(()));
    mock.expect_db_execute().returning(|_| {
        Ok(DbResponse {
            success: true,
            dataset: None,
            affected_rows: Some(1),
            txn_id: None,
            error: None,
        })
    });
    let core = PluginCore::new(mock);
    // 模拟编排场景：switch 节点后 current_output 自动恢复为初始输入
    let input = make_input(serde_json::json!({
        "customer_name": "Alice",
        "product_name": "Widget",
        "quantity": 10,
        "unit_price": 99.9
    }));
    let result = core.tx_create_order(&input).unwrap();
    let parsed: serde_json::Value = serde_json::from_value(result.result).unwrap_or_default();
    assert_eq!(parsed["operation"], "tx_create_order");
    assert!(parsed["order_id"].is_string());
}

#[test]
fn test_tx_update_stock() {
    let mut mock = MockHostFunctions::new();
    mock.expect_log_info().returning(|_| Ok(()));
    mock.expect_db_execute().returning(|_| {
        Ok(DbResponse {
            success: true,
            dataset: None,
            affected_rows: Some(1),
            txn_id: None,
            error: None,
        })
    });
    let core = PluginCore::new(mock);
    // 模拟编排场景：input 是 tx_create_order 的输出（不含库存字段），initial_input 是原始业务参数
    let input = make_input_with_initial(
        serde_json::json!({"operation": "tx_create_order", "order_id": "test-id", "affected_rows": 1}),
        serde_json::json!({
            "product_name": "Widget",
            "quantity": 5,
            "customer_name": "Alice",
            "unit_price": 99.9
        }),
    );
    let result = core.tx_update_stock(&input).unwrap();
    let parsed: serde_json::Value = serde_json::from_value(result.result).unwrap_or_default();
    assert_eq!(parsed["operation"], "tx_update_stock");
    assert_eq!(parsed["product_name"], "Widget");
}

#[test]
fn test_tx_record_approval() {
    let mut mock = MockHostFunctions::new();
    mock.expect_log_info().returning(|_| Ok(()));
    mock.expect_db_execute().returning(|_| {
        Ok(DbResponse {
            success: true,
            dataset: None,
            affected_rows: Some(1),
            txn_id: None,
            error: None,
        })
    });
    let core = PluginCore::new(mock);
    // 模拟编排场景：input 是 tx_update_stock 的输出，initial_input 含客户名称，step_outputs 含 order_id
    let input = make_input_with_steps(
        serde_json::json!({"operation": "tx_update_stock", "affected_rows": 1}),
        vec![(
            "tx_create_order_hv",
            serde_json::json!({"operation": "tx_create_order", "order_id": "order-123", "affected_rows": 1}),
        )],
    );
    let result = core.tx_record_approval(&input).unwrap();
    let parsed: serde_json::Value = serde_json::from_value(result.result).unwrap_or_default();
    assert_eq!(parsed["operation"], "tx_record_approval");
    assert_eq!(parsed["order_id"], "order-123");
    assert!(parsed["approval_id"].is_string());
}

#[test]
fn test_final_process() {
    let mut mock = MockHostFunctions::new();
    mock.expect_log_info().returning(|_| Ok(()));
    mock.expect_cache_set().returning(|_, _, _| {
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
            (
                "tx_create_order_hv",
                serde_json::json!({"operation": "tx_create_order", "order_id": "order-123"}),
            ),
            (
                "tx_update_stock_hv",
                serde_json::json!({"operation": "tx_update_stock"}),
            ),
            (
                "tx_record_approval",
                serde_json::json!({"operation": "tx_record_approval", "approval_id": "approval-456"}),
            ),
        ],
    );
    let result = core.final_process(&input).unwrap();
    let parsed: serde_json::Value = serde_json::from_value(result.result).unwrap_or_default();
    assert_eq!(parsed["final"], true);
    assert_eq!(parsed["message"], "订单处理流程执行完成");
}
