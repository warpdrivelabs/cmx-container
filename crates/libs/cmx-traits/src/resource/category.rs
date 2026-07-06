//! 资源数据类别枚举。
//!
//! 与 cmx-plugin 的 `DataCategory` 对应，定义在 cmx-traits 供跨 crate 使用。

use serde::{Deserialize, Serialize};

/// 数据类别枚举。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ResourceDataCategory {
    /// 菜单数据。
    Menu,
    /// 权限数据。
    Perm,
    /// 表单数据。
    Form,
    /// 流程数据。
    Flow,
}

impl ResourceDataCategory {
    /// 从目录名转换（如 "permdata" → Perm）。
    pub fn from_dir_name(dir_name: &str) -> Option<Self> {
        match dir_name {
            "menudata" => Some(Self::Menu),
            "permdata" => Some(Self::Perm),
            "formdata" => Some(Self::Form),
            "flowdata" => Some(Self::Flow),
            _ => None,
        }
    }

    /// 转为 proto 字符串标识。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Menu => "menu",
            Self::Perm => "perm",
            Self::Form => "form",
            Self::Flow => "flow",
        }
    }

    /// 从 proto 字符串标识解析。
    pub fn parse_from_str(s: &str) -> Option<Self> {
        match s {
            "menu" => Some(Self::Menu),
            "perm" => Some(Self::Perm),
            "form" => Some(Self::Form),
            "flow" => Some(Self::Flow),
            _ => None,
        }
    }
}
