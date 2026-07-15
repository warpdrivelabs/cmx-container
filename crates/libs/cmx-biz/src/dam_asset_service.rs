//! DAM 资产文件服务
//!
//! 管理 DAM 模块资源在文件系统中的物理布局。域/应用/模块的主数据已迁入数据库
//! （cmx_domain / cmx_application / cmx_module 三表），本服务只负责「文件副作用」：
//!
//! - 创建模块时确保 11 个 DAM 树根下的三级资源目录存在
//! - 域/应用/模块改名时搬移文件目录 + 重写 module 的 resource_root/manifest_path 列
//! - 删除域/应用前做引用完整性校验（拒绝域下仍有应用/模块）
//!
//! 复刻自 cmx-portal/src/dam/store.rs 的文件操作逻辑（store.rs 的 CRUD 部分已废弃）。
//! 路径工具（data_root / data_path）来自 cmx-portal-base。

use std::path::Path;

use cmx_database::DatabaseManager;
use cmx_portal_base::data_root;
use tracing::info;

use crate::{BizError, Result};

/// DAM 树根：创建/改名时在每个根下操作 `<domain>[/<app>[/<module>]]` 目录。
///
/// 与 cmx-portal/src/dam/store.rs 的 DAM_TREE_ROOTS 保持一致。
const DAM_TREE_ROOTS: &[&[&str]] = &[
    &["dict", "entries"],
    &["dict", "seeds"],
    &["fact"],
    &["meta", "definitions"],
    &["meta", "flexible-combination"],
    &["form-pages", "sources"],
    &["html-pages", "sources"],
    &["menu-pages"],
    &["modules"],
    &["native-pages", "sources"],
    &["service-catalog"],
];

