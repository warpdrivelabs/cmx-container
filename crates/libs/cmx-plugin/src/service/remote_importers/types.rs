//! 基础服务中心数据类型。
//!
//! 仅保留 `DataCategory` 枚举(供服务名路由)。
//! 原先的 `DispatchContext`/`DispatchResult`/`CenterSendRequest` 等类型
//! 随 dispatcher/sender 一并移除(被上层 Remote 定义导入器取代)。

use serde::{Deserialize, Serialize};

/// 数据类别枚举。
///
/// 标识模块资源类别及对应的目标服务中心,供 `CenterClientConfig` 解析远程服务名。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DataCategory {
    /// 菜单数据 → 门户中心。
    Menu,
    /// 权限数据 → 权限中心。
    Perm,
    /// 表单数据 → 表单中心。
    Form,
    /// 流程定义数据 → 流程中心。
    Flow,
}

impl DataCategory {
    /// 返回数据目录名称。
    pub fn dir_name(&self) -> &str {
        match self {
            Self::Menu => "menudata",
            Self::Perm => "permdata",
            Self::Form => "formdata",
            Self::Flow => "flowdata",
        }
    }

    /// 返回用于 HTTP/gRPC 传输的短标识符。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Menu => "menu",
            Self::Perm => "perm",
            Self::Form => "form",
            Self::Flow => "flow",
        }
    }

    /// 返回对应服务中心的中文名称。
    pub fn center_name(&self) -> &str {
        match self {
            Self::Menu => "门户中心",
            Self::Perm => "权限中心",
            Self::Form => "表单中心",
            Self::Flow => "流程中心",
        }
    }

    /// 返回所有数据类别的有序列表。
    pub fn all() -> &'static [DataCategory] {
        &[Self::Menu, Self::Perm, Self::Form, Self::Flow]
    }
}
