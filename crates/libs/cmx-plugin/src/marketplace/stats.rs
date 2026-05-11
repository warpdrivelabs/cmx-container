//! 插件市场统计服务
//!
//! 提供下载统计和评分汇总功能：
//! - 记录下载事件并更新统计
//! - 获取热门插件趋势数据

use std::sync::Arc;

use chrono::Local;
use tracing::debug;

use super::model::MarketplacePlugin;
use super::repository::MarketplaceRepository;
use crate::error::PluginResult;

/// 统计服务
///
/// 封装下载统计和趋势分析逻辑。
pub struct StatsService {
    /// 市场数据仓库
    repo: Arc<MarketplaceRepository>,
}

impl StatsService {
    /// 创建新的统计服务
    ///
    /// # 参数
    /// * `repo` - 市场数据仓库
    pub fn new(repo: Arc<MarketplaceRepository>) -> Self {
        Self { repo }
    }

    /// 记录下载事件
    ///
    /// UPSERT 到日统计表，并更新插件主表的总下载量。
    ///
    /// # 参数
    /// * `plugin_id` - 插件ID
    /// * `version` - 版本号
    /// * `source_type` - 来源类型（api/cli/marketplace）
    pub async fn record_download(
        &self,
        plugin_id: &str,
        version: &str,
        source_type: &str,
    ) -> PluginResult<()> {
        debug!("记录下载事件: plugin_id={}, version={}, source={}", plugin_id, version, source_type);

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

    /// 获取热门插件
    ///
    /// 根据指定天数内的下载量排序，返回热门插件列表。
    ///
    /// # 参数
    /// * `days` - 统计天数（默认7天）
    /// * `limit` - 返回数量限制（默认10）
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
