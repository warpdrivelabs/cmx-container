use cmx_plugin_sdk::{FunctionInput, SVRContext};
use std::collections::HashMap;

/// 创建测试用 FunctionInput（initial_input 与 input 相同）。
pub fn make_input(input_value: serde_json::Value) -> FunctionInput {
    FunctionInput {
        input: input_value.clone(),
        context: SVRContext::new(
            input_value,
            HashMap::new(),
            chrono::Utc::now(),
            "test-request-id".to_string(),
        ),
        binary_data: HashMap::new(),
    }
}

/// 创建测试用 FunctionInput，指定 initial_input（用于服务编排场景）。
pub fn make_input_with_initial(
    input_value: serde_json::Value,
    initial_input: serde_json::Value,
) -> FunctionInput {
    FunctionInput {
        input: input_value,
        context: SVRContext::new(
            initial_input,
            HashMap::new(),
            chrono::Utc::now(),
            "test-request-id".to_string(),
        ),
        binary_data: HashMap::new(),
    }
}

/// 创建带上下文步骤输出的测试用 FunctionInput。
pub fn make_input_with_steps(
    input_value: serde_json::Value,
    steps: Vec<(&str, serde_json::Value)>,
) -> FunctionInput {
    let mut context = SVRContext::new(
        input_value.clone(),
        HashMap::new(),
        chrono::Utc::now(),
        "test-request-id".to_string(),
    );
    for (key, value) in steps {
        context.add_step_output(key.to_string(), value);
    }
    FunctionInput {
        input: input_value,
        context,
        binary_data: HashMap::new(),
    }
}

pub mod basic;
pub mod cache;
pub mod database;
pub mod orchestration;
