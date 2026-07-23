//! Module 实体的自定义 Service
//!
//! 在标准 CRUD（GenericCrudService）之上叠加 DAM 资产文件副作用：
//! - create：写库后确保模块资源目录存在
//! - update：检测 code/domain_code/application_code 变更时搬移目录
//! - delete：模块无子级，直接删
//!
//! 参照 SysDatasourceService 的事务化包装模式。

use super::{ModuleBmc, ModuleForCreate, ModuleForUpdate};
use crate::dam_asset_service::DamAssetService;
use crate::{BizError, Result};
use cmx_core::model::data::dataset::DataSet;
use cmx_database::DatabaseManager;
use cmx_database::crud::GenericCrudService;
use serde_json::Value;
use tracing::info;

/// Module 自定义服务
pub struct ModuleService;

impl ModuleService {
    /// 创建模块
    ///
    /// # 流程
    /// 1. 开启事务
    /// 2. 写入 cmx_module（含 resource_root/manifest_path 等新列）
    /// 3. 确保模块资源目录存在（11 个 DAM 树根下）
    /// 4. 提交事务（文件操作失败则回滚 DB）
    pub async fn create(
        mm: &DatabaseManager,
        db_id: &str,
        data: ModuleForCreate,
    ) -> Result<DataSet> {
        info!(
            "{:<12} - ModuleService::create - code: {}",
            "SERVICE", data.code
        );

        let tx = mm
            .get_transaction_context()
            .begin_with_guard(db_id)
            .await
            .map_err(|e| BizError::internal(format!("开启事务失败: {}", e)))?;

        let result =
            GenericCrudService::<ModuleBmc>::create(mm, db_id, Some(tx.txn_id()), data.clone())
                .await?;

        // 文件副作用：确保模块资源目录存在
        // data.code / data.domain_code / data.application_code 均为短码，直接作为目录段
        if let Err(e) =
            DamAssetService::ensure_module_dirs(&data.domain_code, &data.application_code, &data.code)
                .await
        {
            tx.rollback()
                .await
                .map_err(|e| BizError::internal(format!("回滚事务失败: {}", e)))?;
            return Err(e);
        }

        tx.commit()
            .await
            .map_err(|e| BizError::internal(format!("提交事务失败: {}", e)))?;

        Ok(result)
    }

    /// 更新模块
    ///
    /// # 流程
    /// 1. 开启事务
    /// 2. 读取旧记录（判断是否改名）
    /// 3. 执行 DB 更新
    /// 4. 若 code 变更 → 搬移文件目录
    /// 5. 提交事务
    pub async fn update(
        mm: &DatabaseManager,
        db_id: &str,
        id: Value,
        data: ModuleForUpdate,
    ) -> Result<DataSet> {
        info!("{:<12} - ModuleService::update - id: {}", "SERVICE", id);

        let tx = mm
            .get_transaction_context()
            .begin_with_guard(db_id)
            .await
            .map_err(|e| BizError::internal(format!("开启事务失败: {}", e)))?;

        // 读取旧记录，提取 code/domain_code/application_code（均为短码）
        let old_ds =
            GenericCrudService::<ModuleBmc>::get(mm, db_id, Some(tx.txn_id()), id.clone()).await?;
        let old_code = Self::get_field(&old_ds, "code").unwrap_or_default();
        let old_domain = Self::get_field(&old_ds, "domain_code").unwrap_or_default();
        let old_app = Self::get_field(&old_ds, "application_code").unwrap_or_default();

        let result = GenericCrudService::<ModuleBmc>::update(
            mm,
            db_id,
            Some(tx.txn_id()),
            id.clone(),
            data.clone(),
        )
        .await?;

        // 从新记录提取三段（短码），判断是否改名
        let new_code = Self::get_field(&result, "code").unwrap_or_else(|| old_code.clone());
        let new_domain = Self::get_field(&result, "domain_code").unwrap_or_else(|| old_domain.clone());
        let new_app = Self
            ::get_field(&result, "application_code")
            .unwrap_or_else(|| old_app.clone());

        // 若 domain/app/module 任一变更 → 搬目录
        if (old_domain != new_domain || old_app != new_app || old_code != new_code)
            && let Err(e) = DamAssetService::on_module_renamed(
                &old_domain,
                &old_app,
                &old_code,
                &new_domain,
                &new_app,
                &new_code,
            )
            .await
        {
            tx.rollback()
                .await
                .map_err(|e| BizError::internal(format!("回滚事务失败: {}", e)))?;
            return Err(e);
        }

        tx.commit()
            .await
            .map_err(|e| BizError::internal(format!("提交事务失败: {}", e)))?;

        Ok(result)
    }

    /// 删除模块（无子级，直接删）
    pub async fn delete(mm: &DatabaseManager, db_id: &str, ids: Vec<Value>) -> Result<DataSet> {
        info!(
            "{:<12} - ModuleService::delete - count: {}",
            "SERVICE",
            ids.len()
        );

        let ids_value: Vec<Value> = ids;
        GenericCrudService::<ModuleBmc>::delete(mm, db_id, None, ids_value)
            .await
            .map_err(BizError::from)
    }

    /// 从 DataSet 提取字符串字段值（第一行）。
    fn get_field(ds: &DataSet, field: &str) -> Option<String> {
        let row = ds.iter().next()?;
        let value = row.get_by_name(&ds.schema, field)?;
        String::try_from(value.clone()).ok()
    }
}
