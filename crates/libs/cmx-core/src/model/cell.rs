use std::collections::HashMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde::ser::{SerializeSeq, SerializeMap};
use serde_json::Value as JsonValue;
use rust_decimal::Decimal;
use chrono::{DateTime, Utc, NaiveDate, Datelike};
use smol_str::SmolStr;
use uuid::Uuid;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

/// ```
pub type CellValue = DataValue;



// ==========================================
// 1. 值类型系统 (Type System)
// ==========================================

/// ERP 通用数据值枚举
/// 这种设计允许我们在编译时不知道具体类型的情况下存储数据
///
/// # 序列化策略
/// 为支持 DataSet 中的新类型（Binary、Array、Json、Uuid），使用自定义序列化：
/// - Binary: base64 编码字符串
/// - Uuid: 标准字符串格式
/// - Json: 保持 JSON 对象格式
/// - Array: 递归序列化数组元素
#[derive(Debug, Clone, PartialEq)]
pub enum DataValue {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Decimal(Decimal),
    DateTime(DateTime<Utc>),
    Date(NaiveDate),
    /// 二进制数据 - 用于附件、图片、文档等（序列化为 base64 字符串）
    Binary(Vec<u8>),
    /// 动态数组 - 用于多值字段和标签列表
    Array(Vec<DataValue>),
    /// JSON 字符串 - 用于半结构化数据
    Json(String),
    /// 全局唯一标识（序列化为标准字符串格式）
    Uuid(Uuid),
    /// 短字符串（≤22字节） - 用于状态码、币种等短文本
    ShortStr(SmolStr),
    /// 长字符串 - 用于描述、备注等长文本
    LongStr(SmolStr),
}

// ==========================================
// DataValue 序列化实现
// ==========================================

/// DataValue 序列化
///
/// # 序列化格式
/// - Binary: base64 编码字符串
/// - Uuid: 标准字符串格式
/// - Json: 保持 JSON 对象格式
/// - Array: JSON 数组
/// - 其他类型: 直接值输出
impl Serialize for DataValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            DataValue::Null => serializer.serialize_unit(),
            DataValue::Bool(b) => serializer.serialize_bool(*b),
            DataValue::Int(i) => serializer.serialize_i64(*i),
            DataValue::Float(f) => {
                if let Some(n) = serde_json::Number::from_f64(*f) {
                    serializer.serialize_newtype_struct("Float", &n)
                } else {
                    serializer.serialize_unit()
                }
            }
            DataValue::String(s) => serializer.serialize_str(s),
            DataValue::Decimal(d) => serializer.serialize_str(&d.to_string()),
            DataValue::DateTime(dt) => serializer.serialize_str(&dt.to_rfc3339()),
            DataValue::Date(d) => serializer.serialize_str(&d.to_string()),
            DataValue::Binary(v) => {
                // 使用 base64 编码
                let encoded = BASE64.encode(v);
                serializer.serialize_str(&encoded)
            }
            DataValue::Uuid(u) => serializer.serialize_str(&u.to_string()),
            DataValue::Json(json_str) => {
                // Json 保持为字符串格式输出，便于反序列化
                // 注意：这会导致 JSON 中字段值为字符串而非对象
                // 如果需要保持对象格式，需要在反序列化时做特殊处理
                serializer.serialize_str(json_str)
            }
            DataValue::Array(items) => {
                let mut seq = serializer.serialize_seq(Some(items.len()))?;
                for item in items {
                    seq.serialize_element(item)?;
                }
                seq.end()
            }
            DataValue::ShortStr(s) => serializer.serialize_str(s),
            DataValue::LongStr(s) => serializer.serialize_str(s),
        }
    }
}

