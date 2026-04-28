//! 编排执行器
//!
//! 核心入口方法，协调 FlowNavigator、TransactionManager 和 NodeHandler 完成服务编排执行。
//! 这是编排器的最顶层模块，对外暴露 `Orchestrator` 结构体和 `execute_service` 方法。

use std::sync::Arc;
use std::time::Instant;

use cmx_core::model::service::SVRContext;
use cmx_traits::{PluginQuery, RuntimeInvoker, ServiceQuery};
use tracing::{debug, info, warn};

use crate::error::ServiceError;
use super::debug_prepare::DebugPrepare;
use super::flow_navigator::FlowNavigator;
use super::node_handler::NodeHandler;
use super::transaction_manager::TransactionManager;
use super::types::*;

/// 编排执行器
///
/// 支持基于 Flow JSON 的 DAG 编排执行，包括：
/// - 线性流程执行：start -> func -> func -> end
/// - 事务框支持：多个函数在同一个数据库事务中执行
/// - 多分支路由：switch 节点根据返回值选择执行路径
/// - SVRContext 上下文传递：初始入参、请求头、各步骤输出在函数间传递
///
/// # 架构设计
/// ```text
/// ┌─────────────────────────────────────────────────────────────────┐
/// │                         Orchestrator                             │
/// ├─────────────────────────────────────────────────────────────────┤
/// │  execute_service()                                               │
/// │       ↓                                                          │
/// │  ┌─────────────┐  ┌──────────────────┐  ┌─────────────────┐    │
/// │  │ FlowNavigator│  │TransactionManager│  │  NodeHandler    │    │
/// │  │ (流程导航)   │  │  (事务管理)      │  │  (节点执行)     │    │
/// │  └─────────────┘  └──────────────────┘  └─────────────────┘    │
/// └─────────────────────────────────────────────────────────────────┘
/// ```
pub struct Orchestrator {
    /// WASM 运行时调用器（用于调用插件函数）
    runtime: Arc<dyn RuntimeInvoker>,
    /// 插件查询器（用于查询插件状态和获取 WASM 路径）
    plugin_query: Arc<dyn PluginQuery>,
    /// 服务查询器（用于获取服务编排定义）
    service_query: Arc<dyn ServiceQuery>,
    /// 默认数据库ID（事务框未指定数据库时使用）
    default_db_id: String,
}

impl Orchestrator {
    /// 创建编排执行器
    ///
    /// # 参数
    /// * `runtime` - WASM 运行时调用器
    /// * `plugin_query` - 插件查询器
    /// * `service_query` - 服务查询器
    /// * `default_db_id` - 默认数据库ID
    pub fn new(
        runtime: Arc<dyn RuntimeInvoker>,
        plugin_query: Arc<dyn PluginQuery>,
        service_query: Arc<dyn ServiceQuery>,
        default_db_id: String,
    ) -> Self {
        Self {
            runtime,
            plugin_query,
            service_query,
            default_db_id,
        }
    }

    /// 设置默认数据库ID（Builder 模式）
    ///
    /// # 参数
    /// * `db_id` - 数据库ID
    pub fn with_db_id(mut self, db_id: impl Into<String>) -> Self {
        self.default_db_id = db_id.into();
        self
    }

