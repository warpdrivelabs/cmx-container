//! `MenuDefinitionImporter` trait 的本地实现。
//!
//! 直调 [`MenuService`],每个 definition 整体透传到 `cmx_menu.definition`(根菜单)。
//! MenuService::create 会自动计算树形字段(leaf/depth/id_path/code_path)。

use async_trait::async_trait;
use cmx_core::model::module::MenuDefinition;
use cmx_database::DatabaseManager;
use cmx_traits::error::TraitError;
use cmx_traits::resource::MenuDefinitionImporter;
use std::sync::Arc;
use tracing::{error, info, warn};

use crate::menu::{MenuForCreate, MenuService};

/// 归一化 parent_code:None 或空串视为 None(根节点)。
///
/// JSON 导入时根节点的 `parent_code` 可能是 `null`(None)或 `""`(空串),二者都表示根节点。
/// 统一归一化为 None,避免空串被误判为「有父但父 code 为空」导致拓扑排序卡住。
fn effective_parent(parent_code: Option<&str>) -> Option<&str> {
    parent_code.filter(|s| !s.is_empty())
}

/// 本地菜单定义导入器(直调 MenuService)。
pub struct LocalMenuDefinitionImporter {
    mm: Arc<DatabaseManager>,
    db_id: String,
}

impl LocalMenuDefinitionImporter {
    /// 创建本地菜单定义导入器。
    ///
    /// # Arguments
    /// * `mm` - 数据库管理器
    /// * `db_id` - 菜单存储库 ID(default 库)
    pub fn new(mm: Arc<DatabaseManager>, db_id: String) -> Self {
        Self { mm, db_id }
    }
}

#[async_trait]
impl MenuDefinitionImporter for LocalMenuDefinitionImporter {
    async fn apply_menu_definitions(
        &self,
        domain_code: &str,
        app_code: &str,
        module_code: &str,
        definitions: &[MenuDefinition],
    ) -> Result<usize, TraitError> {
        if definitions.is_empty() {
            return Ok(0);
        }
        // 调试：确认调用 importer 时是否已有遗留事务上下文
        // （排查 "count=N 但表里没数据" 的事务复用问题）
        let existing_txn = self.mm.get_transaction_context()
            .begin_with_guard(&self.db_id)
            .await
            .map_err(|e| TraitError::Business(format!("开启菜单导入事务失败: {e}")))?;
        info!(
            db_id = %self.db_id,
            txn_id = %existing_txn.txn_id(),
            "apply_menu_definitions 开启事务"
        );
        let guard = existing_txn;
        let txn_id = guard.txn_id().to_string();

        // 幂等:先删整个模块的旧菜单(含子节点),再整体重建
        let _ =
            MenuService::delete_by_module(&self.mm, &self.db_id, Some(&txn_id), module_code).await;

        // 拓扑排序:parent_code 为 None/空串(根)或父已在 done 集合的节点先建(父先于子)
        let mut pending: Vec<&MenuDefinition> = definitions.iter().collect();
        let mut sorted: Vec<&MenuDefinition> = Vec::with_capacity(definitions.len());
        let mut done_codes: std::collections::HashSet<String> = std::collections::HashSet::new();
        // 防环:最多迭代 definitions.len()+1 轮
        let max_rounds = definitions.len() + 1;
        for _ in 0..max_rounds {
            let before = pending.len();
            pending.retain(|def| {
                // parent_code 为 None/空串(根)或父已在 done 集合 → 就绪
                let parent_ready = effective_parent(def.parent_code.as_deref())
                    .is_none_or(|pc| done_codes.contains(pc));
                if parent_ready {
                    sorted.push(*def);
                    done_codes.insert(def.code.clone());
                    false
                } else {
                    true
                }
            });
            if pending.is_empty() {
                break;
            }
            // 本轮无进展则存在环或缺失父节点
            if pending.len() == before {
                let stuck: Vec<&str> = pending.iter().map(|d| d.code.as_str()).collect();
                warn!(menus = ?stuck, "菜单拓扑排序卡住(父节点缺失或环),跳过这些节点");
                break;
            }
        }

        // 逐节点建行,记录 code → 真实 id,供子节点用 parent_id 直传(避免重复查父)
        let mut code_to_id: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        let mut count = 0usize;
        for def in sorted {
            // 父优先用已知真实 id(已建),否则留 None(根节点或父缺失)
            let parent_id = effective_parent(def.parent_code.as_deref())
                .and_then(|pc| code_to_id.get(pc))
                .cloned();
            let dto = MenuForCreate {
                code: def.code.clone(),
                name: def.name.clone(),
                description: def.description.clone(),
                parent_id,
                parent_code: effective_parent(def.parent_code.as_deref()).map(|s| s.to_string()),
                path: def.path.clone(),
                icon: def.icon.clone(),
                component: def.component.clone(),
                sort_order: def.sort_order,
                visible: def.visible,
                open_type: def.open_type,
                fun_code: def.fun_code.clone(),
                definition: def.definition.clone(),
                ext_attributes: def.ext_attributes.clone(),
                domain_code: domain_code.to_string(),
                application_code: app_code.to_string(),
                module_code: module_code.to_string(),
            };
            match MenuService::create(&self.mm, &self.db_id, Some(&txn_id), dto).await {
                Ok(ds) => {
                    // 提取新建节点的真实 id,供后续子节点关联
                    let schema = ds.schema.as_ref();
                    if let Some(row) = ds.iter().next()
                        && let Some(id) = row.get_by_name_as::<String>(schema, "id")
                    {
                        code_to_id.insert(def.code.clone(), id);
                    }
                    count += 1;
                }
                Err(e) => {
                    // 失败立即抛错：guard drop 时整个事务（含 DELETE）自动回滚，
                    // 避免「先删后未插成功」导致菜单表数据丢失。
                    error!(
                        menu = %def.code,
                        parent_code = ?def.parent_code,
                        error = %e,
                        applied_so_far = count,
                        "菜单安装失败，整个导入事务将回滚"
                    );
                    return Err(TraitError::Business(format!(
                        "菜单安装失败 code={} parent_code={:?}: {e}",
                        def.code, def.parent_code
                    )));
                }
            }
        }
        guard
            .commit()
            .await
            .map_err(|e| TraitError::Business(format!("提交菜单导入事务失败: {e}")))?;
        // 调试：commit 后立即查表确认数据落库
        let committed_count = count;
        match MenuService::count_by_module(&self.mm, &self.db_id, module_code).await {
            Ok(n) => info!(committed_count, db_count = n, txn_id = %txn_id, "菜单定义导入完成（commit 后立即查表）"),
            Err(e) => error!(committed_count, error = %e, "菜单定义导入 commit 后查表失败"),
        }
        Ok(committed_count)
    }

    async fn list_menu_definitions(
        &self,
        module_code: &str,
    ) -> Result<Vec<MenuDefinition>, TraitError> {
        MenuService::list_by_module(&self.mm, &self.db_id, module_code)
            .await
            .map_err(|e| TraitError::Business(format!("查询模块菜单定义失败: {e}")))
    }
}
