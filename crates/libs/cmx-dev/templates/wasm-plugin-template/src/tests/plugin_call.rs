use crate::handlers::PluginCore;
use crate::host::MockHostFunctions;
use crate::tests::make_input;
use cmx_plugin_sdk::{PluginFunCallResponse, PluginFunRequest};

#[test]
fn test_check_inventory() {
    let mut mock = MockHostFunctions::new();
    mock.expect_call_plugin()
        .returning(|_: PluginFunRequest| {
            Ok(PluginFunCallResponse {
                success: true,
                result: None,
                error: None,
                elapsed_us: None,
            })
        });
    mock.expect_log_info()
        .returning(|_| Ok(()));
    let core = PluginCore::new(mock);
    let input = make_input(serde_json::json!({
        "product_name": "Widget",
        "quantity": 10
    }));
    let result = core.check_inventory(&input).unwrap();
    let parsed: serde_json::Value = serde_json::from_value(result.result).unwrap_or_default();
    assert_eq!(parsed["success"], true);
}

#[test]
fn test_check_remote_inventory() {
    let mut mock = MockHostFunctions::new();
    mock.expect_call_remote_plugin()
        .returning(|_, _: PluginFunRequest| {
            Ok(PluginFunCallResponse {
                success: true,
                result: None,
                error: None,
                elapsed_us: None,
            })
        });
    mock.expect_log_info()
        .returning(|_| Ok(()));
    let core = PluginCore::new(mock);
    let input = make_input(serde_json::json!({
        "product_name": "Widget",
        "quantity": 10
    }));
    let result = core.check_remote_inventory(&input).unwrap();
    let parsed: serde_json::Value = serde_json::from_value(result.result).unwrap_or_default();
    assert_eq!(parsed["success"], true);
}
