//! 编排器类型定义
//!
//! 包含编排执行结果、步骤记录、执行上下文、错误信息和执行选项等核心类型。

use std::collections::HashMap;
use cmx_core::model::service::SVRContext;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// 执行步骤状态枚举
///
/// 用于标识每个编排步骤的执行结果状态
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq)]
pub enum StepStatus {
    /// 执行成功 - 节点函数正常返回
    Success,
    /// 执行失败 - 节点函数抛出异常或返回错误
    Failed,
    /// 跳过 - 节点被跳过未执行（如条件分支未命中）
    Skipped,
    /// 调试暂停 - 节点被调试模式拦截，未实际执行
    DebugPaused,
}

/// 执行步骤记录
///
/// 记录单个节点的执行情况，包括状态、输出和耗时。
/// 每个步骤对应 Flow JSON 中的一个节点执行结果。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ExecutionStep {
    /// 节点ID（对应 Flow JSON 中的 node.id）
    pub node_id: String,
    /// 节点名称（对应 Flow JSON 中的 node.data.name，用于日志和调试）
    pub node_name: String,
    /// 节点类型（如 skylake-func、skylake-switch、skylake-transaction）
    pub node_type: String,
    /// 步骤执行状态（Success/Failed/Skipped）
    pub status: StepStatus,
    /// 步骤输出（函数执行结果，失败时可能为 None）
    pub output: Option<serde_json::Value>,
    /// 执行耗时（微秒，用于性能分析）
    pub elapsed_us: u64,
    /// 步骤级错误信息（失败时包含具体错误描述，成功时为 None）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// 上一步的输出（失败时便于排错，记录失败前的数据上下文）
    /// 成功时为 None，序列化时跳过
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_output: Option<serde_json::Value>,
}

/// 编排执行结果
///
/// 包含整个服务编排执行的完整结果信息，作为 API 响应返回给调用方。
/// 成功时包含最终输出，失败时包含结构化错误信息。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OrchestrationResult {
    /// 是否执行成功（所有节点都成功执行则为 true）
    pub success: bool,
    /// 最终输出结果（最后一个节点的输出，失败时为 None）
    pub output: Option<serde_json::Value>,
    /// 各步骤执行记录（按执行顺序记录每个节点的执行情况）
    /// 注意：当 include_steps=false 且成功时，此数组为空
    pub steps: Vec<ExecutionStep>,
    /// 总执行耗时（微秒，从开始到结束的总时间）
    pub total_elapsed_us: u64,
    /// 结构化错误信息（失败时包含失败步骤详情和上一步输出）
    /// 成功时为 None，序列化时跳过
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<OrchestrationError>,
    /// 是否触发了调试暂停
    // #[serde(skip_serializing_if = "Option::is_none")]
    pub debug_triggered: Option<bool>,
    /// 调试准备结果（触发调试暂停时包含调试信息）
    // #[serde(skip_serializing_if = "Option::is_none")]
    pub debug_prepare_result: Option<DebugPrepareResult>,
}

/// 编排错误信息
///
/// 失败时提供错误摘要信息，失败步骤的详细信息（包括 previous_output）
/// 统一记录在 steps 数组中对应步骤的 ExecutionStep 里。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct OrchestrationError {
    /// 错误摘要信息（人类可读的错误描述，适合展示给用户）
    pub message: String,
}

/// 执行上下文 — 在编排执行过程中传递
///
/// 包含当前执行状态和跨函数传递的上下文信息。
/// 这是编排器内部使用的核心数据结构，贯穿整个执行生命周期。
#[derive(Debug, Clone)]
pub struct ExecutionContext {
    /// 当前步骤输出（传递给下一个步骤的输入）
    /// - 第一个函数节点：initial_input（初始输入）
    /// - 后续函数节点：前一个函数的输出（链式传递）
    pub current_output: serde_json::Value,
    /// 服务调用上下文（包含初始入参、请求头、各步骤输出、事务ID）
    /// 在整个编排过程中持续传递和更新，所有函数共享
    pub svr_context: SVRContext,
}

