//! 权限检查器模块 - 插件权限管理
//!
//! 提供插件权限的检查、授予和撤销功能。

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// 权限类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionType {
    /// 文件系统访问
    FileSystem,
    /// 网络访问
    Network,
    /// 数据库访问
    Database,
    /// 环境变量访问
    Environment,
    /// 系统调用
    SystemCall,
    /// 其他插件调用
    PluginCall,
    /// 自定义权限
    Custom,
}

/// 权限定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Permission {
    /// 权限名称
    pub name: String,
    /// 权限类型
    pub permission_type: PermissionType,
    /// 权限描述
    pub description: Option<String>,
    /// 权限资源（如文件路径、URL 等）
    pub resource: Option<String>,
    /// 权限操作（如 read, write, execute）
    pub action: Option<String>,
    /// 是否为危险权限
    pub is_dangerous: bool,
}

impl Permission {
    /// 创建新的权限
    pub fn new(name: impl Into<String>, permission_type: PermissionType) -> Self {
        Self {
            name: name.into(),
            permission_type,
            description: None,
            resource: None,
            action: None,
            is_dangerous: false,
        }
    }

    /// 设置描述
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// 设置资源
    pub fn with_resource(mut self, resource: impl Into<String>) -> Self {
        self.resource = Some(resource.into());
        self
    }

    /// 设置操作
    pub fn with_action(mut self, action: impl Into<String>) -> Self {
        self.action = Some(action.into());
        self
    }

    /// 标记为危险权限
    pub fn dangerous(mut self) -> Self {
        self.is_dangerous = true;
        self
    }

    /// 生成权限 ID
    pub fn id(&self) -> String {
        format!("{}:{}:{}", 
            self.permission_type.as_str(),
            self.resource.as_deref().unwrap_or("*"),
            self.action.as_deref().unwrap_or("*")
        )
    }
}

impl PermissionType {
    /// 转换为字符串
    pub fn as_str(&self) -> &'static str {
        match self {
            PermissionType::FileSystem => "fs",
            PermissionType::Network => "net",
            PermissionType::Database => "db",
            PermissionType::Environment => "env",
            PermissionType::SystemCall => "sys",
            PermissionType::PluginCall => "plugin",
            PermissionType::Custom => "custom",
        }
    }
}

/// 插件权限配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginPermissionConfig {
    /// 插件 ID
    pub plugin_id: String,
    /// 请求的权限列表
    pub requested_permissions: Vec<Permission>,
    /// 授予的权限列表
    pub granted_permissions: Vec<String>,
    /// 权限策略
    pub policy: PermissionPolicy,
}

/// 权限策略
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionPolicy {
    /// 严格模式 - 只授予明确允许的权限
    Strict,
    /// 宽松模式 - 自动授予非危险权限
    Permissive,
    /// 自定义模式 - 根据配置决定
    Custom,
}

/// 权限检查结果
#[derive(Debug, Clone)]
pub struct PermissionCheckResult {
    /// 是否通过
    pub granted: bool,
    /// 检查的权限
    pub permission: Permission,
    /// 拒绝原因
    pub reason: Option<String>,
}

/// 权限检查器 - 管理插件权限
pub struct PermissionChecker {
    /// 默认权限策略
    default_policy: PermissionPolicy,
    /// 插件权限配置
    plugin_permissions: Arc<RwLock<HashMap<String, PluginPermissionConfig>>>,
    /// 全局权限白名单
    whitelist: Arc<RwLock<HashSet<String>>>,
    /// 全局权限黑名单
    blacklist: Arc<RwLock<HashSet<String>>>,
}

