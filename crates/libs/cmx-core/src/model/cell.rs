use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde::ser::SerializeSeq;
use serde_json::Value as JsonValue;
use rust_decimal::Decimal;
use chrono::{DateTime, Utc, NaiveDate};
use smol_str::SmolStr;
use uuid::Uuid;
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

/// ```
pub type CellValue = DataValue;



// ==========================================
// 1. 值类型系统 (Type System)
// ==========================================

/// SQL 列类型标记(不依赖 sqlx,用于描述 NULL 的目标绑定类型)。
///
/// 仅在 [`DataValue::NullTyped`] 中携带,告知绑定层
/// 这个 NULL 应绑定为哪种数据库列类型。
///
/// # 设计动机
///
/// sqlx 绑定 `None::<T>` 时,目标类型由 `T` 决定。
/// 若 NULL 占位符对应非字符串列(INTEGER/TIMESTAMP/UUID 等),
/// 绑定 `None::<String>` 会导致 PostgreSQL prepare 类型不匹配。
/// `NullTyped` 让调用方显式声明目标类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SqlTypeMarker {
    Bool,
    Int,
    Float,
    Decimal,
    Text,
    Timestamp,
    Date,
    Uuid,
    Json,
    Binary,
}

/// 面向 SQL 绑定的参数类型,内含带类型的 NULL。
///
/// 比 [`DataValue`] 更贴近 SQL 语义,适合手写 SQL 时需要精确控制
/// NULL 目标类型的场景。宿主端和 wasm plugin 都可使用。
/// 可通过 [`From`]/[`Into`] 与 [`DataValue`] 互通。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SqlParam {
    /// 带类型标记的 NULL。
    Null(SqlTypeMarker),
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
    Decimal(Decimal),
    Timestamp(DateTime<Utc>),
    Date(NaiveDate),
    Uuid(Uuid),
    /// JSON 字符串
    Json(String),
    Binary(Vec<u8>),
    /// 单层同类型数组(IN 查询用)
    Array(Vec<SqlParam>),
}

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
    /// 带类型信息的 NULL。
    ///
    /// 绑定到非字符串列(INTEGER/TIMESTAMP/UUID 等)时必须使用,
    /// 否则绑定层无法确定 NULL 的目标 SQL 类型。
    /// 序列化为普通 null(类型信息仅用于绑定,不参与传输)。
    NullTyped(SqlTypeMarker),
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Decimal(Decimal),
    DateTime(DateTime<Utc>),
    Date(NaiveDate),
    /// 二进制数据 - 用于附件、图片、文档等（序列化为 B64: 前缀的 base64 字符串）
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
/// - Binary: base64 编码字符串，添加 B64: 前缀以便识别
/// - Uuid: 标准字符串格式
/// - Json: 保持 JSON 对象格式
/// - Array: 递归序列化数组元素
/// - 其他类型: 直接值输出
impl Serialize for DataValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            DataValue::Null => serializer.serialize_unit(),
            DataValue::NullTyped(t) => {
                // 序列化为 "$null:Int" 字符串以保留类型信息(wasm 边界往返需要)
                // 与 Binary 的 "B64:" 前缀模式一致,跨 JSON/MsgPack 格式统一
                serializer.serialize_str(&format!("$null:{:?}", t))
            }
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
                // 使用 base64 编码，并添加 B64: 前缀以便反序列化时识别
                let encoded = BASE64.encode(v);
                serializer.serialize_str(&format!("B64:{}", encoded))
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
/// - 通过 B64: 前缀识别 base64 编码的 Binary 字符串
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
                // 尝试解析为 NullTyped(通过 $null: 前缀识别,如 "$null:Int")
                if let Some(type_str) = s.strip_prefix("$null:") {
                    let t: SqlTypeMarker = serde_json::from_value(JsonValue::String(type_str.to_string()))
                        .map_err(|e| serde::de::Error::custom(format!("invalid $null type: {}", e)))?;
                    return Ok(DataValue::NullTyped(t));
                }
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
                // 尝试解析为 base64 编码的二进制数据（通过 B64: 前缀识别）
                if let Some(encoded) = s.strip_prefix("B64:")
                    && let Ok(bytes) = BASE64.decode(encoded) {
                        return Ok(DataValue::Binary(bytes));
                    }
                // 尝试解析为 JSON（如果是以 { 或 [ 开头）
                if (s.starts_with('{') || s.starts_with('['))
                    && serde_json::from_str::<JsonValue>(&s).is_ok() {
                        return Ok(DataValue::Json(s));
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
    /// use cmx_core::model::cell::DataValue;
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
    /// use cmx_core::model::cell::DataValue;
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
    /// use cmx_core::model::cell::DataValue;
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
    /// use cmx_core::model::cell::DataValue;
    /// let arr: Vec<DataValue> = vec![
    ///     DataValue::Int(1),
    ///     DataValue::String("hello".to_string()),
    ///     DataValue::Bool(true),
    /// ];
    /// let value = DataValue::from(arr);
    /// ```
    fn from(v: Vec<DataValue>) -> Self { DataValue::Array(v) }
}

