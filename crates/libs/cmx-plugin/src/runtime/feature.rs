//! 功能管理模块
//! 
//! 管理插件初始化后的功能

use std::collections::HashMap;
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use super::service_registry::ServiceRegistry;
use crate::infrastructure::messaging::event::EventBus;

/// 功能类型
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FeatureType {
    /// 服务
    Service,
    /// 事件处理器
    EventHandler,
    /// 定时任务
    Scheduler,
    /// API端点
    Api,
    /// 自定义
    Custom(String),
}

/// 功能定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Feature {
    /// 功能ID
    pub id: String,
    /// 功能名称
    pub name: String,
    /// 所属插件ID
    pub plugin_id: String,
    /// 功能类型
    pub feature_type: FeatureType,
    /// 描述
    pub description: Option<String>,
    /// 配置
    pub config: Option<serde_json::Value>,
    /// 是否启用
    pub enabled: bool,
}

/// 功能管理器
pub struct FeatureManager {
    /// 功能注册表
    features: Arc<RwLock<HashMap<String, Feature>>>,
    /// 插件功能映射
    plugin_features: Arc<RwLock<HashMap<String, Vec<String>>>>,
    /// 服务注册表
    service_registry: Arc<ServiceRegistry>,
    /// 事件总线
    event_bus: Arc<EventBus>,
}

impl FeatureManager {
    /// 创建新的功能管理器
    pub fn new(service_registry: Arc<ServiceRegistry>, event_bus: Arc<EventBus>) -> Self {
        Self {
            features: Arc::new(RwLock::new(HashMap::new())),
            plugin_features: Arc::new(RwLock::new(HashMap::new())),
            service_registry,
            event_bus,
        }
    }
    
    /// 注册功能
    pub async fn register(&self, feature: Feature) -> Result<(), String> {
        let mut features = self.features.write().await;
        let mut plugin_features = self.plugin_features.write().await;
        
        if features.contains_key(&feature.id) {
            return Err(format!("功能 {} 已存在", feature.id));
        }
        
        let plugin_id = feature.plugin_id.clone();
        let feature_id = feature.id.clone();
        
        features.insert(feature_id.clone(), feature);
        
        plugin_features
            .entry(plugin_id)
            .or_insert_with(Vec::new)
            .push(feature_id);
        
        Ok(())
    }
    
    /// 注销功能
    pub async fn unregister(&self, feature_id: &str) -> Option<Feature> {
        let mut features = self.features.write().await;
        let mut plugin_features = self.plugin_features.write().await;
        
        if let Some(feature) = features.remove(feature_id) {
            if let Some(feature_ids) = plugin_features.get_mut(&feature.plugin_id) {
                feature_ids.retain(|id| id != feature_id);
            }
            Some(feature)
        } else {
            None
        }
    }
    
    /// 获取功能
    pub async fn get(&self, feature_id: &str) -> Option<Feature> {
        let features = self.features.read().await;
        features.get(feature_id).cloned()
    }
    
    /// 获取插件的所有功能
    pub async fn get_plugin_features(&self, plugin_id: &str) -> Vec<Feature> {
        let features = self.features.read().await;
        let plugin_features = self.plugin_features.read().await;
        
        plugin_features
            .get(plugin_id)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| features.get(id).cloned())
                    .collect()
            })
            .unwrap_or_default()
    }
    
    /// 启用功能
    pub async fn enable(&self, feature_id: &str) -> Result<(), String> {
        let mut features = self.features.write().await;
        
        if let Some(feature) = features.get_mut(feature_id) {
            feature.enabled = true;
            Ok(())
        } else {
            Err(format!("功能 {} 不存在", feature_id))
        }
    }
    
    /// 禁用功能
    pub async fn disable(&self, feature_id: &str) -> Result<(), String> {
        let mut features = self.features.write().await;
        
        if let Some(feature) = features.get_mut(feature_id) {
            feature.enabled = false;
            Ok(())
        } else {
            Err(format!("功能 {} 不存在", feature_id))
        }
    }
    
    /// 注销插件的所有功能
    pub async fn unregister_plugin_features(&self, plugin_id: &str) {
        let mut features = self.features.write().await;
        let mut plugin_features = self.plugin_features.write().await;
        
        if let Some(feature_ids) = plugin_features.remove(plugin_id) {
            for feature_id in feature_ids {
                features.remove(&feature_id);
            }
        }
    }
}
