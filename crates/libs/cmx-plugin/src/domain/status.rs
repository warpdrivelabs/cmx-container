//! 状态定义模块
//! 
//! 定义插件状态和状态转换规则

use serde::{Deserialize, Serialize};

/// 插件状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PluginStatus {
    /// 未安装
    NotInstalled,
    /// 已安装
    Installed,
    /// 激活中
    Activating,
    /// 已激活
    Activated,
    /// 停用中
    Deactivating,
    /// 已停用
    Deactivated,
    /// 卸载中
    Uninstalling,
    /// 错误状态
    Error,
}

impl PluginStatus {
    /// 检查是否可以转换到目标状态
    pub fn can_transition_to(&self, target: PluginStatus) -> bool {
        match self {
            PluginStatus::NotInstalled => {
                matches!(target, PluginStatus::Installed)
            }
            PluginStatus::Installed => {
                matches!(target, PluginStatus::Activating | PluginStatus::Uninstalling)
            }
            PluginStatus::Activating => {
                matches!(target, PluginStatus::Activated | PluginStatus::Error)
            }
            PluginStatus::Activated => {
                matches!(target, PluginStatus::Deactivating | PluginStatus::Error)
            }
            PluginStatus::Deactivating => {
                matches!(target, PluginStatus::Deactivated | PluginStatus::Error)
            }
            PluginStatus::Deactivated => {
                matches!(target, PluginStatus::Activating | PluginStatus::Uninstalling)
            }
            PluginStatus::Uninstalling => {
                matches!(target, PluginStatus::NotInstalled | PluginStatus::Error)
            }
            PluginStatus::Error => {
                matches!(target, PluginStatus::Installed | PluginStatus::NotInstalled)
            }
        }
    }
    
    /// 获取有效的目标状态列表
    pub fn valid_transitions(&self) -> Vec<PluginStatus> {
        match self {
            PluginStatus::NotInstalled => vec![PluginStatus::Installed],
            PluginStatus::Installed => vec![PluginStatus::Activating, PluginStatus::Uninstalling],
            PluginStatus::Activating => vec![PluginStatus::Activated, PluginStatus::Error],
            PluginStatus::Activated => vec![PluginStatus::Deactivating, PluginStatus::Error],
            PluginStatus::Deactivating => vec![PluginStatus::Deactivated, PluginStatus::Error],
            PluginStatus::Deactivated => vec![PluginStatus::Activating, PluginStatus::Uninstalling],
            PluginStatus::Uninstalling => vec![PluginStatus::NotInstalled, PluginStatus::Error],
            PluginStatus::Error => vec![PluginStatus::Installed, PluginStatus::NotInstalled],
        }
    }
    
    /// 检查是否为活动状态
    pub fn is_active(&self) -> bool {
        matches!(self, PluginStatus::Activated | PluginStatus::Activating)
    }
    
    /// 检查是否为已安装状态
    pub fn is_installed(&self) -> bool {
        !matches!(self, PluginStatus::NotInstalled)
    }
}

impl std::fmt::Display for PluginStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PluginStatus::NotInstalled => write!(f, "not_installed"),
            PluginStatus::Installed => write!(f, "installed"),
            PluginStatus::Activating => write!(f, "activating"),
            PluginStatus::Activated => write!(f, "activated"),
            PluginStatus::Deactivating => write!(f, "deactivating"),
            PluginStatus::Deactivated => write!(f, "deactivated"),
            PluginStatus::Uninstalling => write!(f, "uninstalling"),
            PluginStatus::Error => write!(f, "error"),
        }
    }
}

impl std::str::FromStr for PluginStatus {
    type Err = String;
    
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "not_installed" => Ok(PluginStatus::NotInstalled),
            "installed" => Ok(PluginStatus::Installed),
            "activating" => Ok(PluginStatus::Activating),
            "activated" => Ok(PluginStatus::Activated),
            "deactivating" => Ok(PluginStatus::Deactivating),
            "deactivated" => Ok(PluginStatus::Deactivated),
            "uninstalling" => Ok(PluginStatus::Uninstalling),
            "error" => Ok(PluginStatus::Error),
            _ => Err(format!("未知插件状态: {}", s)),
        }
    }
}

/// 状态转换结果
#[derive(Debug, Clone)]
pub struct StatusTransition {
    /// 源状态
    pub from: PluginStatus,
    /// 目标状态
    pub to: PluginStatus,
    /// 转换时间
    pub timestamp: chrono::DateTime<chrono::Utc>,
    /// 转换原因
    pub reason: Option<String>,
}

impl StatusTransition {
    /// 创建新的状态转换
    pub fn new(from: PluginStatus, to: PluginStatus) -> Self {
        Self {
            from,
            to,
            timestamp: chrono::Utc::now(),
            reason: None,
        }
    }
    
    /// 设置转换原因
    pub fn with_reason(mut self, reason: String) -> Self {
        self.reason = Some(reason);
        self
    }
}
