//! DocMetaView 进程内缓存(方案 §3.7,集群一致版)。
//!
//! 键:`domain/app/module/file`。值:`Arc<DocMetaView>`。
//!
//! ## 策略
//!
//! 1. **本地缓存**:[`DashMap`] 替代旧 `Mutex<HashMap>`,无锁读,热路径无竞争;
//! 2. **TTL 兜底**:默认 10 分钟过期,即使 `invalidate` 漏调,长期运行内存也不会无界增长;
//! 3. **集群失效**:`invalidate` 仅逐出本节点缓存。集群一致性依赖:
//!    - 文档定义准静态(改动低频);
//!    - 调用方在定义变更后调 `invalidate`(已实现);
//!    - **未实现**:Redis pub/sub 广播(若需强一致,需在本模块订阅广播并调用 `invalidate`)。
//!
//! ## 集群部署合规(AGENTS 第五章)
//!
//! 旧实现 `OnceLock<Mutex<HashMap>>` 缓存业务数据违反"集群部署与无状态约束"红线:
//! - `invalidate` 只本节点生效,A 节点改定义后 B 节点仍读旧值;
//! - 无 TTL,长期运行内存无界增长。
//!
//! 本实现用 TTL 兜底最终一致,集群广播作为后续增强点(见模块 todo)。

use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use dashmap::DashMap;

use cmx_doc_model::meta::DocMetaView;

/// 默认 TTL(10 分钟):平衡命中率与定义变更最终一致延迟。
const DEFAULT_TTL: Duration = Duration::from_secs(600);

/// 缓存条目:持有 `Arc<DocMetaView>` + 过期时间戳。
struct Entry {
    view: Arc<DocMetaView>,
    expire_at: Instant,
}

/// 全局缓存(进程内,无锁读)。
static CACHE: OnceLock<DashMap<String, Entry>> = OnceLock::new();

/// 取全局缓存单例(惰性初始化)。
fn cache() -> &'static DashMap<String, Entry> {
    CACHE.get_or_init(DashMap::new)
}

/// 构造缓存键:`domain/app/module/file`。
pub fn doc_key(domain: &str, app: &str, module: &str, file: &str) -> String {
    format!("{domain}/{app}/{module}/{file}")
}

/// 取缓存(命中且未过期返回 Arc 克隆)。
///
/// 过期条目在读取时惰性清除(避免后台扫描线程)。
pub fn get(key: &str) -> Option<Arc<DocMetaView>> {
    let entry = cache().get(key)?;
    if entry.expire_at < Instant::now() {
        // 过期:丢弃读锁后异步移除(避免死锁)
        drop(entry);
        cache().remove(key);
        return None;
    }
    Some(entry.view.clone())
}

/// 存缓存(TTL = [`DEFAULT_TTL`])。
pub fn put(key: String, view: Arc<DocMetaView>) {
    cache().insert(
        key,
        Entry {
            view,
            expire_at: Instant::now() + DEFAULT_TTL,
        },
    );
}

/// 逐出某键(定义变更后调用)。
///
/// **仅本节点**:集群下需配合 Redis pub/sub 广播(见模块文档)。
pub fn invalidate(key: &str) {
    cache().remove(key);
}

/// 清空全部缓存(测试/运维用)。
pub fn clear() {
    cache().clear();
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn put_get_invalidate() {
        let doc = json!({
            "moduleMeta": { "moduleCode": "T", "version": 1 },
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

    #[test]
    fn ttl_expiration() {
        // 用 put + 手动改 expire_at 模拟过期(避免真等 10 分钟)
        let doc = json!({
            "moduleMeta": { "moduleCode": "T", "version": 1 },
            "voucherSchema": { "schema": [[{"id":"t","level":"L1"}]], "relations": [] },
            "voucherTables": [ { "level":"L1", "tableName":"t",
                "fields":[{"name":"id","dataType":"BIGINT","isPrimaryKey":1}] } ]
        });
        let view = Arc::new(DocMetaView::parse(&doc, &json!(null)).unwrap());
        let key = doc_key("test", "ttl", "exp", "x.json");
        // 直接操作缓存:塞入已过期条目
        cache().insert(
            key.clone(),
            Entry {
                view: view.clone(),
                expire_at: Instant::now() - Duration::from_secs(1),
            },
        );
        // 过期条目应返回 None 并被清除
        assert!(get(&key).is_none());
        assert!(cache().get(&key).is_none());
    }
}
