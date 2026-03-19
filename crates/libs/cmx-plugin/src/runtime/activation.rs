//! 激活管理模块
//! 
//! 管理 WASM 运行时

use std::collections::HashMap;
use std::sync::Arc;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// 运行时实例
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeInstance {
    /// 插件ID
    pub plugin_id: String,
    /// 版本
    pub version: String,
    /// 是否激活
    pub active: bool,
    /// 激活时间
    pub activated_at: DateTime<Utc>,
    /// 内存限制（字节）
    pub memory_limit: Option<u64>,
    /// 已使用内存
    pub memory_used: u64,
    /// 调用次数
    pub call_count: u64,
}

impl RuntimeInstance {
    /// 创建新的运行时实例
    pub fn new(plugin_id: String, version: String) -> Self {
        Self {
            plugin_id,
            version,
            active: true,
            activated_at: Utc::now(),
            memory_limit: None,
            memory_used: 0,
            call_count: 0,
        }
    }
    
    /// 设置内存限制
    pub fn with_memory_limit(mut self, limit: u64) -> Self {
        self.memory_limit = Some(limit);
        self
    }
    
    /// 记录调用
    pub fn record_call(&mut self) {
        self.call_count += 1;
    }
    
    /// 更新内存使用
    pub fn update_memory(&mut self, used: u64) {
        self.memory_used = used;
    }
    
    /// 检查是否超过内存限制
    pub fn is_memory_exceeded(&self) -> bool {
        self.memory_limit.map(|limit| self.memory_used > limit).unwrap_or(false)
    }
}

/// 激活管理器配置
#[derive(Debug, Clone)]
pub struct ActivationManagerConfig {
    /// 默认内存限制（字节）
    pub default_memory_limit: Option<u64>,
    /// 最大激活插件数
    pub max_active_plugins: usize,
}

impl Default for ActivationManagerConfig {
    fn default() -> Self {
        Self {
            default_memory_limit: Some(100 * 1024 * 1024), // 100MB
            max_active_plugins: 100,
        }
    }
}

/// 激活管理器
pub struct ActivationManager {
    /// 运行时实例映射
    instances: Arc<RwLock<HashMap<String, RuntimeInstance>>>,
    /// 配置
    config: ActivationManagerConfig,
}

impl ActivationManager {
    /// 创建新的激活管理器
    pub fn new() -> Self {
        Self {
            instances: Arc::new(RwLock::new(HashMap::new())),
            config: ActivationManagerConfig::default(),
        }
    }
    
    /// 使用配置创建激活管理器
    pub fn with_config(config: ActivationManagerConfig) -> Self {
        Self {
            instances: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }
    
    /// 激活插件
    pub async fn activate(&self, plugin_id: &str, version: &str) -> Result<(), String> {
        let mut instances = self.instances.write().await;
        
        // 检查是否已激活
        if instances.contains_key(plugin_id) {
            return Err(format!("插件 {} 已经激活", plugin_id));
        }
        
        // 检查是否超过最大激活数
        if instances.len() >= self.config.max_active_plugins {
            return Err(format!("已达到最大激活插件数: {}", self.config.max_active_plugins));
        }
        
        // 创建运行时实例
        let mut instance = RuntimeInstance::new(plugin_id.to_string(), version.to_string());
        
        // 设置内存限制
        if let Some(limit) = self.config.default_memory_limit {
            instance = instance.with_memory_limit(limit);
        }
        
        instances.insert(plugin_id.to_string(), instance);
        
        Ok(())
    }
    
    /// 停用插件
    pub async fn deactivate(&self, plugin_id: &str) -> Result<(), String> {
        let mut instances = self.instances.write().await;
        
        if let Some(_instance) = instances.remove(plugin_id) {
            Ok(())
        } else {
            Err(format!("插件 {} 未激活", plugin_id))
        }
    }
    
    /// 检查插件是否激活
    pub async fn is_active(&self, plugin_id: &str) -> bool {
        let instances = self.instances.read().await;
        instances.get(plugin_id).map(|i| i.active).unwrap_or(false)
    }
    
    /// 获取激活的插件列表
    pub async fn get_active_plugins(&self) -> Vec<String> {
        let instances = self.instances.read().await;
        instances.values()
            .filter(|i| i.active)
            .map(|i| i.plugin_id.clone())
            .collect()
    }
    
    /// 获取运行时实例
    pub async fn get_instance(&self, plugin_id: &str) -> Option<RuntimeInstance> {
        let instances = self.instances.read().await;
        instances.get(plugin_id).cloned()
    }
    
    /// 记录调用
    pub async fn record_call(&self, plugin_id: &str) -> Result<(), String> {
        let mut instances = self.instances.write().await;
        
        if let Some(instance) = instances.get_mut(plugin_id) {
            instance.record_call();
            Ok(())
        } else {
            Err(format!("插件 {} 未激活", plugin_id))
        }
    }
    
    /// 更新内存使用
    pub async fn update_memory(&self, plugin_id: &str, used: u64) -> Result<(), String> {
        let mut instances = self.instances.write().await;
        
        if let Some(instance) = instances.get_mut(plugin_id) {
            instance.update_memory(used);
            
            // 检查是否超过内存限制
            if instance.is_memory_exceeded() {
                return Err(format!("插件 {} 超过内存限制", plugin_id));
            }
            
            Ok(())
        } else {
            Err(format!("插件 {} 未激活", plugin_id))
        }
    }
    
    /// 获取激活插件数量
    pub async fn active_count(&self) -> usize {
        let instances = self.instances.read().await;
        instances.values().filter(|i| i.active).count()
    }
    
    /// 获取总调用次数
    pub async fn total_calls(&self) -> u64 {
        let instances = self.instances.read().await;
        instances.values().map(|i| i.call_count).sum()
    }
    
    /// 获取总内存使用
    pub async fn total_memory_used(&self) -> u64 {
        let instances = self.instances.read().await;
        instances.values().map(|i| i.memory_used).sum()
    }
    
    /// 获取所有实例信息
    pub async fn get_all_instances(&self) -> Vec<RuntimeInstance> {
        let instances = self.instances.read().await;
        instances.values().cloned().collect()
    }
    
    /// 停用所有插件
    pub async fn deactivate_all(&self) {
        let mut instances = self.instances.write().await;
        instances.clear();
    }
}

impl Default for ActivationManager {
    fn default() -> Self {
        Self::new()
    }
}
