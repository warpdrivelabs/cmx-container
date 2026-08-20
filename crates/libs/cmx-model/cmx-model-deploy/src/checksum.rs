//! DCT/DOC/RPT 定义内容 checksum（部署矩阵 drift 检测用）。
//!
//! 背景：DCT/DOC/RPT 的部署检测走**版本号比对**（`db_state::scenario_of`），不改版本号
//! 只改当前版本内容时矩阵仍显示 `current`。引入内容 checksum 补齐"版本未变但内容已改"
//! 的漂移检测——对齐 SEED/MENU 已有的 checksum 模式。
//!
//! **部署写入侧（`deploy.rs` upsert_module_kind）与矩阵计算侧（`db_state.rs`）必须共用
//! 本函数**：两端算法不一致 = 永久 drift。

use serde_json::Value;
use sha2::{Digest, Sha256};

/// 计算定义 JSON 的规范化 checksum：剔除顶层 `updatedAt` → 紧凑序列化 → SHA256。
///
/// 为什么剔除 `updatedAt`：`save_definition` 每次保存都强制注入顶层 `updatedAt`
/// （毫秒精度时间戳）。若直接对文件字节哈希（SEED/MENU 的 `aggregate_sha256` 做法），
/// 任何一次"无修改保存"都会让 checksum 变化 → 永久 drift，信号钝化。剔除后
/// checksum 只反映业务内容。
///
/// 已知低危误报：workspace 启用 `preserve_order`，键序重排会触发 drift——
/// 语义上文件确实变了，可接受。
pub(crate) fn normalized_def_checksum(doc: &Value) -> String {
    let mut normalized = doc.clone();
    if let Some(obj) = normalized.as_object_mut() {
        obj.remove("updatedAt");
    }
    let compact = serde_json::to_string(&normalized).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(compact.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn updated_at_excluded() {
        // 同一业务内容、不同 updatedAt（无修改保存）→ checksum 不变
        let a = json!({ "moduleMeta": { "version": 1 }, "updatedAt": "2026-08-20T10:00:00.123Z" });
        let b = json!({ "moduleMeta": { "version": 1 }, "updatedAt": "2026-08-20T18:59:59.999Z" });
        assert_eq!(normalized_def_checksum(&a), normalized_def_checksum(&b));
        // 完全没有 updatedAt 的同内容定义 → 与有 updatedAt 的等价
        let c = json!({ "moduleMeta": { "version": 1 } });
        assert_eq!(normalized_def_checksum(&a), normalized_def_checksum(&c));
    }

    #[test]
    fn content_change_detected() {
        let a = json!({ "moduleMeta": { "version": 1 }, "fields": [1, 2] });
        let b = json!({ "moduleMeta": { "version": 1 }, "fields": [1, 2, 3] });
        assert_ne!(normalized_def_checksum(&a), normalized_def_checksum(&b));
    }

    #[test]
    fn stable_across_calls() {
        // 双端一致性基础：纯函数，同输入必同输出（deploy 写入侧与 db_state 计算侧共用）
        let a = json!({ "x": 1 });
        assert_eq!(normalized_def_checksum(&a), normalized_def_checksum(&a));
    }
}
