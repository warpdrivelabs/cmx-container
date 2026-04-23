//! 调试准备模块
//!
//! 当编排执行到调试目标节点时，负责收集调试所需的准备工作信息：
//! - 通过 PluginQuery trait 获取插件详细信息（由 cmx-plugin 提供）
//! - 通过 cmx-debug 获取 code-server 在线编辑器 URL
//!
//! 调试准备完成后，编排器会暂停执行，将上一步输出、initial_input
//! 和调试准备结果一并返回给前端，供前端发起调试会话。

use std::sync::Arc;

use cmx_core::model::service::ServiceNode;
use cmx_traits::PluginQuery;
use tracing::debug;

use crate::error::ServiceError;
use super::types::DebugPrepareResult;

/// 调试准备器
///
/// 在调试目标节点处收集调试所需的插件信息和 code-server URL。
/// 通过 PluginQuery trait（cmx-traits 封装，cmx-plugin 实现）获取插件详情，
/// 保持架构解耦，不直接依赖 cmx-plugin。
pub struct DebugPrepare<'a> {
    /// 插件查询器（用于获取插件快照信息）
    plugin_query: &'a Arc<dyn PluginQuery>,
}

impl<'a> DebugPrepare<'a> {
    /// 创建调试准备器
    pub fn new(plugin_query: &'a Arc<dyn PluginQuery>) -> Self {
        Self { plugin_query }
    }

    /// 执行调试准备工作
    ///
    /// 1. 从节点元信息获取 plugin_id + function_name
    /// 2. 通过 PluginQuery.get_plugin() 获取插件详细信息（PluginSnapshot，由 cmx-plugin 提供）
    /// 3. 通过 cmx_debug::get_code_server_url_async() 获取 code-server URL
    /// 4. 将 previous_output 和 initial_input 填充到结果中
    /// 5. 组装返回结果
    /// 6. 调用 cmx_debug::start_debug_session_async 创建调试会话
    ///
    /// # 参数
    /// * `node` - 调试目标节点
    /// * `previous_output` - 上一步的执行输出（调试目标节点的输入数据）
    /// * `initial_input` - 服务编排的初始输入（来自请求）
    ///
    /// # 返回值
    /// 返回调试准备结果，包含插件详情、code-server URL、节点信息等
    pub async fn prepare(
        &self,
        node: &ServiceNode,
        previous_output: serde_json::Value,
        initial_input: serde_json::Value,
    ) -> Result<DebugPrepareResult, ServiceError> {
        let node_data = node.data.as_ref()
            .ok_or_else(|| ServiceError::InternalError(
                format!("调试节点 {} 缺少 data", node.id)
            ))?;

        let node_meta = node_data.node_meta.as_ref()
            .ok_or_else(|| ServiceError::InternalError(
                format!("调试节点 {} 缺少 nodeMeta", node.id)
            ))?;

        let plugin_id = &node_meta.plugin_id;
        let function_name = &node_meta.function_name;

        debug!(
            "[debug-prepare] 准备调试: node_id={}, plugin_id={}, function={}",
            node.id, plugin_id, function_name
        );

        let plugin_snapshot = self.plugin_query.get_plugin(plugin_id).await
            .map_err(|e| ServiceError::InternalError(e.to_string()))?
            .ok_or_else(|| ServiceError::InternalError(
                format!("插件 {} 未找到", plugin_id)
            ))?;

        let code_server_url = cmx_debug::get_code_server_url_async().await;

        debug!(
            "[debug-prepare] 调试准备完成: code_server_url={}, source_path={:?}",
            code_server_url, plugin_snapshot.source_path
        );

        let wasm_path = plugin_snapshot.wasm_path.clone().unwrap_or_default();
        let source_path = plugin_snapshot.source_path.clone().unwrap_or_default();

        cmx_debug::start_debug_session_async(
            plugin_snapshot.plugin_id.clone(),
            plugin_snapshot.name.clone(),
            plugin_snapshot.version.clone(),
            function_name.clone(),
            wasm_path.clone(),
            source_path.clone(),
            Vec::new(),
            previous_output.clone(),
            initial_input.clone(),
        ).await;

        Ok(DebugPrepareResult {
            code_server_url,
            plugin_id: plugin_snapshot.plugin_id,
            plugin_name: plugin_snapshot.name,
            plugin_version: plugin_snapshot.version,
            plugin_status: plugin_snapshot.status,
            plugin_install_path: plugin_snapshot.install_path,
            plugin_wasm_path: plugin_snapshot.wasm_path,
            plugin_type: plugin_snapshot.plugin_type,
            domain_code: plugin_snapshot.domain_code,
            application_code: plugin_snapshot.application_code,
            module_code: plugin_snapshot.module_code,
            source_path: plugin_snapshot.source_path,
            function_name: function_name.clone(),
            node_id: node.id.clone(),
            node_name: node_data.name.clone(),
        })
    }
}
