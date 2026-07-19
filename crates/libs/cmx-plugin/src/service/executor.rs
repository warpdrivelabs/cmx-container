//! 插件操作编排器模块。
//!
//! 统一编排持久化 → 运行时 → 事件发布的完整流程，
//! 替代当前散布在各服务中的重复编排逻辑。
//!
//! # 操作模式
//!
//! - **本地操作**（`execute_install` 等）：当前节点接收 API 请求后执行，
//!   发布完整的 GlobalEventBus 事件 + Redis 跨实例通知。

use std::sync::Arc;

use crate::audit::logger::AuditLogger;
use crate::error::PluginResult;
use crate::service::event_publisher::EventPublisher;
use crate::service::persistence::PluginPersistence;
use crate::service::runtime_ops::RuntimeOps;

/// 插件操作编排器。
///
/// 统一编排持久化 → 运行时 → 审计日志 → 事件发布的完整流程，
/// 替代当前散布在各服务中的重复编排逻辑。
///
/// # 设计决策
///
/// 保持独立方法而非策略模式。原因：不同操作的 Request/Response
/// 类型不同，策略模式会引入运行时分支和参数膨胀，降低类型安全性。
///
/// # 编排顺序
///
/// 每个操作统一按以下顺序执行：
/// 1. 持久化（数据库 + 文件系统）
/// 2. 运行时注册/更新/注销（内存 + 缓存）
/// 3. 审计日志（记录操作结果和耗时）
/// 4. 事件发布（GlobalEventBus + Redis 通知）
pub struct PluginOperationExecutor {
    persistence: PluginPersistence,
    runtime: Arc<RuntimeOps>,
    event_publisher: EventPublisher,
    audit_logger: Arc<AuditLogger>,
}

impl PluginOperationExecutor {
    /// 创建插件操作编排器。
    ///
    /// # Arguments
    ///
    /// * `persistence` - 持久化操作层，负责数据库和文件系统操作
    /// * `runtime` - 运行时操作层，负责内存注册和缓存更新
    /// * `event_publisher` - 统一事件发布器，负责进程内和跨实例事件
    /// * `audit_logger` - 审计日志记录器
    pub fn new(
        persistence: PluginPersistence,
        runtime: Arc<RuntimeOps>,
        event_publisher: EventPublisher,
        audit_logger: Arc<AuditLogger>,
    ) -> Self {
        Self {
            persistence,
            runtime,
            event_publisher,
            audit_logger,
        }
    }

    // ===== 本地操作 =====

    /// 安装插件（完整流程：持久化 + 运行时 + 审计 + 事件）。
    ///
    /// 当前节点接收 API 请求后调用，执行完整的安装流程，
    /// 并发布 GlobalEventBus 事件和 Redis 跨实例通知。
    ///
    /// # Errors
    ///
    /// 当持久化失败、运行时注册失败时返回错误。
    /// 审计日志写入失败仅输出警告，不影响操作结果。
    pub async fn execute_install(
        &self,
        request: crate::service::install::InstallRequest,
    ) -> PluginResult<crate::service::install::InstallResponse> {
        let start_time = std::time::Instant::now();

        // 1. 持久化：数据库写入 + 文件安装
        let persist_result = self.persistence.install_persist(request).await?;

        // 2. 运行时注册：写入 Registry + Contexts + Cache
        self.runtime.register_plugin(&persist_result).await?;

        // 3. 审计日志：记录操作结果和耗时
        let duration_ms = start_time.elapsed().as_millis() as i64;
        let audit_record = crate::audit::record::AuditRecord::success(
            persist_result.plugin_id.clone(),
            crate::audit::record::OperationType::Install,
        )
        .with_details(serde_json::json!({
            "version": persist_result.version,
            "install_path": persist_result.install_path.to_string_lossy().to_string(),
        }))
        .with_new_value(persist_result.install_path.to_string_lossy().to_string())
        .with_completed(duration_ms);
        // 审计日志失败不阻塞主流程，仅输出警告
        let _ = self
            .audit_logger
            .log(audit_record)
            .await
            .map_err(|e| {
                tracing::warn!("审计日志写入失败: {}", e);
                e
            })
            .ok();

        // 4. 事件发布：GlobalEventBus 进程内事件 + Redis 跨实例通知
        self.event_publisher
            .publish_installed(&persist_result)
            .await;

        Ok(crate::service::install::InstallResponse {
            plugin_id: persist_result.plugin_id,
            install_path: persist_result.install_path,
            version: persist_result.version,
            success: true,
            message: "插件安装成功".to_string(),
        })
    }

