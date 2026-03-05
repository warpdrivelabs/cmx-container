use serde::{Deserialize, Serialize};
use strum_macros::{AsRefStr, EnumString};

// 宏用于自动实现表名enum的通用方法
macro_rules! impl_table_enum {
    ($enum_name:ident) => {
        impl $enum_name {
            pub fn from_str(s: &str) -> Option<Self> {
                s.parse().ok()
            }

            pub fn to_str(&self) -> &str {
                self.as_ref()
            }
        }
    };
}

#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize, EnumString, AsRefStr)]
pub enum CMX_SYS_TABLE_NAMES {
    #[strum(serialize = "CMX_SYS_CONFIGS")]
    CMX_SYS_CONFIGS,        // 基础配置表 Level 0
    #[strum(serialize = "CMX_SYS_OPLOGS")]
    CMX_SYS_OPLOGS,         // 操作日志表 Level 0
    #[strum(serialize = "CMX_SYS_DOMAINS")]
    CMX_SYS_DOMAINS,        // 领域表 Level 1
    #[strum(serialize = "CMX_SYS_PROJECTS")]
    CMX_SYS_PROJECTS,       // 项目表 Level 2
    #[strum(serialize = "CMX_SYS_APPS")]
    CMX_SYS_APPS,           // 应用表 Level 3
    #[strum(serialize = "CMX_SYS_MODELS")]
    CMX_SYS_MODELS,         // 业务模型表 Level 4
    #[strum(serialize = "CMX_SYS_CUBES")]
    CMX_SYS_CUBES,          // 报表模型表 Level 4
    #[strum(serialize = "CMX_SYS_RLGLS")] 
    CMX_SYS_RLGLS,          // 关系管理表 Level 5
    #[strum(serialize = "CMX_SYS_DICTS")]
    CMX_SYS_DICTS,          // 数据字典表 Level 5
    #[strum(serialize = "CMX_SYS_FACTS")]
    CMX_SYS_FACTS,          // 业务事实表 Level 5

    // #[strum(serialize = "CMX_SYS_MDL_CTN")]
    // CMX_SYS_MDL_CTN,        // 模型内容表
    // #[strum(serialize = "CMX_SYS_MDL_VAL")]
    // CMX_SYS_MDL_VAL,        // 模型值表

    // #[strum(serialize = "CMX_SYS_DCT_CST")]
    // CMX_SYS_DCT_CST,        // 数据字典自定义表
    // #[strum(serialize = "CMX_SYS_KEYS")]
    // CMX_SYS_KEYS,           // 主键管理表
    // #[strum(serialize = "CMX_SYS_INDEXS")]
    // CMX_SYS_INDEXS,         // 索引表
    // #[strum(serialize = "CMX_SYS_OBJCOLS")]
    // CMX_SYS_OBJCOLS,        // 对象列表
    // #[strum(serialize = "CMX_SYS_OBJECTS")]
    // CMX_SYS_OBJECTS,        // 对象表
    // #[strum(serialize = "CMX_SYS_OBJ_VAL")]
    // CMX_SYS_OBJ_VAL,        // 对象值表

    // #[strum(serialize = "CMX_SYS_OPLOG")]
    // CMX_SYS_OPLOG,          // 操作日志表

}

impl_table_enum!(CMX_SYS_TABLE_NAMES);