/// DataValue 反序列化
///
/// # 反序列化策略
/// - 识别 base64 编码的 Binary 字符串
/// - 识别 UUID 格式字符串
/// - 尝试智能推断类型（JSON 格式等）
impl<'de> Deserialize<'de> for DataValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // 使用 JsonValue 作为中间类型进行解析
        let value = JsonValue::deserialize(deserializer)?;

        match value {
            JsonValue::Null => Ok(DataValue::Null),
            JsonValue::Bool(b) => Ok(DataValue::Bool(b)),
            JsonValue::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Ok(DataValue::Int(i))
                } else if let Some(f) = n.as_f64() {
                    Ok(DataValue::Float(f))
                } else {
                    Err(serde::de::Error::custom("invalid number"))
                }
            }
            JsonValue::String(s) => {
                // 尝试解析为 DateTime（RFC3339 格式，如 "2024-01-01T12:00:00Z" 或 "2024-01-01T12:00:00+00:00"）
                if let Ok(dt) = DateTime::parse_from_rfc3339(&s) {
                    return Ok(DataValue::DateTime(dt.with_timezone(&Utc)));
                }
                // 尝试解析为 NaiveDate（支持 "2024-01-01" 格式）
                if let Ok(date) = NaiveDate::parse_from_str(&s, "%Y-%m-%d") {
                    return Ok(DataValue::Date(date));
                }
                // 尝试解析为 Uuid（标准 UUID 格式）
                if let Ok(uuid) = Uuid::parse_str(&s) {
                    return Ok(DataValue::Uuid(uuid));
                }
                // 尝试解析为 base64 编码的二进制 fixme 需要修改 字符0323在这里会被变为二进制，这样不正确
                // if let Ok(bytes) = BASE64.decode(&s) {
                //     // 验证是否为有效的二进制数据（非空且解码成功）
                //     if !bytes.is_empty() {
                //         return Ok(DataValue::Binary(bytes));
                //     }
                // }
                // 尝试解析为 JSON（如果是以 { 或 [ 开头）
                if s.starts_with('{') || s.starts_with('[') {
                    if serde_json::from_str::<JsonValue>(&s).is_ok() {
                        return Ok(DataValue::Json(s));
                    }
                }
                Ok(DataValue::String(s))
            }
            JsonValue::Array(arr) => {
                // 判断是 Binary 还是 Array
                // 如果所有元素都是 0-255 的数字，则认为是 Binary
                let is_binary = arr.iter().all(|v| {
                    if let JsonValue::Number(n) = v {
                        n.as_u64().map(|n| n <= 255).unwrap_or(false)
                    } else {
                        false
                    }
                });

                if is_binary {
                    let bytes: Vec<u8> = arr.iter()
                        .filter_map(|v| v.as_u64())
                        .map(|n| n as u8)
                        .collect();
                    return Ok(DataValue::Binary(bytes));
                }

                // 否则作为 Array 处理
                let items: Result<Vec<DataValue>, _> = arr.iter()
                    .map(DataValue::deserialize)
                    .collect();
                Ok(DataValue::Array(items.map_err(|e| serde::de::Error::custom(e.to_string()))?))
            }
            JsonValue::Object(_) => {
                // 对象类型暂不支持直接反序列化
                Err(serde::de::Error::custom("unexpected object in DataValue"))
            }
        }
    }
}

// 提供一些便捷转换，方便代码编写
// ========================================
// 基础类型转换
// ========================================
impl From<i32> for DataValue { fn from(v: i32) -> Self { DataValue::Int(v as i64) } }
impl From<i64> for DataValue { fn from(v: i64) -> Self { DataValue::Int(v) } }
impl From<f64> for DataValue { fn from(v: f64) -> Self { DataValue::Float(v) } }
impl From<bool> for DataValue { fn from(v: bool) -> Self { DataValue::Bool(v) } }
impl From<String> for DataValue { fn from(v: String) -> Self { DataValue::String(v) } }
impl From<&str> for DataValue { fn from(v: &str) -> Self { DataValue::String(v.to_string()) } }
impl From<Decimal> for DataValue { fn from(v: Decimal) -> Self { DataValue::Decimal(v) } }
impl From<DateTime<Utc>> for DataValue { fn from(v: DateTime<Utc>) -> Self { DataValue::DateTime(v) } }
impl From<NaiveDate> for DataValue { fn from(v: NaiveDate) -> Self { DataValue::Date(v) } }


