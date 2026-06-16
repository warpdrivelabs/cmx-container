use crate::handlers::PluginCore;
use crate::host::MockHostFunctions;
use crate::tests::make_input;
use cmx_plugin_sdk::CacheResponse;

#[test]
fn test_cache_order_status_update_request() {
    let mut mock = MockHostFunctions::new();
    mock.expect_cache_set()
        .returning(|_, _, _| {
            Ok(CacheResponse {
                success: true,
                value: None,
                exists: None,
                error: None,
            })
        });
    mock.expect_log_info()
        .returning(|_| Ok(()));
    let core = PluginCore::new(mock);
    let input = make_input(serde_json::json!({
        "order_id": "ORD-001",
        "status": "confirmed"
    }));
    let result = core.cache_order_status(&input).unwrap();
    let parsed: serde_json::Value = serde_json::from_value(result.result).unwrap_or_default();
    assert_eq!(parsed["success"], true);
    assert_eq!(parsed["message"], "订单 ORD-001 状态已缓存");
}

#[test]
fn test_cache_order_status_dataset() {
    let mut mock = MockHostFunctions::new();
    mock.expect_cache_set()
        .returning(|_, _, _| {
            Ok(CacheResponse {
                success: true,
                value: None,
                exists: None,
                error: None,
            })
        });
    mock.expect_log_info()
        .returning(|_| Ok(()));
    let core = PluginCore::new(mock);
    let input = make_input(serde_json::json!({
        "success": true,
        "dataset": {
            "columns": ["id", "customer_name", "status"],
            "rows": [
                ["ORD-001", "张三", "pending"],
                ["ORD-002", "李四", "confirmed"]
            ]
        }
    }));
    let result = core.cache_order_status(&input).unwrap();
    let parsed: serde_json::Value = serde_json::from_value(result.result).unwrap_or_default();
    assert_eq!(parsed["success"], true);
    assert_eq!(parsed["cached_count"], 2);
}

#[test]
fn test_cache_order_status_empty_dataset() {
    let mut mock = MockHostFunctions::new();
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
    let input = make_input(serde_json::json!({
        "success": true,
        "dataset": {
            "columns": ["id", "status"],
            "rows": []
        }
    }));
    let result = core.cache_order_status(&input).unwrap();
    let parsed: serde_json::Value = serde_json::from_value(result.result).unwrap_or_default();
    assert_eq!(parsed["success"], false);
    assert_eq!(parsed["message"], "无订单数据可缓存");
}

#[test]
fn test_get_cached_order() {
    let mut mock = MockHostFunctions::new();
    mock.expect_cache_get()
        .returning(|_| {
            Ok(CacheResponse {
                success: true,
                value: Some(serde_json::json!({"status": "confirmed"})),
                exists: Some(true),
                error: None,
            })
        });
    let core = PluginCore::new(mock);
    let input = make_input(serde_json::json!("ORD-001"));
    let result = core.get_cached_order(&input).unwrap();
    let parsed: serde_json::Value = serde_json::from_value(result.result).unwrap_or_default();
    assert_eq!(parsed["success"], true);
    assert_eq!(parsed["exists"], true);
}

#[test]
fn test_remove_order_cache() {
    let mut mock = MockHostFunctions::new();
    mock.expect_cache_delete()
        .returning(|_| {
            Ok(CacheResponse {
                success: true,
                value: None,
                exists: None,
                error: None,
            })
        });
    mock.expect_log_info()
        .returning(|_| Ok(()));
    let core = PluginCore::new(mock);
    let input = make_input(serde_json::json!("ORD-001"));
    let result = core.remove_order_cache(&input).unwrap();
    let parsed: serde_json::Value = serde_json::from_value(result.result).unwrap_or_default();
    assert_eq!(parsed["success"], true);
}
