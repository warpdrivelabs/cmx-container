//! 实体创建和更新操作的字段预处理工具模块
//!
//! 此模块提供了用于在数据库实体创建和更新操作期间自动设置必要字段的工具函数，
//! 包括所有者ID字段等。
//!
//! 注意：时间戳字段（created_at, updated_at）由数据库自动处理，不在此处设置。

use crate::crud::traits::DbBmc;
use serde_json::Value;
use cmx_utils::snowflake_id_str;

/// 当模型控制器准备创建实体时调用此方法。
/// 此函数会根据模型的配置自动添加所有者ID（如果需要）。
///
/// # 参数
/// * `data` - 要创建的实体数据（JSON对象），会被修改以包含所需的额外字段
/// * `user_id` - 当前用户的ID，用于设置所有者ID和创建者ID
///
/// # 类型参数
/// * `MC` - 实现了DbBmc trait的模型控制器类型
///
/// # 注意
/// 时间戳字段（created_at, updated_at）由数据库自动处理
pub fn prep_fields_for_create<MC>(data: &mut Value, user_id: Option<String>)
where
    MC: DbBmc,
{
    if let Some(obj) = data.as_object_mut() {
        if MC::has_owner_id() {
            if let Some(uid) = &user_id {
                obj.insert("owner_id".to_string(), Value::String(uid.clone()));
            }
        }
        if !obj.contains_key(MC::PK_COLUMN) {
            obj.insert(MC::PK_COLUMN.into(), Value::String(snowflake_id_str()));
        }
    }
}

/// 当模型控制器计划更新其管理的实体时调用此方法。
/// 此函数目前为空，因为时间戳由数据库自动处理。
///
/// # 参数
/// * `data` - 要更新的实体数据（JSON对象），会被修改以包含所需的额外字段
/// * `user_id` - 当前用户的ID，用于设置最后修改者ID
///
/// # 类型参数
/// * `MC` - 实现了DbBmc trait的模型控制器类型
///
/// # 注意
/// 时间戳字段（updated_at）由数据库自动处理
#[allow(unused_variables)]
pub fn prep_fields_for_update<MC>(data: &mut Value, user_id: Option<String>)
where
    MC: DbBmc,
{
    // 时间戳字段由数据库自动处理，此处只保留接口
    // 如果需要手动设置 updated_by，可以在这里添加
}