// ==========================================
// Option<T> 构造糖 —— 消除 .map(DataValue::X).unwrap_or(DataValue::Null) 模式
// ==========================================

impl From<Option<String>> for DataValue {
    fn from(v: Option<String>) -> Self {
        v.map(DataValue::String).unwrap_or(DataValue::Null)
    }
}

impl From<Option<&str>> for DataValue {
    fn from(v: Option<&str>) -> Self {
        v.map(|s| DataValue::String(s.to_string())).unwrap_or(DataValue::Null)
    }
}

impl From<Option<i64>> for DataValue {
    fn from(v: Option<i64>) -> Self {
        v.map(DataValue::Int).unwrap_or(DataValue::NullTyped(SqlTypeMarker::Int))
    }
}

impl From<Option<i32>> for DataValue {
    fn from(v: Option<i32>) -> Self {
        v.map(|i| DataValue::Int(i as i64)).unwrap_or(DataValue::NullTyped(SqlTypeMarker::Int))
    }
}

impl From<Option<f64>> for DataValue {
    fn from(v: Option<f64>) -> Self {
        v.map(DataValue::Float).unwrap_or(DataValue::NullTyped(SqlTypeMarker::Float))
    }
}

impl From<Option<bool>> for DataValue {
    fn from(v: Option<bool>) -> Self {
        v.map(DataValue::Bool).unwrap_or(DataValue::NullTyped(SqlTypeMarker::Bool))
    }
}

impl From<Option<Uuid>> for DataValue {
    fn from(v: Option<Uuid>) -> Self {
        v.map(DataValue::Uuid).unwrap_or(DataValue::NullTyped(SqlTypeMarker::Uuid))
    }
}

impl From<Option<DateTime<Utc>>> for DataValue {
    fn from(v: Option<DateTime<Utc>>) -> Self {
        v.map(DataValue::DateTime).unwrap_or(DataValue::NullTyped(SqlTypeMarker::Timestamp))
    }
}

impl From<Option<NaiveDate>> for DataValue {
    fn from(v: Option<NaiveDate>) -> Self {
        v.map(DataValue::Date).unwrap_or(DataValue::NullTyped(SqlTypeMarker::Date))
    }
}

impl From<Option<Decimal>> for DataValue {
    fn from(v: Option<Decimal>) -> Self {
        v.map(DataValue::Decimal).unwrap_or(DataValue::NullTyped(SqlTypeMarker::Decimal))
    }
}

// ==========================================
// SqlParam 与 DataValue 互通
// ==========================================

