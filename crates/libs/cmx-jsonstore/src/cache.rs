//! 页面源码 / 索引文件的进程内 L1 缓存，与内容版本锚点 `rev` 工具。
//!
//! 设计要点（详见 `.trae/documents/20260730_前后端页面加载性能优化方案.md` 第三章）：
//!
//! - **`rev` = xxhash64(source bytes) 截断 16 hex**：非密码学哈希，CMX 页面缓存属"非安全上下文"
//!   （输入受信：服务端算、内部作者编辑；无对抗方；碰撞后果轻微=一次刷新即修）。
//!   `rev` 同时作 HTTP ETag 值与前端 IndexedDB 缓存校验锚点，从「保存时算一次」贯穿全链路。
//! - **进程内 moka L1 缓存**：缓存索引文件（`index.json` / `pages-list.json` / 分片）解析结果与
//!   页面源文件文本。TTL 5-30s；`save_*` 写后调 [`invalidate_path`] 失效本进程缓存。
//! - **集群一致性**：`rev` 写入共享索引文件，所有节点读同一份 → 天然一致；跨节点 moka 各自 TTL
//!   收敛（5-30s），不依赖即时广播（秒级一致留远期 Redis pub-sub）。
//!
//! 关于 `LazyLock` 与无状态约束（AGENTS.md 第五章）：本缓存存的是**只读文件内容的派生缓存**，
//! 进程重启可从磁盘无损重建，非用户业务状态，属于"基础设施/只读配置"豁免类别。
//! 多节点各自独立缓存，写后靠 `invalidate_path` 失效本节点 + 其它节点 TTL 收敛，无单点风险。

use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::Duration;

use moka::future::Cache;

use crate::error::PortalResult;

/// `rev` 字符串长度（xxhash64 → 16 hex）。
pub const REV_LEN: usize = 16;

/// 缓存条目：文件内容可作文本或 JSON `Value` 复用（同一文件二选一，按入口预解析）。
///
/// 用枚举而非泛型，是因为 moka 单实例需统一 value 类型；文本与 JSON 在不同入口被请求，
/// 同一文件缓存命中后按需转换（JSON 入口若只缓存了文本，则现解析一次并回填）。
#[derive(Clone, Debug)]
enum CacheEntry {
    /// 纯文本内容（页面源文件）。
    Text(String),
    /// 已解析的 JSON 值（索引文件）。
    Json(serde_json::Value),
}

/// 进程级共享 L1 缓存：`PathBuf → CacheEntry`。
///
/// - `time_to_live` 由 `assets.page_cache_ttl_secs` 配置（缺省 30s）；不用 `time_to_idle`，避免热 key
///   内存膨胀（与 cmx-auth `blacklist.rs` 同一考量）。
/// - `max_capacity` 由 `assets.page_cache_max_entries` 配置（缺省 4096 条）：索引文件数量有限
///   （域分片 + v1 list + native index ≈ 数十），页面源文件按热点页缓存；超出由 moka LRU 淘汰。
/// - 容量按条目数估，不做精确 weigher：缓存对象多为 KB 级，条数上限已足够约束内存。
///
/// 注：TTL 与容量在 `LazyLock` 首次访问时（即进程首个页面请求）读一次配置后固定，**改配置需重启生效**
/// （与 `cache_enabled()` 的运行时热读不同——重建 moka 实例代价大且会丢缓存，不值得）。
static L1: LazyLock<Cache<PathBuf, CacheEntry>> = LazyLock::new(|| {
    let ttl_secs = cmx_utils::ConfigManager::try_global()
        .map(|cfg| cfg.get_as_or("assets.page_cache_ttl_secs", 30u64))
        .unwrap_or(30);
    let max_entries = cmx_utils::ConfigManager::try_global()
        .map(|cfg| cfg.get_as_or("assets.page_cache_max_entries", 4_096u64))
        .unwrap_or(4_096);
    Cache::builder()
        .time_to_live(Duration::from_secs(ttl_secs))
        .max_capacity(max_entries)
        .build()
});

