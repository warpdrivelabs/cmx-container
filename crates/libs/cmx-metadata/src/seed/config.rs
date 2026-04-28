//! 种子数据配置相关结构体

pub use cmx_core::model::meta::plugin::SeedDataConfig;

/// 单表种子数据执行结果
#[derive(Debug, Clone)]
pub struct SeedDataTableResult {
    /// 目标表名
    pub table_name: String,
    /// 数据文件路径
    pub file_path: String,
    /// 文件中的数据条数
    pub file_row_count: usize,
    /// 成功执行的行数
    pub success_count: usize,
    /// 失败的行数
    pub failed_count: usize,
    /// 失败详情列表
    pub failures: Vec<SeedDataFailure>,
    /// 数据库中的实际行数（执行后查询）
    pub db_row_count: Option<usize>,
}

impl SeedDataTableResult {
    /// 创建一个空的成功结果
    pub fn new(table_name: String, file_path: String) -> Self {
        Self {
            table_name,
            file_path,
            file_row_count: 0,
            success_count: 0,
            failed_count: 0,
            failures: Vec::new(),
            db_row_count: None,
        }
    }

    /// 加载文件失败时创建结果
    pub fn new_load_failure(
        table_name: String,
        file_path: String,
        error: &str,
    ) -> Self {
        Self {
            table_name,
            file_path,
            file_row_count: 0,
            success_count: 0,
            failed_count: 0,
            failures: vec![SeedDataFailure {
                row_index: 0,
                row_data: serde_json::Value::Null,
                error_message: format!("加载数据文件失败: {}", error),
            }],
            db_row_count: None,
        }
    }

    /// 获取失败率
    pub fn failure_rate(&self) -> f64 {
        let total = self.success_count + self.failed_count;
        if total == 0 {
            0.0
        } else {
            self.failed_count as f64 / total as f64
        }
    }
}

/// 单条种子数据执行失败记录
#[derive(Debug, Clone)]
pub struct SeedDataFailure {
    /// 行号（从 1 开始，CSV 行号或 JSON 数组索引）
    pub row_index: usize,
    /// 行数据（JSON Value 格式）
    pub row_data: serde_json::Value,
    /// 错误信息
    pub error_message: String,
}

/// 全部种子数据执行汇总结果
#[derive(Debug, Clone)]
pub struct SeedDataSummary {
    /// 各表执行结果
    pub table_results: Vec<SeedDataTableResult>,
    /// 总耗时（毫秒）
    pub total_duration_ms: u64,
}

impl SeedDataSummary {
    /// 是否有错误
    pub fn has_errors(&self) -> bool {
        self.table_results.iter().any(|r| r.failed_count > 0)
    }

    /// 是否有数据条数不一致的警告
    pub fn has_warnings(&self) -> bool {
        self.table_results.iter().any(|r| {
            r.db_row_count.is_some_and(|db_count| db_count < r.file_row_count)
        })
    }

    /// 获取总成功行数
    pub fn total_success(&self) -> usize {
        self.table_results.iter().map(|r| r.success_count).sum()
    }

    /// 获取总失败行数
    pub fn total_failed(&self) -> usize {
        self.table_results.iter().map(|r| r.failed_count).sum()
    }

    /// 获取总文件行数
    pub fn total_file_rows(&self) -> usize {
        self.table_results.iter().map(|r| r.file_row_count).sum()
    }
}
