//! 数据权限范围枚举

use serde::{Deserialize, Serialize};

/// 数据权限范围 — 本次仅定义枚举，不实现过滤逻辑
///
/// 未来实现数据权限时，BMC/Service 层根据此枚举注入 WHERE 条件
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub enum DataScope {
    /// 全部数据（默认）
    #[default]
    All,
    /// 自定义数据范围（部门ID列表）
    Custom(Vec<String>),
    /// 本部门数据
    Dept,
    /// 本部门及子部门数据
    DeptAndSub,
    /// 仅本人数据
    SelfData,
}

/// 从 int4 数据库值转换（cmx_role.data_scope 字段）
impl From<i64> for DataScope {
    fn from(v: i64) -> Self {
        match v {
            2 => DataScope::Custom(vec![]),
            3 => DataScope::Dept,
            4 => DataScope::DeptAndSub,
            5 => DataScope::SelfData,
            _ => DataScope::All,
        }
    }
}

impl From<&DataScope> for i64 {
    fn from(ds: &DataScope) -> Self {
        match ds {
            DataScope::All => 1,
            DataScope::Custom(_) => 2,
            DataScope::Dept => 3,
            DataScope::DeptAndSub => 4,
            DataScope::SelfData => 5,
        }
    }
}
