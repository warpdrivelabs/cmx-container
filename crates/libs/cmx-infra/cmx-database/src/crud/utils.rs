//! CRUD 工具函数
//!
//! 提供字段预处理等辅助功能

use crate::crud::DbBmc;
use chrono::Utc;
use cmx_utils::snowflake_id_str;
use modql::field::SeaField;
use modql::field::SeaFields;
use sea_query::SimpleExpr;

pub const FIELD_CREATE_TIME: &str = "create_time";
pub const FIELD_UPDATE_TIME: &str = "update_time";
pub const FIELD_CREATE_BY: &str = "create_by";
pub const FIELD_UPDATE_BY: &str = "update_by";

/// 为创建操作准备字段
///
/// 根据模型控制器的元数据，自动处理创建时需要的字段：
/// - 添加主键（如果不存在）
/// - 添加 owner_id（如果模型支持）
///
/// # 参数
/// * `fields` - SeaFields 字段集合
/// * `user_id` - 用户 ID（用于审计字段）
/// return 主键
pub fn prep_fields_for_create<MC>(fields: &mut SeaFields, user_id: Option<&str>) -> String
where
    MC: DbBmc,
{
    // 添加 owner_id
    if MC::has_owner_id() {
        if let Some(uid) = user_id {
            let field: SeaField = (
                "owner_id",
                SimpleExpr::Value(sea_query::Value::String(Some(uid.to_string().into()))),
            )
                .into();
            fields.push(field);
        }
    }
    if MC::has_timestamps() {
        add_timestamps_for_create(fields, user_id);
    }

    // 添加主键
    let pk_value = snowflake_id_str();
    let field: SeaField = (
        MC::PK_COLUMN,
        SimpleExpr::Value(sea_query::Value::String(Some(pk_value.clone().into()))),
    )
        .into();
    fields.push(field);
    pk_value
}

/// 为更新操作准备字段
///
/// 根据模型控制器的元数据，自动处理更新时需要的字段
/// 时间戳由数据库自动处理
///
/// # 参数
/// * `fields` - SeaFields 字段集合
/// * `user_id` - 用户 ID（用于审计字段）
pub fn prep_fields_for_update<MC>(_fields: &mut SeaFields, _user_id: Option<&str>)
where
    MC: DbBmc,
{
    // 更新操作通常不需要添加额外字段
    // 时间戳由数据库自动处理
}
/// 为创建操作添加时间戳信息
/// (例如，cid、ctime、mid、mtime 将被设置为相同的值)
/// 创建时，创建者ID和修改者ID相同，创建时间和修改时间也相同。
///
/// # 参数
/// * `fields` - 要添加时间戳字段的字段集合
/// * `user_id` - 当前用户的ID，用作创建者ID和修改者ID
fn add_timestamps_for_create(fields: &mut SeaFields, user_id: Option<&str>) {
    let now = Utc::now();
    fields.push(SeaField::new(FIELD_CREATE_BY, user_id));
    fields.push(SeaField::new(FIELD_UPDATE_BY, user_id));
    fields.push(SeaField::new(FIELD_CREATE_TIME, now));

    fields.push(SeaField::new(FIELD_UPDATE_TIME, now));
}
/// 仅为更新操作添加时间戳信息
/// (例如，只更新 mid 和 mtime 字段)
/// 更新时，只修改最后修改者ID和修改时间，不改变创建者信息。
///
/// # 参数
/// * `fields` - 要添加时间戳字段的字段集合
/// * `user_id` - 当前用户的ID，用作最后修改者ID
fn add_timestamps_for_update(fields: &mut SeaFields, user_id: Option<&str>) {
    let now = Utc::now();
    fields.push(SeaField::new(FIELD_UPDATE_BY, user_id));
    fields.push(SeaField::new(FIELD_UPDATE_TIME, now));
}
