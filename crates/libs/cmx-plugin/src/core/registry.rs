//! 插件注册表模块
//!
//! 管理已加载插件的元数据

use std::collections::HashMap;

use crate::domain::plugin::{PluginInfo, PluginStatus, PluginFilter};

/// 插件注册表
///
/// 管理已加载插件的元数据，提供快速查询能力。
pub struct PluginRegistry {
    /// 插件信息映射 (plugin_id -> PluginInfo)
    plugins: HashMap<String, PluginInfo>,
}

impl PluginRegistry {
    /// 创建新的插件注册表
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
        }
    }

    /// 注册插件
    ///
    /// 将插件信息添加到注册表中。
    pub fn register(&mut self, plugin: PluginInfo) {
        self.plugins.insert(plugin.id.clone(), plugin);
    }

    /// 注销插件
    ///
    /// 从注册表中移除插件。
    pub fn unregister(&mut self, plugin_id: &str) {
        self.plugins.remove(plugin_id);
    }

    /// 获取插件信息
    ///
    /// 根据插件ID获取插件信息。
    pub fn get(&self, plugin_id: &str) -> Option<&PluginInfo> {
        self.plugins.get(plugin_id)
    }

    /// 检查插件是否存在
    ///
    /// 检查注册表中是否包含指定插件。
    pub fn contains(&self, plugin_id: &str) -> bool {
        self.plugins.contains_key(plugin_id)
    }

    /// 列出所有插件
    ///
    /// 返回注册表中的所有插件信息。
    pub fn list_all(&self) -> Vec<PluginInfo> {
        self.plugins.values().cloned().collect()
    }

    /// 按状态筛选插件
    ///
    /// 返回指定状态的插件列表。
    pub fn filter_by_status(&self, status: PluginStatus) -> Vec<&PluginInfo> {
        self.plugins.values()
            .filter(|p| p.status == status)
            .collect()
    }

    /// 按名称搜索插件
    ///
    /// 返回名称包含指定字符串的插件列表。
    pub fn search_by_name(&self, name: &str) -> Vec<&PluginInfo> {
        self.plugins.values()
            .filter(|p| p.name.to_lowercase().contains(&name.to_lowercase()))
            .collect()
    }

    // /// 按作者筛选插件
    // ///
    // /// 返回指定作者的插件列表。
    // pub fn filter_by_author(&self, author: &str) -> Vec<&PluginInfo> {
    //     self.plugins.values()
    //         .filter(|p| p.author.as_ref().map(|a| a.to_lowercase().contains(&author.to_lowercase())).unwrap_or(false))
    //         .collect()
    // }

    /// 使用筛选条件查询插件
    ///
    /// 根据筛选条件返回匹配的插件列表。
    pub fn filter(&self, filter: &PluginFilter) -> Vec<&PluginInfo> {
        self.plugins.values()
            .filter(|p| {
                let mut matches = true;

                if let Some(ref status) = filter.status {
                    matches = matches && p.status == *status;
                }

                if let Some(ref name) = filter.name {
                    matches = matches && p.name.to_lowercase().contains(&name.to_lowercase());
                }



                matches
            })
            .collect()
    }

    /// 获取插件数量
    ///
    /// 返回注册表中的插件总数。
    pub fn count(&self) -> usize {
        self.plugins.len()
    }

    /// 按状态统计插件数量
    ///
    /// 返回指定状态的插件数量。
    pub fn count_by_status(&self, status: PluginStatus) -> usize {
        self.plugins.values()
            .filter(|p| p.status == status)
            .count()
    }

    /// 清空注册表
    ///
    /// 移除注册表中的所有插件。
    pub fn clear(&mut self) {
        self.plugins.clear();
    }

    /// 更新插件状态
    ///
    /// 更新指定插件的状态。
    pub fn update_status(&mut self, plugin_id: &str, status: PluginStatus) -> bool {
        if let Some(plugin) = self.plugins.get_mut(plugin_id) {
            plugin.status = status;
            plugin.updated_at = Some(chrono::Utc::now());
            true
        } else {
            false
        }
    }

    /// 遍历所有插件
    ///
    /// 对每个插件执行指定的操作。
    pub fn for_each<F>(&self, mut f: F)
    where
        F: FnMut(&PluginInfo),
    {
        for plugin in self.plugins.values() {
            f(plugin);
        }
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}
