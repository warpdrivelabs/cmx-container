//! 表元数据结构定义模块
//!
//! 本模块定义了数据库表的元数据结构，包括：
//! - [`FieldType`] - 字段数据类型枚举
//! - [`ColumnDefine`] - 列定义结构
//! - [`IndexKind`] - 索引类型枚举
//! - [`IndexDefine`] - 索引定义结构
//! - [`PartitionType`] - 分区类型枚举
//! - [`TableDefine`] - 表定义结构
//!
//! 这些类型用于描述数据库表的静态结构定义，与运行时数据值 [`DataValue`](super::cell::DataValue) 是不同的概念。

use std::collections::HashMap;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

// ==========================================
// 字段类型系统
// ==========================================

/// 字段类型枚举
///
/// 用于定义表列的数据类型，对应数据库中的列类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FieldType {
    /// 字符串类型 - 用于短文本，如名称、编码
    String,
    /// 整数类型 - 用于数量、ID等
    Int,
    /// 浮点数类型 - 用于测量值（金额推荐用Decimal）
    Float,
    /// 高精度十进制 - 用于金额、汇率等精确计算
    Decimal,
    /// 日期时间类型 - 用于带时间的日期
    DateTime,
    /// 日期类型 - 用于仅日期（不含时间）
    Date,
    /// 布尔类型 - 用于开关、状态标志
    Bool,
    /// 长文本类型 - 用于描述、备注等
    Text,
    /// 二进制类型 - 用于附件、图片、文档等
    /// 对应数据库 BLOB/BYTEA 类型
    Binary,
    /// 数组类型 - 用于多值字段、标签列表
    /// 对应数据库 JSONB 数组或原生数组类型
    Array,
    /// JSON 类型 - 用于半结构化数据
    /// 对应数据库 JSON/JSONB 类型
    Json,
    /// UUID 类型 - 用于全局唯一标识
    /// 对应数据库 UUID 类型
    Uuid,
    /// 未知类型 - 用于null值字段反序列化时不清楚类型的情况
    Unknown,
}

/// 简单字段定义（结构体形式）
///
/// 与 [`FieldType`] 的区别：
/// - `FieldType` 是枚举，仅表示类型
/// `Field` 是结构体，包含名称和类型，用于需要字段元数据的场景
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Field {
    /// 字段名称
    pub name: String,
    /// 字段类型
    pub field_type: FieldType,
    /// 用于前端表头显示的标签
    #[serde(default)]
    pub label: String,
}

// ==========================================
// 列定义
// ==========================================

/// 列定义结构
///
/// 描述数据库表中的一个列的完整元信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnDefine {
    /// 数据库列名 (例如 "unit_price")
    pub name: String,
    /// UI 显示名 (例如 "单价")
    #[serde(default)]
    pub label: String,
    /// 字段数据类型
    pub field_type: FieldType,
    /// 是否为主键列
    #[serde(default)]
    pub is_primary_key: bool,
    /// 是否允许为空
    #[serde(default)]
    pub is_nullable: bool,
    /// 列默认值（SQL 表达式字符串形式）
    pub default_value: Option<String>,
    /// 是否支持多语言（该列参与多语言表翻译）；缺省为 false
    #[serde(default)]
    pub i18n: bool,
    /// 字符串/字符类型最大长度，如 VARCHAR2(30) -> 30；缺省时表示未设置
    #[serde(default)]
    pub length: Option<u32>,
    /// 数值类型总精度（总位数），如 NUMBER(10,2) -> 10；缺省时表示未设置
    #[serde(default)]
    pub precision: Option<u32>,
    /// 数值类型小数位数，如 NUMBER(10,2) -> 2；缺省时表示未设置
    #[serde(default)]
    pub scale: Option<u32>,
    /// 原始数据库类型字符串，便于 DDL 往返，如 "VARCHAR2(30)"、"TIMESTAMP"、"BLOB"
    #[serde(default)]
    pub db_type: Option<String>,
    /// 列在表中的顺序（从 1 开始），缺省时由 columns 数组顺序决定
    #[serde(default)]
    pub ordinal: Option<u32>,
    /// 最初创建时间；缺省时表示未设置
    #[serde(default)]
    pub create_time: Option<DateTime<Utc>>,
    /// 最后修改时间；缺省时表示未设置
    #[serde(default)]
    pub update_time: Option<DateTime<Utc>>,
    /// 是否引用外键表；缺省为 false
    #[serde(default)]
    pub is_foreign_key: bool,
    /// 外键表名（仅当 is_foreign_key 为 true 时有效）；缺省时为空
    #[serde(default)]
    pub foreign_key_table: Option<String>,
    /// 外键表的列名（仅当 is_foreign_key 为 true 时有效）；缺省时为空
    #[serde(default)]
    pub foreign_key_column: Option<String>,
    /// 扩展属性（key-value），用于存放业务自定义元数据；缺省时为空
    #[serde(default)]
    pub extensions: HashMap<String, JsonValue>,
}

// ==========================================
// 索引定义
// ==========================================

/// 索引类型枚举
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum IndexKind {
    /// 唯一索引
    Unique,
    /// 普通索引
    Normal,
}

impl Default for IndexKind {
    fn default() -> Self {
        IndexKind::Normal
    }
}

