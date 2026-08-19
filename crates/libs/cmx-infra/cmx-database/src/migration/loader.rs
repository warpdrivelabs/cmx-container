use std::fs;
use std::path::PathBuf;

use sha2::{Digest, Sha256};
use tracing::{debug, info, warn};

use super::error::{MigrationError, MigrationResult};
use super::record::PendingMigration;

/// 迁移文件加载器
///
/// 负责从文件系统扫描和加载迁移 SQL 文件
pub struct MigrationLoader {
    /// 迁移文件目录路径
    migration_dir: PathBuf,
}

impl MigrationLoader {
    /// 创建新的迁移加载器
    ///
    /// # 参数
    /// * `migration_dir` - 迁移文件所在目录路径
    pub fn new(migration_dir: PathBuf) -> Self {
        Self { migration_dir }
    }

    /// 加载所有待执行的迁移文件
    ///
    /// 扫描目录中 .up.sql 文件，按文件名排序，
    /// 解析文件名获取版本号和名称，读取 SQL 内容并计算校验和，
    /// 同时查找对应的 .down.sql 回滚文件
    pub fn load_migrations(&self) -> MigrationResult<Vec<PendingMigration>> {
        if !self.migration_dir.exists() {
            warn!("迁移目录不存在: {:?}", self.migration_dir);
            return Ok(Vec::new());
        }

        let mut up_files: Vec<String> = Vec::new();

        // 扫描目录中的 .up.sql 文件
        for entry in fs::read_dir(&self.migration_dir)? {
            let entry = entry?;
            let file_name = entry.file_name().to_string_lossy().to_string();
            if file_name.ends_with(".up.sql") {
                up_files.push(file_name);
            }
        }

        // 按文件名排序，确保迁移按顺序执行
        up_files.sort();

        let mut migrations = Vec::new();

        for up_file in up_files {
            let (version, name) = Self::parse_migration_filename(&up_file)?;

            // 读取升级 SQL 内容
            let up_path = self.migration_dir.join(&up_file);
            let up_sql = fs::read_to_string(&up_path)?;

            // 计算校验和
            let checksum = Self::compute_checksum(&up_sql);

            // 查找对应的回滚 SQL 文件
            let down_file = up_file.replace(".up.sql", ".down.sql");
            let down_path = self.migration_dir.join(&down_file);
            let down_sql = if down_path.exists() {
                Some(fs::read_to_string(&down_path)?)
            } else {
                None
            };

            debug!(
                version = %version,
                name = %name,
                checksum = %checksum,
                has_rollback = down_sql.is_some(),
                "加载迁移文件"
            );

            migrations.push(PendingMigration {
                version,
                name,
                up_sql,
                down_sql,
                checksum,
            });
        }

        info!("共加载 {} 个迁移文件", migrations.len());
        Ok(migrations)
    }

    /// 解析迁移文件名
    ///
    /// 文件名格式：YYYYMMDD_NNN_description.up.sql
    /// 解析出版本号（YYYYMMDD_NNN）和名称（description）
    ///
    /// # 参数
    /// * `filename` - 迁移文件名
    ///
    /// # 返回值
    /// * `(version, name)` - 版本号和迁移名称的元组
    pub fn parse_migration_filename(filename: &str) -> MigrationResult<(String, String)> {
        // 去掉 .up.sql 后缀
        let base_name = filename
            .strip_suffix(".up.sql")
            .or_else(|| filename.strip_suffix(".down.sql"))
            .ok_or_else(|| MigrationError::InvalidFileName(filename.to_string()))?;

        // 按 '_' 分割，前两部分为日期和序号，其余为描述
        let parts: Vec<&str> = base_name.splitn(3, '_').collect();
        if parts.len() < 3 {
            return Err(MigrationError::InvalidFileName(format!(
                "文件名格式不正确，期望 YYYYMMDD_NNN_description: {}",
                filename
            )));
        }

        let version = format!("{}_{}", parts[0], parts[1]);
        let name = parts[2].to_string();

        Ok((version, name))
    }

    /// 计算内容的 SHA256 校验和
    ///
    /// # 参数
    /// * `content` - 需要计算校验和的字符串内容
    ///
    /// # 返回值
    /// * 十六进制格式的 SHA256 校验和字符串
    pub fn compute_checksum(content: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        let result = hasher.finalize();
        format!("{:x}", result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造唯一临时目录（测试结束由调用方负责清理）
    fn temp_root(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "cmx_migration_loader_{}_{}_{}",
            tag,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// 扫描是**非递归**的：仅匹配本层 `*.up.sql`，不进入子目录、不匹配其他后缀。
    ///
    /// v2 目录约定 `<dir>/<platform|biz>/migrations/*.up.sql`，调用方必须把
    /// 目录拼到 `migrations` 子目录层（见 cmx-platform-app config/migration.rs
    /// 的 `migration_round_dir`），否则本测试所述行为会导致静默漏扫
    #[test]
    fn 扫描非递归_仅匹配本层up文件() {
        let root = temp_root("nonrecursive");
        // 本层：1 个有效迁移 + 2 个应被忽略的文件
        fs::write(root.join("20260819_001_baseline.up.sql"), "SELECT 1;").unwrap();
        fs::write(root.join("20260819_001_baseline.down.sql"), "SELECT 2;").unwrap();
        fs::write(root.join("init_ddl.sql"), "CREATE TABLE t (id INT);").unwrap();
        // 子目录：即使内含 up 文件也不得被扫描
        let sub = root.join("migrations");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("20260820_001_hidden.up.sql"), "SELECT 3;").unwrap();

        let migrations = MigrationLoader::new(root.clone()).load_migrations().unwrap();
        assert_eq!(migrations.len(), 1, "只应扫描到本层 1 个 .up.sql");
        assert_eq!(migrations[0].version, "20260819_001");
        assert!(migrations[0].down_sql.is_some(), "同名 down 文件应被识别");

        fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn 目录不存在_返回空且不报错() {
        let migrations = MigrationLoader::new(PathBuf::from(
            "/nonexistent/cmx/migration/dir",
        ))
        .load_migrations()
        .unwrap();
        assert!(migrations.is_empty());
    }

    #[test]
    fn 文件名解析_日期序号与描述() {
        let (version, name) =
            MigrationLoader::parse_migration_filename("20260812_001_mdm_治理表.up.sql")
                .unwrap();
        assert_eq!(version, "20260812_001");
        assert_eq!(name, "mdm_治理表");
    }
}
