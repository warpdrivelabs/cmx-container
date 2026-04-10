//! 编排执行器
//!
//! 解析和执行插件编排定义，支持步骤间数据传递和并行执行。

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tracing::{debug, warn};
use utoipa::ToSchema;

use cmx_traits::{PluginQuery, RuntimeInvoker, WasmInvokeResult};

use crate::error::ServiceError;
use crate::request::StepResult;

/// 编排步骤
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OrchestrationStep {
    /// 步骤ID
    pub step_id: String,
    /// 目标插件ID
    pub plugin_id: String,
    /// 目标函数名
    pub function_name: String,
    /// 输入数据（JSON 或引用前序步骤输出）
    pub input: StepInput,
    /// 是否并行执行（与前一步骤）
    pub parallel: bool,
    /// 条件表达式（可选，决定是否执行此步骤）
    #[serde(default)]
    pub condition: Option<String>,
}

/// 步骤输入定义
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
#[serde(tag = "type")]
#[schema(no_recursion)]
pub enum StepInput {
    /// 静态 JSON 值
    #[serde(rename = "static")]
    Static { value: serde_json::Value },
    /// 引用前序步骤的输出
    #[serde(rename = "reference")]
    Reference { step_id: String, path: Option<String> },
    /// 合并多个来源
    #[serde(rename = "merge")]
    Merge { sources: Vec<StepInput> },
}

/// 编排定义
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Orchestration {
    /// 编排ID
    pub id: String,
    /// 编排名称
    pub name: String,
    /// 编排描述
    #[serde(default)]
    pub description: Option<String>,
    /// 编排步骤列表（有序）
    pub steps: Vec<OrchestrationStep>,
}

/// 编排执行结果
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OrchestrationResult {
    /// 是否成功
    pub success: bool,
    /// 最终输出数据
    pub final_output: Option<serde_json::Value>,
    /// 各步骤执行结果
    pub step_results: Vec<StepResult>,
    /// 总执行耗时（微秒）
    pub total_elapsed_us: u64,
    /// 错误信息
    pub error: Option<String>,
}

/// 编排执行器
///
/// 负责解析编排定义并按顺序执行各步骤。
pub struct Orchestrator {
    /// WASM 运行时调用器
    runtime: Arc<dyn RuntimeInvoker>,
    /// 插件查询器
    plugin_query: Arc<dyn PluginQuery>,
}

impl Orchestrator {
    /// 创建新的编排执行器
    pub fn new(runtime: Arc<dyn RuntimeInvoker>, plugin_query: Arc<dyn PluginQuery>) -> Self {
        Self { runtime, plugin_query }
    }

