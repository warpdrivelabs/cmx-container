//! 数据库迁移文件加载器。
//!
//! 扫描迁移目录、解析文件名、计算校验和，构造待执行的迁移项。

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
