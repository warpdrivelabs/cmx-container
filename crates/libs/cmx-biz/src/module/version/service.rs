//! 模块版本管理 Service
//!
//! 提供当前版本读写、版本历史登记(record_import)能力。
//! record_import 在事务内:upsert cmx_module_current_version(唯一约束 module_code)
//! + insert cmx_module_version_history(唯一约束 module_code+package_version)。

use cmx_core::model::cell::DataValue;
use cmx_core::model::data::dataset::DataSet;
use cmx_database::DatabaseManager;
use serde_json::Value;
use tracing::instrument;

use crate::error::{BizError, Result};

/// 版本登记入参
#[derive(Debug, Clone)]
pub struct ModuleVersionRecord {
    pub module_id: String,
    pub domain_code: String,
    pub application_code: String,
    pub module_code: String,
    pub package_version: String,
    pub checksum: Option<String>,
    pub manifest_snapshot: Value,
    pub imported_by: Option<String>,
    pub source: Option<String>,
}

/// 模块版本管理服务(静态方法模式)
pub struct ModuleVersionService;

impl ModuleVersionService {
    /// 查询某模块当前版本(从 cmx_module_current_version 读一行)
    ///
    /// # Errors
    /// 数据库查询失败时返回错误
    pub async fn get_current(
        mm: &DatabaseManager,
        db_id: &str,
        module_code: &str,
    ) -> Result<Option<CurrentVersionInfo>> {
        let sql = "SELECT module_id, module_code, package_version, checksum \
                   FROM cmx_module_current_version WHERE module_code = $1";
        let ds = mm
            .query_sql_with_datavalues(
                db_id,
                None,
                sql,
                vec![DataValue::String(module_code.to_string())],
                "get_module_current_version",
            )
            .await
            .map_err(|e| BizError::business(format!("查询当前版本失败: {e}")))?;
        let json = serde_json::to_value(&ds)?;
        let row = json
            .get("rows")
            .and_then(|r| r.as_array())
            .and_then(|rows| rows.first());
        match row {
            Some(r) => Ok(Some(CurrentVersionInfo {
                package_version: r
                    .get("package_version")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                checksum: r
                    .get("checksum")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
            })),
            None => Ok(None),
        }
    }

    /// 登记一次导入:事务内 upsert current + insert history
    ///
    /// # Errors
    /// 数据库写入失败时返回错误
    #[instrument(skip(mm, record))]
    pub async fn record_import(
        mm: &DatabaseManager,
        db_id: &str,
        record: ModuleVersionRecord,
    ) -> Result<()> {
        let txn_ctx = mm.get_transaction_context();
        let guard = txn_ctx
            .begin_with_guard(db_id)
            .await
            .map_err(|e| BizError::business(format!("开启版本登记事务失败: {e}")))?;
        let txn_id = guard.txn_id();

        // 1. upsert current_version(ON CONFLICT module_code 更新)
        let id = uuid::Uuid::new_v4().to_string();
        let upsert_sql = "INSERT INTO cmx_module_current_version \
                          (id, module_id, domain_code, application_code, module_code, \
                           package_version, checksum, manifest_snapshot, imported_at, imported_by, source, archived) \
                          VALUES ($1, $2, $3, $4, $5, $6, $7, $8, CURRENT_TIMESTAMP, $9, $10, 0) \
                          ON CONFLICT (module_code) DO UPDATE SET \
                          package_version = EXCLUDED.package_version, \
                          checksum = EXCLUDED.checksum, \
                          manifest_snapshot = EXCLUDED.manifest_snapshot, \
                          imported_at = CURRENT_TIMESTAMP, \
                          imported_by = EXCLUDED.imported_by, \
                          source = EXCLUDED.source, \
                          update_time = CURRENT_TIMESTAMP";
        let snapshot_str = serde_json::to_string(&record.manifest_snapshot)?;
        mm.execute_sql_with_datavalues(
            db_id,
            Some(txn_id),
            upsert_sql,
            vec![
                DataValue::String(id),
                DataValue::String(record.module_id.clone()),
                DataValue::String(record.domain_code.clone()),
                DataValue::String(record.application_code.clone()),
                DataValue::String(record.module_code.clone()),
                DataValue::String(record.package_version.clone()),
                record.checksum.clone().into(),
                DataValue::String(snapshot_str),
                record.imported_by.clone().into(),
                record.source.clone().into(),
            ],
        )
        .await
        .map_err(|e| BizError::business(format!("更新当前版本失败: {e}")))?;

        // 2. insert history(ON CONFLICT module_code+package_version DO NOTHING 防重)
        let hid = uuid::Uuid::new_v4().to_string();
        let history_sql = "INSERT INTO cmx_module_version_history \
                           (id, module_id, domain_code, application_code, module_code, \
                            package_version, checksum, manifest_snapshot, imported_at, imported_by, source, archived) \
                           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, CURRENT_TIMESTAMP, $9, $10, 0) \
                           ON CONFLICT (module_code, package_version) DO NOTHING";
        mm.execute_sql_with_datavalues(
            db_id,
            Some(txn_id),
            history_sql,
            vec![
                DataValue::String(hid),
                DataValue::String(record.module_id),
                DataValue::String(record.domain_code),
                DataValue::String(record.application_code),
                DataValue::String(record.module_code),
                DataValue::String(record.package_version),
                record.checksum.into(),
                DataValue::String(serde_json::to_string(&record.manifest_snapshot)?),
                record.imported_by.into(),
                record.source.into(),
            ],
        )
        .await
        .map_err(|e| BizError::business(format!("写入版本历史失败: {e}")))?;

        guard
            .commit()
            .await
            .map_err(|e| BizError::business(format!("版本登记事务提交失败: {e}")))?;
        Ok(())
    }

    /// 查询某模块的完整版本历史
    ///
    /// # Errors
    /// 数据库查询失败时返回错误
    pub async fn list_history(
        mm: &DatabaseManager,
        db_id: &str,
        module_code: &str,
    ) -> Result<DataSet> {
        let sql = "SELECT package_version, checksum, imported_at, imported_by, source \
                   FROM cmx_module_version_history WHERE module_code = $1 \
                   ORDER BY imported_at DESC";
        mm.query_sql_with_datavalues(
            db_id,
            None,
            sql,
            vec![DataValue::String(module_code.to_string())],
            "list_module_version_history",
        )
        .await
        .map_err(|e| BizError::business(format!("查询版本历史失败: {e}")))
    }
}

/// 当前版本信息(导入校验用)
#[derive(Debug, Clone)]
pub struct CurrentVersionInfo {
    pub package_version: String,
    pub checksum: Option<String>,
}
