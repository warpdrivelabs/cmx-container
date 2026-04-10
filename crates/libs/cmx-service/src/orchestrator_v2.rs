//! 编排执行器 V2
//!
//! 支持服务编排 JSON 格式、多分支节点、事务框和 SVRContext 上下文传递。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use cmx_core::model::service::{
    FunctionInput, FunctionOutput, ServiceNode,
    SVRContext,
};
use cmx_database::transaction::begin_transaction_guard_by_db_id;
use cmx_traits::{CallerData, PluginQuery, RuntimeInvoker, ServiceQuery};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};
use utoipa::ToSchema;

use crate::error::ServiceError;

/// 编排执行结果 V2
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OrchestrationResultV2 {
    pub success: bool,
    pub output: Option<String>,
    pub steps: Vec<ExecutionStep>,
    pub total_elapsed_us: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ExecutionStep {
    pub node_id: String,
    pub node_name: String,
    pub output: Option<String>,
    pub elapsed_us: u64,
}

#[derive(Debug, Clone)]
pub struct ExecutionContext {
    pub current_output: String,
    pub svr_context: SVRContext,
}

pub struct OrchestratorV2 {
    runtime: Arc<dyn RuntimeInvoker>,
    plugin_query: Arc<dyn PluginQuery>,
    service_query: Arc<dyn ServiceQuery>,
    default_db_id: String,
}

impl OrchestratorV2 {
    pub fn new(
        runtime: Arc<dyn RuntimeInvoker>,
        plugin_query: Arc<dyn PluginQuery>,
        service_query: Arc<dyn ServiceQuery>,
    ) -> Self {
        Self {
            runtime,
            plugin_query,
            service_query,
            default_db_id: "default".to_string(),
        }
    }

    pub fn with_db_id(
        mut self,
        db_id: impl Into<String>,
    ) -> Self {
        self.default_db_id = db_id.into();
        self
    }

    pub async fn execute_service(
        &self,
        service_key: &str,
        initial_input: &str,
        headers: HashMap<String, String>,
        caller_data: &CallerData,
    ) -> Result<OrchestrationResultV2, ServiceError> {
        let start_time = Instant::now();
        let mut steps = Vec::new();

        let orchestration = self.service_query.get_orchestration(service_key).await
            .map_err(|e| ServiceError::InternalError(e.to_string()))?
            .ok_or_else(|| ServiceError::InternalError(format!("服务未找到: {}", service_key)))?;

        let svr_context = SVRContext::new(initial_input.to_string(), headers);

        let start_node = orchestration.flow.nodes.iter()
            .find(|n| n.node_type == "skylake-start")
            .ok_or_else(|| ServiceError::InternalError("未找到开始节点".to_string()))?;

        let mut exec_context = ExecutionContext {
            current_output: String::new(),
            svr_context,
        };

        let mut current_node_id = start_node.id.clone();
        let flow = &orchestration.flow;
        let mut result = Ok(());

        loop {
            let node = match flow.nodes.iter().find(|n| n.id == current_node_id) {
                Some(n) => n,
                None => {
                    result = Err(ServiceError::InternalError(format!("节点未找到: {}", current_node_id)));
                    break;
                }
            };

            match node.node_type.as_str() {
                "skylake-start" => {
                    if let Some(next_edge) = flow.edges.iter().find(|e| e.source_node_id == current_node_id && e.source_port_id == "out") {
                        current_node_id = next_edge.target_node_id.clone();
                        continue;
                    }
                    break;
                }
                "skylake-end" => {
                    break;
                }
                "skylake-func" => {
                    result = self.execute_func_node(node, &mut exec_context, caller_data, &mut steps).await;
                    if result.is_err() {
                        break;
                    }
                    if let Some(next_edge) = flow.edges.iter().find(|e| e.source_node_id == current_node_id && e.source_port_id == "out") {
                        current_node_id = next_edge.target_node_id.clone();
                        continue;
                    }
                    break;
                }
                "skylake-switch" => {
                    result = self.execute_switch_node(node, &mut exec_context, caller_data, &mut steps).await;
                    if result.is_err() {
                        break;
                    }
                    if let Some(next_edge) = flow.edges.iter().find(|e| e.source_node_id == current_node_id && e.source_port_id == format!("out_{}", exec_context.current_output)) {
                        current_node_id = next_edge.target_node_id.clone();
                        continue;
                    }
                    break;
                }
                "skylake-transaction" => {
                    result = self.execute_transaction_node(flow, node, &mut exec_context, caller_data, &mut steps).await;
                    if result.is_err() {
                        break;
                    }
                    if let Some(next_edge) = flow.edges.iter().find(|e| e.source_node_id == current_node_id && e.source_port_id == "out") {
                        current_node_id = next_edge.target_node_id.clone();
                        continue;
                    }
                    break;
                }
                _ => {
                    result = Err(ServiceError::InternalError(format!("未知节点类型: {}", node.node_type)));
                    break;
                }
            }
        }

        let final_output = match &result {
            Ok(_) => Some(exec_context.current_output.clone()),
            Err(_) => None,
        };

        Ok(OrchestrationResultV2 {
            success: result.is_ok(),
            output: final_output,
            steps,
            total_elapsed_us: start_time.elapsed().as_micros() as u64,
        })
    }

    async fn execute_switch_node(
        &self,
        node: &ServiceNode,
        exec_context: &mut ExecutionContext,
        caller_data: &CallerData,
        steps: &mut Vec<ExecutionStep>,
    ) -> Result<(), ServiceError> {
        let node_meta = node.data.node_meta.as_ref()
            .ok_or_else(|| ServiceError::InternalError("switch 节点缺少 nodeMeta".to_string()))?;

        let plugin_id = &node_meta.plugin_id;
        let function_name = &node_meta.function_name;

        let is_active = self.plugin_query.is_active(plugin_id).await
            .map_err(|e| ServiceError::InternalError(e.to_string()))?;
        if !is_active {
            return Err(ServiceError::plugin_not_active(plugin_id));
        }

        if !self.runtime.is_loaded(plugin_id).await {
            let wasm_path = self.plugin_query.get_wasm_path(plugin_id).await
                .map_err(|e| ServiceError::InternalError(e.to_string()))?;
            self.runtime.load_module(plugin_id, &wasm_path).await
                .map_err(|e| ServiceError::InvokeFailed(e.to_string()))?;
        }

        let func_input = FunctionInput {
            input: exec_context.current_output.clone(),
            context: exec_context.svr_context.clone(),
            txn_id: None,
        };
        let input_bytes = serde_json::to_vec(&func_input)
            .map_err(|e| ServiceError::InputParseError(e.to_string()))?;

        let step_start = Instant::now();
        let invoke_result = self.runtime
            .invoke(plugin_id, function_name, &input_bytes, caller_data)
            .await
            .map_err(|e| ServiceError::InvokeFailed(e.to_string()))?;

        let output: FunctionOutput = serde_json::from_slice(&invoke_result.output)
            .map_err(|e| ServiceError::OutputSerializeError(e.to_string()))?;

        let elapsed_us = step_start.elapsed().as_micros() as u64;
        exec_context.current_output = output.result.clone();

        steps.push(ExecutionStep {
            node_id: node.id.clone(),
            node_name: node.data.name.clone(),
            output: Some(output.result.clone()),
            elapsed_us,
        });

        Ok(())
    }

    async fn execute_func_node(
        &self,
        node: &ServiceNode,
        exec_context: &mut ExecutionContext,
        caller_data: &CallerData,
        steps: &mut Vec<ExecutionStep>,
    ) -> Result<(), ServiceError> {
        let node_meta = node.data.node_meta.as_ref()
            .ok_or_else(|| ServiceError::InternalError("func 节点缺少 nodeMeta".to_string()))?;

        let plugin_id = &node_meta.plugin_id;
        let function_name = &node_meta.function_name;

        let is_active = self.plugin_query.is_active(plugin_id).await
            .map_err(|e| ServiceError::InternalError(e.to_string()))?;
        if !is_active {
            return Err(ServiceError::plugin_not_active(plugin_id));
        }

        if !self.runtime.is_loaded(plugin_id).await {
            let wasm_path = self.plugin_query.get_wasm_path(plugin_id).await
                .map_err(|e| ServiceError::InternalError(e.to_string()))?;
            self.runtime.load_module(plugin_id, &wasm_path).await
                .map_err(|e| ServiceError::InvokeFailed(e.to_string()))?;
        }

        let func_input = FunctionInput {
            input: exec_context.current_output.clone(),
            context: exec_context.svr_context.clone(),
            txn_id: None,
        };
        let input_bytes = serde_json::to_vec(&func_input)
            .map_err(|e| ServiceError::InputParseError(e.to_string()))?;

        let step_start = Instant::now();
        let invoke_result = self.runtime
            .invoke(plugin_id, function_name, &input_bytes, caller_data)
            .await
            .map_err(|e| ServiceError::InvokeFailed(e.to_string()))?;

        let output: FunctionOutput = serde_json::from_slice(&invoke_result.output)
            .map_err(|e| ServiceError::OutputSerializeError(e.to_string()))?;

        let elapsed_us = step_start.elapsed().as_micros() as u64;
        exec_context.current_output = output.result.clone();

        steps.push(ExecutionStep {
            node_id: node.id.clone(),
            node_name: node.data.name.clone(),
            output: Some(output.result),
            elapsed_us,
        });

        Ok(())
    }

    async fn execute_transaction_node(
        &self,
        flow: &cmx_core::model::service::ServiceFlow,
        transaction_node: &ServiceNode,
        exec_context: &mut ExecutionContext,
        caller_data: &CallerData,
        steps: &mut Vec<ExecutionStep>,
    ) -> Result<(), ServiceError> {
        info!("事务框开始: node_id={}", transaction_node.id);

        let db_id = transaction_node.data.node_meta.as_ref()
            .and_then(|m| m.database_id.clone())
            .unwrap_or_else(|| self.default_db_id.clone());

        let txn_guard = begin_transaction_guard_by_db_id(&db_id, Default::default())
            .await
            .map_err(|e| ServiceError::InternalError(format!("开启事务失败: {}", e)))?;

        info!("事务已开启: db_id={}", db_id);

        let child_nodes: Vec<&ServiceNode> = flow.nodes.iter()
            .filter(|n| n.parent.as_ref() == Some(&transaction_node.id))
            .collect();

        let txn_id = txn_guard.txn_id().to_string();

        for child_node in child_nodes {
            match child_node.node_type.as_str() {
                "skylake-func" => {
                    self.execute_func_node_with_txn(child_node, exec_context, caller_data, steps, &txn_id).await?;
                }
                _ => {
                    warn!("事务框内遇到非 func 节点类型: {}", child_node.node_type);
                }
            }
        }

        txn_guard.commit().await
            .map_err(|e| ServiceError::InternalError(format!("提交事务失败: {}", e)))?;

        info!("事务已提交: node_id={}, db_id={}", transaction_node.id, db_id);

        Ok(())
    }

    async fn execute_func_node_with_txn(
        &self,
        node: &ServiceNode,
        exec_context: &mut ExecutionContext,
        caller_data: &CallerData,
        steps: &mut Vec<ExecutionStep>,
        txn_id: &str,
    ) -> Result<(), ServiceError> {
        let node_meta = node.data.node_meta.as_ref()
            .ok_or_else(|| ServiceError::InternalError("func 节点缺少 nodeMeta".to_string()))?;

        let plugin_id = &node_meta.plugin_id;
        let function_name = &node_meta.function_name;

        let is_active = self.plugin_query.is_active(plugin_id).await
            .map_err(|e| ServiceError::InternalError(e.to_string()))?;
        if !is_active {
            return Err(ServiceError::plugin_not_active(plugin_id));
        }

        if !self.runtime.is_loaded(plugin_id).await {
            let wasm_path = self.plugin_query.get_wasm_path(plugin_id).await
                .map_err(|e| ServiceError::InternalError(e.to_string()))?;
            self.runtime.load_module(plugin_id, &wasm_path).await
                .map_err(|e| ServiceError::InvokeFailed(e.to_string()))?;
        }

        let func_input = FunctionInput {
            input: exec_context.current_output.clone(),
            context: exec_context.svr_context.clone(),
            txn_id: Some(txn_id.to_string()),
        };
        let input_bytes = serde_json::to_vec(&func_input)
            .map_err(|e| ServiceError::InputParseError(e.to_string()))?;

        let step_start = Instant::now();
        let invoke_result = self.runtime
            .invoke(plugin_id, function_name, &input_bytes, caller_data)
            .await
            .map_err(|e| ServiceError::InvokeFailed(e.to_string()))?;

        let output: FunctionOutput = serde_json::from_slice(&invoke_result.output)
            .map_err(|e| ServiceError::OutputSerializeError(e.to_string()))?;

        let elapsed_us = step_start.elapsed().as_micros() as u64;
        exec_context.current_output = output.result.clone();

        steps.push(ExecutionStep {
            node_id: node.id.clone(),
            node_name: node.data.name.clone(),
            output: Some(output.result),
            elapsed_us,
        });

        Ok(())
    }
}