/// 单条表索引定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexDefine {
    /// 索引名（如 "idx_order_date"、"uk_doc_no"）
    pub name: String,
    /// 参与索引的列名列表，顺序影响复合索引的匹配
    pub columns: Vec<String>,
    /// 索引类型：唯一索引或普通索引
    #[serde(default)]
    pub kind: IndexKind,
}

// ==========================================
// 分区定义
// ==========================================

/// 表分区类型（如 Oracle/MySQL 的 RANGE、LIST、HASH 等）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PartitionType {
    /// 范围分区
    Range,
    /// 列表分区
    List,
    /// 哈希分区
    Hash,
    /// 间隔分区（如 Oracle INTERVAL）
    Interval,
}

// ==========================================
// 表定义
// ==========================================

/// 表定义结构
///
/// 描述数据库表的完整元信息，包括列、索引、分区等
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableDefine {
    /// 表名（例如 "sale_order"）
    pub table_name: String,
    /// 显示名称（例如 "销售订单"）
    #[serde(default)]
    pub display_name: String,
    /// 表包含的所有列定义
    #[serde(default)]
    pub columns: Vec<ColumnDefine>,
    /// 主键列名列表，顺序影响复合主键；缺省时为空（可与列上的 is_primary_key 并存）
    #[serde(default)]
    pub primary_keys: Vec<String>,
    /// 表上的索引（唯一索引与普通索引）；缺省时为空
    #[serde(default)]
    pub indexes: Vec<IndexDefine>,
    /// 表定义版本，用于变更追踪；缺省为 1
    #[serde(default = "default_table_version")]
    pub version: u32,
    /// 最初创建时间；缺省时表示未设置
    #[serde(default)]
    pub create_time: Option<DateTime<Utc>>,
    /// 最后修改时间；缺省时表示未设置
    #[serde(default)]
    pub update_time: Option<DateTime<Utc>>,
    /// 是否支持多语言（表级开关，为 true 时可生成/创建多语言伴生表）；缺省为 false
    #[serde(default)]
    pub i18n: bool,
    /// 表级注释（对应 COMMENT ON TABLE）；缺省时表示未设置
    #[serde(default)]
    pub comment: Option<String>,
    /// 所属 schema/owner，如 Oracle 的 schema 名；缺省时表示默认 schema
    #[serde(default)]
    pub schema: Option<String>,
    /// 表空间（如 Oracle TABLESPACE）；缺省时表示默认表空间
    #[serde(default)]
    pub tablespace: Option<String>,
    /// 是否分区表；缺省为 false（不分区）
    #[serde(default)]
    pub is_partitioned: bool,
    /// 分区类型（仅当 is_partitioned 为 true 时有效）；缺省时表示未设置
    #[serde(default)]
    pub partition_type: Option<PartitionType>,
    /// 分区列组合（分区键列名列表，顺序影响复合分区）；缺省时为空
    #[serde(default)]
    pub partition_columns: Vec<String>,
    /// 扩展属性（key-value），用于存放业务自定义元数据；缺省时为空
    #[serde(default)]
    pub extensions: HashMap<String, JsonValue>,
}

/// 表定义版本默认值
fn default_table_version() -> u32 {
    1
}

// ============================================
// 单元测试
// ============================================

#[cfg(test)]
mod tests {
    use super::*;

    /// 测试 FieldType 枚举的序列化
    ///
    /// 验证 Binary、Array、Json、Uuid 等新增类型的序列化格式
    #[test]
    fn test_fieldtype_serialization() {
        // 测试 Binary - serde 默认序列化使用首字母大写 (Binary)
        let ft_binary = FieldType::Binary;
        let json = serde_json::to_string(&ft_binary).unwrap();
        assert!(json.contains("Binary"));

        // 测试 Array
        let ft_array = FieldType::Array;
        let json = serde_json::to_string(&ft_array).unwrap();
        assert!(json.contains("Array"));

        // 测试 Json
        let ft_json = FieldType::Json;
        let json = serde_json::to_string(&ft_json).unwrap();
        assert!(json.contains("Json"));

        // 测试 Uuid
        let ft_uuid = FieldType::Uuid;
        let json = serde_json::to_string(&ft_uuid).unwrap();
        assert!(json.contains("Uuid"));
    }

    /// 测试 FieldType 枚举的反序列化
    ///
    /// 验证 Binary、Array、Json、Uuid 等新增类型的反序列化
    #[test]
    fn test_fieldtype_deserialization() {
        // 测试 Binary - serde 默认使用首字母大写
        let ft: FieldType = serde_json::from_str("\"Binary\"").unwrap();
        assert_eq!(ft, FieldType::Binary);

        // 测试 Array
        let ft: FieldType = serde_json::from_str("\"Array\"").unwrap();
        assert_eq!(ft, FieldType::Array);

        // 测试 Json
        let ft: FieldType = serde_json::from_str("\"Json\"").unwrap();
        assert_eq!(ft, FieldType::Json);

        // 测试 Uuid - serde 默认使用首字母大写
        let ft: FieldType = serde_json::from_str("\"Uuid\"").unwrap();
        assert_eq!(ft, FieldType::Uuid);
    }
}