    /// 升级插件（完整流程：持久化 + 运行时 + 审计 + 事件）。
    ///
    /// # Errors
    ///
    /// 当持久化失败、运行时更新失败时返回错误。
    pub async fn execute_upgrade(
        &self,
        request: crate::service::upgrade::UpgradeRequest,
    ) -> PluginResult<crate::service::upgrade::UpgradeResponse> {
        let start_time = std::time::Instant::now();

        // 1. 持久化
        let persist_result = self.persistence.upgrade_persist(request).await?;

        // 2. 运行时更新：更新 Registry + Contexts + Cache 中的版本信息
        self.runtime.update_plugin(&persist_result).await?;

        // 3. 审计日志
        let duration_ms = start_time.elapsed().as_millis() as i64;
        let audit_record = crate::audit::record::AuditRecord::success(
            persist_result.plugin_id.clone(),
            crate::audit::record::OperationType::Upgrade,
        )
        .with_details(serde_json::json!({
            "old_version": persist_result.old_version.as_deref().unwrap_or("unknown"),
            "new_version": persist_result.version,
        }))
        .with_old_value(persist_result.old_version.clone().unwrap_or_default())
        .with_new_value(persist_result.version.clone())
        .with_completed(duration_ms);
        let _ = self
            .audit_logger
            .log(audit_record)
            .await
            .map_err(|e| {
                tracing::warn!("审计日志写入失败: {}", e);
                e
            })
            .ok();

        // 4. 事件发布
        self.event_publisher.publish_upgraded(&persist_result).await;

        Ok(crate::service::upgrade::UpgradeResponse {
            plugin_id: persist_result.plugin_id,
            old_version: persist_result.old_version.unwrap_or_default(),
            new_version: persist_result.version,
            success: true,
            message: "插件升级成功".to_string(),
        })
    }

    /// 降级插件（完整流程：持久化 + 运行时 + 审计 + 事件）。
    ///
    /// # Errors
    ///
    /// 当持久化失败、运行时更新失败时返回错误。
    pub async fn execute_downgrade(
        &self,
        request: crate::service::downgrade::DowngradeRequest,
    ) -> PluginResult<crate::service::downgrade::DowngradeResponse> {
        let start_time = std::time::Instant::now();

        // 1. 持久化
        let persist_result = self.persistence.downgrade_persist(request).await?;

        // 2. 运行时更新
        self.runtime.update_plugin(&persist_result).await?;

        // 3. 审计日志
        let duration_ms = start_time.elapsed().as_millis() as i64;
        let audit_record = crate::audit::record::AuditRecord::success(
            persist_result.plugin_id.clone(),
            crate::audit::record::OperationType::Downgrade,
        )
        .with_details(serde_json::json!({
            "old_version": persist_result.old_version.as_deref().unwrap_or("unknown"),
            "new_version": persist_result.version,
        }))
        .with_old_value(persist_result.old_version.clone().unwrap_or_default())
        .with_new_value(persist_result.version.clone())
        .with_completed(duration_ms);
        let _ = self
            .audit_logger
            .log(audit_record)
            .await
            .map_err(|e| {
                tracing::warn!("审计日志写入失败: {}", e);
                e
            })
            .ok();

        // 4. 事件发布
        self.event_publisher
            .publish_downgraded(&persist_result)
            .await;

        Ok(crate::service::downgrade::DowngradeResponse {
            plugin_id: persist_result.plugin_id,
            old_version: persist_result.old_version.unwrap_or_default(),
            new_version: persist_result.version,
            success: true,
            message: "插件降级成功".to_string(),
        })
    }

