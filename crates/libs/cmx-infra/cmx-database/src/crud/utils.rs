//! CRUD 工具函数
//!
//! 提供字段预处理等辅助功能

use crate::crud::DbBmc;
use chrono::Utc;
use cmx_core::model::cell::DataValue;
use cmx_core::model::data::dataset::DataSet;
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

/// 对 SeaFields 中声明为加密字段的值进行自动加密处理
///
/// 遍历 MC::encrypted_fields() 中声明的字段名，
/// 在 SeaFields 中查找匹配的字段，对其值调用 CryptoService::encrypt() 加密。
/// 如果 CryptoService 未初始化或没有加密字段，则原样返回。
///
/// # 参数
/// * `fields` - SeaFields 字段集合（消费）
///
/// # 返回值
/// 加密处理后的 SeaFields
pub fn encrypt_sea_fields<MC>(fields: SeaFields) -> SeaFields
where
    MC: DbBmc,
{
    // 获取模型声明的加密字段列表
    let enc_fields = MC::encrypted_fields();
    if enc_fields.is_empty() {
        return fields;
    }

    // 获取全局 CryptoService 实例，未初始化则原样返回
    let crypto = match cmx_utils::crypto::CryptoService::global() {
        Ok(s) => s,
        Err(_) => {
            return fields;
        }
    };

    // 消费 SeaFields，逐字段检查并加密
    let mut vec = fields.into_vec();
    for field in &mut vec {
        let field_name = field.iden.to_string();
        // 跳过非加密字段
        if !enc_fields.contains(&field_name.as_str()) {
            continue;
        }

        // 尝试提取字段值中的字符串
        if let Some(sea_val) = field.sea_value() {
            if let Some(str_val) = extract_string_from_sea_value(sea_val) {
                match crypto.encrypt(&str_val) {
                    Ok(encrypted) => {
                        // 将字段值替换为加密后的字符串
                        field.value = sea_query::SimpleExpr::Value(
                            sea_query::Value::String(Some(encrypted.into())),
                        );
                    }
                    Err(e) => {
                        tracing::warn!("字段 {} 加密失败: {}", field_name, e);
                    }
                }
            }
        }
    }

    SeaFields::new(vec)
}

/// 从 sea_query::Value 中提取字符串值
///
/// # 参数
/// * `value` - sea_query 的 Value 枚举引用
///
/// # 返回值
/// 如果是包含字符串的 Value，返回 Some(String)；否则返回 None
fn extract_string_from_sea_value(value: &sea_query::Value) -> Option<String> {
    if let sea_query::Value::String(Some(s)) = value {
        Some(s.to_string())
    } else {
        None
    }
}

/// 对 DataSet 中声明为加密字段的值进行解密处理
///
/// 遍历 DataSet 的每一行，根据字段名找到加密字段的位置，
/// 对加密字段的值调用 CryptoService::decrypt() 解密。
/// 如果 CryptoService 未初始化或没有加密字段，则不做任何处理。
///
/// # 参数
/// * `dataset` - 查询返回的 DataSet（可变引用）
/// * `fields` - 需要解密的字段名列表
pub fn decrypt_dataset_fields(dataset: &mut DataSet, fields: &[&str]) {
    // 没有需要解密的字段，直接返回
    if fields.is_empty() {
        return;
    }

    // 获取全局 CryptoService 实例，未初始化则不做处理
    let crypto = match cmx_utils::crypto::CryptoService::global() {
        Ok(s) => s,
        Err(_) => return,
    };

    // 从 schema 中查找加密字段对应的列索引位置
    let schema = &dataset.schema;
    let col_indices: Vec<usize> = fields
        .iter()
        .filter_map(|f| schema.get_index(f))
        .collect();

    // 没有匹配到任何列索引，直接返回
    if col_indices.is_empty() {
        return;
    }

    // 遍历每行，对加密字段列调用 decrypt 并替换值
    for row in &mut dataset.rows {
        for &col_idx in &col_indices {
            // 先读取解密结果，再写回，避免借用冲突
            let decrypted = match row.get(col_idx) {
                Some(DataValue::String(s)) => crypto.decrypt(s).ok().map(DataValue::String),
                _ => None,
            };
            if let Some(new_val) = decrypted {
                if let Some(value) = row.get_mut(col_idx) {
                    *value = new_val;
                }
            }
        }
    }
}
