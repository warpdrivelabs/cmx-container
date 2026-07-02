// 导入 time 库中的持续时间和带偏移量的日期时间类型
use time::{Duration, OffsetDateTime};

/// 导出 RFC3339 标准格式定义，用于日期时间格式化
pub use time::format_description::well_known::Rfc3339;

/// 获取当前 UTC 时间
///
/// # 返回值
///
/// 返回当前的 UTC 时间，类型为 OffsetDateTime
pub fn now_utc() -> OffsetDateTime {
    OffsetDateTime::now_utc()
}

/// 将时间格式化为 RFC3339 标准字符串
///
/// # 参数
///
/// * `time` - 要格式化的 OffsetDateTime 对象
///
/// # 返回值
///
/// 返回格式化后的 RFC3339 字符串，例如 "2023-12-01T10:30:00Z"
///
/// # 注意
///
/// 当前使用 unwrap() 方法，存在潜在的 panic 风险
pub fn format_time(time: OffsetDateTime) -> String {
    // fixme: need to check if safe.
    time.format(&Rfc3339).unwrap()
}

/// 计算从当前 UTC 时间加上指定秒数后的时间，并格式化为字符串
///
/// # 参数
///
/// * `sec` - 要增加的秒数（可为小数）
///
/// # 返回值
///
/// 返回加上指定秒数后的格式化时间字符串
pub fn now_utc_plus_sec_str(sec: f64) -> String {
    let new_time = now_utc() + Duration::seconds_f64(sec);
    format_time(new_time)
}

/// 解析 RFC3339 格式的字符串为 OffsetDateTime 对象
///
/// # 参数
///
/// * `moment` - RFC3339 格式的时间字符串
///
/// # 返回值
///
/// 成功时返回解析后的 OffsetDateTime 对象，失败时返回 ParseError 错误
pub fn parse_utc(moment: &str) -> Result<OffsetDateTime> {
    OffsetDateTime::parse(moment, &Rfc3339).map_err(|_| Error::FailToDateParse(moment.to_string()))
}

// region:    --- Error

/// 定义时间处理模块的自定义结果类型
pub type Result<T> = core::result::Result<T, Error>;

/// 时间处理模块的自定义错误枚举
#[derive(Debug)]
pub enum Error {
    /// 日期解析失败错误，包含原始输入字符串
    FailToDateParse(String),
}

// region:    --- Error Boilerplate
impl core::fmt::Display for Error {
    fn fmt(&self, fmt: &mut core::fmt::Formatter) -> core::result::Result<(), core::fmt::Error> {
        write!(fmt, "{self:?}")
    }
}

impl std::error::Error for Error {}
// endregion: --- Error Boilerplate

// endregion: --- Error