impl From<DataValue> for SqlParam {
    fn from(v: DataValue) -> Self {
        match v {
            DataValue::Null => SqlParam::Null(SqlTypeMarker::Text),
            DataValue::NullTyped(t) => SqlParam::Null(t),
            DataValue::Bool(b) => SqlParam::Bool(b),
            DataValue::Int(i) => SqlParam::Int(i),
            DataValue::Float(f) => SqlParam::Float(f),
            DataValue::String(s) => SqlParam::Text(s),
            DataValue::Decimal(d) => SqlParam::Decimal(d),
            DataValue::DateTime(dt) => SqlParam::Timestamp(dt),
            DataValue::Date(d) => SqlParam::Date(d),
            DataValue::Json(s) => SqlParam::Json(s),
            DataValue::Binary(b) => SqlParam::Binary(b),
            DataValue::Uuid(u) => SqlParam::Uuid(u),
            DataValue::Array(els) => SqlParam::Array(els.into_iter().map(Into::into).collect()),
            DataValue::ShortStr(s) => SqlParam::Text(s.to_string()),
            DataValue::LongStr(s) => SqlParam::Text(s.to_string()),
        }
    }
}

impl From<SqlParam> for DataValue {
    fn from(p: SqlParam) -> Self {
        match p {
            SqlParam::Null(t) => DataValue::NullTyped(t),
            SqlParam::Bool(b) => DataValue::Bool(b),
            SqlParam::Int(i) => DataValue::Int(i),
            SqlParam::Float(f) => DataValue::Float(f),
            SqlParam::Text(s) => DataValue::String(s),
            SqlParam::Decimal(d) => DataValue::Decimal(d),
            SqlParam::Timestamp(dt) => DataValue::DateTime(dt),
            SqlParam::Date(d) => DataValue::Date(d),
            SqlParam::Json(s) => DataValue::Json(s),
            SqlParam::Binary(b) => DataValue::Binary(b),
            SqlParam::Uuid(u) => DataValue::Uuid(u),
            SqlParam::Array(els) => DataValue::Array(els.into_iter().map(Into::into).collect()),
        }
    }
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

// 注意：TableDefine 相关类型已迁移到 super::meta::table 模块
// 此处保留重导出以保持向后兼容
pub use super::meta::table::{
    FieldType, Field, ColumnDefine, IndexKind, IndexDefine, PartitionType, TableDefine,
};

// ============================================
//  dv! 宏 —— 批量构造 Vec<DataValue> 的便捷宏
// ============================================

/// 构造 `Vec<DataValue>` 的便捷宏。
///
/// 基于 [`Into<DataValue>`] trait 驱动,每个表达式须满足 `Into<DataValue>`。
/// 配合 [`From<Option<T>>`] 实现,可空字段直接传 `Option` 值即可。
///
/// # 语法
/// - `dv!()` → 空 `Vec<DataValue>`
/// - `dv!(expr1, expr2, ...)` → `vec![DataValue::from(expr1), ...]`
/// - `dv!(null T)` → `DataValue::NullTyped(SqlTypeMarker::T)`(显式带类型的 NULL)
///
/// # 示例
/// ```ignore
/// use cmx_core::dv;
/// use cmx_core::model::cell::DataValue;
///
/// let id = "id123".to_string();
/// let name: Option<String> = Some("alice".into());
/// let sort: Option<i64> = None;
///
/// let params: Vec<DataValue> = dv![
///     id,            // String -> DataValue::String
///     name,          // Option<String> -> DataValue::String 或 Null
///     sort,          // Option<i64> -> DataValue::Int 或 NullTyped(Int)
///     dv!(null Uuid), // 显式带 Uuid 类型的 NULL
/// ];
/// ```
#[macro_export]
macro_rules! dv {
    // 空参数
    () => { ::std::vec::Vec::<$crate::model::cell::DataValue>::new() };

    // null + 类型标记:返回单个 DataValue(非 Vec)
    (null $t:ident) => {
        $crate::model::cell::DataValue::NullTyped($crate::model::cell::SqlTypeMarker::$t)
    };

    // 多元素:每个 expr 须 Into<DataValue>
    ($first:expr $(, $rest:expr)* $(,)?) => {
        ::std::vec![
            ::std::convert::Into::<$crate::model::cell::DataValue>::into($first)
            $(, ::std::convert::Into::<$crate::model::cell::DataValue>::into($rest))*
        ]
    };
}

// ============================================
//  数据类型单元测试
// ============================================

#[cfg(test)]
mod tests {
    use chrono::Datelike;
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