    /// 执行服务编排（核心入口方法）
    ///
    /// 根据服务编排定义，从 start 节点开始，沿边遍历执行各节点，
    /// 直到遇到 end 节点或发生错误。
    ///
    /// # 执行流程
    /// ```text
    /// 1. 查询服务编排定义（ServiceOrchestration）
    /// 2. 初始化执行上下文（ExecutionContext）
    /// 3. 查找开始节点（skylake-start）
    /// 4. 循环执行节点：
    ///    a. 查找当前节点
    ///    b. 管理事务状态（TransactionManager）
    ///    c. 根据节点类型执行：
    ///       - skylake-start: 跳转到下一个节点
    ///       - skylake-end: 提交事务，结束循环
    ///       - skylake-func: 执行函数，跳转到下一个节点
    ///       - skylake-switch: 执行函数，根据返回值选择分支
    ///       - skylake-transaction: 执行事务框内的所有子节点
    /// 5. 构建返回结果（OrchestrationResult）
    /// ```
    ///
    /// # 参数
    /// * `service_key` - 服务唯一标识
    /// * `svr_context` - 服务调用上下文（包含 initial_input、headers、time_in、request_id 等）
    /// * `options` - 执行选项（控制是否返回 steps 数据）
    ///
    /// # 返回值
    /// 返回编排执行结果，包含成功状态、最终输出、步骤记录、错误信息
    pub async fn execute_service(
        &self,
        service_key: &str,
        svr_context: SVRContext,
        options: ExecuteOptions,
    ) -> Result<OrchestrationResult, ServiceError> {
        // ==================== 阶段1: 初始化 ====================
        // 记录开始时间，用于计算总耗时
        let start_time = Instant::now();

        // 步骤记录列表：记录每个节点的执行情况
        // - 成功时：根据 include_steps 决定是否返回
        // - 失败时：始终返回（便于排错）
        let mut steps: Vec<ExecutionStep> = Vec::new();

        // 结构化错误信息：失败时构建，包含失败步骤详情
        let mut orch_error: Option<OrchestrationError> = None;

        // ==================== 阶段2: 加载服务编排定义 ====================
        // 通过 service_key 查询服务编排定义（从数据库或缓存）
        // ServiceOrchestration 包含：name, code, flow(nodes + edges)

        self.service_query.get_service(service_key).await
            .map_err(|e| ServiceError::InternalError(e.to_string()))?
            .ok_or_else(|| ServiceError::InternalError(format!("服务未找到: {}", service_key)))?;
        let orchestration = self.service_query.get_orchestration(service_key).await
            .map_err(|e| ServiceError::InternalError(e.to_string()))?
            .ok_or_else(|| ServiceError::InternalError(format!("服务编排配置未找到: {}", service_key)))?;

        // ==================== 阶段3: 初始化执行上下文 ====================
        // SVRContext 是服务调用上下文，由外部（middleware/handler）创建并传入
        // 包含：initial_input（初始输入）、headers（请求头）、step_outputs（各步骤输出）、txn_id（事务ID）、time_in（请求时间）、request_id（请求ID）

        // 创建流程导航器：用于查找节点和边
        let flow = &orchestration.flow;
        let navigator = FlowNavigator::new(flow);

        // 查找开始节点：每个 Flow JSON 必须有且仅有一个 skylake-start 节点
        let start_node = navigator.find_start_node()
            .ok_or_else(|| ServiceError::InternalError("未找到开始节点".to_string()))?;

        // 初始化执行上下文
        // - current_output: 当前步骤的输出，作为下一个步骤的输入（初始为 svr_context.initial_input）
        // - svr_context: 服务调用上下文，在函数间传递
        let mut exec_context = ExecutionContext {
            current_output: svr_context.initial_input.clone(),
            svr_context,
        };

        // 当前执行的节点ID，从开始节点开始
        let mut current_node_id = start_node.id.clone();

        // 执行结果：用于判断是否成功
        let mut result: Result<(), ServiceError> = Ok(());

        // 事务管理器：负责事务的开启、提交和回滚
        let mut txn_manager = TransactionManager::new(self.default_db_id.clone());

        // 节点执行器：负责调用 WASM 函数
        let node_handler = NodeHandler::new(&self.runtime, &self.plugin_query);

        // ==================== 阶段4: 主执行循环 ====================
        // 循环执行节点，直到遇到 end 节点或发生错误
        loop {
            // 步骤4.1: 查找当前节点
            // 根据 current_node_id 在 flow.nodes 中查找节点定义
            let node = match navigator.find_node(&current_node_id) {
                Some(n) => n,
                None => {
                    debug!("节点未找到: node_id={}", current_node_id);
                    orch_error = Some(OrchestrationError {
                        message: format!("节点未找到: {}", current_node_id),
                    });
                    result = Err(ServiceError::InternalError(format!("节点未找到: {}", current_node_id)));
                    break;
                }
            };

            debug!(
                ">>> 进入节点: node_id={}, node_type={}, node_name={}",
                node.id, node.node_type,
                node.data.as_ref().map(|d| d.name.as_str()).unwrap_or("unknown")
            );

            // 步骤4.2: 事务状态管理
            // 根据节点的 parent 属性决定是否需要开启/提交/切换事务
            // - 无活跃事务 + 节点在事务框中 → 开启新事务
            // - 有活跃事务 + 节点离开事务框 → 提交当前事务
            // - 有活跃事务 + 节点在同一事务框中 → 继续执行
            if let Err(e) = txn_manager.ensure_transaction(node, &navigator, &mut exec_context.svr_context).await {
                orch_error = Some(OrchestrationError {
                    message: format!("事务管理失败: {}", e),
                });
                result = Err(e);
                break;
            }

            // 步骤4.3: 根据节点类型执行
            match node.node_type.as_str() {
                // ==================== 开始节点 ====================
                // 开始节点不执行函数，仅作为流程入口
                // 查找从 "out" 端口出发的边，跳转到下一个节点
                "skylake-start" => {
                    debug!("执行开始节点: node_id={}", node.id);
                    // 查找从开始节点出发的边（固定使用 "out" 端口）
                    if let Some(next_edge) = navigator.find_next_edge(&current_node_id, "out") {
                        debug!("开始节点跳转: from={} -> to={}", current_node_id, next_edge.target_node_id);
                        // 更新当前节点ID，继续循环执行下一个节点
                        current_node_id = next_edge.target_node_id.clone();
                        continue;
                    }
                    // 没有出边，流程结束（异常情况：开始节点没有连接任何节点）
                    debug!("开始节点无出边，退出循环");
                    break;
                }

                // ==================== 结束节点 ====================
                // 结束节点标记流程结束
                // 提交当前活跃的事务（如果有），然后退出循环
                "skylake-end" => {
                    debug!("执行结束节点: node_id={}", node.id);
                    // 提交当前活跃的事务
                    // 注意：事务可能在事务框内已经提交，这里处理的是事务框外的事务
                    txn_manager.commit_active(&mut exec_context.svr_context).await?;
                    break; // 退出循环，流程正常结束
                }

                // ==================== 函数节点 ====================
                // 函数节点执行 WASM 函数
                // 函数输出作为下一个节点的输入
                "skylake-func" => {
                    debug!("执行函数节点: node_id={}", node.id);

                    // 调试拦截：如果当前节点是调试目标节点，暂停执行并返回调试信息
                    if options.debug_options.is_debug_node(&current_node_id) {
                        debug!("调试模式拦截: node_id={}", node.id);
                        let previous_output = exec_context.current_output.clone();
                        let debug_prepare = DebugPrepare::new(&self.plugin_query);
                        let prepare_result = debug_prepare.prepare(
                            node,
                            previous_output.clone(),
                            exec_context.svr_context.initial_input.clone(),
                            options.clone(),
                        ).await?;

                        // 调试暂停时回滚活跃事务，避免数据库状态不一致
                        if txn_manager.has_active() {
                            txn_manager.rollback_active().await;
                        }

                        // 记录 DebugPaused 步骤，便于前端展示执行进度
                        steps.push(ExecutionStep {
                            node_id: node.id.clone(),
                            node_name: node.data.as_ref().map(|d| d.name.clone()).unwrap_or_default(),
                            node_type: node.node_type.clone(),
                            status: StepStatus::DebugPaused,
                            output: None,
                            elapsed_us: 0,
                            error: None,
                            previous_output: Some(previous_output),
                        });

                        // 构建调试输出：包含上一步输出、初始输入和调试准备信息
                        let debug_output = serde_json::json!({
                            "previous_output": exec_context.current_output,
                            "initial_input": exec_context.svr_context.initial_input,
                            "debug_info": &prepare_result,
                        });

                        return Ok(OrchestrationResult {
                            success: true,
                            output: Some(debug_output),
                            steps,
                            total_elapsed_us: start_time.elapsed().as_micros() as u64,
                            error: None,
                            debug_triggered: Some(true),
                            debug_prepare_result: Some(prepare_result),
                        });
                    }

                    let previous_output = exec_context.current_output.clone();
                    result = node_handler.execute_node(
                        node, &mut exec_context, &mut steps, options.include_steps
                    ).await;

                    if let Err(ref err) = result {
                        debug!("函数节点执行失败: node_id={}, error={:?}", node.id, err);
                        let node_name = node.data.as_ref()
                            .map(|d| d.name.as_str())
                            .unwrap_or("unknown");
                        steps.push(ExecutionStep {
                            node_id: node.id.clone(),
                            node_name: node_name.to_string(),
                            node_type: node.node_type.clone(),
                            status: StepStatus::Failed,
                            output: None,
                            elapsed_us: 0,
                            error: Some(err.to_string()),
                            previous_output: Some(previous_output),
                        });
                        orch_error = Some(OrchestrationError {
                            message: format!("步骤 [{}({})] 执行失败: {}", node_name, node.id, err),
                        });
                        break;
                    }

                    // 查找下一个节点（固定使用 "out" 端口）
                    // func 节点只有一条出边，连接到下一个节点
                    if let Some(next_edge) = navigator.find_next_edge(&current_node_id, "out") {
                        debug!("函数节点执行完成跳转: from={} -> to={}", current_node_id, next_edge.target_node_id);
                        current_node_id = next_edge.target_node_id.clone();
                        continue;
                    }
                    // 没有出边，流程结束
                    debug!("函数节点无出边，退出循环");
                    break;
                }

                // ==================== 多分支节点 ====================
                // 多分支节点根据函数返回值选择执行路径
                // 返回值 "1" → 端口 "out_1"，返回值 "2" → 端口 "out_2"
                "skylake-switch" => {
                    debug!("执行多分支节点: node_id={}", node.id);

                    // 调试拦截：如果当前节点是调试目标节点，暂停执行并返回调试信息
                    if options.debug_options.is_debug_node(&current_node_id) {
                        debug!("调试模式拦截(多分支): node_id={}", node.id);
                        let previous_output = exec_context.current_output.clone();
                        let debug_prepare = DebugPrepare::new(&self.plugin_query);
                        let prepare_result = debug_prepare.prepare(
                            node,
                            previous_output.clone(),
                            exec_context.svr_context.initial_input.clone(),
                            options.clone(),
                        ).await?;

                        // 调试暂停时回滚活跃事务，避免数据库状态不一致
                        if txn_manager.has_active() {
                            txn_manager.rollback_active().await;
                        }

                        // 记录 DebugPaused 步骤，便于前端展示执行进度
                        steps.push(ExecutionStep {
                            node_id: node.id.clone(),
                            node_name: node.data.as_ref().map(|d| d.name.clone()).unwrap_or_default(),
                            node_type: node.node_type.clone(),
                            status: StepStatus::DebugPaused,
                            output: None,
                            elapsed_us: 0,
                            error: None,
                            previous_output: Some(previous_output),
                        });

                        // 构建调试输出：包含上一步输出、初始输入和调试准备信息
                        let debug_output = serde_json::json!({
                            "previous_output": exec_context.current_output,
                            "initial_input": exec_context.svr_context.initial_input,
                            "debug_info": &prepare_result,
                        });

                        return Ok(OrchestrationResult {
                            success: true,
                            output: Some(debug_output),
                            steps,
                            total_elapsed_us: start_time.elapsed().as_micros() as u64,
                            error: None,
                            debug_triggered: Some(true),
                            debug_prepare_result: Some(prepare_result),
                        });
                    }

                    let previous_output = exec_context.current_output.clone();
                    result = node_handler.execute_node(
                        node, &mut exec_context, &mut steps, options.include_steps
                    ).await;

                    if let Err(ref err) = result {
                        debug!("多分支节点执行失败: node_id={}, error={:?}", node.id, err);
                        let node_name = node.data.as_ref()
                            .map(|d| d.name.as_str())
                            .unwrap_or("unknown");
                        steps.push(ExecutionStep {
                            node_id: node.id.clone(),
                            node_name: node_name.to_string(),
                            node_type: node.node_type.clone(),
                            status: StepStatus::Failed,
                            output: None,
                            elapsed_us: 0,
                            error: Some(err.to_string()),
                            previous_output: Some(previous_output),
                        });
                        orch_error = Some(OrchestrationError {
                            message: format!("步骤 [{}({})] 执行失败: {}", node_name, node.id, err),
                        });
                        break;
                    }

                    // 根据函数返回值构建端口ID
                    // 例如：返回 "1" → 端口 "out_1"，返回 "2" → 端口 "out_2"
                    // current_output 必须是 serde_json::Value::String 类型，否则报错
                    let branch_name = exec_context.current_output.as_str()
                        .ok_or_else(|| ServiceError::orchestration_failed(
                            &node.id,
                            &format!("多分支节点返回值不是字符串类型，无法确定分支端口。当前值: {}", exec_context.current_output)
                        ))?;
                    let source_port_id = format!("out_{}", branch_name);
                    debug!("多分支节点执行完成，选择分支: node_id={}, output={}, port={}",
                        node.id, exec_context.current_output, source_port_id);

                    // 查找匹配的边（根据端口ID）
                    if let Some(next_edge) = navigator.find_next_edge(&current_node_id, &source_port_id) {
                        debug!("多分支节点跳转: from={} -> to={}", current_node_id, next_edge.target_node_id);
                        current_node_id = next_edge.target_node_id.clone();
                        continue;
                    }
                    // 没有匹配的出边，流程结束（可能是分支未配置）
                    warn!("多分支节点无匹配出边，退出循环");
                    break;
                }

                // // ==================== 事务框节点 ====================
                // // 事务框节点将其内部的所有子节点在同一个数据库事务中执行
                // // 子节点通过 parent 字段指向事务框节点ID
                // "skylake-transaction" => {
                //     debug!("执行事务框节点: node_id={}", node.id);
                //     // 调用事务框执行方法
                //     // 内部会：开启事务 → 执行子节点 → 提交/回滚事务
                //     result = self.execute_transaction_node(
                //         flow, node, &mut exec_context, &mut steps, &node_handler, &options
                //     ).await;
                //
                //     if let Err(ref err) = result {
                //         // 执行失败，构建错误信息并退出循环
                //         // 事务框内的事务已经回滚
                //         error!("事务框节点执行失败: node_id={}, error={:?}", node.id, err);
                //         orch_error = Self::build_error_info(
                //             node, &steps, err, &mut exec_context
                //         );
                //         break;
                //     }
                //
                //     // 查找下一个节点（固定使用 "out" 端口）
                //     if let Some(next_edge) = navigator.find_next_edge(&current_node_id, "out") {
                //         debug!("事务框节点执行完成跳转: from={} -> to={}", current_node_id, next_edge.target_node_id);
                //         current_node_id = next_edge.target_node_id.clone();
                //         continue;
                //     }
                //     // 没有出边，流程结束
                //     debug!("事务框节点无出边，退出循环");
                //     break;
                // }

                // ==================== 未知节点类型 ====================
                // 遇到不支持的节点类型，记录错误并退出
                _ => {
                    debug!("遇到未知节点类型: node_id={}, node_type={}", node.id, node.node_type);
                    orch_error = Some(OrchestrationError {
                        message: format!("未知节点类型: {}", node.node_type),
                    });
                    result = Err(ServiceError::InternalError(format!("未知节点类型: {}", node.node_type)));
                    break;
                }
            }
        }

        // ==================== 阶段5: 循环退出后处理 ====================
        // 处理剩余的活跃事务
        if txn_manager.has_active() {
            if result.is_err() {
                // 执行失败，回滚事务
                // 注意：事务框内的事务已经在 execute_transaction_node 中处理
                // 这里处理的是事务框外的事务（通过 TransactionManager 管理的事务）
                warn!("执行失败，回滚事务");
                txn_manager.rollback_active().await;
                exec_context.svr_context.clear_txn_id();
            } else {
                // 正常退出，提交剩余事务
                // 这种情况发生在：事务框节点后还有节点，但事务未提交
                debug!("循环正常退出，提交剩余事务");
                txn_manager.commit_active(&mut exec_context.svr_context).await?;
            }
        }

        // ==================== 阶段6: 构建返回结果 ====================
        // 判断执行是否成功
        let is_success = result.is_ok();

        // 最终输出：成功时返回最后一个节点的输出，失败时返回 None
        let final_output = if is_success {
            info!("执行成功，返回最终结果:  output={:?}", exec_context.current_output);
            Some(exec_context.current_output.clone())
        } else {
            None
        };

        // 步骤数据处理：
        // - 失败时始终返回 steps（便于排错）
        // - 成功时根据 include_steps 决定是否返回
        let final_steps = if is_success && !options.include_steps {
            Vec::new()  // 成功且不需要步骤数据，返回空数组
        } else {
            steps       // 失败或需要步骤数据，返回完整步骤列表
        };

        // 构建并返回编排结果
        Ok(OrchestrationResult {
            success: is_success,
            output: final_output,
            steps: final_steps,
            total_elapsed_us: start_time.elapsed().as_micros() as u64,
            error: orch_error,
            debug_triggered: Some(false),
            debug_prepare_result: None,
        })
    }