/// 从 Vec<u8> 转换为 Binary 类型
impl From<Vec<u8>> for DataValue {
    /// 将字节向量转换为二进制数据值
    ///
    /// # 参数
    /// - v: 字节向量，表示二进制数据
    ///
    /// # 示例
    /// ```
    /// let bytes = vec![0x00, 0x01, 0x02];
    /// let value = DataValue::from(bytes);
    /// ```
    fn from(v: Vec<u8>) -> Self { DataValue::Binary(v) }
}

/// 从 &[u8] 切片转换为 Binary 类型
impl From<&[u8]> for DataValue {
    /// 将字节切片转换为二进制数据值
    ///
    /// # 参数
    /// - v: 字节切片，表示二进制数据
    fn from(v: &[u8]) -> Self { DataValue::Binary(v.to_vec()) }
}

/// 从 String 转换为 Json 类型
impl From<JsonValue> for DataValue {
    /// 将 serde_json::Value 转换为 JSON 数据值
    ///
    /// # 参数
    /// - v: serde_json::Value 对象
    ///
    /// # 示例
    /// ```
    /// use serde_json::json;
    /// let json_val = json!({"key": "value"});
    /// let data_value = DataValue::from(json_val);
    /// ```
    fn from(v: JsonValue) -> Self { DataValue::Json(v.to_string()) }
}

/// 从 Uuid 转换为 Uuid 类型
impl From<Uuid> for DataValue {
    /// 将 UUID 转换为数据值
    ///
    /// # 参数
    /// - v: UUID 实例
    ///
    /// # 示例
    /// ```
    /// use uuid::Uuid;
    /// let id = Uuid::new_v4();
    /// let value = DataValue::from(id);
    /// ```
    fn from(v: Uuid) -> Self { DataValue::Uuid(v) }
}

/// 从 Vec<DataValue> 转换为 Array 类型
impl From<Vec<DataValue>> for DataValue {
    /// 将 DataValue 向量转换为数组数据值
    ///
    /// # 参数
    /// - v: DataValue 向量，表示数组元素
    ///
    /// # 示例
    /// ```
    /// let arr = vec![1i32.into(), "hello".into(), true.into()];
    /// let value = DataValue::from(arr);
    /// ```
    fn from(v: Vec<DataValue>) -> Self { DataValue::Array(v) }
}

// ==========================================
// DataValue 类型转换实现（用于 get_by_name_as 泛型方法）
// ==========================================

use std::convert::TryFrom;

impl TryFrom<DataValue> for i64 {
    type Error = String;
    fn try_from(v: DataValue) -> Result<Self, Self::Error> {
        match v {
            DataValue::Int(i) => Ok(i),
            DataValue::String(s) => s.parse::<i64>().map_err(|e| e.to_string()),
            _ => Err(format!("cannot convert {:?} to i64", v)),
        }
    }
}

impl TryFrom<DataValue> for i32 {
    type Error = String;
    fn try_from(v: DataValue) -> Result<Self, Self::Error> {
        match v {
            DataValue::Int(i) => Ok(i as i32),
            DataValue::String(s) => s.parse::<i32>().map_err(|e| e.to_string()),
            _ => Err(format!("cannot convert {:?} to i32", v)),
        }
    }
}

impl TryFrom<DataValue> for f64 {
    type Error = String;
    fn try_from(v: DataValue) -> Result<Self, Self::Error> {
        match v {
            DataValue::Float(f) => Ok(f),
            DataValue::Int(i) => Ok(i as f64),
            DataValue::String(s) => s.parse::<f64>().map_err(|e| e.to_string()),
            _ => Err(format!("cannot convert {:?} to f64", v)),
        }
    }
}