/// 读取页面缓存总开关：`assets.page_cache_enabled`，缺省 `false`（默认关闭，等价加缓存前行为）。
///
/// 仅控制 **moka L1 进程内缓存**（省磁盘 I/O）：
/// 关闭时 [`cached_read_text`] / [`cached_read_json`] 直接穿透读盘、不回填；
/// [`invalidate_path`] / [`invalidate_paths`] / [`invalidate_all`] 空操作。
///
/// **不控制** ETag/304 与 batch diff（HTTP 协议层缓存，rev 实时算不依赖 moka，始终生效）。
///
/// 设计为运行时热读（每次调用查 ConfigManager），改配置后新请求即生效，无需重启。
pub fn cache_enabled() -> bool {
    // 测试覆盖钩子：测试用 [`set_enabled_for_test`] 强制开关，绕过全局配置（测试环境无 config.toml）。
    #[cfg(test)]
    {
        if let Some(v) = TEST_OVERRIDE.with(|c| c.get()) {
            return v;
        }
    }
    cmx_utils::ConfigManager::try_global()
        .map(|cfg| cfg.get_as_or("assets.page_cache_enabled", false))
        .unwrap_or(false)
}

// 测试专用：强制覆盖缓存开关（测试环境无 config.toml，无法经配置开启）。
#[cfg(test)]
thread_local! {
    static TEST_OVERRIDE: std::cell::Cell<Option<bool>> = std::cell::Cell::new(None);
}

/// 测试专用：设置缓存开关覆盖值。`None` 清除覆盖（恢复读配置）。
#[cfg(test)]
pub fn set_enabled_for_test(enabled: Option<bool>) {
    TEST_OVERRIDE.with(|c| c.set(enabled));
}

/// 计算内容的 `rev`（xxhash64 → 16 hex 小写），作 ETag 值与缓存校验锚点。
///
/// 保存页面时调一次；后续读比较均为 O(1) 字符串相等判断。
pub fn content_rev(bytes: &[u8]) -> String {
    let h = xxhash_rust::xxh64::xxh64(bytes, 0);
    format!("{h:016x}")
}

/// 缓存读取纯文本文件；文件不存在返回 `None`（与 [`crate::fsutil::read_text_opt`] 语义一致）。
///
/// 命中 L1 直接返回；未命中则读盘并回填。TTL 到期或 [`invalidate_path`] 后自动失效。
/// **缓存开关关闭时**：直接穿透 [`crate::fsutil::read_text_opt`]，不查不写 L1。
pub async fn cached_read_text(path: &Path) -> PortalResult<Option<String>> {
    if !cache_enabled() {
        return crate::fsutil::read_text_opt(path).await;
    }
    if let Some(entry) = L1.get(path).await {
        match entry {
            CacheEntry::Text(t) => return Ok(Some(t)),
            // 缓存里是 JSON 形态：文本入口不直接复用，落盘重读以保留原始字节。
            CacheEntry::Json(_) => {}
        }
    }
    let opt = crate::fsutil::read_text_opt(path).await?;
    if let Some(ref t) = opt {
        L1.insert(path.to_path_buf(), CacheEntry::Text(t.clone())).await;
    }
    Ok(opt)
}

/// 缓存读取 JSON 文件为 [`serde_json::Value`]；文件不存在返回 `None`。
///
/// 命中 L1 的 JSON 条目直接返回；命中文本条目则现解析一次并回填 JSON 形态。
/// **缓存开关关闭时**：直接穿透 [`crate::fsutil::read_json_opt`]，不查不写 L1。
pub async fn cached_read_json(path: &Path) -> PortalResult<Option<serde_json::Value>> {
    if !cache_enabled() {
        return crate::fsutil::read_json_opt(path).await;
    }
    if let Some(entry) = L1.get(path).await {
        match entry {
            CacheEntry::Json(v) => return Ok(Some(v)),
            CacheEntry::Text(t) => {
                let v: serde_json::Value = serde_json::from_slice(t.as_bytes())?;
                L1.insert(path.to_path_buf(), CacheEntry::Json(v.clone())).await;
                return Ok(Some(v));
            }
        }
    }
    let opt = crate::fsutil::read_json_opt(path).await?;
    if let Some(ref v) = opt {
        L1.insert(path.to_path_buf(), CacheEntry::Json(v.clone())).await;
    }
    Ok(opt)
}

/// 失效指定路径的本进程缓存条目。
///
/// 在 `save_*` 写完源文件 / 索引后调用。跨节点失效靠共享索引文件 `rev` + 对端 TTL 收敛。
/// **缓存开关关闭时**：空操作（本就未缓存）。
///
/// 注：`L1.invalidate` 的泛型 bound 对 `PathBuf` key 需传 `&PathBuf`（owned 引用），
/// 而非 `&Path`，故这里取引用而非 `as_path`。
pub async fn invalidate_path(path: &Path) {
    if !cache_enabled() {
        return;
    }
    let key = path.to_path_buf();
    L1.invalidate(&key).await;
}

/// 失效一批路径（索引双写场景：v1 list + v2 shard 同时变更）。
pub async fn invalidate_paths(paths: &[&Path]) {
    if !cache_enabled() {
        return;
    }
    for p in paths {
        invalidate_path(p).await;
    }
}

