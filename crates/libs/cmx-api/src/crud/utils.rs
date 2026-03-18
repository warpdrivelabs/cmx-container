//! CRUD 工具函数
//!
//! 提供字段预处理等辅助功能

use modql::field::SeaField;
use modql::field::SeaFields;
use sea_query::SimpleExpr;
use cmx_utils::snowflake_id_str;
use crate::crud::traits::DbBmc;



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
pub fn prep_fields_for_create<MC>(fields: &mut SeaFields, user_id: Option<&str>)-> String
where
    MC: DbBmc,
{
    // 添加 owner_id
    if MC::has_owner_id() {
        if let Some(uid) = user_id {
            let field: SeaField = ("owner_id", SimpleExpr::Value(sea_query::Value::String(Some(uid.to_string().into())))).into();
            fields.push(field);
        }
    }

    // 添加主键
    let pk_value = snowflake_id_str();
    let field: SeaField = (MC::PK_COLUMN, SimpleExpr::Value(sea_query::Value::String(Some(pk_value.clone().into())))).into();
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
