//! 统一错误码 + 错误信息库（落库校验 / 约束翻译专用）
//!
//! 目标：把「数据落库前列级校验失败」与「PG 原始约束错误」翻译成**稳定错误码 +
//! 优雅中文提示 + 结构化诊断**，替代当前直接把 PostgreSQL 英文错误串塞给前端的做法。
//!
//! 三层信息（仿 `cmx-plugin::PluginError::error_code()` 范式）：
//!   - [`CmxErrCode::code_str`]：稳定 SCREAMING_SNAKE 字符串码（前端判定 / 审计 / i18n key）。
//!   - [`CmxErrCode::http_code`]：映射到 `cmx_api_types::ErrCode` 的 HTTP 类别码（422/409/500…）。
//!   - [`CmxErrCode::message_template`]：中文消息模板（`{key}` 占位，由 [`render`] 渲染）。
//!
//! 校验失败以 [`Violation`] 结构化承载（行号 / 表 / 列 / 码 / 中文消息），一次回报全部错误。

use serde::Serialize;

/// 落库校验 / 约束类错误码（细粒度业务码，区别于 `cmx_api_types::ErrCode` 的 HTTP 粗分类）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmxErrCode {
    // ── 前置列级校验（落库前，来自定义规范）──
    /// 值的类型与列定义不匹配（如给整数列传了非数字文本）。
    TypeMismatch,
    /// 字符串超过列的最大长度（VARCHAR(n)）。
    ValueTooLong,
    /// 非空列缺值 / 传了 null（且非服务端 backfill 列）。
    NotNullViolation,
    /// 整数超出该整数类型范围（TINYINT/INT/BIGINT）。
    NumericOutOfRange,
    /// DECIMAL 整数位或小数位超过 precision/scale。
    DecimalScaleExceeded,
    /// 日期 / 时间字符串无法解析为目标类型。
    InvalidDate,
    /// 数据里出现了目标表不存在的列（防拼写错 / 脏字段）。
    UnknownColumn,

    // ── 落库时 DB 约束（PG 原始错误翻译，兜底）──
    /// 唯一键 / 主键冲突。
    UniqueViolation,
    /// 外键不存在（引用了不存在的父行）。
    ForeignKeyViolation,
    /// CHECK 约束不满足。
    CheckViolation,
    /// DB 层非空约束（前置校验漏过时的兜底）。
    NotNullDbViolation,

    // ── 并发 ──
    /// 乐观锁冲突（数据已被他人修改）。对齐既有 HTTP 409。
    OptimisticLockConflict,

    // ── 兜底 ──
    /// 未能归类的数据库错误（翻译失败时的保底，仍不暴露原始英文串）。
    DbError,
    /// 内部错误。
    Internal,
}

