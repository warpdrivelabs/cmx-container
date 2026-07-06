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
use tracing::{info, warn};

use crate::menu::{MenuForCreate, MenuService};

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
        let mut count = 0usize;
        for def in definitions {
            // 幂等:先删同 code 根菜单,再建
            let _ = MenuService::delete_by_code(&self.mm, &self.db_id, &def.code).await;
            let dto = MenuForCreate {
                code: def.code.clone(),
                name: def.name.clone(),
                parent_id: None, // 模块导入的菜单均为根菜单
                path: def
                    .definition
                    .get("path")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                icon: None,
                component: None,
                sort_order: 0,
                visible: 1,
                definition: Some(def.definition.clone()),
                ext_attributes: None,
                domain_code: domain_code.to_string(),
                application_code: app_code.to_string(),
                module_code: module_code.to_string(),
            };
            match MenuService::create(&self.mm, &self.db_id, dto).await {
                Ok(_) => {
                    count += 1;
                }
                Err(e) => {
                    warn!(menu = %def.code, error = %e, "菜单安装失败");
                }
            }
        }
        info!(count, "菜单定义导入完成");
        Ok(count)
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
