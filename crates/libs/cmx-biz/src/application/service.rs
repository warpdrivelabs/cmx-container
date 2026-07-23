//! Application 实体的自定义 Service
//!
//! 在标准 CRUD 之上叠加 DAM 资产文件副作用：
//! - create：写库后确保应用级资源目录存在
//! - update：检测 code 变更时搬移目录 + 重写 module 列
//! - delete：删前校验应用下无 module
//!
//! 参照 SysDatasourceService 的事务化包装模式。

use super::{ApplicationBmc, ApplicationForCreate, ApplicationForUpdate};
use crate::dam_asset_service::DamAssetService;
use crate::{BizError, Result};
use cmx_core::model::data::dataset::DataSet;
use cmx_database::DatabaseManager;
use cmx_database::crud::GenericCrudService;
use serde_json::Value;
use tracing::info;

/// Application 自定义服务
pub struct ApplicationService;

impl ApplicationService {
    /// 创建应用
    ///
    /// # 流程
    /// 1. 开启事务
    /// 2. 写入 cmx_application
    /// 3. 确保应用级资源目录存在（11 个 DAM 树根下创建 domain/app 二级目录）
    /// 4. 提交事务
    pub async fn create(
        mm: &DatabaseManager,
        db_id: &str,
        data: ApplicationForCreate,
    ) -> Result<DataSet> {
        info!(
            "{:<12} - ApplicationService::create - code: {}",
            "SERVICE", data.code
        );

        let tx = mm
            .get_transaction_context()
            .begin_with_guard(db_id)
            .await
            .map_err(|e| BizError::internal(format!("开启事务失败: {}", e)))?;

        let result = GenericCrudService::<ApplicationBmc>::create(
            mm,
            db_id,
            Some(tx.txn_id()),
            data.clone(),
        )
        .await?;

        // 文件副作用：确保应用级目录存在
        // data.code 是纯净短码，data.domain_code 是域短码，直接作为目录段。
        if let Err(e) = DamAssetService::ensure_app_dirs(&data.domain_code, &data.code).await {
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

    /// 更新应用
    ///
    /// # 流程
    /// 1. 开启事务
    /// 2. 读取旧 code
    /// 3. 执行 DB 更新
    /// 4. 若 code 变更 → 搬移应用级目录 + 重写 module 的 resource_root/manifest_path/application_code
    /// 5. 提交事务
    pub async fn update(
        mm: &DatabaseManager,
        db_id: &str,
        id: Value,
        data: ApplicationForUpdate,
    ) -> Result<DataSet> {
        info!(
            "{:<12} - ApplicationService::update - id: {}",
            "SERVICE", id
        );

        let tx = mm
            .get_transaction_context()
            .begin_with_guard(db_id)
            .await
            .map_err(|e| BizError::internal(format!("开启事务失败: {}", e)))?;

        // 读取旧记录，提取 code 与 domain_code（均为短码）
        let old_ds =
            GenericCrudService::<ApplicationBmc>::get(mm, db_id, Some(tx.txn_id()), id.clone())
                .await?;
        let old_code = Self::get_field(&old_ds, "code").unwrap_or_default();
        let old_domain = Self::get_field(&old_ds, "domain_code").unwrap_or_default();

        let result = GenericCrudService::<ApplicationBmc>::update(
            mm,
            db_id,
            Some(tx.txn_id()),
            id.clone(),
            data,
        )
        .await?;

        let new_code = Self::get_field(&result, "code").unwrap_or_else(|| old_code.clone());
        let new_domain = Self::get_field(&result, "domain_code").unwrap_or_else(|| old_domain.clone());

        // 若 code 变更 → 搬目录 + 重写 module 列
        if old_code != new_code && !old_code.is_empty() && !new_code.is_empty() {
            // 同域内改名（code/domain_code 都是短码，直接传入）
            if old_domain == new_domain && !old_domain.is_empty() {
                if let Err(e) = DamAssetService::on_application_renamed(
                    mm,
                    db_id,
                    Some(tx.txn_id()),
                    &old_domain,
                    &old_code,
                    &new_code,
                )
                .await
                {
                    tx.rollback()
                        .await
                        .map_err(|e| BizError::internal(format!("回滚事务失败: {}", e)))?;
                    return Err(e);
                }
            }
        }

        tx.commit()
            .await
            .map_err(|e| BizError::internal(format!("提交事务失败: {}", e)))?;

        Ok(result)
    }

    /// 删除应用
    ///
    /// 删前校验：拒绝应用下仍有 module。
    pub async fn delete(mm: &DatabaseManager, db_id: &str, ids: Vec<Value>) -> Result<DataSet> {
        info!(
            "{:<12} - ApplicationService::delete - count: {}",
            "SERVICE",
            ids.len()
        );

        for id in &ids {
            let ds = GenericCrudService::<ApplicationBmc>::get(mm, db_id, None, id.clone()).await?;
            let code = Self::get_field(&ds, "code")
                .unwrap_or_else(|| id.as_str().unwrap_or("").to_string());
            // code 为应用短码（即 cmx_application.code）
            if !code.is_empty() {
                DamAssetService::check_application_deletable(mm, db_id, &code).await?;
            }
        }

        let ids_value: Vec<Value> = ids;
        GenericCrudService::<ApplicationBmc>::delete(mm, db_id, None, ids_value)
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