/// 执行选项
///
/// 控制编排执行的附加行为，如是否返回步骤数据、是否开启调试模式。
/// 通过此参数实现生产环境（精简响应）和调试环境（详细响应）的区分。
#[derive(Debug, Clone)]
#[derive(Default)]
pub struct ExecuteOptions {
    /// 是否返回 steps 数据
    /// - false: 仅返回最终结果，steps 为空数组（生产环境推荐，减少数据传输）
    /// - true: 返回所有步骤数据（调试/排错时使用）
    /// - 注意：执行失败时无论此参数设置如何，都会返回步骤数据（便于排错）
    pub include_steps: bool,
    /// 调试选项（包含 debug 开关和 debug_node_id）
    pub debug_options: DebugOptions,
}


impl ExecuteOptions {
    /// 创建执行选项
    ///
    /// # 参数
    /// * `include_steps` - 是否返回步骤数据
    pub fn new(include_steps: bool) -> Self {
        Self {
            include_steps,
            debug_options: DebugOptions::default(),
        }
    }

    /// 设置调试选项（Builder 模式）
    ///
    /// # 参数
    /// * `debug` - 是否开启调试模式
    /// * `debug_node_id` - 调试目标节点ID（开启 debug 时必填）
    pub fn with_debug(mut self, debug: bool, debug_node_id: Option<String>,debug_params: Option<HashMap<String, String>>) -> Self {
        self.debug_options = DebugOptions::new(debug, debug_node_id, debug_params);
        self
    }
}

/// 调试选项
///
/// 控制编排执行的调试行为。当 debug=true 且 debug_node_id 不为空时，
/// 编排器在执行到目标节点时会暂停执行，转而调用 DebugPrepare 进行调试准备。
#[derive(Debug, Clone, Default)]
pub struct DebugOptions {
    /// 是否开启调试模式
    pub debug: bool,
    /// 调试目标节点ID（当编排执行到该节点时暂停）
    pub debug_node_id: Option<String>,
    /// 调试参数（开启 debug 时有）
    pub debug_params: Option<HashMap<String, String>>,
}

impl DebugOptions {
    /// 创建调试选项
    pub fn new(debug: bool, debug_node_id: Option<String>, debug_params: Option<HashMap<String, String>>) -> Self {
        Self { debug, debug_node_id, debug_params }
    }

    /// 判断调试模式是否已启用（debug=true 且 debug_node_id 不为空）
    pub fn is_debug_enabled(&self) -> bool {
        self.debug && self.debug_node_id.is_some()
    }

    /// 判断指定节点是否为调试目标节点
    pub fn is_debug_node(&self, node_id: &str) -> bool {
        self.is_debug_enabled() && self.debug_node_id.as_deref() == Some(node_id)
    }
}

/// 调试准备结果
///
/// 当编排执行到调试目标节点时，由 DebugPrepare 模块收集的调试信息。
/// 包含插件详细信息、code-server URL、节点信息等，供前端发起调试会话。
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DebugPrepareResult {
    /// code-server 在线编辑器 URL
    pub code_server_url: String,
    /// 插件唯一标识
    pub plugin_id: String,
    /// 插件名称
    pub plugin_name: String,
    /// 插件版本
    pub plugin_version: String,
    /// 插件状态（installed/activated/deactivated/error）
    pub plugin_status: String,
    /// 插件安装路径（绝对路径）
    pub plugin_install_path: String,
    /// WASM 文件路径（相对于安装路径）
    pub plugin_wasm_path: Option<String>,
    /// 插件类型（wasm/rhai）
    pub plugin_type: String,
    /// 插件所属域编码
    pub domain_code: String,
    /// 插件所属应用编码
    pub application_code: String,
    /// 插件所属模块编码
    pub module_code: String,
    /// 要调试的函数名称
    pub function_name: String,
    /// 插件源码路径（从 manifest.json 读取）
    pub source_path: Option<String>,
    /// 调试目标节点ID
    pub node_id: String,
    /// 调试目标节点名称
    pub node_name: String,
}
