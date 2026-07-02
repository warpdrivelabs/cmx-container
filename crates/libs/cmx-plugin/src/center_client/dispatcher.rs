//! 服务中心数据分发调度器。
//!
//! 负责将插件安装目录下的各数据子目录并行打包并发送到对应的外部基础服务中心，
//! 或在卸载时并行通知各中心清理数据，最终汇总所有中心的调用结果。

use super::packer::{has_files, pack_directory_to_zip};
use super::sender::ServiceCenterSender;
use super::types::*;
use crate::error::{PluginError, PluginResult};
use futures::future::join_all;
use std::sync::Arc;

/// 服务中心数据分发调度器。
///
/// 持有一个 `ServiceCenterSender` 实例，负责：
/// - 安装/升级/降级时并行打包并发送数据到各中心。
/// - 卸载时并行通知各中心清理数据。
/// - 汇总所有中心的调用结果。
pub struct CenterDataDispatcher {
    sender: Arc<dyn ServiceCenterSender>,
}

impl CenterDataDispatcher {
    /// 创建新的分发调度器。
    ///
    /// # Arguments
    ///
    /// * `sender` - 服务中心发送器实例。
    pub fn new(sender: Arc<dyn ServiceCenterSender>) -> Self {
        Self { sender }
    }

    /// 安装/升级/降级时：并行读取各数据子目录 → 打包 ZIP → 发送到中心 → 汇总结果。
    ///
    /// 对于不存在数据目录的类别会自动跳过。
    /// 使用 `futures::join_all` 并行调用所有中心，最大化吞吐量。
    ///
    /// # Arguments
    ///
    /// * `ctx` - 分发上下文，包含插件信息和安装路径。
    ///
    /// # Returns
    ///
    /// 返回 `DispatchResult` 包含各中心的调用结果。
    /// 调用方需检查 `dispatch_result.is_all_success()` 判断是否全部成功。
    ///
    /// # Errors
    ///
    /// 当 ZIP 打包失败时直接返回错误（无法恢复）。
    /// 中心调用失败记录在 `DispatchResult` 中，不直接返回错误。
    pub async fn dispatch_install(&self, ctx: &DispatchContext) -> PluginResult<DispatchResult> {
        let mut futures = Vec::new();

        for category in DataCategory::all() {
            let dir = ctx.install_path.join(category.dir_name());
            if !dir.exists() {
                tracing::info!(
                    "插件 {} 无 {} 数据目录，跳过",
                    ctx.plugin_id,
                    category.dir_name()
                );
                continue;
            }

            if !has_files(&dir) {
                tracing::info!(
                    "插件 {} {} 数据目录为空，跳过",
                    ctx.plugin_id,
                    category.dir_name()
                );
                continue;
            }

            let zip_data = pack_directory_to_zip(&dir).map_err(|e| {
                PluginError::CenterData(format!(
                    "{}数据目录打包失败: {}",
                    category.center_name(),
                    e
                ))
            })?;

            let request = CenterSendRequest {
                plugin_id: ctx.plugin_id.clone(),
                app_id: ctx.app_id.clone(),
                version: ctx.version.clone(),
                category: *category,
                zip_data,
                zip_file_name: format!("{}.zip", category.dir_name()),
                domain_code: ctx.domain_code.clone(),
                application_code: ctx.application_code.clone(),
                module_code: ctx.module_code.clone(),
            };

            let sender = self.sender.clone();
            let cat = *category;
            futures.push(async move {
                let result = sender.send_data(request).await;
                CategoryResult {
                    category: cat,
                    result,
                }
            });
        }

        let results = join_all(futures).await;
        let dispatch_result = DispatchResult { results };

        log_dispatch_install_results(ctx, &dispatch_result);

        Ok(dispatch_result)
    }

    /// 卸载时：并行通知各中心清理与指定插件关联的数据 → 汇总结果。
    ///
    /// 使用 `futures::join_all` 并行调用所有中心的清理接口。
    ///
    /// # Arguments
    ///
    /// * `ctx` - 分发上下文，包含插件信息。
    ///
    /// # Returns
    ///
    /// 返回 `DispatchResult` 包含各中心的调用结果。
    pub async fn dispatch_cleanup(&self, ctx: &DispatchContext) -> PluginResult<DispatchResult> {
        let mut futures = Vec::new();

        for category in DataCategory::all() {
            let request = CenterCleanupRequest {
                plugin_id: ctx.plugin_id.clone(),
                app_id: ctx.app_id.clone(),
                version: Some(ctx.version.clone()),
                category: *category,
                domain_code: ctx.domain_code.clone(),
                application_code: ctx.application_code.clone(),
                module_code: ctx.module_code.clone(),
            };

            let sender = self.sender.clone();
            let cat = *category;
            futures.push(async move {
                let result = sender.cleanup_data(request).await;
                CategoryResult {
                    category: cat,
                    result,
                }
            });
        }

        let results = join_all(futures).await;
        let dispatch_result = DispatchResult { results };

        log_dispatch_cleanup_results(ctx, &dispatch_result);

        Ok(dispatch_result)
    }
}

fn log_dispatch_install_results(ctx: &DispatchContext, dispatch_result: &DispatchResult) {
    for r in &dispatch_result.results {
        match &r.result {
            Ok(resp) if resp.success => tracing::info!(
                "插件 {} {} 数据推送成功: {}",
                ctx.plugin_id,
                r.category.center_name(),
                resp.message
            ),
            Ok(resp) => tracing::error!(
                "插件 {} {} 数据推送被拒绝: {}",
                ctx.plugin_id,
                r.category.center_name(),
                resp.message
            ),
            Err(e) => tracing::error!(
                "插件 {} {} 数据推送失败: {}",
                ctx.plugin_id,
                r.category.center_name(),
                e
            ),
        }
    }
}

fn log_dispatch_cleanup_results(ctx: &DispatchContext, dispatch_result: &DispatchResult) {
    for r in &dispatch_result.results {
        match &r.result {
            Ok(resp) if resp.success => tracing::info!(
                "插件 {} {} 数据清理成功",
                ctx.plugin_id,
                r.category.center_name()
            ),
            Ok(resp) => tracing::error!(
                "插件 {} {} 数据清理被拒绝: {}",
                ctx.plugin_id,
                r.category.center_name(),
                resp.message
            ),
            Err(e) => tracing::error!(
                "插件 {} {} 数据清理失败: {}",
                ctx.plugin_id,
                r.category.center_name(),
                e
            ),
        }
    }
}
