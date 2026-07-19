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
        // application code = {domain}_{app_id}，拆出 domain 和 app_id
        let parts: Vec<&str> = data.code.splitn(2, '_').collect();
        if parts.len() == 2
            && let Err(e) = DamAssetService::ensure_app_dirs(parts[0], parts[1]).await
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

        // 读取旧 code
        let old_ds =
            GenericCrudService::<ApplicationBmc>::get(mm, db_id, Some(tx.txn_id()), id.clone())
                .await?;
        let old_code = Self::get_field(&old_ds, "code").unwrap_or_default();

        let result = GenericCrudService::<ApplicationBmc>::update(
            mm,
            db_id,
            Some(tx.txn_id()),
            id.clone(),
            data,
        )
        .await?;

        let new_code = Self::get_field(&result, "code").unwrap_or_else(|| old_code.clone());

        // 若 code 变更 → 搬目录 + 重写 module 列
        if old_code != new_code && !old_code.is_empty() && !new_code.is_empty() {
            // code = {domain}_{app_id}，拆出 domain 和 app_id
            let old_parts: Vec<&str> = old_code.splitn(2, '_').collect();
            let new_parts: Vec<&str> = new_code.splitn(2, '_').collect();
            if old_parts.len() == 2 && new_parts.len() == 2 && old_parts[0] == new_parts[0] {
                // 同域内改名
                if let Err(e) = DamAssetService::on_application_renamed(
                    mm,
                    db_id,
                    Some(tx.txn_id()),
                    old_parts[0],
                    old_parts[1],
                    new_parts[1],
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
            // code = {domain}_{app_id}
            let parts: Vec<&str> = code.splitn(2, '_').collect();
            if parts.len() == 2 {
                DamAssetService::check_application_deletable(mm, db_id, parts[0], parts[1]).await?;
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
