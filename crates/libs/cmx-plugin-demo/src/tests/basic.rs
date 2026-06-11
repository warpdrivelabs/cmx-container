use crate::handlers::PluginCore;
use crate::host::MockHostFunctions;
use crate::tests::make_input;

#[test]
fn test_greet() {
    let mock = MockHostFunctions::new();
    let core = PluginCore::new(mock);
    let input = make_input(serde_json::json!("Alice"));
    let result = core.greet(&input).unwrap();
    let parsed: serde_json::Value = serde_json::from_value(result.result).unwrap_or_default();
    assert_eq!(parsed["message"], "Hello, Alice!");
}

#[test]
fn test_greet_default() {
    let mock = MockHostFunctions::new();
    let core = PluginCore::new(mock);
    let input = make_input(serde_json::json!(null));
    let result = core.greet(&input).unwrap();
    let parsed: serde_json::Value = serde_json::from_value(result.result).unwrap_or_default();
    assert_eq!(parsed["message"], "Hello, World!");
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
    let parsed: serde_json::Value = serde_json::from_value(result.result).unwrap_or_default();
    assert_eq!(parsed["success"], true);
    assert_eq!(parsed["message"], "四级日志记录完成");
}