/// 全量清空本进程 L1 缓存（所有源文件 + 索引条目）。
///
/// 用于「索引重建」等可能批量改动大量文件的场景：逐路径失效不现实（可能几十上百文件），
/// 直接全量清空最稳妥，下次读全部重新从磁盘加载。跨节点仍靠各自 TTL 收敛。
/// **缓存开关关闭时**：空操作。
///
/// 注：moka 的 `invalidate_all` 是同步方法（标记删除、立即返回），与 `invalidate`（async）不同。
pub fn invalidate_all() {
    if !cache_enabled() {
        return;
    }
    L1.invalidate_all();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_rev_is_16_hex() {
        let rev = content_rev(b"hello");
        assert_eq!(rev.len(), REV_LEN);
        assert!(rev.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn content_rev_stable_for_same_input() {
        assert_eq!(content_rev(b"hello"), content_rev(b"hello"));
    }

    #[test]
    fn content_rev_differs_for_different_input() {
        assert_ne!(content_rev(b"hello"), content_rev(b"world"));
    }

    #[tokio::test]
    async fn cached_read_text_hits_after_invalidate_misses() {
        // 测试环境无 config.toml，强制开启缓存验证缓存逻辑本身。
        set_enabled_for_test(Some(true));
        let dir = std::env::temp_dir().join(format!(
            "cmx-portal-base-cache-test-text-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = dir.join("a.txt");
        crate::fsutil::write_text_atomic(&path, "v1").await.unwrap();

        // 首读回填缓存。
        assert_eq!(cached_read_text(&path).await.unwrap().as_deref(), Some("v1"));
        // 覆盖写（绕过缓存，模拟外部写）。
        crate::fsutil::write_text_atomic(&path, "v2").await.unwrap();
        // 未失效：仍读到旧值（TTL 内）。
        assert_eq!(cached_read_text(&path).await.unwrap().as_deref(), Some("v1"));
        // 失效后重读：拿到新值并回填。
        invalidate_path(&path).await;
        assert_eq!(cached_read_text(&path).await.unwrap().as_deref(), Some("v2"));

        let _ = tokio::fs::remove_dir_all(&dir).await;
        set_enabled_for_test(None);
    }

    #[tokio::test]
    async fn cached_read_bypasses_when_disabled() {
        // 开关关闭：cached_read_* 应穿透读盘，每次拿最新内容（不缓存）。
        set_enabled_for_test(Some(false));
        let dir = std::env::temp_dir().join(format!(
            "cmx-portal-base-cache-test-disabled-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = dir.join("c.txt");
        crate::fsutil::write_text_atomic(&path, "v1").await.unwrap();
        assert_eq!(cached_read_text(&path).await.unwrap().as_deref(), Some("v1"));
        // 覆盖写后，关闭缓存下应立即读到新值（未走缓存）。
        crate::fsutil::write_text_atomic(&path, "v2").await.unwrap();
        assert_eq!(cached_read_text(&path).await.unwrap().as_deref(), Some("v2"));

        let _ = tokio::fs::remove_dir_all(&dir).await;
        set_enabled_for_test(None);
    }

    #[tokio::test]
    async fn cached_read_json_roundtrip_and_invalidate() {
        set_enabled_for_test(Some(true));
        let dir = std::env::temp_dir().join(format!(
            "cmx-portal-base-cache-test-json-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = dir.join("b.json");
        let v = serde_json::json!({ "k": 1 });
        crate::fsutil::write_json_atomic(&path, &v, false)
            .await
            .unwrap();

        let got = cached_read_json(&path).await.unwrap().unwrap();
        assert_eq!(got, v);

        let v2 = serde_json::json!({ "k": 2 });
        crate::fsutil::write_json_atomic(&path, &v2, false)
            .await
            .unwrap();
        // 未失效仍是旧值。
        assert_eq!(cached_read_json(&path).await.unwrap(), Some(v.clone()));
        invalidate_path(&path).await;
        assert_eq!(cached_read_json(&path).await.unwrap().unwrap(), v2);

        let _ = tokio::fs::remove_dir_all(&dir).await;
        set_enabled_for_test(None);
    }

    #[tokio::test]
    async fn cached_read_missing_returns_none() {
        let path = std::env::temp_dir().join(format!(
            "cmx-portal-base-cache-test-missing-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        assert!(cached_read_text(&path).await.unwrap().is_none());
        assert!(cached_read_json(&path).await.unwrap().is_none());
    }
}
