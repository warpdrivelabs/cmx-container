//! 编排执行器 V2
//!
//! 支持服务编排 JSON 格式、多分支节点、事务框和 SVRContext 上下文传递。
//!
//! # 核心概念
//!
//! - **服务编排**: 基于 Flow JSON 定义的 DAG（有向无环图）流程执行
//! - **节点类型**: start（开始）、end（结束）、func（函数）、switch（多分支）、transaction（事务框）
//! - **执行上下文**: SVRContext 在函数间传递初始入参、请求头、各步骤输出
//! - **事务框**: 父节点指向事务框的函数在同一个数据库事务中执行

// ==================== 依赖导入 ====================

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use cmx_core::model::service::{
    FunctionInput, FunctionOutput, SVRContext,
    ServiceNode,
};
use cmx_database::transaction::begin_transaction_guard_by_db_id;
use cmx_traits::{PluginQuery, RuntimeInvoker, ServiceQuery};
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};
use utoipa::ToSchema;

use crate::error::ServiceError;

// ==================== 结果结构体定义 ====================

/// 编排执行结果 V2
///
/// 包含整个服务编排执行的完整结果信息
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OrchestrationResultV2 {
    /// 是否执行成功（所有节点都成功执行则为 true）
    pub success: bool,
    /// 最终输出结果（最后一个节点的输出，失败时为 None）
    pub output: Option<String>,
    /// 各步骤执行记录（按执行顺序记录每个节点的执行情况）
    pub steps: Vec<ExecutionStep>,
    /// 总执行耗时（微秒，从开始到结束的总时间）
    pub total_elapsed_us: u64,
}

/// 执行步骤记录
///
/// 记录单个节点的执行情况
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ExecutionStep {
    /// 节点ID（对应 Flow JSON 中的 node.id）
    pub node_id: String,
    /// 节点名称（对应 Flow JSON 中的 node.data.name）
    pub node_name: String,
    /// 步骤输出（函数执行结果，失败时可能为 None）
    pub output: Option<String>,
    /// 执行耗时（微秒，单个节点的执行时间）
    pub elapsed_us: u64,
}

// ==================== 执行上下文定义 ====================

/// 执行上下文 — 在编排执行过程中传递
///
/// 包含当前执行状态和跨函数传递的上下文信息
#[derive(Debug, Clone)]
pub struct ExecutionContext {
    /// 当前步骤输出（传递给下一个步骤的输入）
    /// - 第一个函数节点：initial_input
    /// - 后续函数节点：前一个函数的输出
    pub current_output: String,
    /// 服务调用上下文（包含初始入参、请求头、各步骤输出）
    /// 在整个编排过程中持续传递和更新
    pub svr_context: SVRContext,
}

// ==================== 编排执行器定义 ====================

/// 编排执行器 V2
///
/// 支持基于 Flow JSON 的 DAG 编排执行，包括：
/// - 线性流程执行：start -> func -> func -> end
/// - 事务框支持：多个函数在同一个数据库事务中执行
/// - 多分支路由：switch 节点根据返回值选择执行路径
/// - SVRContext 上下文传递：初始入参、请求头、各步骤输出在函数间传递
pub struct OrchestratorV2 {
    /// WASM 运行时调用器（用于调用插件函数）
    runtime: Arc<dyn RuntimeInvoker>,
    /// 插件查询器（用于查询插件状态和获取 WASM 路径）
    plugin_query: Arc<dyn PluginQuery>,
    /// 服务查询器（用于获取服务编排定义）
    service_query: Arc<dyn ServiceQuery>,
    /// 默认数据库ID（事务框未指定数据库时使用）
    default_db_id: String,
}

