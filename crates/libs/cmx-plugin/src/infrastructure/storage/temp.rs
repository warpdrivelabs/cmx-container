//! 临时文件管理模块
//!
//! 提供临时目录和文件的创建、清理功能。

use std::path::PathBuf;

/// 临时目录清理器
///
/// RAII 风格的临时目录清理，在 Drop 时自动删除临时目录。
///
/// # 使用示例
///
/// ```rust,no_run
/// use cmx_plugin::infrastructure::storage::temp::TempDirCleanup;
///
/// fn example() {
///     let temp_path = std::path::PathBuf::from("/tmp/plugin_install_xxx");
///     let _cleanup = TempDirCleanup::new(Some(temp_path));
///     
///     // 在这里进行操作...
///     
///     // 函数结束时，_cleanup 被 drop，临时目录自动删除
/// }
/// ```
pub struct TempDirCleanup {
    path: Option<PathBuf>,
}

impl TempDirCleanup {
    /// 创建新的临时目录清理器
    ///
    /// # 参数
    /// - `path`: 需要清理的临时目录路径，如果为 None 则不执行清理
    pub fn new(path: Option<PathBuf>) -> Self {
        Self { path }
    }

    /// 取消自动清理
    ///
    /// 在某些情况下，可能需要保留临时目录（例如安装失败但需要保留日志）。
    pub fn cancel(&mut self) {
        self.path = None;
    }

    /// 手动执行清理
    ///
    /// 提前清理临时目录，而不是等待 Drop。
    pub fn cleanup(&mut self) {
        if let Some(ref path) = self.path
            && path.exists()
        {
            if let Err(e) = std::fs::remove_dir_all(path) {
                tracing::warn!("清理临时目录失败: {} - {}", path.display(), e);
            } else {
                tracing::debug!("已清理临时目录: {}", path.display());
            }
        }
        self.path = None;
    }
}

impl Drop for TempDirCleanup {
    fn drop(&mut self) {
        if let Some(ref path) = self.path
            && path.exists()
        {
            if let Err(e) = std::fs::remove_dir_all(path) {
                tracing::warn!("清理临时目录失败: {} - {}", path.display(), e);
            } else {
                tracing::debug!("已清理临时目录: {}", path.display());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_temp_dir_cleanup() {
        let temp_dir = std::env::temp_dir().join("test_temp_cleanup");
        fs::create_dir_all(&temp_dir).unwrap();

        // 创建一个测试文件
        fs::write(temp_dir.join("test.txt"), "test").unwrap();

        {
            let _cleanup = TempDirCleanup::new(Some(temp_dir.clone()));
            assert!(temp_dir.exists());
        }

        // Drop 后目录应该被删除
        assert!(!temp_dir.exists());
    }

    #[test]
    fn test_temp_dir_cleanup_cancel() {
        let temp_dir = std::env::temp_dir().join("test_temp_cleanup_cancel");
        fs::create_dir_all(&temp_dir).unwrap();

        {
            let mut cleanup = TempDirCleanup::new(Some(temp_dir.clone()));
            cleanup.cancel();
        }

        // 取消后目录应该保留
        assert!(temp_dir.exists());

        // 清理测试目录
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_temp_dir_cleanup_manual() {
        let temp_dir = std::env::temp_dir().join("test_temp_cleanup_manual");
        fs::create_dir_all(&temp_dir).unwrap();

        let mut cleanup = TempDirCleanup::new(Some(temp_dir.clone()));
        assert!(temp_dir.exists());

        cleanup.cleanup();
        assert!(!temp_dir.exists());
    }
}