    // /// 执行事务框节点
    // ///
    // /// 事务框节点将其内部的所有函数节点在同一个数据库事务中执行。
    // /// 子节点通过 parent 字段指向事务框节点ID。
    // ///
    // /// # 执行流程
    // /// ```text
    // /// 1. 解析事务框的数据库ID
    // /// 2. 开启数据库事务
    // /// 3. 查找事务框内的所有子节点
    // /// 4. 依次执行子节点
    // /// 5. 根据执行结果提交或回滚事务
    // /// ```
    // ///
    // /// # 参数
    // /// * `flow` - 流程定义
    // /// * `transaction_node` - 事务框节点
    // /// * `exec_context` - 执行上下文
    // /// * `steps` - 步骤记录列表
    // /// * `node_handler` - 节点执行器
    // /// * `options` - 执行选项
    // async fn execute_transaction_node(
    //     &self,
    //     flow: &ServiceFlow,
    //     transaction_node: &ServiceNode,
    //     exec_context: &mut ExecutionContext,
    //     steps: &mut Vec<ExecutionStep>,
    //     node_handler: &NodeHandler<'_>,
    //     options: &ExecuteOptions,
    // ) -> Result<(), ServiceError> {
    //     info!("事务框开始: node_id={}", transaction_node.id);
    //
    //     // 步骤1: 解析事务框节点数据
    //     let node_data = transaction_node.data.as_ref()
    //         .ok_or_else(|| ServiceError::InternalError("事务框节点缺少 data".to_string()))?;
    //     debug!("[transaction] 开始执行: node_id={}, node_name={}", transaction_node.id, node_data.name);
    //
    //     // 步骤2: 解析事务框使用的数据库ID
    //     // 优先使用事务框指定的 database_id，否则使用默认值
    //     let db_id = node_data.node_meta.as_ref()
    //         .and_then(|m| m.database_id.clone())
    //         .unwrap_or_else(|| self.default_db_id.clone());
    //     debug!("[transaction] 数据库ID: db_id={}", db_id);
    //
    //     // 步骤3: 开启数据库事务
    //     let txn_guard = begin_transaction_guard_by_db_id(&db_id, Default::default())
    //         .await
    //         .map_err(|e| ServiceError::InternalError(format!("开启事务失败: {}", e)))?;
    //
    //     let txn_id = txn_guard.txn_id().to_string();
    //     debug!("[transaction] 事务已开启: db_id={}, txn_id={}", db_id, txn_id);
    //
    //     // 将事务ID设置到上下文中，传递给子节点
    //     // WASM 函数通过 context.txn_id 获取事务ID，用于后续数据库操作
    //     exec_context.svr_context.set_txn_id(txn_id.clone());
    //
    //     // 步骤4: 查找事务框内的所有子节点
    //     // 子节点通过 parent 字段指向事务框节点ID
    //     let child_nodes: Vec<&ServiceNode> = flow.nodes.iter()
    //         .filter(|n| n.parent.as_ref() == Some(&transaction_node.id))
    //         .collect();
    //     debug!("[transaction] 找到 {} 个子节点", child_nodes.len());
    //
    //     let mut child_result: Result<(), ServiceError> = Ok(());
    //
    //     // 步骤5: 依次执行事务框内的子节点
    //     for (idx, child_node) in child_nodes.iter().enumerate() {
    //         debug!("[transaction] 开始执行子节点 {}/{}: node_id={}", idx + 1, child_nodes.len(), child_node.id);
    //
    //         // 只支持 func 和 switch 类型的子节点
    //         match child_node.node_type.as_str() {
    //             "skylake-func" | "skylake-switch" => {
    //                 child_result = node_handler.execute_node(
    //                     child_node, exec_context, steps, options.include_steps
    //                 ).await;
    //
    //                 if let Err(ref e) = child_result {
    //                     debug!("[transaction] 子节点执行失败: node_id={}, error={:?}", child_node.id, e);
    //                     break;  // 失败时跳出循环
    //                 }
    //                 debug!("[transaction] 子节点执行成功: node_id={}", child_node.id);
    //             }
    //             _ => {
    //                 warn!("事务框内遇到不支持的节点类型: {}", child_node.node_type);
    //             }
    //         }
    //     }
    //
    //     // 步骤6: 根据执行结果决定提交还是回滚
    //     if child_result.is_err() {
    //         warn!("[transaction] 子节点执行失败，回滚事务: txn_id={}", txn_id);
    //         exec_context.svr_context.clear_txn_id();
    //         // drop(txn_guard) 会触发自动回滚
    //         drop(txn_guard);
    //         return child_result;
    //     }
    //
    //     // 步骤7: 提交事务
    //     debug!("[transaction] 准备提交事务: txn_id={}", txn_id);
    //     txn_guard.commit().await
    //         .map_err(|e| ServiceError::InternalError(format!("提交事务失败: {}", e)))?;
    //
    //     exec_context.svr_context.clear_txn_id();
    //     info!("事务已提交: node_id={}, db_id={}", transaction_node.id, db_id);
    //     debug!("[transaction] 事务提交成功: node_id={}", transaction_node.id);
    //
    //     Ok(())
    // }

}