    /// 执行编排
    ///
    /// # 参数
    ///
    /// * `orchestration` - 编排定义
    /// * `initial_input` - 初始输入数据
    ///
    /// # 返回值
    ///
    /// 返回编排执行结果。
    pub async fn execute(
        &self,
        orchestration: &Orchestration,
        initial_input: &serde_json::Value,
    ) -> Result<OrchestrationResult, ServiceError> {
        let start_time = std::time::Instant::now();
        let mut step_results = Vec::new();
        let mut step_outputs: HashMap<String, serde_json::Value> = HashMap::new();
        // 初始输入作为 "$initial" 步骤的输出
        step_outputs.insert("$initial".to_string(), initial_input.clone());

        for step in &orchestration.steps {
            debug!("执行编排步骤: {} ({})", step.step_id, step.function_name);

            // 解析步骤输入
            let input = self.resolve_step_input(&step.input, &step_outputs)?;

            // 检查插件是否激活
            let is_active = self.plugin_query.is_active(&step.plugin_id).await?;
            if !is_active {
                let err = ServiceError::plugin_not_active(&step.plugin_id);
                step_results.push(StepResult {
                    step_id: step.step_id.clone(),
                    success: false,
                    output: None,
                    elapsed_us: 0,
                    error: Some(err.to_string()),
                });
                return Ok(OrchestrationResult {
                    success: false,
                    final_output: None,
                    step_results,
                    total_elapsed_us: start_time.elapsed().as_micros() as u64,
                    error: Some(err.to_string()),
                });
            }

            // 确保 WASM 模块已加载
            if !self.runtime.is_loaded(&step.plugin_id).await {
                let wasm_path = self.plugin_query.get_wasm_path(&step.plugin_id).await?;
                self.runtime.load_module(&step.plugin_id, &wasm_path).await?;
            }

            // 序列化输入
            let input_bytes = serde_json::to_vec(&input)
                .map_err(|e| ServiceError::InputParseError(e.to_string()))?;

            // 执行 WASM 调用
            let step_start = std::time::Instant::now();
            let result: Result<WasmInvokeResult, _> = self.runtime
                .invoke(&step.plugin_id, &step.function_name, &input_bytes)
                .await;

            match result {
                Ok(invoke_result) => {
                    let output: Option<serde_json::Value> = if invoke_result.output.is_empty() {
                        None
                    } else {
                        serde_json::from_slice(&invoke_result.output).ok()
                    };

                    // 保存步骤输出供后续引用
                    if let Some(ref out) = output {
                        step_outputs.insert(step.step_id.clone(), out.clone());
                    }

                    step_results.push(StepResult {
                        step_id: step.step_id.clone(),
                        success: true,
                        output,
                        elapsed_us: invoke_result.elapsed_us,
                        error: None,
                    });
                }
                Err(e) => {
                    let err_msg = e.to_string();
                    warn!("编排步骤 {} 执行失败: {}", step.step_id, err_msg);
                    step_results.push(StepResult {
                        step_id: step.step_id.clone(),
                        success: false,
                        output: None,
                        elapsed_us: step_start.elapsed().as_micros() as u64,
                        error: Some(err_msg.clone()),
                    });

                    return Ok(OrchestrationResult {
                        success: false,
                        final_output: None,
                        step_results,
                        total_elapsed_us: start_time.elapsed().as_micros() as u64,
                        error: Some(ServiceError::orchestration_failed(&step.step_id, &err_msg).to_string()),
                    });
                }
            }
        }

        // 获取最后一个成功步骤的输出作为最终输出
        let final_output = step_results
            .iter()
            .rev()
            .find(|r| r.success)
            .and_then(|r| r.output.clone());

        Ok(OrchestrationResult {
            success: true,
            final_output,
            step_results,
            total_elapsed_us: start_time.elapsed().as_micros() as u64,
            error: None,
        })
    }

    /// 解析步骤输入
    ///
    /// 根据输入定义，从步骤输出映射中获取实际输入值。
    fn resolve_step_input(
        &self,
        input: &StepInput,
        step_outputs: &HashMap<String, serde_json::Value>,
    ) -> Result<serde_json::Value, ServiceError> {
        match input {
            StepInput::Static { value } => Ok(value.clone()),
            StepInput::Reference { step_id, path } => {
                let output = step_outputs
                    .get(step_id)
                    .ok_or_else(|| ServiceError::InternalError(format!("步骤输出未找到: {}", step_id)))?;
                if let Some(p) = path {
                    // 使用 jsonpath 简化实现：仅支持点分隔路径
                    self.get_json_path(output, p)
                } else {
                    Ok(output.clone())
                }
            }
            StepInput::Merge { sources } => {
                let mut merged = serde_json::Map::new();
                for source in sources {
                    let value = self.resolve_step_input(source, step_outputs)?;
                    if let serde_json::Value::Object(map) = value {
                        for (k, v) in map {
                            merged.insert(k, v);
                        }
                    }
                }
                Ok(serde_json::Value::Object(merged))
            }
        }
    }

    /// 获取 JSON 路径值（简化实现）
    ///
    /// 支持点分隔路径，如 "data.name"。
    fn get_json_path(&self, value: &serde_json::Value, path: &str) -> Result<serde_json::Value, ServiceError> {
        let parts: Vec<&str> = path.split('.').collect();
        let mut current = value;
        for part in &parts {
            match current {
                serde_json::Value::Object(map) => {
                    current = map
                        .get(*part)
                        .ok_or_else(|| ServiceError::InternalError(format!("JSON 路径未找到: {}", part)))?;
                }
                _ => {
                    return Err(ServiceError::InternalError(format!(
                        "JSON 路径解析失败，非对象类型: {}",
                        part
                    )));
                }
            }
        }
        Ok(current.clone())
    }
}