    /// 卸载插件（完整流程：持久化 + 运行时 + 审计 + 事件）。
    ///
    /// # Errors
    ///
    /// 当持久化失败、运行时注销失败时返回错误。
    pub async fn execute_uninstall(
        &self,
        request: crate::service::uninstall::UninstallRequest,
    ) -> PluginResult<crate::service::uninstall::UninstallResponse> {
        let start_time = std::time::Instant::now();

        // 1. 持久化：事务内删除数据库记录，提交后删除物理文件
        let persist_result = self.persistence.uninstall_persist(request).await?;

        // 2. 运行时注销：从 Registry + Contexts + Cache 中移除
        self.runtime
            .unregister_plugin(&persist_result.plugin_id)
            .await?;

        // 3. 审计日志
        let duration_ms = start_time.elapsed().as_millis() as i64;
        let audit_record = crate::audit::record::AuditRecord::success(
            persist_result.plugin_id.clone(),
            crate::audit::record::OperationType::Uninstall,
        )
        .with_details(serde_json::json!({
            "version": persist_result.version,
        }))
        .with_old_value(persist_result.version.clone())
        .with_completed(duration_ms);
        let _ = self
            .audit_logger
            .log(audit_record)
            .await
            .map_err(|e| {
                tracing::warn!("审计日志写入失败: {}", e);
                e
            })
            .ok();

        // 4. 事件发布
        self.event_publisher
            .publish_uninstalled(&persist_result)
            .await;

        Ok(crate::service::uninstall::UninstallResponse {
            plugin_id: persist_result.plugin_id,
            success: true,
            message: "插件卸载成功".to_string(),
        })
    }

    /// 覆盖安装插件（完整流程：持久化 + 运行时 + 审计 + 事件）。
    ///
    /// 覆盖安装是先卸载再安装的非原子操作，中间状态依赖一致性校验任务补偿。
    ///
    /// # Arguments
    ///
    /// * `request` - 部署请求，包含插件来源信息
    /// * `plugin_id` - 待覆盖安装的插件 ID（由调用方从元数据解析中获取）
    /// * `old_version` - 旧版本号（用于 `PersistResult::old_version` 和审计日志）
    ///
    /// # Errors
    ///
    /// 当卸载或安装的持久化失败、运行时更新失败时返回错误。
    pub async fn execute_reinstall(
        &self,
        request: crate::service::deploy::DeployRequest,
        plugin_id: &str,
        old_version: &str,
    ) -> PluginResult<crate::service::deploy::DeployResponse> {
        let start_time = std::time::Instant::now();

        // 1. 持久化：先卸载再安装（非原子操作）
        let persist_result = self
            .persistence
            .reinstall_persist(request, plugin_id, old_version)
            .await?;

        // 2. 运行时更新：更新 Registry + Contexts + Cache
        self.runtime.update_plugin(&persist_result).await?;

        // 3. 审计日志
        let duration_ms = start_time.elapsed().as_millis() as i64;
        let audit_record = crate::audit::record::AuditRecord::success(
            persist_result.plugin_id.clone(),
            crate::audit::record::OperationType::Install, // 复用 Install 类型
        )
        .with_details(serde_json::json!({
            "old_version": persist_result.old_version.as_deref().unwrap_or("unknown"),
            "new_version": persist_result.version,
        }))
        .with_completed(duration_ms);
        let _ = self
            .audit_logger
            .log(audit_record)
            .await
            .map_err(|e| {
                tracing::warn!("审计日志写入失败: {}", e);
                e
            })
            .ok();

        // 4. 事件发布
        self.event_publisher
            .publish_reinstalled(&persist_result)
            .await;

        Ok(crate::service::deploy::DeployResponse {
            plugin_id: persist_result.plugin_id,
            action: crate::service::deploy::DeployAction::Reinstall,
            old_version: persist_result.old_version,
            new_version: persist_result.version,
            install_path: persist_result.install_path,
            success: true,
            message: "插件覆盖安装成功".to_string(),
            marketplace_publish: None,
        })
    }
}
