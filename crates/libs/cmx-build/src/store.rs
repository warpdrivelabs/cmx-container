//! 构建作业存储契约（W1）—— 驱动无关。平台侧用 PG 实现（落 `cmx_plugin_build_job`）。

use crate::model::{BuildJob, BuildStatus};
use async_trait::async_trait;

#[async_trait]
pub trait BuildJobStore: Send + Sync {
    /// 新建作业（Queued）。
    async fn create(&self, job: &BuildJob) -> Result<(), String>;
    /// 更新状态（+ 可选错误摘要 / 产物路径 / rev / 耗时）。
    async fn update_status(
        &self,
        id: &str,
        status: BuildStatus,
        error_summary: Option<&str>,
    ) -> Result<(), String>;
    /// 记录成功产物。
    async fn set_artifact(&self, id: &str, wasm_path: &str, rev: &str) -> Result<(), String>;
    /// 取作业。
    async fn get(&self, id: &str) -> Result<Option<BuildJob>, String>;
    /// 列最近 N 条。
    async fn list_recent(&self, limit: i64) -> Result<Vec<BuildJob>, String>;
}
