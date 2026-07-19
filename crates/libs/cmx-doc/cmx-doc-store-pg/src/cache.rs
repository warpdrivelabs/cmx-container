//! DocMetaView 进程内缓存（方案 §3.7）
//!
//! 键：`domain/app/module/file@version`。值：`Arc<DocMetaView>`。
//! 用 `OnceLock<Mutex<HashMap>>`，不引入额外依赖（dashmap 等）。
//!
//! 失效：定义保存/删除后按 docKey 逐出（`invalidate`）；带版本号则版本变即键变，天然免失效。

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use cmx_doc_model::meta::DocMetaView;

static CACHE: OnceLock<Mutex<HashMap<String, Arc<DocMetaView>>>> = OnceLock::new();

fn cache() -> &'static Mutex<HashMap<String, Arc<DocMetaView>>> {
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// 构造缓存键。
pub fn doc_key(domain: &str, app: &str, module: &str, file: &str) -> String {
    format!("{domain}/{app}/{module}/{file}")
}

/// 取缓存（命中返回 Arc 克隆）。
pub fn get(key: &str) -> Option<Arc<DocMetaView>> {
    cache().lock().ok()?.get(key).cloned()
}

/// 存缓存。
pub fn put(key: String, view: Arc<DocMetaView>) {
    if let Ok(mut m) = cache().lock() {
        m.insert(key, view);
    }
}

/// 逐出某键（定义变更后调用）。
pub fn invalidate(key: &str) {
    if let Ok(mut m) = cache().lock() {
        m.remove(key);
    }
}

/// 清空（测试/运维用）。
pub fn clear() {
    if let Ok(mut m) = cache().lock() {
        m.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn put_get_invalidate() {
        let doc = json!({
            "docMeta": { "docCode": "T", "version": 1 },
            "voucherSchema": { "schema": [[{"id":"t","level":"L1"}]], "relations": [] },
            "voucherTables": [ { "level":"L1", "tableName":"t",
                "fields":[{"name":"id","dataType":"BIGINT","isPrimaryKey":1}] } ]
        });
        let view = Arc::new(DocMetaView::parse(&doc, &json!(null)).unwrap());
        let key = doc_key("d", "a", "m", "f.json");
        put(key.clone(), view.clone());
        assert!(get(&key).is_some());
        invalidate(&key);
        assert!(get(&key).is_none());
    }
}