impl OrchestratorV2 {
    /// 创建编排执行器
    ///
    /// # 参数
    /// * `runtime` - WASM 运行时调用器
    /// * `plugin_query` - 插件查询器
    /// * `service_query` - 服务查询器
    ///
    /// # 返回值
    /// 返回编排执行器实例
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
            // 默认数据库ID，事务框未指定时使用
            default_db_id,
        }
    }

    /// 设置默认数据库ID（Builder 模式）
    ///
    /// # 参数
    /// * `db_id` - 数据库ID
    ///
    /// # 返回值
    /// 返回修改后的 Self，支持链式调用
    pub fn with_db_id(
        mut self,
        db_id: impl Into<String>,
    ) -> Self {
        self.default_db_id = db_id.into();
        self
    }

    /// 执行服务编排（核心入口方法）
    ///
    /// 根据服务编排定义，从 start 节点开始，沿边遍历执行各节点，
    /// 直到遇到 end 节点或发生错误。
    ///
    /// # 参数
    /// * `service_key` - 服务唯一标识（对应 JSON 中的 code 字段）
    /// * `initial_input` - 初始输入数据（API 请求传入的参数）
    /// * `headers` - HTTP 请求头（传递给所有函数）
    ///
    /// # 返回值
    /// 返回编排执行结果，包含成功状态、最终输出、各步骤记录、总耗时
    ///
    /// # 执行流程
    /// 1. 加载服务编排定义
    /// 2. 初始化执行上下文（SVRContext）
    /// 3. 从 start 节点开始循环执行
    /// 4. 根据节点类型分发执行
    /// 5. 收集执行结果，返回最终输出
    pub async fn execute_service(
        &self,
        service_key: &str,
        initial_input: &str,
        headers: HashMap<String, String>,
    ) -> Result<OrchestrationResultV2, ServiceError> {
        // ==================== 初始化阶段 ====================

        // 记录开始时间，用于计算总耗时
        let start_time = Instant::now();

        // 初始化步骤记录列表，用于记录每个节点的执行情况
        let mut steps = Vec::new();

        // 从服务查询器获取编排定义
        // 如果服务不存在，返回错误
        let orchestration = self.service_query.get_orchestration(service_key).await
            .map_err(|e| ServiceError::InternalError(e.to_string()))?
            .ok_or_else(|| ServiceError::InternalError(format!("服务未找到: {}", service_key)))?;

        let orchestration_debug = &orchestration;
        // dbg!(orchestration_debug);

        // 创建服务调用上下文
        // - initial_input: 初始入参，函数可通过 context.initial_input 获取
        // - headers: 请求头，函数可通过 context.headers 获取
        // - step_outputs: 各步骤输出，执行过程中逐步填充
        let svr_context = SVRContext::new(initial_input.to_string(), headers.clone());

        // 查找开始节点（节点类型为 skylake-start）
        // 开始节点是流程的入口点，不执行任何函数
        let start_node = orchestration.flow.nodes.iter()
            .find(|n| n.node_type == "skylake-start")
            .ok_or_else(|| ServiceError::InternalError("未找到开始节点".to_string()))?;

        // 初始化执行上下文：
        // - current_output 设置为 initial_input，这样第一个函数节点能够接收到初始输入
        // - SVRContext 包含 initial_input 和 headers，在所有函数间传递
        let mut exec_context = ExecutionContext {
            current_output: initial_input.to_string(),
            svr_context,
        };

        // 设置当前节点ID为开始节点，准备进入主循环
        let mut current_node_id = start_node.id.clone();

        // 获取流程定义的引用，避免重复访问
        let flow = &orchestration.flow;

        // 初始化执行结果，Ok(()) 表示成功，Err 表示失败
        let mut result = Ok(());

        // ==================== 主执行循环 ====================
        // 循环遍历节点，直到遇到 end 节点或发生错误
        // 事务状态：当前活跃的事务守卫和对应的事务框ID
        let mut active_txn_guard: Option<cmx_database::transaction::TransactionGuard> = None;
        let mut active_txn_parent_id: Option<String> = None;

        loop {
            // 根据当前节点ID查找节点定义
            let node = match flow.nodes.iter().find(|n| n.id == current_node_id) {
                Some(n) => n,
                None => {
                    // 节点未找到，记录错误并退出循环
                    debug!("节点未找到: node_id={}", current_node_id);
                    result = Err(ServiceError::InternalError(format!("节点未找到: {}", current_node_id)));
                    break;
                }
            };

            debug!(
                ">>> 进入节点: node_id={}, node_type={}, node_name={}",
                node.id, node.node_type, node.data.as_ref().map(|d| d.name.as_str()).unwrap_or("unknown")
            );

            // ==================== 事务状态管理 ====================
            // 检查当前节点的事务归属，决定是否需要开启/提交事务
            let node_parent_id = node.parent.clone();

            match (&active_txn_guard, &node_parent_id) {
                // 情况1: 当前无活跃事务，节点不在事务框中 -> 正常执行
                (None, None) => {
                    debug!("节点不在事务框中，正常执行: node_id={}", node.id);
                }

                // 情况2: 当前无活跃事务，节点在事务框中 -> 开启新事务
                (None, Some(parent_id)) => {
                    debug!("检测到节点归属事务框，开启事务: node_id={}, parent_id={}", node.id, parent_id);

                    // 获取事务框节点以确定数据库ID
                    let db_id = if let Some(txn_node) = flow.nodes.iter().find(|n| n.id == *parent_id && n.node_type == "skylake-transaction") {
                        txn_node.data.as_ref()
                            .and_then(|d| d.node_meta.as_ref())
                            .and_then(|m| m.database_id.clone())
                            .unwrap_or_else(|| self.default_db_id.clone())
                    } else {
                        self.default_db_id.clone()
                    };

                    // 开启事务
                    let txn_guard = begin_transaction_guard_by_db_id(&db_id, Default::default())
                        .await
                        .map_err(|e| ServiceError::InternalError(format!("开启事务失败: {}", e)))?;

                    let txn_id = txn_guard.txn_id().to_string();
                    debug!("事务已开启: txn_id={}, db_id={}", txn_id, db_id);

                    // 设置事务ID到上下文
                    exec_context.svr_context.set_txn_id(txn_id);

                    active_txn_guard = Some(txn_guard);
                    active_txn_parent_id = Some(parent_id.clone());
                }

                // 情况3: 当前有活跃事务，节点在同一个事务框中 -> 继续在事务中执行
                (Some(_), Some(parent_id)) if active_txn_parent_id.as_ref() == Some(parent_id) => {
                    debug!("节点在同一个事务框中，继续在事务中执行: node_id={}, parent_id={}", node.id, parent_id);
                }

                // 情况4: 当前有活跃事务，节点不在事务框中或在不同事务框中 -> 提交当前事务
                (Some(_), _) => {
                    debug!("节点不在当前事务框中，提交事务: node_id={}", node.id);

                    // 提交当前事务
                    if let Some(txn_guard) = active_txn_guard.take() {
                        txn_guard.commit().await
                            .map_err(|e| ServiceError::InternalError(format!("提交事务失败: {}", e)))?;
                        debug!("事务已提交: parent_id={:?}", active_txn_parent_id);
                    }

                    // 清除上下文中的事务ID
                    exec_context.svr_context.clear_txn_id();

                    active_txn_parent_id = None;

                    // 如果节点在新的事务框中，开启新事务
                    if let Some(ref new_parent_id) = node_parent_id {
                        debug!("开启新事务: new_parent_id={}", new_parent_id);

                        let db_id = if let Some(txn_node) = flow.nodes.iter().find(|n| n.id == *new_parent_id && n.node_type == "skylake-transaction") {
                            txn_node.data.as_ref()
                                .and_then(|d| d.node_meta.as_ref())
                                .and_then(|m| m.database_id.clone())
                                .unwrap_or_else(|| self.default_db_id.clone())
                        } else {
                            self.default_db_id.clone()
                        };

                        let txn_guard = begin_transaction_guard_by_db_id(&db_id, Default::default())
                            .await
                            .map_err(|e| ServiceError::InternalError(format!("开启事务失败: {}", e)))?;

                        let txn_id = txn_guard.txn_id().to_string();
                        debug!("新事务已开启: txn_id={}, db_id={}", txn_id, db_id);

                        // 设置事务ID到上下文
                        exec_context.svr_context.set_txn_id(txn_id);

                        active_txn_guard = Some(txn_guard);
                        active_txn_parent_id = Some(new_parent_id.clone());
                    }
                }
            }

            // 根据节点类型分发执行
            match node.node_type.as_str() {
                // ==================== 开始节点 ====================
                // 开始节点不执行任何函数，只是流程的入口标识
                // 查找从开始节点出发的边，跳转到下一个节点
                "skylake-start" => {
                    debug!("执行开始节点: node_id={}", node.id);
                    // 查找从当前节点出发、源端口为 "out" 的边
                    // 开始节点只有一个出口端口 "out"
                    if let Some(next_edge) = flow.edges.iter().find(|e| {
                        e.source_node_id == current_node_id && e.source_port_id == "out"
                    }) {
                        debug!("开始节点跳转: from={} -> to={}", current_node_id, next_edge.target_node_id);
                        // 设置下一个节点ID，继续循环
                        current_node_id = next_edge.target_node_id.clone();
                        continue;
                    }
                    // 没有找到下一条边，退出循环
                    debug!("开始节点无出边，退出循环");
                    break;
                }

                // ==================== 结束节点 ====================
                // 结束节点不执行任何函数，只是流程的出口标识
                // 遇到结束节点，提交活跃事务后退出循环
                "skylake-end" => {
                    debug!("执行结束节点: node_id={}", node.id);

                    // 如果有活跃事务，提交事务
                    if let Some(txn_guard) = active_txn_guard.take() {
                        txn_guard.commit().await
                            .map_err(|e| ServiceError::InternalError(format!("提交事务失败: {}", e)))?;
                        debug!("结束节点提交事务: parent_id={:?}", active_txn_parent_id);

                        // 清除上下文中的事务ID
                        exec_context.svr_context.clear_txn_id();
                    }

                    break;
                }

                // ==================== 函数节点 ====================
                // 函数节点执行 WASM 插件中的函数
                "skylake-func" => {
                    debug!("执行函数节点: node_id={}", node.id);
                    // 执行函数节点（事务ID已设置在 SVRContext 中）
                    result = self.execute_func_node(node, &mut exec_context, &mut steps).await;

                    // 如果执行失败，退出循环
                    if result.is_err() {
                        debug!("函数节点执行失败: node_id={}, error={:?}", node.id, result);
                        break;
                    }

                    // 查找从当前节点出发、源端口为 "out" 的边
                    // 函数节点只有一个出口端口 "out"
                    if let Some(next_edge) = flow.edges.iter().find(|e| {
                        e.source_node_id == current_node_id && e.source_port_id == "out"
                    }) {
                        debug!("函数节点执行完成跳转: from={} -> to={}", current_node_id, next_edge.target_node_id);
                        // 设置下一个节点ID，继续循环
                        current_node_id = next_edge.target_node_id.clone();
                        continue;
                    }
                    // 没有找到下一条边，退出循环
                    debug!("函数节点无出边，退出循环");
                    break;
                }

                // ==================== 多分支节点 ====================
                // 多分支节点根据函数返回值选择执行路径
                // 返回值匹配 options 数组中的值，选择对应的出边
                "skylake-switch" => {
                    debug!("执行多分支节点: node_id={}", node.id);
                    // 执行 switch 节点（事务ID已设置在 SVRContext 中）
                    result = self.execute_switch_node(node, &mut exec_context, &mut steps).await;

                    // 如果执行失败，退出循环
                    if result.is_err() {
                        debug!("多分支节点执行失败: node_id={}, error={:?}", node.id, result);
                        break;
                    }

                    // 根据函数返回值构建出边端口ID
                    // 端口ID格式为 "out_{value}"，例如 "out_1"、"out_2"
                    let source_port_id = format!("out_{}", exec_context.current_output);
                    debug!("多分支节点执行完成，选择分支: node_id={}, output={}, port={}", node.id, exec_context.current_output, source_port_id);

                    // 查找匹配的出边
                    if let Some(next_edge) = flow.edges.iter().find(|e| {
                        e.source_node_id == current_node_id && e.source_port_id == source_port_id
                    }) {
                        debug!("多分支节点跳转: from={} -> to={}", current_node_id, next_edge.target_node_id);
                        // 设置下一个节点ID，继续循环
                        current_node_id = next_edge.target_node_id.clone();
                        continue;
                    }
                    // 没有找到匹配的出边，退出循环
                    debug!("多分支节点无匹配出边，退出循环");
                    break;
                }

                // ==================== 事务框节点 ====================
                // 事务框节点将其内部的所有函数节点在同一个数据库事务中执行
                "skylake-transaction" => {
                    debug!("执行事务框节点: node_id={}", node.id);
                    // 执行事务框节点
                    result = self.execute_transaction_node(flow, node, &mut exec_context, &mut steps).await;

                    // 如果执行失败，退出循环
                    if result.is_err() {
                        debug!("事务框节点执行失败: node_id={}, error={:?}", node.id, result);
                        break;
                    }

                    // 查找从当前节点出发、源端口为 "out" 的边
                    // 事务框节点只有一个出口端口 "out"
                    if let Some(next_edge) = flow.edges.iter().find(|e| {
                        e.source_node_id == current_node_id && e.source_port_id == "out"
                    }) {
                        debug!("事务框节点执行完成跳转: from={} -> to={}", current_node_id, next_edge.target_node_id);
                        // 设置下一个节点ID，继续循环
                        current_node_id = next_edge.target_node_id.clone();
                        continue;
                    }
                    // 没有找到下一条边，退出循环
                    debug!("事务框节点无出边，退出循环");
                    break;
                }

                // ==================== 未知节点类型 ====================
                // 遇到未知的节点类型，返回错误
                _ => {
                    debug!("遇到未知节点类型: node_id={}, node_type={}", node.id, node.node_type);
                    result = Err(ServiceError::InternalError(format!("未知节点类型: {}", node.node_type)));
                    break;
                }
            }
        }

        // ==================== 循环退出后事务处理 ====================
        // 如果循环退出时仍有活跃事务，说明是异常退出，需要回滚事务
        if let Some(txn_guard) = active_txn_guard.take() {
            if result.is_err() {
                warn!("执行失败，回滚事务: parent_id={:?}", active_txn_parent_id);
                // 事务守卫在 drop 时会自动回滚，这里显式记录日志
                drop(txn_guard);
            } else {
                // 正常退出但仍有事务，提交事务
                debug!("循环正常退出，提交剩余事务: parent_id={:?}", active_txn_parent_id);
                txn_guard.commit().await
                    .map_err(|e| ServiceError::InternalError(format!("提交事务失败: {}", e)))?;
            }
        }

        // ==================== 构建返回结果 ====================

        // 根据执行结果确定最终输出
        // 成功时返回当前输出，失败时返回 None
        let final_output = match &result {
            Ok(_) => Some(exec_context.current_output.clone()),
            Err(_) => None,
        };

        // 构建并返回编排执行结果
        Ok(OrchestrationResultV2 {
            success: result.is_ok(),
            output: final_output,
            steps,
            total_elapsed_us: start_time.elapsed().as_micros() as u64,
        })
    }

    /// 执行 switch（多分支）节点
    ///
    /// 多分支节点执行一个函数，根据函数返回值选择执行路径。
    /// 返回值匹配 options 数组中的值，选择对应的出边（out_{value}）。
    ///
    /// # 参数
    /// * `node` - switch 节点定义
    /// * `exec_context` - 执行上下文（可变，会更新 current_output 和 step_outputs）
    /// * `steps` - 执行步骤记录列表（可变，会添加新记录）
    ///
    /// # 返回值
    /// 成功返回 Ok(())，失败返回 ServiceError
    ///
    /// # 执行流程
    /// 1. 获取插件ID和函数名
    /// 2. 检查插件是否激活
    /// 3. 加载 WASM 模块（如果未加载）
    /// 4. 构建函数输入（包含当前输出和上下文）
    /// 5. 调用函数
    /// 6. 解析输出，更新执行上下文
    /// 7. 记录执行步骤
    async fn execute_switch_node(
        &self,
        node: &ServiceNode,
        exec_context: &mut ExecutionContext,
        steps: &mut Vec<ExecutionStep>,
    ) -> Result<(), ServiceError> {
        // ==================== 获取节点元信息 ====================

        let node_data = node.data.as_ref().ok_or_else(|| ServiceError::InternalError("switch 节点缺少 data".to_string()))?;
        debug!("[switch] 开始执行: node_id={}, node_name={}, txn_id={:?}", node.id, node_data.name, exec_context.svr_context.txn_id);

        // 获取节点的函数元信息（插件ID、函数名等）
        let node_meta = node_data.node_meta.as_ref()
            .ok_or_else(|| ServiceError::InternalError("switch 节点缺少 nodeMeta".to_string()))?;

        // 提取插件ID和函数名
        let plugin_id = &node_meta.plugin_id;
        let function_name = &node_meta.function_name;
        debug!("[switch] 调用函数: plugin_id={}, function={}", plugin_id, function_name);

        // ==================== 检查插件状态 ====================

        // 检查插件是否已激活（已安装且未禁用）
        // let is_active = self.plugin_query.is_active(plugin_id).await
        //     .map_err(|e| ServiceError::InternalError(e.to_string()))?;
        //
        // // 如果插件未激活，返回错误
        // if !is_active {
        //     debug!("[switch] 插件未激活: plugin_id={}", plugin_id);
        //     return Err(ServiceError::plugin_not_active(plugin_id));
        // }

        // ==================== 加载 WASM 模块 ====================

        // 检查 WASM 模块是否已加载到运行时
        if !self.runtime.is_loaded(plugin_id).await {
            // 获取 WASM 文件路径
            let wasm_path = self.plugin_query.get_wasm_path(plugin_id).await
                .map_err(|e| ServiceError::InternalError(e.to_string()))?;
            debug!("[switch] 加载 WASM 模块: plugin_id={}, path={}", plugin_id, wasm_path.display());

            // 加载 WASM 模块到运行时
            self.runtime.load_module(plugin_id, &wasm_path).await
                .map_err(|e| ServiceError::InvokeFailed(e.to_string()))?;
        }

        // ==================== 构建函数输入 ====================

        // 构建函数输入结构体
        // - input: 当前输出（前一个节点的输出或初始输入）
        // - context: 服务调用上下文（包含初始入参、请求头、各步骤输出、txn_id）
        let func_input = FunctionInput {
            input: exec_context.current_output.clone(),
            context: exec_context.svr_context.clone(),
        };

        debug!("[switch] 函数输入: input={}, txn_id={:?}", func_input.input, func_input.context.txn_id);

        // 将函数输入序列化为 JSON 字节数组
        let input_bytes = serde_json::to_vec(&func_input)
            .map_err(|e| ServiceError::InputParseError(e.to_string()))?;

        // ==================== 调用函数 ====================

        // 记录步骤开始时间
        let step_start = Instant::now();

        // 调用 WASM 函数
        let invoke_result = self.runtime
            .invoke(plugin_id, function_name, &input_bytes)
            .await
            .map_err(|e| ServiceError::InvokeFailed(e.to_string()))?;

        // ==================== 解析函数输出 ====================

        // 将函数输出反序列化为 FunctionOutput 结构体
        let output: FunctionOutput = serde_json::from_slice(&invoke_result.output)
            .map_err(|e| ServiceError::OutputSerializeError(e.to_string()))?;

        // ==================== 更新执行上下文 ====================

        // 计算步骤耗时
        let elapsed_us = step_start.elapsed().as_micros() as u64;
        debug!("[switch] 函数执行完成: node_id={}, output={}, elapsed_us={}", node.id, output.result, elapsed_us);

        // 更新当前输出（用于选择出边和传递给下一个节点）
        exec_context.current_output = output.result.clone();

        // 将步骤输出保存到上下文中（后续节点可通过 context.step_outputs 获取）
        exec_context.svr_context.add_step_output(node.id.clone(), output.result.clone());

        // ==================== 记录执行步骤 ====================

        // 将执行步骤添加到记录列表
        steps.push(ExecutionStep {
            node_id: node.id.clone(),
            node_name: node_data.name.clone(),
            output: Some(output.result.clone()),
            elapsed_us,
        });

        debug!("[switch] 执行完成: node_id={}, 选择分支={}", node.id, exec_context.current_output);

        Ok(())
    }

    /// 执行普通函数节点
    ///
    /// 执行 WASM 插件中的函数，更新执行上下文。
    ///
    /// # 参数
    /// * `node` - 函数节点定义
    /// * `exec_context` - 执行上下文（可变，会更新 current_output 和 step_outputs）
    /// * `steps` - 执行步骤记录列表（可变，会添加新记录）
    ///
    /// # 返回值
    /// 成功返回 Ok(())，失败返回 ServiceError
    ///
    /// # 执行流程
    /// 1. 获取插件ID和函数名
    /// 2. 检查插件是否激活
    /// 3. 加载 WASM 模块（如果未加载）
    /// 4. 构建函数输入（包含当前输出和上下文）
    /// 5. 调用函数
    /// 6. 解析输出，更新执行上下文
    /// 7. 记录执行步骤
    async fn execute_func_node(
        &self,
        node: &ServiceNode,
        exec_context: &mut ExecutionContext,
        steps: &mut Vec<ExecutionStep>,
    ) -> Result<(), ServiceError> {
        // ==================== 获取节点元信息 ====================
        let node_data = node.data.as_ref().ok_or_else(|| ServiceError::InternalError("func 节点缺少 data".to_string()))?;
        debug!("[func] 开始执行: node_id={}, node_name={}, txn_id={:?}", node.id, node_data.name, exec_context.svr_context.txn_id);

        // 获取节点的函数元信息（插件ID、函数名等）
        let node_meta = node_data.node_meta.as_ref()
            .ok_or_else(|| ServiceError::InternalError("func 节点缺少 nodeMeta".to_string()))?;

        // 提取插件ID和函数名
        let plugin_id = &node_meta.plugin_id;
        let function_name = &node_meta.function_name;
        debug!("[func] 调用函数: plugin_id={}, function={}", plugin_id, function_name);

        // ==================== 检查插件状态 ====================

        // 检查插件是否已激活（已安装且未禁用）
        // let is_active = self.plugin_query.is_active(plugin_id).await
        //     .map_err(|e| ServiceError::InternalError(e.to_string()))?;
        //
        // // 如果插件未激活，返回错误
        // if !is_active {
        //     debug!("[func] 插件未激活: plugin_id={}", plugin_id);
        //     return Err(ServiceError::plugin_not_active(plugin_id));
        // }

        // ==================== 加载 WASM 模块 ====================

        // 检查 WASM 模块是否已加载到运行时
        if !self.runtime.is_loaded(plugin_id).await {
            // 获取 WASM 文件路径
            let wasm_path = self.plugin_query.get_wasm_path(plugin_id).await
                .map_err(|e| ServiceError::InternalError(e.to_string()))?;
            debug!("[func] 加载 WASM 模块: plugin_id={}, path={}", plugin_id, wasm_path.display());

            // 加载 WASM 模块到运行时
            self.runtime.load_module(plugin_id, &wasm_path).await
                .map_err(|e| ServiceError::InvokeFailed(e.to_string()))?;
        }

        // ==================== 构建函数输入 ====================

        // 构建函数输入结构体
        // - input: 当前输出（前一个节点的输出或初始输入）
        // - context: 服务调用上下文（包含初始入参、请求头、各步骤输出、txn_id）
        let func_input = FunctionInput {
            input: exec_context.current_output.clone(),
            context: exec_context.svr_context.clone(),
        };
        debug!("[func] 函数输入: input={}, txn_id={:?}", func_input.input, func_input.context.txn_id);

        // 将函数输入序列化为 JSON 字节数组
        let input_bytes = serde_json::to_vec(&func_input)
            .map_err(|e| ServiceError::InputParseError(e.to_string()))?;

        // ==================== 调用函数 ====================

        // 记录步骤开始时间
        let step_start = Instant::now();

        // 调用 WASM 函数
        let invoke_result = self.runtime
            .invoke(plugin_id, function_name, &input_bytes)
            .await
            .map_err(|e| ServiceError::InvokeFailed(e.to_string()))?;

        // ==================== 解析函数输出 ====================

        // 将函数输出反序列化为 FunctionOutput 结构体
        let output: FunctionOutput = serde_json::from_slice(&invoke_result.output)
            .map_err(|e| ServiceError::OutputSerializeError(e.to_string()))?;

        // ==================== 更新执行上下文 ====================

        // 计算步骤耗时
        let elapsed_us = step_start.elapsed().as_micros() as u64;
        debug!("[func] 函数执行完成: node_id={}, output={}, elapsed_us={}", node.id, output.result, elapsed_us);

        // 更新当前输出（传递给下一个节点）
        exec_context.current_output = output.result.clone();

        // 将步骤输出保存到上下文中（后续节点可通过 context.step_outputs 获取）
        exec_context.svr_context.add_step_output(node.id.clone(), output.result.clone());

        // ==================== 记录执行步骤 ====================

        // 将执行步骤添加到记录列表
        steps.push(ExecutionStep {
            node_id: node.id.clone(),
            node_name: node_data.name.clone(),
            output: Some(output.result),
            elapsed_us,
        });

        debug!("[func] 执行完成: node_id={}", node.id);

        Ok(())
    }

    /// 执行事务框节点
    ///
    /// 事务框节点将其内部的所有函数节点在同一个数据库事务中执行。
    /// 子节点通过 parent 字段指向事务框节点ID。
    ///
    /// # 参数
    /// * `flow` - 流程定义（用于查找子节点）
    /// * `transaction_node` - 事务框节点定义
    /// * `exec_context` - 执行上下文（可变，会更新 current_output 和 step_outputs）
    /// * `steps` - 执行步骤记录列表（可变，会添加新记录）
    ///
    /// # 返回值
    /// 成功返回 Ok(())，失败返回 ServiceError
    ///
    /// # 执行流程
    /// 1. 确定数据库ID（从节点元信息获取或使用默认值）
    /// 2. 开启数据库事务
    /// 3. 收集所有子节点（parent 指向事务框的节点）
    /// 4. 依次执行子节点（在事务中）
    /// 5. 提交事务
    ///
    /// # 事务特性
    /// - 所有子节点共享同一个事务
    /// - 任一子节点失败，整个事务回滚
    /// - 事务ID传递给子节点，子节点使用同一事务执行 SQL
    async fn execute_transaction_node(
        &self,
        flow: &cmx_core::model::service::ServiceFlow,
        transaction_node: &ServiceNode,
        exec_context: &mut ExecutionContext,
        steps: &mut Vec<ExecutionStep>,
    ) -> Result<(), ServiceError> {
        // ==================== 日志记录 ====================

        info!("事务框开始: node_id={}", transaction_node.id);
        let node_data = transaction_node.data.as_ref().ok_or_else(|| ServiceError::InternalError("switch 节点缺少 data".to_string()))?;
        debug!("[transaction] 开始执行: node_id={}, node_name={}", transaction_node.id, node_data.name);

        // ==================== 确定数据库ID ====================

        // 从节点元信息获取数据库ID，如果未指定则使用默认值
        let db_id = node_data.node_meta.as_ref()
            .and_then(|m| m.database_id.clone())
            .unwrap_or_else(|| self.default_db_id.clone());
        debug!("[transaction] 数据库ID: db_id={}", db_id);

        // ==================== 开启事务 ====================

        // 通过数据库管理器开启事务
        // 返回事务守卫，用于控制事务的提交和回滚
        let txn_guard = begin_transaction_guard_by_db_id(&db_id, Default::default())
            .await
            .map_err(|e| ServiceError::InternalError(format!("开启事务失败: {}", e)))?;

        let txn_id = txn_guard.txn_id().to_string();
        debug!("[transaction] 事务已开启: db_id={}, txn_id={}", db_id, txn_id);

        // 设置事务ID到上下文
        exec_context.svr_context.set_txn_id(txn_id.clone());

        // ==================== 收集子节点 ====================

        // 查找所有 parent 字段指向当前事务框节点的子节点
        // 这些子节点将在同一个事务中执行
        let child_nodes: Vec<&ServiceNode> = flow.nodes.iter()
            .filter(|n| n.parent.as_ref() == Some(&transaction_node.id))
            .collect();
        debug!("[transaction] 找到 {} 个子节点", child_nodes.len());

        // ==================== 执行子节点 ====================

        // 依次执行每个子节点
        for (idx, child_node) in child_nodes.iter().enumerate() {
            debug!("[transaction] 开始执行子节点 {}/{}: node_id={}", idx + 1, child_nodes.len(), child_node.id);
            // 根据节点类型分发执行
            match child_node.node_type.as_str() {
                // 函数节点：在事务中执行
                "skylake-func" => {
                    // 执行函数节点（事务ID已设置在 SVRContext 中）
                    self.execute_func_node(child_node, exec_context, steps).await?;
                    debug!("[transaction] 子节点执行成功: node_id={}", child_node.id);
                }
                // 多分支节点：在事务中执行
                "skylake-switch" => {
                    // 执行 switch 节点（事务ID已设置在 SVRContext 中）
                    self.execute_switch_node(child_node, exec_context, steps).await?;
                    debug!("[transaction] switch 子节点执行成功: node_id={}", child_node.id);
                }
                // 其他节点类型：记录警告，跳过
                _ => {
                    warn!("事务框内遇到不支持的节点类型: {}", child_node.node_type);
                }
            }
        }

        // ==================== 提交事务 ====================

        // 所有子节点执行成功，提交事务
        debug!("[transaction] 准备提交事务: txn_id={}", txn_id);
        txn_guard.commit().await
            .map_err(|e| ServiceError::InternalError(format!("提交事务失败: {}", e)))?;

        // 清除上下文中的事务ID
        exec_context.svr_context.clear_txn_id();

        info!("事务已提交: node_id={}, db_id={}", transaction_node.id, db_id);
        debug!("[transaction] 事务提交成功: node_id={}", transaction_node.id);

        Ok(())
    }
}
