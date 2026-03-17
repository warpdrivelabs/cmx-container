//! 实体创建和更新操作的字段预处理工具模块
//!
//! 此模块提供了用于在数据库实体创建和更新操作期间自动设置必要字段的工具函数，
//! 包括时间戳字段和所有者ID字段等。

use crate::crud::traits::DbBmc;
use cmx_utils::time::{format_time, now_utc};
use serde_json::Value;

/// 当模型控制器准备创建实体时调用此方法。
/// 此函数会根据模型的配置自动添加所有者ID（如果需要）和时间戳字段。
///
/// # 参数
/// * `data` - 要创建的实体数据（JSON对象），会被修改以包含所需的额外字段
/// * `user_id` - 当前用户的ID，用于设置所有者ID和创建者ID
///
/// # 类型参数
/// * `MC` - 实现了DbBmc trait的模型控制器类型
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
        if MC::has_timestamps() {
            add_timestamps_for_create(obj, user_id);
        }
    }
}

/// 当模型控制器计划更新其管理的实体时调用此方法。
/// 此函数会根据模型配置自动更新时间戳字段。
///
/// # 参数
/// * `data` - 要更新的实体数据（JSON对象），会被修改以包含所需的额外字段
/// * `user_id` - 当前用户的ID，用于设置最后修改者ID
///
/// # 类型参数
/// * `MC` - 实现了DbBmc trait的模型控制器类型
pub fn prep_fields_for_update<MC>(data: &mut Value, user_id: Option<String>)
where
    MC: DbBmc,
{
    if let Some(obj) = data.as_object_mut() {
        if MC::has_timestamps() {
            add_timestamps_for_update(obj, user_id);
        }
    }
}

/// 为创建操作添加时间戳信息
/// (例如，created_by、created_at、updated_by、updated_at 将被设置为相同的值)
/// 创建时，创建者ID和修改者ID相同，创建时间和修改时间也相同。
///
/// # 参数
/// * `obj` - 要添加时间戳字段的JSON对象
/// * `user_id` - 当前用户的ID，用作创建者ID和修改者ID
fn add_timestamps_for_create(obj: &mut serde_json::Map<String, Value>, user_id: Option<String>) {
    let now = format_time(now_utc());
    
    if let Some(uid) = &user_id {
        obj.insert("created_by".to_string(), Value::String(uid.clone()));
        obj.insert("updated_by".to_string(), Value::String(uid.clone()));
    }
    
    obj.insert("created_at".to_string(), Value::String(now.clone()));
    obj.insert("updated_at".to_string(), Value::String(now));
}

/// 仅为更新操作添加时间戳信息
/// (例如，只更新 updated_by 和 updated_at 字段)
/// 更新时，只修改最后修改者ID和修改时间，不改变创建者信息。
///
/// # 参数
/// * `obj` - 要添加时间戳字段的JSON对象
/// * `user_id` - 当前用户的ID，用作最后修改者ID
fn add_timestamps_for_update(obj: &mut serde_json::Map<String, Value>, user_id: Option<String>) {
    let now = format_time(now_utc());
    
    if let Some(uid) = &user_id {
        obj.insert("updated_by".to_string(), Value::String(uid.clone()));
    }
    
    obj.insert("updated_at".to_string(), Value::String(now));
}