impl CmxErrCode {
    /// 稳定字符串码（SCREAMING_SNAKE）。前端按此判定，永不随文案变化。
    pub fn code_str(&self) -> &'static str {
        match self {
            Self::TypeMismatch => "TYPE_MISMATCH",
            Self::ValueTooLong => "VALUE_TOO_LONG",
            Self::NotNullViolation => "NOT_NULL_VIOLATION",
            Self::NumericOutOfRange => "NUMERIC_OUT_OF_RANGE",
            Self::DecimalScaleExceeded => "DECIMAL_SCALE_EXCEEDED",
            Self::InvalidDate => "INVALID_DATE",
            Self::UnknownColumn => "UNKNOWN_COLUMN",
            Self::UniqueViolation => "UNIQUE_VIOLATION",
            Self::ForeignKeyViolation => "FOREIGN_KEY_VIOLATION",
            Self::CheckViolation => "CHECK_VIOLATION",
            Self::NotNullDbViolation => "NOT_NULL_DB_VIOLATION",
            Self::OptimisticLockConflict => "OPTIMISTIC_LOCK_CONFLICT",
            Self::DbError => "DB_ERROR",
            Self::Internal => "INTERNAL_ERROR",
        }
    }

    /// 稳定**整数**错误码（与 `code_str` 一一对应，永不复用/重排）。前端可显示 `[数字-字符串]`，
    /// 便于口头/工单快速引用。分段：1000+ 前置列级校验；1100+ DB 约束；1200+ 并发；1900+ 兜底。
    pub fn code_num(&self) -> u32 {
        match self {
            // 前置列级校验（落库前）
            Self::TypeMismatch => 1001,
            Self::ValueTooLong => 1002,
            Self::NotNullViolation => 1003,
            Self::NumericOutOfRange => 1004,
            Self::DecimalScaleExceeded => 1005,
            Self::InvalidDate => 1006,
            Self::UnknownColumn => 1007,
            // 落库时 DB 约束
            Self::UniqueViolation => 1101,
            Self::ForeignKeyViolation => 1102,
            Self::CheckViolation => 1103,
            Self::NotNullDbViolation => 1104,
            // 并发
            Self::OptimisticLockConflict => 1201,
            // 兜底
            Self::DbError => 1901,
            Self::Internal => 1902,
        }
    }

    /// 映射到 HTTP 类别码（响应信封 code / HTTP status 用）。
    ///
    /// 校验类 → `ValidationError`(422)；唯一/外键/check/非空落库 → `BadRequest`(400)；
    /// 乐观锁 → `Conflict`(409)；其余 → `BusinessError`/`InternalError`。
    pub fn http_code(&self) -> cmx_api_types::ErrCode {
        use cmx_api_types::ErrCode;
        match self {
            Self::TypeMismatch
            | Self::ValueTooLong
            | Self::NotNullViolation
            | Self::NumericOutOfRange
            | Self::DecimalScaleExceeded
            | Self::InvalidDate
            | Self::UnknownColumn => ErrCode::ValidationError,
            Self::UniqueViolation
            | Self::ForeignKeyViolation
            | Self::CheckViolation
            | Self::NotNullDbViolation => ErrCode::BadRequest,
            Self::OptimisticLockConflict => ErrCode::Conflict,
            Self::DbError => ErrCode::BusinessError,
            Self::Internal => ErrCode::InternalError,
        }
    }

    /// 中文消息模板（`{key}` 占位，由 [`render`] 填充）。可用占位：
    /// `caption`(列中文名) / `column`(列名) / `max` / `actual` / `type` / `table` / `detail`。
    pub fn message_template(&self) -> &'static str {
        match self {
            Self::TypeMismatch => "「{caption}」类型不匹配：期望 {type}，实际值「{actual}」",
            Self::ValueTooLong => "「{caption}」长度超限：最多 {max} 个字符，实际 {actual} 个",
            Self::NotNullViolation => "「{caption}」不能为空",
            Self::NumericOutOfRange => "「{caption}」数值超出范围：允许 {type}，实际 {actual}",
            Self::DecimalScaleExceeded => "「{caption}」精度超限：最多 {max}，实际 {actual}",
            Self::InvalidDate => "「{caption}」不是合法的日期/时间：「{actual}」",
            Self::UnknownColumn => "字段「{column}」在表「{table}」中不存在",
            Self::UniqueViolation => "数据已存在（唯一约束冲突）：{detail}",
            Self::ForeignKeyViolation => "引用的关联数据不存在（外键约束）：{detail}",
            Self::CheckViolation => "数据不满足校验约束：{detail}",
            Self::NotNullDbViolation => "「{caption}」不能为空",
            Self::OptimisticLockConflict => "数据已被他人修改，请刷新后重试",
            Self::DbError => "数据保存失败：{detail}",
            Self::Internal => "系统内部错误：{detail}",
        }
    }
}

