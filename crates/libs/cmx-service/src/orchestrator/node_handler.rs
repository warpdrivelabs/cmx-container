//! 节点执行器
//!
//! 统一处理 func 和 switch 节点的 WASM 调用逻辑，消除代码重复。
//! 核心设计：两种节点类型的执行流程完全一致，仅日志标签不同。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use cmx_core::model::service::{FunctionInput, FunctionOutput, ServiceNode};
use cmx_traits::plugin::PluginQuery;
use cmx_traits::runtime::RuntimeInvoker;
use tracing::debug;

use super::types::{ExecutionContext, ExecutionStep, StepStatus};
use crate::error::ServiceError;

/// 节点执行器
///
/// 统一处理 func 和 switch 节点的 WASM 调用逻辑。
/// 两种节点类型的核心执行流程完全一致，仅日志标签不同。
///
/// # 执行流程
/// ```text
/// 解析节点元信息 → 加载 WASM 模块 → 构建输入 → 调用函数 → 解析输出 → 更新上下文
/// ```
pub struct NodeHandler<'a> {
    /// WASM 运行时调用器（用于调用插件函数）
    runtime: &'a Arc<dyn RuntimeInvoker>,
    /// 插件查询器（用于获取 WASM 模块路径）
    plugin_query: &'a Arc<dyn PluginQuery>,
}

impl<'a> NodeHandler<'a> {
    /// 创建节点执行器
    ///
    /// # 参数
    /// * `runtime` - WASM 运行时调用器
    /// * `plugin_query` - 插件查询器
    pub fn new(
        runtime: &'a Arc<dyn RuntimeInvoker>,
        plugin_query: &'a Arc<dyn PluginQuery>,
    ) -> Self {
        Self {
            runtime,
            plugin_query,
        }
    }