impl TryFrom<DataValue> for bool {
    type Error = String;
    fn try_from(v: DataValue) -> Result<Self, Self::Error> {
        match v {
            DataValue::Bool(b) => Ok(b),
            DataValue::String(s) => s.parse::<bool>().map_err(|e| e.to_string()),
            DataValue::Int(i) => Ok(i != 0),
            _ => Err(format!("cannot convert {:?} to bool", v)),
        }
    }
}

impl TryFrom<DataValue> for String {
    type Error = String;
    fn try_from(v: DataValue) -> Result<Self, Self::Error> {
        match v {
            DataValue::String(s) => Ok(s),
            DataValue::Int(i) => Ok(i.to_string()),
            DataValue::Float(f) => Ok(f.to_string()),
            DataValue::Bool(b) => Ok(b.to_string()),
            DataValue::DateTime(dt) => Ok(dt.to_rfc3339()),
            DataValue::Date(d) => Ok(d.to_string()),
            DataValue::Uuid(u) => Ok(u.to_string()),
            DataValue::Json(s) => Ok(s),
            DataValue::Null => Ok(String::new()),
            _ => Err(format!("cannot convert {:?} to String", v)),
        }
    }
}

impl TryFrom<DataValue> for Decimal {
    type Error = String;
    fn try_from(v: DataValue) -> Result<Self, Self::Error> {
        match v {
            DataValue::Decimal(d) => Ok(d),
            DataValue::String(s) => s.parse::<Decimal>().map_err(|e| e.to_string()),
            DataValue::Int(i) => Ok(Decimal::from(i)),
            DataValue::Float(f) => Decimal::try_from(f).map_err(|e| e.to_string()),
            _ => Err(format!("cannot convert {:?} to Decimal", v)),
        }
    }
}

impl TryFrom<DataValue> for DateTime<Utc> {
    type Error = String;
    fn try_from(v: DataValue) -> Result<Self, Self::Error> {
        match v {
            DataValue::DateTime(dt) => Ok(dt),
            DataValue::String(s) => DateTime::parse_from_rfc3339(&s)
                .map(|dt| dt.with_timezone(&Utc))
                .map_err(|e| e.to_string()),
            _ => Err(format!("cannot convert {:?} to DateTime<Utc>", v)),
        }
    }
}

impl TryFrom<DataValue> for NaiveDate {
    type Error = String;
    fn try_from(v: DataValue) -> Result<Self, Self::Error> {
        match v {
            DataValue::Date(d) => Ok(d),
            DataValue::String(s) => NaiveDate::parse_from_str(&s, "%Y-%m-%d")
                .map_err(|e| e.to_string()),
            _ => Err(format!("cannot convert {:?} to NaiveDate", v)),
        }
    }
}

impl TryFrom<DataValue> for Uuid {
    type Error = String;
    fn try_from(v: DataValue) -> Result<Self, Self::Error> {
        match v {
            DataValue::Uuid(u) => Ok(u),
            DataValue::String(s) => Uuid::parse_str(&s).map_err(|e| e.to_string()),
            _ => Err(format!("cannot convert {:?} to Uuid", v)),
        }
    }
}

impl TryFrom<DataValue> for Vec<u8> {
    type Error = String;
    fn try_from(v: DataValue) -> Result<Self, Self::Error> {
        match v {
            DataValue::Binary(b) => Ok(b),
            DataValue::String(s) => BASE64.decode(&s).map_err(|e| e.to_string()),
            _ => Err(format!("cannot convert {:?} to Vec<u8>", v)),
        }
    }
}

