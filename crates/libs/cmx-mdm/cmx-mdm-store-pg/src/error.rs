//! cmx-mdm-store-pg 错误助手。
//!
//! 公共错误构造（`api_err`/`api_err_db`/`pg_detail`）已上提到 `cmx_biz::error`，
//! 此处 re-export 保持本 crate 调用点零改动。

// 公共错误助手重导出（向后兼容：本 crate 内 `api_err`/`api_err_db` 调用点零改动）。
pub use cmx_biz::{api_err, api_err_db};

use serde_json::Value;

/// 把 Value 对象里某 String 字段 parse 回 JSON（JSONB 列在 DB 返回 text，需还原）。
///
/// 供 activation_store / match_config_store / doc_accessor 共用（消除 3 份复刻）。
pub(crate) fn parse_jsonb_field(v: &mut Value, field: &str) {
    if let Some(obj) = v.as_object()
        && let Some(s) = obj.get(field).and_then(|x| x.as_str())
        && let Ok(parsed) = serde_json::from_str::<Value>(s)
        && let Some(obj) = v.as_object_mut() {
            obj.insert(field.to_string(), parsed);
        }
}
