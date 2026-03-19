//! 权限管理模块
//! 
//! 管理插件权限

use std::collections::HashSet;
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// 权限类型
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Permission {
    /// 文件系统读取
    FileRead,
    /// 文件系统写入
    FileWrite,
    /// 网络访问
    NetworkAccess,
    /// 数据库访问
    DatabaseAccess,
    /// 执行系统命令
    ExecuteCommand,
    /// 环境变量访问
    EnvAccess,
    /// 自定义权限
    Custom(String),
}

impl std::fmt::Display for Permission {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Permission::FileRead => write!(f, "file.read"),
            Permission::FileWrite => write!(f, "file.write"),
            Permission::NetworkAccess => write!(f, "network.access"),
            Permission::DatabaseAccess => write!(f, "database.access"),
            Permission::ExecuteCommand => write!(f, "execute.command"),
            Permission::EnvAccess => write!(f, "env.access"),
            Permission::Custom(name) => write!(f, "custom.{}", name),
        }
    }
}

/// 权限管理器
pub struct PermissionManager {
    /// 插件权限映射
    plugin_permissions: Arc<RwLock<std::collections::HashMap<String, HashSet<Permission>>>>,
}

impl PermissionManager {
    /// 创建新的权限管理器
    pub fn new() -> Self {
        Self {
            plugin_permissions: Arc::new(RwLock::new(std::collections::HashMap::new())),
        }
    }
    
    /// 授予权限
    pub async fn grant(&self, plugin_id: &str, permissions: Vec<Permission>) {
        let mut plugin_permissions = self.plugin_permissions.write().await;
        let entry = plugin_permissions.entry(plugin_id.to_string()).or_insert_with(HashSet::new);
        for permission in permissions {
            entry.insert(permission);
        }
    }
    
    /// 撤销权限
    pub async fn revoke(&self, plugin_id: &str, permissions: &[Permission]) {
        let mut plugin_permissions = self.plugin_permissions.write().await;
        if let Some(entry) = plugin_permissions.get_mut(plugin_id) {
            for permission in permissions {
                entry.remove(permission);
            }
        }
    }
    
    /// 检查权限
    pub async fn check(&self, plugin_id: &str, permission: &Permission) -> bool {
        let plugin_permissions = self.plugin_permissions.read().await;
        plugin_permissions
            .get(plugin_id)
            .map(|perms| perms.contains(permission))
            .unwrap_or(false)
    }
    
    /// 获取插件权限列表
    pub async fn get_permissions(&self, plugin_id: &str) -> Vec<Permission> {
        let plugin_permissions = self.plugin_permissions.read().await;
        plugin_permissions
            .get(plugin_id)
            .map(|perms| perms.iter().cloned().collect())
            .unwrap_or_default()
    }
    
    /// 清除插件权限
    pub async fn clear(&self, plugin_id: &str) {
        let mut plugin_permissions = self.plugin_permissions.write().await;
        plugin_permissions.remove(plugin_id);
    }
}

impl Default for PermissionManager {
    fn default() -> Self {
        Self::new()
    }
}
