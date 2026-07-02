//! 插件市场统计服务。

//! 提供下载统计和评分汇总功能：记录下载事件并更新统计，获取热门插件趋势数据。

use std::sync::Arc;

use chrono::Local;
use tracing::debug;

use super::model::MarketplacePlugin;
use super::repository::MarketplaceRepository;
use crate::error::PluginResult;

/// 统计服务。
///
/// 封装下载统计和趋势分析逻辑。
pub struct StatsService {
    /// 市场数据仓库。
    repo: Arc<MarketplaceRepository>,
}

impl StatsService {
    /// 创建新的统计服务实例。
    ///
    /// # Arguments
    ///
    /// * `repo` - 市场数据仓库实例。
    pub fn new(repo: Arc<MarketplaceRepository>) -> Self {
        Self { repo }
    }

    /// 记录插件下载事件。
    ///
    /// 将下载记录 UPSERT 到日统计表，并原子性更新插件主表的总下载量。
    ///
    /// # Arguments
    ///
    /// * `plugin_id` - 插件业务唯一标识。
    /// * `version` - 下载的版本号。
    /// * `source_type` - 下载来源类型（如 `marketplace`、`url`、`api`）。
    ///
    /// # Returns
    ///
    /// 成功时返回 `Ok(())`。
    ///
    /// # Errors
    ///
    /// 当数据库操作失败时返回错误。
    pub async fn record_download(
        &self,
        plugin_id: &str,
        version: &str,
        source_type: &str,
    ) -> PluginResult<()> {
        debug!(
            "记录下载事件: plugin_id={}, version={}, source={}",
            plugin_id, version, source_type
        );

        let today = Local::now().date_naive();
        let today_str = today.format("%Y-%m-%d").to_string();

        // UPSERT 到日统计表
        self.repo
            .upsert_download_stat(plugin_id, version, &today_str, source_type)
            .await?;

        // 更新插件主表的总下载量
        self.repo.increment_download_count(plugin_id).await?;

        Ok(())
    }

    /// 获取热门插件。
    ///
    /// 根据最近 N 天的下载量聚合统计，返回下载量最高的插件列表。
    ///
    /// # Arguments
    ///
    /// * `days` - 统计周期（天数），只统计最近该天数内的下载量。
    /// * `limit` - 返回的插件数量上限。
    ///
    /// # Returns
    ///
    /// 按下载量降序排列的热门插件列表。
    ///
    /// # Errors
    ///
    /// 当数据库操作失败时返回错误。
    pub async fn get_trending(
        &self,
        days: i64,
        limit: i64,
    ) -> PluginResult<Vec<MarketplacePlugin>> {
        debug!("获取热门插件: days={}, limit={}", days, limit);

        let since = Local::now().date_naive() - chrono::Duration::days(days);
        let since_str = since.format("%Y-%m-%d").to_string();

        self.repo.get_trending_since(&since_str, limit).await
    }
}