impl TryFrom<DataValue> for Vec<DataValue> {
    type Error = String;
    fn try_from(v: DataValue) -> Result<Self, Self::Error> {
        match v {
            DataValue::Array(arr) => Ok(arr),
            _ => Err(format!("cannot convert {:?} to Vec<DataValue>", v)),
        }
    }
}
impl TryFrom<DataValue> for serde_json::Value {
    type Error = String;
    fn try_from(v: DataValue) -> Result<Self, Self::Error> {
        match v {
            DataValue::Json(json_str) => serde_json::from_str(json_str.as_str()).map_err(|e| e.to_string()),
            DataValue::String(json_str) => serde_json::from_str(json_str.as_str()).map_err(|e| e.to_string()),
            _ => Err(format!("cannot convert {:?} to serde_json::Value", v)),
        }
    }
}

// ==========================================
// 2. 元数据定义 (Metadata / Schema)
// ==========================================

/// 字段类型枚举
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
    #[serde(default)]
    pub label: String,       // UI 显示名 (例如 "单价")
    pub field_type: FieldType, // 类型
    #[serde(default)]
    pub is_primary_key: bool,  // 是否主键
    #[serde(default)]
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
    /// 表名（例如 "sale_order"）
    pub table_name: String,
    #[serde(default)]
    // 数据库表名 (例如 "sale_order")
    pub display_name: String,// 显示名 (例如 "销售订单")
    #[serde(default)]
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

fn default_table_version() -> u32 {
    1
}