impl PermissionChecker {
    /// 创建新的权限检查器
    pub fn new(default_policy: PermissionPolicy) -> Self {
        Self {
            default_policy,
            plugin_permissions: Arc::new(RwLock::new(HashMap::new())),
            whitelist: Arc::new(RwLock::new(HashSet::new())),
            blacklist: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    /// 使用严格策略创建
    pub fn strict() -> Self {
        Self::new(PermissionPolicy::Strict)
    }

    /// 使用宽松策略创建
    pub fn permissive() -> Self {
        Self::new(PermissionPolicy::Permissive)
    }

    /// 注册插件权限
    pub async fn register_plugin(&self, config: PluginPermissionConfig) -> Result<(), PermissionError> {
        let plugin_id = config.plugin_id.clone();
        
        // 根据策略自动授予权限
        let mut config = config;
        for permission in &config.requested_permissions {
            if self.should_auto_grant(permission).await {
                config.granted_permissions.push(permission.id());
            }
        }

        let mut permissions = self.plugin_permissions.write().await;
        permissions.insert(plugin_id.clone(), config);
        
        log::info!("插件权限注册成功: {}", plugin_id);
        Ok(())
    }

    /// 注销插件权限
    pub async fn unregister_plugin(&self, plugin_id: &str) -> Result<(), PermissionError> {
        let mut permissions = self.plugin_permissions.write().await;
        permissions.remove(plugin_id);
        
        log::info!("插件权限注销成功: {}", plugin_id);
        Ok(())
    }

    /// 检查权限
    pub async fn check(&self, plugin_id: &str, permission: &Permission) -> PermissionCheckResult {
        let permission_id = permission.id();

        // 检查黑名单
        {
            let blacklist = self.blacklist.read().await;
            if blacklist.contains(&permission_id) {
                return PermissionCheckResult {
                    granted: false,
                    permission: permission.clone(),
                    reason: Some("权限在黑名单中".to_string()),
                };
            }
        }

        // 检查白名单
        {
            let whitelist = self.whitelist.read().await;
            if whitelist.contains(&permission_id) {
                return PermissionCheckResult {
                    granted: true,
                    permission: permission.clone(),
                    reason: None,
                };
            }
        }

        // 检查插件权限配置
        let permissions = self.plugin_permissions.read().await;
        match permissions.get(plugin_id) {
            Some(config) => {
                let granted = config.granted_permissions.contains(&permission_id);
                PermissionCheckResult {
                    granted,
                    permission: permission.clone(),
                    reason: if granted {
                        None
                    } else {
                        Some("插件未被授予此权限".to_string())
                    },
                }
            }
            None => PermissionCheckResult {
                granted: false,
                permission: permission.clone(),
                reason: Some("插件未注册权限配置".to_string()),
            },
        }
    }

    /// 授予权限
    pub async fn grant(&self, plugin_id: &str, permission_id: &str) -> Result<(), PermissionError> {
        let mut permissions = self.plugin_permissions.write().await;
        match permissions.get_mut(plugin_id) {
            Some(config) => {
                if !config.granted_permissions.contains(&permission_id.to_string()) {
                    config.granted_permissions.push(permission_id.to_string());
                    log::info!("授予权限: {} -> {}", plugin_id, permission_id);
                }
                Ok(())
            }
            None => Err(PermissionError::PluginNotRegistered(plugin_id.to_string())),
        }
    }

    /// 撤销权限
    pub async fn revoke(&self, plugin_id: &str, permission_id: &str) -> Result<(), PermissionError> {
        let mut permissions = self.plugin_permissions.write().await;
        match permissions.get_mut(plugin_id) {
            Some(config) => {
                config.granted_permissions.retain(|p| p != permission_id);
                log::info!("撤销权限: {} -> {}", plugin_id, permission_id);
                Ok(())
            }
            None => Err(PermissionError::PluginNotRegistered(plugin_id.to_string())),
        }
    }

    /// 添加到白名单
    pub async fn add_to_whitelist(&self, permission_id: &str) {
        let mut whitelist = self.whitelist.write().await;
        whitelist.insert(permission_id.to_string());
        log::info!("添加权限到白名单: {}", permission_id);
    }

    /// 添加到黑名单
    pub async fn add_to_blacklist(&self, permission_id: &str) {
        let mut blacklist = self.blacklist.write().await;
        blacklist.insert(permission_id.to_string());
        log::info!("添加权限到黑名单: {}", permission_id);
    }

    /// 从白名单移除
    pub async fn remove_from_whitelist(&self, permission_id: &str) {
        let mut whitelist = self.whitelist.write().await;
        whitelist.remove(permission_id);
    }

    /// 从黑名单移除
    pub async fn remove_from_blacklist(&self, permission_id: &str) {
        let mut blacklist = self.blacklist.write().await;
        blacklist.remove(permission_id);
    }

    /// 获取插件的已授予权限
    pub async fn get_granted_permissions(&self, plugin_id: &str) -> Vec<String> {
        let permissions = self.plugin_permissions.read().await;
        match permissions.get(plugin_id) {
            Some(config) => config.granted_permissions.clone(),
            None => Vec::new(),
        }
    }

    /// 判断是否应该自动授予权限
    async fn should_auto_grant(&self, permission: &Permission) -> bool {
        match self.default_policy {
            PermissionPolicy::Permissive => !permission.is_dangerous,
            PermissionPolicy::Strict => false,
            PermissionPolicy::Custom => {
                // 自定义模式下检查白名单
                let whitelist = self.whitelist.read().await;
                whitelist.contains(&permission.id())
            }
        }
    }
}

impl Default for PermissionChecker {
    fn default() -> Self {
        Self::strict()
    }
}

/// 权限错误
#[derive(Debug, thiserror::Error)]
pub enum PermissionError {
    #[error("插件未注册: {0}")]
    PluginNotRegistered(String),
    #[error("权限不存在: {0}")]
    PermissionNotFound(String),
    #[error("权限被拒绝: {0}")]
    Denied(String),
}

/// 预定义权限
pub mod predefined {
    use super::*;

    /// 文件读取权限
    pub fn file_read(path: &str) -> Permission {
        Permission::new("file_read", PermissionType::FileSystem)
            .with_resource(path)
            .with_action("read")
    }

    /// 文件写入权限
    pub fn file_write(path: &str) -> Permission {
        Permission::new("file_write", PermissionType::FileSystem)
            .with_resource(path)
            .with_action("write")
            .dangerous()
    }

    /// 网络访问权限
    pub fn network_access(host: &str) -> Permission {
        Permission::new("network_access", PermissionType::Network)
            .with_resource(host)
    }

    /// 数据库访问权限
    pub fn database_access(db_id: &str) -> Permission {
        Permission::new("database_access", PermissionType::Database)
            .with_resource(db_id)
            .dangerous()
    }

    /// 环境变量读取权限
    pub fn env_read(key: &str) -> Permission {
        Permission::new("env_read", PermissionType::Environment)
            .with_resource(key)
    }

    /// 插件调用权限
    pub fn plugin_call(plugin_id: &str) -> Permission {
        Permission::new("plugin_call", PermissionType::PluginCall)
            .with_resource(plugin_id)
    }
}