/// 单条校验 / 约束失败的结构化描述。回给前端 `data.violations[]`，可逐行逐列高亮。
#[derive(Debug, Clone, Serialize)]
pub struct Violation {
    /// 出错行在本次提交中的索引（0 基）；非行级错误（如整体约束）为 None。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub row: Option<usize>,
    /// 表名 / 字典 code。
    pub table: String,
    /// 出错列名（DB 列名）；非列级错误为 None。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<String>,
    /// 列中文名（供展示）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
    /// 稳定错误码（`CmxErrCode::code_str`）。
    pub code: &'static str,
    /// 稳定整数错误码（`CmxErrCode::code_num`）；与 `code` 一一对应，供 `[数字-字符串]` 展示/引用。
    pub code_num: u32,
    /// 渲染后的中文提示。
    pub message: String,
}

impl Violation {
    /// 用错误码 + 参数构造（自动渲染中文消息）。
    pub fn new(
        code: CmxErrCode,
        table: impl Into<String>,
        column: Option<String>,
        caption: Option<String>,
        row: Option<usize>,
        params: &[(&str, String)],
    ) -> Self {
        Self {
            row,
            table: table.into(),
            column,
            caption,
            code: code.code_str(),
            code_num: code.code_num(),
            message: render(code.message_template(), params),
        }
    }
}

/// 极简模板渲染：把模板里的 `{key}` 替换成 `params` 里对应值。未提供的占位原样保留。
/// 不引 i18n / 正则依赖，够用即可。
pub fn render(template: &str, params: &[(&str, String)]) -> String {
    let mut out = template.to_string();
    for (k, v) in params {
        out = out.replace(&format!("{{{k}}}"), v);
    }
    out
}

/// 把 PostgreSQL 原始错误串归类成 [`CmxErrCode`]（落库兜底：前置校验漏过时仍不暴露英文串）。
///
/// 按 PG 错误文案的稳定子串匹配（SQLSTATE 文案跨版本稳定）。`detail` 提取留给调用方
/// （通常直接把整段 PG 串作为 `detail` 参数传给模板，或截断脱敏）。
pub fn classify_db_error(err: &str) -> CmxErrCode {
    let e = err.to_ascii_lowercase();
    if e.contains("duplicate key") || e.contains("unique constraint") {
        CmxErrCode::UniqueViolation
    } else if e.contains("foreign key") || e.contains("violates foreign key constraint") {
        CmxErrCode::ForeignKeyViolation
    } else if e.contains("violates check constraint") {
        CmxErrCode::CheckViolation
    } else if e.contains("null value") && e.contains("not-null") {
        CmxErrCode::NotNullDbViolation
    } else {
        CmxErrCode::DbError
    }
}