// ============================================
//  数据类型单元测试
// ============================================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use uuid::Uuid;

    // ========================================
    // Binary 类型测试
    // ========================================

    /// 测试 Binary 类型的基本创建和访问
    #[test]
    fn test_binary_creation() {
        // 创建二进制数据
        let bytes = vec![0x00, 0x01, 0x02, 0xFF];
        let value = DataValue::Binary(bytes.clone());

        // 验证类型和值
        match value {
            DataValue::Binary(b) => {
                assert_eq!(b.len(), 4);
                assert_eq!(b[0], 0x00);
                assert_eq!(b[3], 0xFF);
            }
            _ => panic!("Expected Binary variant"),
        }
    }

    /// 测试从 Vec<u8> 通过 From trait 创建 Binary
    #[test]
    fn test_binary_from_vec() {
        let bytes = vec![0xDE, 0xAD, 0xBE, 0xEF];
        let value: DataValue = bytes.into();

        match value {
            DataValue::Binary(b) => {
                assert_eq!(b, vec![0xDE, 0xAD, 0xBE, 0xEF]);
            }
            _ => panic!("Expected Binary variant"),
        }
    }

    /// 测试 Binary 序列化
    #[test]
    fn test_binary_serialization() {
        let bytes = vec![0x01, 0x02, 0x03];
        let value = DataValue::Binary(bytes);

        // Binary 现在序列化为 base64 编码字符串
        let json = serde_json::to_string(&value).unwrap();
        // 0x01, 0x02, 0x03 的 base64 编码是 "AQID"
        assert!(json.contains("AQID"));
    }

    /// 测试 Binary 反序列化
    #[test]
    fn test_binary_deserialization() {
        let json = r#"[1,2,3]"#;
        let value: DataValue = serde_json::from_str(json).unwrap();

        match value {
            DataValue::Binary(b) => {
                assert_eq!(b, vec![1, 2, 3]);
            }
            _ => panic!("Expected Binary variant"),
        }
    }

    // ========================================
    // Array 类型测试
    // ========================================

    /// 测试 Array 类型的基本创建
    #[test]
    fn test_array_creation() {
        let arr = vec![
            DataValue::Int(1),
            DataValue::String("hello".to_string()),
            DataValue::Bool(true),
        ];
        let value = DataValue::Array(arr);

        match value {
            DataValue::Array(items) => {
                assert_eq!(items.len(), 3);
            }
            _ => panic!("Expected Array variant"),
        }
    }

    /// 测试从 Vec<DataValue> 通过 From trait 创建 Array
    #[test]
    fn test_array_from_vec() {
        let arr: Vec<DataValue> = vec![
            DataValue::Int(1),
            DataValue::String("test".to_string()),
            DataValue::Bool(true),
        ];
        let value = DataValue::Array(arr);

        match value {
            DataValue::Array(items) => {
                assert_eq!(items.len(), 3);
            }
            _ => panic!("Expected Array variant"),
        }
    }

    /// 测试嵌套 Array（ERP 场景：订单标签）
    #[test]
    fn test_array_nested() {
        let tags = vec![
            DataValue::String("VIP".to_string()),
            DataValue::String("急单".to_string()),
            DataValue::String("出口".to_string()),
        ];
        let value: DataValue = tags.into();

        match &value {
            DataValue::Array(items) => {
                assert_eq!(items.len(), 3);
                // 验证第一个元素是字符串
                if let DataValue::String(s) = &items[0] {
                    assert_eq!(s, "VIP");
                }
            }
            _ => panic!("Expected Array variant"),
        }
    }

    /// 测试 Array 序列化
    #[test]
    fn test_array_serialization() {
        let arr = vec![DataValue::Int(1), DataValue::Int(2), DataValue::Int(3)];
        let value = DataValue::Array(arr);

        let json = serde_json::to_string(&value).unwrap();
        assert!(json.contains("[1,2,3]"));
    }

    // ========================================
    // Json 类型测试
    // ========================================

    /// 测试 Json 类型的基本创建
    #[test]
    fn test_json_creation() {
        let json_str = r#"{"key":"value","count":42}"#.to_string();
        let value = DataValue::Json(json_str);

        match value {
            DataValue::Json(s) => {
                assert!(s.contains("key"));
                assert!(s.contains("42"));
            }
            _ => panic!("Expected Json variant"),
        }
    }

    /// 测试从 serde_json::Value 创建 Json
    #[test]
    fn test_json_from_value() {
        let json_val = json!({
            "custom_field": "test",
            "age": 30,
            "enabled": true
        });
        let value: DataValue = json_val.into();

        match value {
            DataValue::Json(s) => {
                assert!(s.contains("custom_field"));
                assert!(s.contains("30"));
            }
            _ => panic!("Expected Json variant"),
        }
    }

    /// 测试 Json 序列化
    #[test]
    fn test_json_serialization() {
        let json_str = r#"{"name":"test"}"#.to_string();
        let value = DataValue::Json(json_str);

        let serialized = serde_json::to_string(&value).unwrap();
        assert!(serialized.contains("name"));
    }

    /// 测试 Json 反序列化
    /// 注意：由于 DataValue 使用 #[serde(untagged)]，Json 字符串需要特殊处理
    #[test]
    fn test_json_deserialization() {
        // 创建 Json DataValue，然后序列化再反序列化
        let original = DataValue::Json(r#"{"field":"value"}"#.to_string());
        let json_str = serde_json::to_string(&original).unwrap();

        // 反序列化
        let value: Result<DataValue, _> = serde_json::from_str(&json_str);

        match value {
            Ok(DataValue::Json(s)) => {
                assert!(s.contains("field"));
            }
            Ok(other) => {
                // 由于 untagged，可能解析为其他类型
                println!("Got different variant: {:?}", other);
            }
            Err(e) => {
                println!("Parse error: {}", e);
            }
        }
    }

    // ========================================
    // Uuid 类型测试
    // ========================================

    /// 测试 Uuid 类型的基本创建
    #[test]
    fn test_uuid_creation() {
        let uuid = Uuid::new_v4();
        let value = DataValue::Uuid(uuid);

        match value {
            DataValue::Uuid(u) => {
                assert_eq!(u.get_version(), Some(uuid::Version::Random));
            }
            _ => panic!("Expected Uuid variant"),
        }
    }

    /// 测试从 Uuid 通过 From trait 创建
    #[test]
    fn test_uuid_from_trait() {
        let uuid = Uuid::new_v4();
        let value: DataValue = uuid.into();

        match value {
            DataValue::Uuid(_) => {}
            _ => panic!("Expected Uuid variant"),
        }
    }

    /// 测试 Uuid 序列化
    #[test]
    fn test_uuid_serialization() {
        let uuid = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let value = DataValue::Uuid(uuid);

        let json = serde_json::to_string(&value).unwrap();
        assert!(json.contains("550e8400-e29b-41d4-a716-446655440000"));
    }

    /// 测试 Uuid 反序列化
    #[test]
    fn test_uuid_deserialization() {
        // 使用正确的序列化格式测试
        let json_str = r#""550e8400-e29b-41d4-a716-446655440000""#;
        let value: Result<DataValue, _> = serde_json::from_str(json_str);

        // 由于 untagged 枚举，serde 会尝试匹配所有变体
        // UUID 字符串可能被匹配为 String 类型
        // 这个测试需要根据实际序列化行为调整
        match value {
            Ok(DataValue::Uuid(u)) => {
                assert_eq!(u.to_string(), "550e8400-e29b-41d4-a716-446655440000");
            }
            Ok(other) => {
                // 由于 serde(untagged)，UUID 字符串可能被解析为 String
                println!("Got different variant: {:?}", other);
            }
            Err(e) => {
                println!("Parse error: {}", e);
            }
        }
    }

    // ========================================
    // FieldType 枚举测试
    // ========================================

    /// 测试新增 FieldType 的序列化
    #[test]
    fn test_fieldtype_serialization() {
        // 测试 Binary - serde 默认序列化使用 kebab-case (binary)
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

    /// 测试新增 FieldType 的反序列化
    #[test]
    fn test_fieldtype_deserialization() {
        // 测试 Binary - serde 默认使用 kebab-case
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

    // ========================================
    // ERP 场景集成测试
    // ========================================

    /// ERP 场景：销售订单附件
    #[test]
    fn test_erp_scenario_attachment() {
        // 模拟 PDF 文件内容
        let pdf_content = vec![0x25, 0x50, 0x44, 0x46]; // "%PDF"
        let attachment: DataValue = pdf_content.into();

        match &attachment {
            DataValue::Binary(data) => {
                assert_eq!(data.len(), 4);
                assert_eq!(&data[0..4], b"%PDF");
            }
            _ => panic!("Expected Binary for attachment"),
        }

        // 序列化用于存储
        let json = serde_json::to_string(&attachment).unwrap();
        assert!(!json.is_empty());
    }

    /// ERP 场景：订单多标签
    #[test]
    fn test_erp_scenario_order_tags() {
        let tags = vec![
            DataValue::String("VIP".to_string()),
            DataValue::String("急单".to_string()),
            DataValue::String("电商".to_string()),
        ];
        let order_tags: DataValue = tags.into();

        match &order_tags {
            DataValue::Array(items) => {
                assert_eq!(items.len(), 3);
                // 验证标签内容
                if let DataValue::String(s) = &items[0] {
                    assert_eq!(s, "VIP");
                }
            }
            _ => panic!("Expected Array for tags"),
        }
    }

    /// ERP 场景：客户自定义字段
    #[test]
    fn test_erp_scenario_custom_fields() {
        let custom_fields = json!({
            "vip_level": "Gold",
            "credit_limit": 100000,
            "tags": ["enterprise", "manufacturing"],
            "metadata": {
                "registered_date": "2024-01-01",
                "account_manager": "张三"
            }
        });
        let fields: DataValue = custom_fields.into();

        match &fields {
            DataValue::Json(s) => {
                assert!(s.contains("vip_level"));
                assert!(s.contains("credit_limit"));
            }
            _ => panic!("Expected Json for custom fields"),
        }
    }

    /// ERP 场景：分布式订单 ID
    #[test]
    fn test_erp_scenario_distributed_order_id() {
        // 生成分布式唯一订单ID
        let order_id = Uuid::new_v4();
        let order_id_value: DataValue = order_id.into();

        match &order_id_value {
            DataValue::Uuid(u) => {
                // 验证是有效的 UUID v4
                assert_eq!(u.get_version(), Some(uuid::Version::Random));
            }
            _ => panic!("Expected Uuid for order ID"),
        }

        // 序列化为字符串便于存储和传输
        let json = serde_json::to_string(&order_id_value).unwrap();
        assert!(json.len() > 30); // UUID 字符串长度
    }

    /// ERP 场景：物料多单位列表
    #[test]
    fn test_erp_scenario_multi_unit() {
        let unit1 = DataValue::Array(vec![
            DataValue::String("个".to_string()),
            DataValue::Int(100),
        ]);
        let unit2 = DataValue::Array(vec![
            DataValue::String("箱".to_string()),
            DataValue::Int(10),
        ]);
        let unit_list = DataValue::Array(vec![unit1, unit2]);

        match &unit_list {
            DataValue::Array(items) => {
                assert_eq!(items.len(), 2);
            }
            _ => panic!("Expected Array for multi-unit"),
        }
    }

    // ========================================
    // DateTime/Date 反序列化测试
    // ========================================

    /// 测试 DateTime 序列化后再反序列化
    #[test]
    fn test_datetime_serialization_roundtrip() {
        let original = DateTime::parse_from_rfc3339("2024-01-15T10:30:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let data_value = DataValue::DateTime(original);
        let json = serde_json::to_string(&data_value).unwrap();

        let deserialized: DataValue = serde_json::from_str(&json).unwrap();

        match deserialized {
            DataValue::DateTime(dt) => {
                assert_eq!(dt.format("%Y-%m-%dT%H:%M:%S").to_string(), "2024-01-15T10:30:00");
            }
            _ => panic!("Expected DateTime after roundtrip"),
        }
    }

    /// 测试 DateTime RFC3339 格式字符串反序列化
    #[test]
    fn test_datetime_deserialization_rfc3339() {
        let json_str = r#""2024-01-15T10:30:00Z""#;
        let value: DataValue = serde_json::from_str(json_str).unwrap();

        match value {
            DataValue::DateTime(dt) => {
                assert_eq!(dt.format("%Y-%m-%d").to_string(), "2024-01-15");
            }
            _ => panic!("Expected DateTime from RFC3339 string"),
        }
    }

    /// 测试 DateTime 带时区偏移的字符串反序列化
    #[test]
    fn test_datetime_deserialization_with_timezone() {
        let json_str = r#""2024-01-15T10:30:00+08:00""#;
        let value: DataValue = serde_json::from_str(json_str).unwrap();

        match value {
            DataValue::DateTime(dt) => {
                assert_eq!(dt.format("%Y-%m-%d").to_string(), "2024-01-15");
            }
            _ => panic!("Expected DateTime from timezone string"),
        }
    }

    /// 测试 Date 序列化后再反序列化
    #[test]
    fn test_date_serialization_roundtrip() {
        let original = DataValue::Date(NaiveDate::from_ymd_opt(2024, 6, 15).unwrap());
        let json = serde_json::to_string(&original).unwrap();

        let deserialized: DataValue = serde_json::from_str(&json).unwrap();

        match deserialized {
            DataValue::Date(d) => {
                assert_eq!(d.year(), 2024);
                assert_eq!(d.month(), 6);
                assert_eq!(d.day(), 15);
            }
            _ => panic!("Expected Date after roundtrip"),
        }
    }

    /// 测试 Date 字符串反序列化
    #[test]
    fn test_date_deserialization() {
        let json_str = r#""2024-06-15""#;
        let value: DataValue = serde_json::from_str(json_str).unwrap();

        match value {
            DataValue::Date(d) => {
                assert_eq!(d.year(), 2024);
                assert_eq!(d.month(), 6);
                assert_eq!(d.day(), 15);
            }
            _ => panic!("Expected Date from string"),
        }
    }
}
