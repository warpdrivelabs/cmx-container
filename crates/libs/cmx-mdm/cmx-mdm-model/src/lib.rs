//! cmx-mdm-model —— 主数据（MDM）模块的语义中立层（纯逻辑，DB-free）。
//!
//! - [`LifecycleStatus`]：主数据生命周期状态枚举（cm_*.lifecycle_status）。
//! - [`activation`]：激活器纯逻辑（ActivationConfig/LineMapping/plan_create/plan_update/plan_lines）。
//! - [`codegen`]：编码生成 CodeGenerator trait + RandomCodeGenerator stub（M8 接 cmx-code）。
//! - [`MdmQuery`]：查询 DTO。
//!
//! V3 铁律：cm_* 主数据只存 [`LifecycleStatus::Published`]，草稿一律走 CR 单据（cv_mdm_apply），
//! 激活器是写入 cm_* published 的唯一入口。

pub mod activation;
pub mod codegen;
pub mod match_algo;
pub mod survivorship;

use serde::{Deserialize, Serialize};

/// 主数据生命周期状态（cm_*.lifecycle_status，只 4 值，无 draft）。
///
/// V3 铁律:cm_* 主数据只存 [`LifecycleStatus::Published`]。草稿走 CR 单据（cv_mdm_apply），
/// 审批通过由激活器落字典为 published。冻结/归档/合并是 published 之后的终态流转。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LifecycleStatus {
    /// 已发布（黄金记录，全企业零过滤消费）
    Published,
    /// 冻结（不可变更，但可查）
    Frozen,
    /// 归档（历史保留，不再活跃）
    Archived,
    /// 已合并（victim 被合并存活，不硬删）
    Merged,
}

impl LifecycleStatus {
    /// 状态字符串（落库 / 序列化用）。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Published => "published",
            Self::Frozen => "frozen",
            Self::Archived => "archived",
            Self::Merged => "merged",
        }
    }

    /// 默认状态（新建主数据激活时）。
    pub fn default_for_new() -> Self {
        Self::Published
    }
}

impl std::fmt::Display for LifecycleStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// M0 占位查询 DTO（M1 起按激活器/审计端点需求扩展）。
#[derive(Debug, Default, Deserialize)]
pub struct MdmQuery {
    /// 字典码（如 supplier）
    #[serde(default)]
    pub dict_code: Option<String>,
}
