//! 生命周期管理模块
//! 
//! 定义和转换插件状态

/// 插件生命周期状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleState {
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

impl std::fmt::Display for LifecycleState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LifecycleState::NotInstalled => write!(f, "not_installed"),
            LifecycleState::Installed => write!(f, "installed"),
            LifecycleState::Activating => write!(f, "activating"),
            LifecycleState::Activated => write!(f, "activated"),
            LifecycleState::Deactivating => write!(f, "deactivating"),
            LifecycleState::Deactivated => write!(f, "deactivated"),
            LifecycleState::Uninstalling => write!(f, "uninstalling"),
            LifecycleState::Error => write!(f, "error"),
        }
    }
}

/// 生命周期状态机
pub struct LifecycleStateMachine;

impl LifecycleStateMachine {
    /// 检查状态转换是否有效
    pub fn can_transition(from: LifecycleState, to: LifecycleState) -> bool {
        match from {
            LifecycleState::NotInstalled => {
                matches!(to, LifecycleState::Installed)
            }
            LifecycleState::Installed => {
                matches!(to, LifecycleState::Activating | LifecycleState::Uninstalling)
            }
            LifecycleState::Activating => {
                matches!(to, LifecycleState::Activated | LifecycleState::Error)
            }
            LifecycleState::Activated => {
                matches!(to, LifecycleState::Deactivating | LifecycleState::Error)
            }
            LifecycleState::Deactivating => {
                matches!(to, LifecycleState::Deactivated | LifecycleState::Error)
            }
            LifecycleState::Deactivated => {
                matches!(to, LifecycleState::Activating | LifecycleState::Uninstalling)
            }
            LifecycleState::Uninstalling => {
                matches!(to, LifecycleState::NotInstalled | LifecycleState::Error)
            }
            LifecycleState::Error => {
                matches!(to, LifecycleState::Installed | LifecycleState::NotInstalled)
            }
        }
    }
    
    /// 获取有效的目标状态
    pub fn valid_transitions(from: LifecycleState) -> Vec<LifecycleState> {
        match from {
            LifecycleState::NotInstalled => vec![LifecycleState::Installed],
            LifecycleState::Installed => vec![LifecycleState::Activating, LifecycleState::Uninstalling],
            LifecycleState::Activating => vec![LifecycleState::Activated, LifecycleState::Error],
            LifecycleState::Activated => vec![LifecycleState::Deactivating, LifecycleState::Error],
            LifecycleState::Deactivating => vec![LifecycleState::Deactivated, LifecycleState::Error],
            LifecycleState::Deactivated => vec![LifecycleState::Activating, LifecycleState::Uninstalling],
            LifecycleState::Uninstalling => vec![LifecycleState::NotInstalled, LifecycleState::Error],
            LifecycleState::Error => vec![LifecycleState::Installed, LifecycleState::NotInstalled],
        }
    }
}