/// 从 PG 唯一/外键错误里尽量抽出「约束名 / 列」作为 detail（脱敏用，避免整段英文暴露）。
/// 抽不到就返回一个通用短语。
pub fn brief_db_detail(err: &str) -> String {
    // PG 常见形态：... unique constraint "cf_currency_pkey" ... / ... column "xxx" ...
    if let Some(start) = err.find("constraint \"") {
        let rest = &err[start + "constraint \"".len()..];
        if let Some(end) = rest.find('"') {
            return format!("约束 {}", &rest[..end]);
        }
    }
    if let Some(start) = err.find("column \"") {
        let rest = &err[start + "column \"".len()..];
        if let Some(end) = rest.find('"') {
            return format!("列 {}", &rest[..end]);
        }
    }
    "请检查数据后重试".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_str_stable() {
        assert_eq!(CmxErrCode::ValueTooLong.code_str(), "VALUE_TOO_LONG");
        assert_eq!(CmxErrCode::UniqueViolation.code_str(), "UNIQUE_VIOLATION");
    }

    #[test]
    fn http_code_mapping() {
        use cmx_api_types::ErrCode;
        // 用 as u16 比较，不依赖 ErrCode 派生 PartialEq。
        assert_eq!(
            CmxErrCode::ValueTooLong.http_code() as u16,
            ErrCode::ValidationError as u16
        );
        assert_eq!(
            CmxErrCode::UniqueViolation.http_code() as u16,
            ErrCode::BadRequest as u16
        );
        assert_eq!(
            CmxErrCode::OptimisticLockConflict.http_code() as u16,
            ErrCode::Conflict as u16
        );
    }

    #[test]
    fn render_replaces_placeholders() {
        let s = render(
            "「{caption}」长度超限：最多 {max} 个字符，实际 {actual} 个",
            &[
                ("caption", "科目名称".into()),
                ("max", "128".into()),
                ("actual", "200".into()),
            ],
        );
        assert_eq!(s, "「科目名称」长度超限：最多 128 个字符，实际 200 个");
    }

    #[test]
    fn violation_renders_message() {
        let v = Violation::new(
            CmxErrCode::ValueTooLong,
            "cf_gl_account",
            Some("name".into()),
            Some("科目名称".into()),
            Some(0),
            &[
                ("caption", "科目名称".into()),
                ("max", "128".into()),
                ("actual", "200".into()),
            ],
        );
        assert_eq!(v.code, "VALUE_TOO_LONG");
        assert_eq!(v.code_num, 1002);
        assert!(v.message.contains("128"));
        assert_eq!(v.row, Some(0));
    }

    #[test]
    fn code_num_stable_and_unique() {
        use std::collections::HashSet;
        let all = [
            CmxErrCode::TypeMismatch,
            CmxErrCode::ValueTooLong,
            CmxErrCode::NotNullViolation,
            CmxErrCode::NumericOutOfRange,
            CmxErrCode::DecimalScaleExceeded,
            CmxErrCode::InvalidDate,
            CmxErrCode::UnknownColumn,
            CmxErrCode::UniqueViolation,
            CmxErrCode::ForeignKeyViolation,
            CmxErrCode::CheckViolation,
            CmxErrCode::NotNullDbViolation,
            CmxErrCode::OptimisticLockConflict,
            CmxErrCode::DbError,
            CmxErrCode::Internal,
        ];
        // 整数码互不重复
        let nums: HashSet<u32> = all.iter().map(|c| c.code_num()).collect();
        assert_eq!(nums.len(), all.len(), "code_num 必须互不重复");
        // 关键码值锁定（防误改/重排）
        assert_eq!(CmxErrCode::ValueTooLong.code_num(), 1002);
        assert_eq!(CmxErrCode::OptimisticLockConflict.code_num(), 1201);
    }

    #[test]
    fn classify_db_error_variants() {
        assert_eq!(
            classify_db_error(
                "db error: ERROR: duplicate key value violates unique constraint \"cf_currency_pkey\""
            ),
            CmxErrCode::UniqueViolation
        );
        assert_eq!(
            classify_db_error("null value in column \"code\" violates not-null constraint"),
            CmxErrCode::NotNullDbViolation
        );
        assert_eq!(
            classify_db_error("insert or update on table violates foreign key constraint \"fk_x\""),
            CmxErrCode::ForeignKeyViolation
        );
        assert_eq!(
            classify_db_error("some other db error"),
            CmxErrCode::DbError
        );
    }

    #[test]
    fn brief_db_detail_extracts_constraint() {
        assert_eq!(
            brief_db_detail("duplicate key value violates unique constraint \"cf_currency_pkey\""),
            "约束 cf_currency_pkey"
        );
        assert_eq!(
            brief_db_detail("null value in column \"code\" violates not-null constraint"),
            "列 code"
        );
    }
}

/// 构造校验失败响应：`{code:422, msg, data:{violations:[...]}}`（结构化，前端逐行逐列高亮）。
///
/// doc/dct 等回存 handler 在 changeset 校验失败时统一调用，保证 422 信封形态一致。
/// 从 cmx-api/src/validation.rs 迁入（避免 cmx-api-core 反向依赖 cmx-biz）。
pub fn validation_fail_resp(violations: &[Violation]) -> cmx_api_types::ApiResp<serde_json::Value> {
    cmx_api_types::ApiResp::fail_with_data(
        422,
        format!("数据校验未通过（{} 处）", violations.len()),
        serde_json::json!({ "violations": violations }),
    )
}
