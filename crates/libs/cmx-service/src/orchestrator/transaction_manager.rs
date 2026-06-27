//! 事务状态管理器
//!
//! 跟踪编排执行过程中的事务状态，负责事务的开启、提交和回滚。
//! 核心职责：根据节点的 parent 属性自动管理事务生命周期。

use cmx_core::model::service::{ServiceNode, SVRContext};
use cmx_database::transaction::{begin_transaction_guard_by_db_id, TransactionGuard};
use tracing::{debug, warn};

use crate::error::ServiceError;
use super::flow_navigator::FlowNavigator;

/// 事务状态管理器
///
/// 跟踪当前活跃事务，根据节点的事务框归属关系管理事务生命周期。
///
/// # 事务状态机
/// ```text
/// [无事务] --节点进入事务框--> [有事务] --节点离开事务框--> [提交事务] --> [无事务]
///     |                              |
///     +--节点不在事务框--> 正常执行   +--执行失败--> [回滚事务] --> [无事务]
/// ```
pub struct TransactionManager {
    /// 当前活跃的事务守卫
    /// - Some: 有活跃事务，当前节点在事务框内
    /// - None: 无活跃事务
    active_guard: Option<TransactionGuard>,
    /// 当前活跃事务所属的事务框节点ID
    /// 用于判断下一个节点是否在同一个事务框内
    active_parent_id: Option<String>,
    /// 默认数据库ID
    /// 事务框未指定 database_id 时使用
    default_db_id: String,
}

impl TransactionManager {
    /// 创建事务管理器
    ///
    /// # 参数
    /// * `default_db_id` - 默认数据库ID（事务框未指定数据库时使用）
    pub fn new(default_db_id: String) -> Self {
        Self {
            active_guard: None,
            active_parent_id: None,
            default_db_id,
        }
    }

    /// 根据当前节点的 parent 属性管理事务状态
    ///
    /// 根据节点的事务框归属关系，自动处理事务的开启、提交和切换。
    /// 这是事务管理的核心方法，在执行每个节点前调用。
    ///
    /// # 状态转换规则
    /// | 当前状态 | 节点 parent | 操作 |
    /// |----------|-------------|------|
    /// | 无活跃事务 | None | 无操作，正常执行 |
    /// | 无活跃事务 | Some(id) | 开启新事务 |
    /// | 有活跃事务 | Some(id) 且 id 相同 | 继续在当前事务中执行 |
    /// | 有活跃事务 | None 或 id 不同 | 提交当前事务，必要时开启新事务 |
    ///
    /// # 参数
    /// * `node` - 当前要执行的节点
    /// * `navigator` - 流程导航器（用于解析事务框数据库ID）
    /// * `svr_context` - 服务调用上下文（用于设置/清除事务ID）
    pub async fn ensure_transaction(
        &mut self,
        node: &ServiceNode,
        navigator: &FlowNavigator<'_>,
        svr_context: &mut SVRContext,
    ) -> Result<(), ServiceError> {
        // 获取节点的父节点ID（事务框节点ID）
        let node_parent_id = node.parent.clone();

        match (&self.active_guard, &node_parent_id) {
            // 情况1: 无活跃事务 + 节点不在事务框中 → 正常执行
            // 场景：普通流程节点，无需事务管理
            (None, None) => {
                debug!("节点不在事务框中，正常执行: node_id={}", node.id);
            }

            // 情况2: 无活跃事务 + 节点在事务框中 → 开启新事务
            // 场景：首次进入事务框，需要开启数据库事务
            (None, Some(parent_id)) => {
                self.begin_transaction(parent_id, navigator, svr_context).await?;
            }

            // 情况3: 有活跃事务 + 节点在同一个事务框中 → 继续在事务中执行
            // 场景：事务框内的连续节点，共享同一个事务
            (Some(_), Some(parent_id)) if self.active_parent_id.as_ref() == Some(parent_id) => {
                debug!("节点在同一个事务框中，继续在事务中执行: node_id={}, parent_id={}", node.id, parent_id);
            }

            // 情况4: 有活跃事务 + 节点不在事务框或在不同事务框中 → 提交当前事务
            // 场景：离开当前事务框，或切换到另一个事务框
            (Some(_), _) => {
                // 先提交当前事务
                self.commit_active(svr_context).await?;

                // 如果节点在新的事务框中，开启新事务
                if let Some(ref new_parent_id) = node_parent_id {
                    self.begin_transaction(new_parent_id, navigator, svr_context).await?;
                }
            }
        }

        Ok(())
    }

