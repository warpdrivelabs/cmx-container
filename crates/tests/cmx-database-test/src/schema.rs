//! 表结构与数据模型。
//!
//! 50 列宽表，类型分布贴近真实业务表：
//!   - 1  BIGINT 主键（行号，唯一）
//!   - 15 BIGINT / INTEGER 数值列
//!   - 15 TEXT / VARCHAR 文本列
//!   - 8  NUMERIC 金额列
//!   - 5  TIMESTAMPTZ 时间列
//!   - 3  BOOLEAN 标志列
//!   - 2  UUID 列
//!   - 1  JSONB 列
//!
//! 两条驱动路径共用同一 DDL、同一列顺序、同一行数据，确保对比公平。

/// 表名（每次运行用带后缀的独立表，避免并发/残留干扰）。
pub const TABLE: &str = "bench_wide";

/// 除主键外的数据列数量（主键 id 单列 + 49 数据列 = 50 列）。
pub const DATA_COLS: usize = 49;

/// 一行的数据模型（不含主键 id，id 由行号生成）。
///
/// 所有行内容相同（符合“插入相同数据”要求），仅主键 id 递增。
#[derive(Debug, Clone)]
pub struct RowTemplate {
    pub ints: Vec<i64>,          // 15 个整数列
    pub texts: Vec<String>,      // 15 个文本列
    pub nums: Vec<rust_decimal::Decimal>, // 8 个金额列
    pub times: Vec<chrono::DateTime<chrono::Utc>>, // 5 个时间列
    pub flags: Vec<bool>,        // 3 个布尔列
    pub uuids: Vec<uuid::Uuid>,  // 2 个 UUID 列
    pub json: serde_json::Value, // 1 个 JSONB 列
}

impl RowTemplate {
    /// 数据列总数自检（应等于 DATA_COLS）。
    pub fn col_count(&self) -> usize {
        self.ints.len()
            + self.texts.len()
            + self.nums.len()
            + self.times.len()
            + self.flags.len()
            + self.uuids.len()
            + 1 // json
    }
}

/// 列名列表（顺序即 INSERT/COPY 的列顺序），含主键 id。
pub fn column_names() -> Vec<String> {
    let mut cols = vec!["id".to_string()];
    for i in 0..15 {
        cols.push(format!("int_{i}"));
    }
    for i in 0..15 {
        cols.push(format!("txt_{i}"));
    }
    for i in 0..8 {
        cols.push(format!("num_{i}"));
    }
    for i in 0..5 {
        cols.push(format!("ts_{i}"));
    }
    for i in 0..3 {
        cols.push(format!("flag_{i}"));
    }
    for i in 0..2 {
        cols.push(format!("uid_{i}"));
    }
    cols.push("payload".to_string());
    cols
}

/// 生成 CREATE TABLE DDL。
pub fn create_table_ddl(table: &str) -> String {
    let mut cols = vec!["id BIGINT PRIMARY KEY".to_string()];
    for i in 0..15 {
        cols.push(format!("int_{i} BIGINT"));
    }
    for i in 0..15 {
        cols.push(format!("txt_{i} TEXT"));
    }
    for i in 0..8 {
        cols.push(format!("num_{i} NUMERIC(18,4)"));
    }
    for i in 0..5 {
        cols.push(format!("ts_{i} TIMESTAMPTZ"));
    }
    for i in 0..3 {
        cols.push(format!("flag_{i} BOOLEAN"));
    }
    for i in 0..2 {
        cols.push(format!("uid_{i} UUID"));
    }
    cols.push("payload JSONB".to_string());
    format!("CREATE TABLE {table} (\n  {}\n)", cols.join(",\n  "))
}

/// 占位符列表 `$1..$N`（N = 列数，含 id）。
pub fn placeholders(n: usize) -> String {
    (1..=n).map(|i| format!("${i}")).collect::<Vec<_>>().join(",")
}