    /// 执行节点（统一入口）
    ///
    /// 合并了原 execute_func_node 和 execute_switch_node 的共享逻辑：
    /// 1. 解析节点元信息（插件ID、函数名）
    /// 2. 检查并加载 WASM 模块
    /// 3. 构建 FunctionInput（当前输出 + SVRContext）
    /// 4. 调用 WASM 函数
    /// 5. 解析 FunctionOutput
    /// 6. 更新 ExecutionContext（current_output、step_outputs）
    /// 7. 记录 ExecutionStep
    ///
    /// # 参数
    /// * `node` - 要执行的节点定义
    /// * `exec_context` - 执行上下文（可变，会更新 current_output 和 step_outputs）
    /// * `steps` - 步骤记录列表（可变，会添加新记录）
    /// * `include_steps` - 是否记录步骤数据到 steps 列表
    ///
    /// # 返回值
    /// 成功返回 Ok(())，失败返回 ServiceError
    pub async fn execute_node(
        &self,
        node: &ServiceNode,
        exec_context: &mut ExecutionContext,
        steps: &mut Vec<ExecutionStep>,
        include_steps: bool,
    ) -> Result<(), ServiceError> {
        // 使用节点类型作为日志标签，区分 func 和 switch
        let tag = &node.node_type;

        // 解析节点数据（包含函数名、插件ID等元信息）
        let node_data = node
            .data
            .as_ref()
            .ok_or_else(|| ServiceError::InternalError(format!("{} 节点缺少 data", tag)))?;

        debug!(
            "[{}] 开始执行: node_id={}, node_name={}, txn_id={:?}",
            tag, node.id, node_data.name, exec_context.svr_context.txn_id
        );

        // 解析节点元信息（插件ID、函数名）
        let node_meta = node_data
            .node_meta
            .as_ref()
            .ok_or_else(|| ServiceError::InternalError(format!("{} 节点缺少 nodeMeta", tag)))?;

        let plugin_id = &node_meta.plugin_id;
        let function_name = &node_meta.function_name;
        debug!(
            "[{}] 调用函数: plugin_id={}, function={}",
            tag, plugin_id, function_name
        );

        // 步骤1: 确保 WASM 模块已加载（懒加载机制）
        self.ensure_module_loaded(plugin_id, tag).await?;

        // 步骤2: 构建函数输入
        // FunctionInput 包含：input（当前步骤输入）、context（服务上下文）、binary_data（二进制数据）
        let func_input = FunctionInput {
            input: exec_context.current_output.clone(), // 上一步的输出作为当前步骤的输入
            context: exec_context.svr_context.clone(),  // 上下文在整个编排过程中传递
            binary_data: HashMap::new(),                // 二进制数据暂未使用
        };
        debug!(
            "[{}] 函数输入: input={}, txn_id={:?}",
            tag, func_input.input, func_input.context.txn_id
        );

        // 序列化输入为 JSON 字节
        // let input_bytes = serde_json::to_vec(&func_input)
        //     .map_err(|e| ServiceError::InputParseError(e.to_string()))?;
        // 序列化输入为 MessagePack字节
        let input_bytes = rmp_serde::to_vec(&func_input)
            .map_err(|e| ServiceError::InputParseError(e.to_string()))?;

        // 步骤3: 调用 WASM 函数
        let step_start = Instant::now();
        let invoke_result = self
            .runtime
            .invoke(plugin_id, function_name, &input_bytes)
            .await
            .map_err(|e| ServiceError::InvokeFailed(e.to_string()))?;

        // 步骤4: 解析函数输出
        // let output: FunctionOutput = serde_json::from_slice(&invoke_result.output)
        //     .map_err(|e| ServiceError::OutputSerializeError(e.to_string()))?;
        // 步骤4:  从 MsgPack 二进制数据反序列化回结构体
        let output: FunctionOutput = rmp_serde::from_slice(&invoke_result.output)
            .map_err(|e| ServiceError::OutputSerializeError(e.to_string()))?;

        let elapsed_us = step_start.elapsed().as_micros() as u64;
        debug!(
            "[{}] 函数执行完成: node_id={}, node_name={}, output={}, elapsed_us={}",
            tag, node.id, node_data.name, output.result, elapsed_us
        );

        // 步骤5: 更新执行上下文
        // current_output 作为下一个节点的输入
        exec_context.current_output = output.result.clone();
        // 将输出保存到 step_outputs，供后续节点通过 context.step_outputs[node_id] 访问
        exec_context
            .svr_context
            .add_step_output(node.id.clone(), output.result.clone());

        // 步骤6: 记录执行步骤（仅当 include_steps=true 时）
        if include_steps {
            steps.push(ExecutionStep {
                node_id: node.id.clone(),
                node_name: node_data.name.clone(),
                node_type: node.node_type.clone(),
                status: StepStatus::Success,
                output: Some(output.result),
                elapsed_us,
                error: None,
                previous_output: None,
            });
        }

        debug!("[{}] 执行完成: node_id={}", tag, node.id);

        Ok(())
    }

    /// 确保 WASM 模块已加载
    ///
    /// 如果模块未加载，则从插件查询器获取路径并加载。
    /// 实现懒加载机制：首次调用时才加载模块。
    ///
    /// # 参数
    /// * `plugin_id` - 插件ID
    /// * `tag` - 日志标签（用于区分节点类型）
    ///
    /// # 性能优化
    /// - 已加载的模块会跳过重复加载
    /// - 模块在运行时中缓存，后续调用直接使用
    async fn ensure_module_loaded(&self, plugin_id: &str, tag: &str) -> Result<(), ServiceError> {
        // 检查模块是否已加载
        if !self.runtime.is_loaded(plugin_id).await {
            // 获取 WASM 模块路径
            let wasm_path = self
                .plugin_query
                .get_wasm_path(plugin_id)
                .await
                .map_err(|e| ServiceError::InternalError(e.to_string()))?;
            debug!(
                "[{}] 加载 WASM 模块: plugin_id={}, path={}",
                tag,
                plugin_id,
                wasm_path.display()
            );

            // 加载模块到运行时
            self.runtime
                .load_module(plugin_id, &wasm_path)
                .await
                .map_err(|e| ServiceError::InvokeFailed(e.to_string()))?;
        }
        Ok(())
    }
}