    /// 提交当前活跃事务
    ///
    /// 在正常流程结束时调用，提交事务框内的所有数据库操作。
    ///
    /// # 参数
    /// * `svr_context` - 服务调用上下文（用于清除事务ID）
    ///
    /// # 注意
    /// 提交后 active_guard 和 active_parent_id 都会被清空
    pub async fn commit_active(&mut self, svr_context: &mut SVRContext) -> Result<(), ServiceError> {
        // take() 会取出值并将 active_guard 设为 None
        if let Some(txn_guard) = self.active_guard.take() {
            // 调用数据库层提交事务
            txn_guard.commit().await
                .map_err(|e| ServiceError::InternalError(format!("提交事务失败: {}", e)))?;
            debug!("事务已提交: parent_id={:?}", self.active_parent_id);

            // 清除上下文中的事务ID，避免影响后续操作
            svr_context.clear_txn_id();
            self.active_parent_id = None;
        }
        Ok(())
    }

    /// 回滚当前活跃事务（异常退出时调用）
    ///
    /// 当编排执行失败时调用，回滚事务框内的所有数据库操作。
    ///
    /// # 注意
    /// - TransactionGuard 在 drop 时会自动回滚，此方法主要用于记录日志
    /// - 回滚后 active_guard 和 active_parent_id 都会被清空
    pub async fn rollback_active(&mut self) {
        if let Some(txn_guard) = self.active_guard.take() {
            warn!("回滚事务: parent_id={:?}", self.active_parent_id);
            // drop(txn_guard) 会触发自动回滚
            drop(txn_guard);
            self.active_parent_id = None;
        }
    }

    /// 检查是否有活跃事务
    ///
    /// # 返回值
    /// - true: 有活跃事务，当前在事务框内
    /// - false: 无活跃事务
    pub fn has_active(&self) -> bool {
        self.active_guard.is_some()
    }

    /// 开启新事务
    ///
    /// 内部方法，在节点首次进入事务框时调用。
    ///
    /// # 参数
    /// * `parent_id` - 事务框节点ID
    /// * `navigator` - 流程导航器（用于解析数据库ID）
    /// * `svr_context` - 服务调用上下文（用于设置事务ID）
    ///
    /// # 流程
    /// 1. 解析事务框的数据库ID
    /// 2. 调用数据库层开启事务
    /// 3. 将事务ID设置到上下文中（传递给 WASM 函数）
    /// 4. 记录活跃事务状态
    async fn begin_transaction(
        &mut self,
        parent_id: &str,
        navigator: &FlowNavigator<'_>,
        svr_context: &mut SVRContext,
    ) -> Result<(), ServiceError> {
        debug!("检测到节点归属事务框，开启事务: parent_id={}", parent_id);

        // 解析事务框使用的数据库ID（可能使用默认值）
        let db_id = navigator.resolve_transaction_db_id(parent_id, &self.default_db_id);

        // 调用数据库层开启事务，获取事务守卫
        let txn_guard = begin_transaction_guard_by_db_id(&db_id, Default::default())
            .await
            .map_err(|e| ServiceError::InternalError(format!("开启事务失败: {}", e)))?;

        // 获取事务ID并设置到上下文中
        // WASM 函数通过 context.txn_id 获取事务ID，用于后续数据库操作
        let txn_id = txn_guard.txn_id().to_string();
        debug!("事务已开启: txn_id={}, db_id={}", txn_id, db_id);

        // 设置事务ID到上下文，传递给 WASM 函数
        svr_context.set_txn_id(txn_id);

        // 记录活跃事务状态
        self.active_guard = Some(txn_guard);
        self.active_parent_id = Some(parent_id.to_string());

        Ok(())
    }
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use cmx_core::model::service::{
        NodeData, NodeMeta, NodePosition, NodeSize, ServiceFlow, ServiceNode,
    };

    /// 构造测试用节点元数据
    fn make_meta() -> NodeMeta {
        NodeMeta {
            z_index: 1,
            size: NodeSize { width: 100, height: 50 },
            position: NodePosition { x: 0.0, y: 0.0 },
        }
    }

    /// 构造一个无 parent 的普通节点（不在事务框中）
    fn make_node_no_parent(id: &str) -> ServiceNode {
        ServiceNode {
            id: id.to_string(),
            node_type: "skylake-func".to_string(),
            parent: None,
            meta: make_meta(),
            data: Some(NodeData {
                name: "普通节点".to_string(),
                node_meta: None,
                inputs: serde_json::Value::Array(vec![]),
                outputs: serde_json::Value::Array(vec![]),
                options: None,
            }),
        }
    }

    /// 构造空流程
    fn make_empty_flow() -> ServiceFlow {
        ServiceFlow {
            nodes: vec![],
            edges: vec![],
        }
    }

