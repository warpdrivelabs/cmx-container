use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use rust_decimal::Decimal;
use chrono::{DateTime, Utc, NaiveDate};
use smol_str::SmolStr;  

/// ```
pub type CellValue = DataValue;//serde_json::Value;


// // use std::borrow::Cow;  
// use rust_decimal::Decimal;
// use serde::{Deserialize, Serialize};
// use smol_str::SmolStr; // 关键优化库  
  
// #[derive(Debug, Serialize, Deserialize,Clone)] 
// pub enum DataValue {  
//     Null,  
//     Bool(bool),  
//     Int64(i64),  
//     Float64(f64),
//     Uint64(u64),
//     Uint32(u32),
//     Uint16(u16),
//     Uint8(u8),
//     Int32(i32),
//     Int16(i16),
//     Int8(i8),
//     Byte(u8),

//     Timestamp(u64),
//     Date(u64),
//     Time(u64),
//     DateTime(u64),
//     Decimal(Decimal),
//     Char(char),
//     // 优化1: 针对短字符串（如状态码、币种），直接存在栈上，不分配堆内存  
//     // SmolStr 可以内联存储 22 字节以内的字符串  
//     ShortStr(SmolStr), 
//     LongStr(SmolStr),
//     String(String),
//     // 优化2: 对于长文本或二进制，使用 Cow 实现零拷贝  
//     // 如果是从网络 Buffer 读出来的，直接引用，不拷贝  
//     Binary(Vec<u8>),  // 二进制数据
//     Json(String), // JSON 字符串
//     // 优化3: 时间戳直接存 int，不要存 String  
// }  
// ==========================================  
// 1. 值类型系统 (Type System)  
// ==========================================  
  
/// ERP 通用数据值枚举  
/// 这种设计允许我们在编译时不知道具体类型的情况下存储数据  
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]  
#[serde(untagged)] // 序列化时去掉枚举标签，直接输出值，前端友好  
pub enum DataValue {  
    Null,  
    Bool(bool),  
    Int(i64),  
    Float(f64), // 注：f64 无 Eq，故 DataValue 仅 PartialEq  
    String(String),  
    Decimal(Decimal),  
    DateTime(DateTime<Utc>),  
    Date(NaiveDate),  
    // 可以在此扩展：Binary(Vec<u8>), Array(Vec<DataValue>)  
    ShortStr(SmolStr), 
    LongStr(SmolStr),
}  
  
// 提供一些便捷转换，方便代码编写  
impl From<i32> for DataValue { fn from(v: i32) -> Self { DataValue::Int(v as i64) } }  
impl From<i64> for DataValue { fn from(v: i64) -> Self { DataValue::Int(v) } }  
impl From<f64> for DataValue { fn from(v: f64) -> Self { DataValue::Float(v) } }  
impl From<String> for DataValue { fn from(v: String) -> Self { DataValue::String(v) } }  
impl From<&str> for DataValue { fn from(v: &str) -> Self { DataValue::String(v.to_string()) } }  
impl From<Decimal> for DataValue { fn from(v: Decimal) -> Self { DataValue::Decimal(v) } }  
impl From<DateTime<Utc>> for DataValue { fn from(v: DateTime<Utc>) -> Self { DataValue::DateTime(v) } }  
impl From<NaiveDate> for DataValue { fn from(v: NaiveDate) -> Self { DataValue::Date(v) } }  
  
// ==========================================  
// 2. 元数据定义 (Metadata / Schema)  
// ==========================================  
  
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]  
pub enum FieldType {  
    String,  
    Int,  
    Float,  
    Decimal,  
    DateTime,  
    Bool,
    Text,
}  
  
#[derive(Debug, Clone, Serialize, Deserialize)]  
pub struct Field {  
    pub name: String,  
    pub field_type: FieldType,  
    #[serde(default)]  
    pub label: String, // 用于前端表头显示  
}  


#[derive(Debug, Clone, Serialize, Deserialize)]  
pub struct ColumnDefine {  
    pub name: String,        // 数据库列名 (例如 "unit_price")  
    pub label: String,       // UI 显示名 (例如 "单价")  
    pub field_type: FieldType, // 类型  
    pub is_primary_key: bool,  // 是否主键  
    pub is_nullable: bool,     // 是否允许为空  
    pub default_value: Option<String>, // 默认值  
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
    pub created_at: Option<DateTime<Utc>>,
    /// 最后修改时间；缺省时表示未设置
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
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
// 表索引定义
// ==========================================

/// 索引类型：唯一索引或普通索引
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum IndexKind {
    /// 唯一索引
    Unique,
    /// 普通索引
    Normal,
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

impl Default for IndexKind {
    fn default() -> Self {
        IndexKind::Normal
    }
}

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

/// 表定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableDefine {  
    pub table_name: String,  // 数据库表名 (例如 "sale_order")  
    pub display_name: String,// 显示名 (例如 "销售订单")  
    pub columns: Vec<ColumnDefine>, // 包含的所有列  
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
    pub created_at: Option<DateTime<Utc>>,
    /// 最后修改时间；缺省时表示未设置
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
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

fn default_table_version() -> u32 {
    1
}  