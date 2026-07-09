//! `FormDefinitionImporter` trait 的本地实现。
//!
//! 直调 [`FormService`],无 ZIP 开销,是单体部署的默认实现。
//! 远程部署时由 cmx-plugin 的 Remote 实现经 gRPC 调用本实现。

use async_trait::async_trait;
use cmx_core::model::module::FormDefinition;
use cmx_database::DatabaseManager;
use cmx_traits::error::TraitError;
use cmx_traits::resource::FormDefinitionImporter;
use std::sync::Arc;
use tracing::{info, warn};

use crate::form::{FormForCreate, FormService};

/// 本地表单定义导入器(直调 FormService)。
pub struct LocalFormDefinitionImporter {
    mm: Arc<DatabaseManager>,
    db_id: String,
}

impl LocalFormDefinitionImporter {
    /// 创建本地表单定义导入器。
    ///
    /// # Arguments
    /// * `mm` - 数据库管理器
    /// * `db_id` - 表单存储库 ID(default 库)
    pub fn new(mm: Arc<DatabaseManager>, db_id: String) -> Self {
        Self { mm, db_id }
    }
}

#[async_trait]
impl FormDefinitionImporter for LocalFormDefinitionImporter {
    async fn apply_form_definitions(
        &self,
        domain_code: &str,
        app_code: &str,
        module_code: &str,
        definitions: &[FormDefinition],
    ) -> Result<usize, TraitError> {
        if definitions.is_empty() {
            return Ok(0);
        }
        // 内部自开事务:一次 apply 一个事务,异常时 guard drop 自动回滚
        let guard = self
            .mm
            .get_transaction_context()
            .begin_with_guard(&self.db_id)
            .await
            .map_err(|e| TraitError::Business(format!("开启表单导入事务失败: {e}")))?;
        let txn_id = guard.txn_id();
        let mut count = 0usize;
        for def in definitions {
            // 幂等:先删同 code 记录,再创建
            let _ = FormService::delete_by_code(&self.mm, &self.db_id, Some(txn_id), &def.code)
                .await;
            let dto = FormForCreate {
                code: def.code.clone(),
                name: def.name.clone(),
                description: def.description.clone(),
                definition: Some(def.definition.clone()),
                domain_code: domain_code.to_string(),
                application_code: app_code.to_string(),
                module_code: module_code.to_string(),
            };
            match FormService::create(&self.mm, &self.db_id, Some(txn_id), dto).await {
                Ok(_) => {
                    count += 1;
                }
                Err(e) => {
                    warn!(form = %def.code, error = %e, "表单安装失败");
                }
            }
        }
        guard
            .commit()
            .await
            .map_err(|e| TraitError::Business(format!("提交表单导入事务失败: {e}")))?;
        info!(count, "表单定义导入完成");
        Ok(count)
    }

    async fn list_form_definitions(
        &self,
        module_code: &str,
    ) -> Result<Vec<FormDefinition>, TraitError> {
        FormService::list_by_module(&self.mm, &self.db_id, module_code)
            .await
            .map_err(|e| TraitError::Business(format!("查询模块表单定义失败: {e}")))
    }
}