    /// 构造测试用 SVRContext
    fn make_svr_context() -> SVRContext {
        use std::collections::HashMap;
        use chrono::Utc;
        SVRContext::new(
            serde_json::Value::Null,
            HashMap::new(),
            Utc::now(),
            "test_request_id".to_string(),
        )
    }

    // ==================== 初始化测试 ====================

    #[test]
    fn new_应创建无活跃事务的管理器() {
        let manager = TransactionManager::new("db_default".to_string());
        assert!(!manager.has_active(), "新创建的事务管理器不应有活跃事务");
    }

    #[test]
    fn has_active_初始状态为false() {
        let manager = TransactionManager::new("db_default".to_string());
        assert!(!manager.has_active());
    }

    // ==================== commit_active 测试 ====================

    #[tokio::test]
    async fn commit_active_无活跃事务时为空操作并返回ok() {
        // 当没有活跃事务时，commit_active 应该是空操作并返回 Ok
        let mut manager = TransactionManager::new("db_default".to_string());
        let mut ctx = make_svr_context();

        // 无活跃事务时调用提交
        let result = manager.commit_active(&mut ctx).await;
        assert!(result.is_ok(), "无活跃事务时 commit 应返回 Ok");
        assert!(!manager.has_active(), "提交后仍应无活跃事务");
        assert!(ctx.txn_id.is_none(), "上下文中不应有事务ID");
    }

    // ==================== rollback_active 测试 ====================

    #[tokio::test]
    async fn rollback_active_无活跃事务时为空操作() {
        // 当没有活跃事务时，rollback_active 应该是空操作，不会 panic
        let mut manager = TransactionManager::new("db_default".to_string());

        // 无活跃事务时调用回滚 - 不应 panic
        manager.rollback_active().await;
        assert!(!manager.has_active(), "回滚后仍应无活跃事务");
    }

    // ==================== ensure_transaction 测试 ====================

    #[tokio::test]
    async fn ensure_transaction_无活跃事务且节点不在事务框中时为空操作() {
        // 情况1: (None, None) - 无活跃事务 + 节点不在事务框中
        // 这是普通流程节点的场景，应直接返回 Ok 而不触发任何 DB 操作
        let mut manager = TransactionManager::new("db_default".to_string());
        let flow = make_empty_flow();
        let navigator = FlowNavigator::new(&flow);
        let mut ctx = make_svr_context();
        let node = make_node_no_parent("func_1");

        let result = manager.ensure_transaction(&node, &navigator, &mut ctx).await;
        assert!(result.is_ok(), "无事务+无parent 应返回 Ok");
        assert!(!manager.has_active(), "不应开启事务");
        assert!(ctx.txn_id.is_none(), "上下文中不应有事务ID");
    }

    #[tokio::test]
    async fn ensure_transaction_无活跃事务时连续处理多个无parent节点() {
        // 模拟线性流程中连续的多个普通节点
        // 每个节点都不在事务框中，不应触发事务开启
        let mut manager = TransactionManager::new("db_default".to_string());
        let flow = make_empty_flow();
        let navigator = FlowNavigator::new(&flow);
        let mut ctx = make_svr_context();

        // 处理第一个节点
        let node1 = make_node_no_parent("func_1");
        let result1 = manager.ensure_transaction(&node1, &navigator, &mut ctx).await;
        assert!(result1.is_ok());
        assert!(!manager.has_active());

        // 处理第二个节点
        let node2 = make_node_no_parent("func_2");
        let result2 = manager.ensure_transaction(&node2, &navigator, &mut ctx).await;
        assert!(result2.is_ok());
        assert!(!manager.has_active());

        // 处理第三个节点
        let node3 = make_node_no_parent("func_3");
        let result3 = manager.ensure_transaction(&node3, &navigator, &mut ctx).await;
        assert!(result3.is_ok());
        assert!(!manager.has_active());

        // 始终没有事务ID
        assert!(ctx.txn_id.is_none());
    }

    // 注意：以下场景需要真实数据库连接，无法在纯单元测试中覆盖：
    // - ensure_transaction (None, Some(parent_id)): 节点进入事务框，触发 begin_transaction
    // - ensure_transaction (Some, Some(同parent)): 节点在同一事务框中继续执行
    // - ensure_transaction (Some, None/不同parent): 节点离开事务框，触发 commit
    // - commit_active 有活跃事务时: 实际提交事务
    // - rollback_active 有活跃事务时: 实际回滚事务
    //
    // 这些场景的完整端到端测试需要数据库环境，建议在集成测试中覆盖。
    // 这里通过 mock 的 Orchestrator 集成测试覆盖"事务开启失败时的错误传播"路径
    // （见 tests/orchestrator_test.rs 中的事务框错误传播测试）。
}