/// id 段校验：`[a-zA-Z0-9_-]{1,64}`。
fn is_dam_id(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn assert_id(field: &str, value: &str) -> Result<String> {
    let s = value.trim();
    if !is_dam_id(s) {
        return Err(BizError::business(format!(
            "{field} 仅允许字母、数字、_-，长度 1-64"
        )));
    }
    Ok(s.to_string())
}

/// DAM 资产文件服务（无状态，全部方法为关联函数）
pub struct DamAssetService;

impl DamAssetService {
    // ───────────────────────── 目录创建 ─────────────────────────

    /// 确保模块的三级资源目录存在（11 个 DAM 树根下创建 domain/app/module 目录）。
    ///
    /// 在 ModuleService::create 时调用。
    pub async fn ensure_module_dirs(domain: &str, app: &str, module: &str) -> Result<()> {
        let parts = [assert_id("domain", domain)?, assert_id("app", app)?, assert_id("module", module)?];
        Self::ensure_tree_dirs(&parts).await
    }

    /// 确保应用的二级资源目录存在（11 个 DAM 树根下创建 domain/app 目录）。
    ///
    /// 在 ApplicationService::create 时调用。
    pub async fn ensure_app_dirs(domain: &str, app: &str) -> Result<()> {
        let parts = [assert_id("domain", domain)?, assert_id("app", app)?];
        Self::ensure_tree_dirs(&parts).await
    }

    /// 在每个 DAM 树根下创建 `parts` 目录（parts 已校验）。
    async fn ensure_tree_dirs(parts: &[String]) -> Result<()> {
        for seg in parts {
            assert_id("path.segment", seg)?;
        }
        for root in DAM_TREE_ROOTS {
            let mut p = data_root();
            for r in *root {
                p.push(r);
            }
            for seg in parts {
                p.push(seg);
            }
            tokio::fs::create_dir_all(&p)
                .await
                .map_err(|e| BizError::internal(format!("创建目录失败 {}: {}", p.display(), e)))?;
        }
        Ok(())
    }

    // ───────────────────────── 改名级联 ─────────────────────────

    /// 域改名：在 11 个根下把 `<old_domain>/` 搬到 `<new_domain>/`，
    /// 并重写该域下所有 module 的 resource_root/manifest_path/domain_code 列。
    ///
    /// 在 DomainService::update 检测到 code 变更时调用。
    pub async fn on_domain_renamed(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: Option<&str>,
        old_code: &str,
        new_code: &str,
    ) -> Result<()> {
        if old_code == new_code {
            return Ok(());
        }
        let old = assert_id("old_domain", old_code)?;
        let new = assert_id("new_domain", new_code)?;
        info!("DAM 域改名级联: {} -> {}", old, new);

        // 1) 搬文件目录（一级）
        Self::rename_tree_dirs(&[old.clone()], &[new.clone()]).await?;

        // 2) 重写 DB 列：该域下所有 module 的 resource_root / manifest_path / domain_code
        let sql = format!(
            "UPDATE cmx_module SET \
             resource_root = REPLACE(resource_root, '{old}/', '{new}/'), \
             manifest_path = REPLACE(manifest_path, 'modules/{old}/', 'modules/{new}/'), \
             domain_code = '{new}' \
             WHERE domain_code = '{old}'",
            old = old,
            new = new
        );
        mm.execute_sql(db_id, txn_id, &sql)
            .await
            .map_err(|e| BizError::internal(format!("重写 module 列失败: {}", e)))?;

        // 3) 重写 application 的 domain_code
        let sql_app = format!(
            "UPDATE cmx_application SET domain_code = '{new}' WHERE domain_code = '{old}'",
            old = old,
            new = new
        );
        mm.execute_sql(db_id, txn_id, &sql_app)
            .await
            .map_err(|e| BizError::internal(format!("重写 application domain_code 失败: {}", e)))?;

        Ok(())
    }

    /// 应用改名：搬二级目录 + 重写 module 的 resource_root/manifest_path/application_code 列。
    ///
    /// 在 ApplicationService::update 检测到 code 变更时调用。
    pub async fn on_application_renamed(
        mm: &DatabaseManager,
        db_id: &str,
        txn_id: Option<&str>,
        domain: &str,
        old_app: &str,
        new_app: &str,
    ) -> Result<()> {
        if old_app == new_app {
            return Ok(());
        }
        let d = assert_id("domain", domain)?;
        let old = assert_id("old_app", old_app)?;
        let new = assert_id("new_app", new_app)?;
        info!("DAM 应用改名级联: {}/{old} -> {d}/{new}", d);

        // 1) 搬文件目录（二级）
        Self::rename_tree_dirs(&[d.clone(), old.clone()], &[d.clone(), new.clone()]).await?;

        // 2) 重写 DB 列：该应用下所有 module
        let sql = format!(
            "UPDATE cmx_module SET \
             resource_root = REPLACE(resource_root, '{d}/{old}/', '{d}/{new}/'), \
             manifest_path = REPLACE(manifest_path, 'modules/{d}/{old}/', 'modules/{d}/{new}/'), \
             application_code = '{d}_{new}' \
             WHERE domain_code = '{d}' AND application_code = '{d}_{old}'",
            d = d,
            old = old,
            new = new
        );
        mm.execute_sql(db_id, txn_id, &sql)
            .await
            .map_err(|e| BizError::internal(format!("重写 module 列失败: {}", e)))?;

        Ok(())
    }

    /// 模块改名/迁移：搬三级目录。
    ///
    /// 支持 domain/app/module 三段中任意一段或全部变更。
    /// module 的 resource_root/manifest_path 列由 ModuleService::update 的 data 直接写入，
    /// 本方法只搬文件目录。
    pub async fn on_module_renamed(
        old_domain: &str,
        old_app: &str,
        old_module: &str,
        new_domain: &str,
        new_app: &str,
        new_module: &str,
    ) -> Result<()> {
        if old_domain == new_domain && old_app == new_app && old_module == new_module {
            return Ok(());
        }
        let od = assert_id("old_domain", old_domain)?;
        let oa = assert_id("old_app", old_app)?;
        let om = assert_id("old_module", old_module)?;
        let nd = assert_id("new_domain", new_domain)?;
        let na = assert_id("new_app", new_app)?;
        let nm = assert_id("new_module", new_module)?;
        info!("DAM 模块改名级联: {od}/{oa}/{om} -> {nd}/{na}/{nm}");

        Self::rename_tree_dirs(&[od, oa, om], &[nd, na, nm]).await
    }

    // ───────────────────────── 引用完整性校验 ─────────────────────────

    /// 删域前校验：拒绝域下仍有 application 或 module。
    ///
    /// 在 DomainService::delete 时调用。
    pub async fn check_domain_deletable(
        mm: &DatabaseManager,
        db_id: &str,
        domain_code: &str,
    ) -> Result<()> {
        let safe = assert_id("domain", domain_code)?;

        // 检查 application
        let sql_app = format!(
            "SELECT COUNT(*) AS cnt FROM cmx_application WHERE domain_code = '{safe}'"
        );
        let ds = mm
            .query_sql(db_id, None, &sql_app, "cnt")
            .await
            .map_err(|e| BizError::internal(format!("查询应用计数失败: {}", e)))?;
        let cnt_app = Self::extract_count(&ds);
        if cnt_app > 0 {
            return Err(BizError::business(format!(
                "域 [{safe}] 下仍有 {cnt_app} 个应用，不能删除"
            )));
        }

        // 检查 module
        let sql_mod = format!(
            "SELECT COUNT(*) AS cnt FROM cmx_module WHERE domain_code = '{safe}'"
        );
        let ds = mm
            .query_sql(db_id, None, &sql_mod, "cnt")
            .await
            .map_err(|e| BizError::internal(format!("查询模块计数失败: {}", e)))?;
        let cnt_mod = Self::extract_count(&ds);
        if cnt_mod > 0 {
            return Err(BizError::business(format!(
                "域 [{safe}] 下仍有 {cnt_mod} 个模块，不能删除"
            )));
        }

        Ok(())
    }

    /// 删应用前校验：拒绝应用下仍有 module。
    ///
    /// 在 ApplicationService::delete 时调用。
    pub async fn check_application_deletable(
        mm: &DatabaseManager,
        db_id: &str,
        domain: &str,
        app: &str,
    ) -> Result<()> {
        let d = assert_id("domain", domain)?;
        let a = assert_id("app", app)?;
        let app_code = format!("{d}_{a}");

        let sql = format!(
            "SELECT COUNT(*) AS cnt FROM cmx_module WHERE application_code = '{app_code}'"
        );
        let ds = mm
            .query_sql(db_id, None, &sql, "cnt")
            .await
            .map_err(|e| BizError::internal(format!("查询模块计数失败: {}", e)))?;
        let cnt = Self::extract_count(&ds);
        if cnt > 0 {
            return Err(BizError::business(format!(
                "应用 [{app_code}] 下仍有 {cnt} 个模块，不能删除"
            )));
        }

        Ok(())
    }

    // ───────────────────────── 内部工具 ─────────────────────────

    /// 递归把 from 目录内容并入 to（已存在的子目录递归合并；冲突文件报错）。
    ///
    /// 复刻自 store.rs:move_dir_contents。
    async fn move_dir_contents(from: &Path, to: &Path) -> Result<()> {
        if tokio::fs::metadata(from).await.is_err() {
            return Ok(());
        }
        tokio::fs::create_dir_all(to)
            .await
            .map_err(|e| BizError::internal(format!("创建目录失败: {}", e)))?;

        let mut stack = vec![(from.to_path_buf(), to.to_path_buf())];
        while let Some((fd, td)) = stack.pop() {
            let mut rd = match tokio::fs::read_dir(&fd).await {
                Ok(rd) => rd,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => return Err(BizError::internal(format!("读目录失败: {}", e))),
            };
            tokio::fs::create_dir_all(&td)
                .await
                .map_err(|e| BizError::internal(format!("创建目录失败: {}", e)))?;
            while let Some(entry) = rd
                .next_entry()
                .await
                .map_err(|e| BizError::internal(format!("读目录项失败: {}", e)))?
            {
                let from_path = entry.path();
                let to_path = td.join(entry.file_name());
                let ft = entry
                    .file_type()
                    .await
                    .map_err(|e| BizError::internal(format!("读文件类型失败: {}", e)))?;
                let to_exists = tokio::fs::metadata(&to_path).await.is_ok();
                let to_is_dir = tokio::fs::metadata(&to_path)
                    .await
                    .map(|m| m.is_dir())
                    .unwrap_or(false);
                if ft.is_dir() && to_exists && to_is_dir {
                    stack.push((from_path, to_path));
                    continue;
                }
                if to_exists {
                    return Err(BizError::business(format!(
                        "目标路径已存在，不能覆盖：{}",
                        to_path.display()
                    )));
                }
                tokio::fs::rename(&from_path, &to_path)
                    .await
                    .map_err(|e| BizError::internal(format!("搬移文件失败: {}", e)))?;
            }
            // 该层处理完后删源目录
            let _ = tokio::fs::remove_dir_all(&fd).await;
        }
        Ok(())
    }

    /// 把每个 DAM 树根下 from_parts 目录搬到 to_parts。
    ///
    /// 复刻自 store.rs:rename_tree_dirs。
    async fn rename_tree_dirs(from_parts: &[String], to_parts: &[String]) -> Result<()> {
        for seg in from_parts.iter().chain(to_parts.iter()) {
            assert_id("path.segment", seg)?;
        }
        if from_parts.join("/") == to_parts.join("/") {
            return Ok(());
        }
        for root in DAM_TREE_ROOTS {
            let mut from = data_root();
            let mut to = data_root();
            for r in *root {
                from.push(r);
                to.push(r);
            }
            for seg in from_parts {
                from.push(seg);
            }
            for seg in to_parts {
                to.push(seg);
            }
            Self::move_dir_contents(&from, &to).await?;
        }
        Ok(())
    }

    /// 从 DataSet 提取 COUNT(*) 结果（第一行 cnt 列）。
    fn extract_count(ds: &cmx_core::model::data::dataset::DataSet) -> i64 {
        ds.iter()
            .next()
            .and_then(|row| row.get_by_name(&ds.schema, "cnt"))
            .and_then(|v| i64::try_from(v.clone()).ok())
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_dam_id() {
        assert!(is_dam_id("fi"));
        assert!(is_dam_id("fi_cmxfico"));
        assert!(is_dam_id("fi-cmxfico-gl"));
        assert!(!is_dam_id(""));
        assert!(!is_dam_id("fi/cmxfico")); // 含 /
        assert!(!is_dam_id(&"a".repeat(65))); // 超长
    }

    #[test]
    fn test_assert_id() {
        assert!(assert_id("test", "fi").is_ok());
        assert!(assert_id("test", "fi/gl").is_err());
    }
}
