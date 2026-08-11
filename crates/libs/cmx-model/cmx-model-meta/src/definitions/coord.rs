//! DAM 坐标全局反查与定义树"代数"——DAM 可选化的共享基础设施。
//!
//! `/api/dct/*` 与 `/api/doc/*` 的 domain/application/module 三段坐标可选：缺失时由
//! DCT/DOC 各自咽喉点（`resolve_dict` / `resolve_doc_meta`）调本模块按业务编码
//! （dictCode / moduleCode）或文件名全局反查补全：
//! - 唯一命中 → 返回坐标；
//! - 零命中 → business_error；
//! - 多 DAM 冲突 → `Error::Conflict`（HTTP 409）枚举候选坐标（字典序，跨副本确定）。
//!
//! 另提供定义树"代数"（generation）：stat 指纹节流扫描 + 写路径即时 bump，
//! 让全部定义派生缓存（file cache / DocMetaView / TableSpec / CODE_INDEX）自动感知
//! 带外文件变更（手动改定义 JSON 无需重启、无需手动调接口）。

use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use serde_json::Value;
use tokio::sync::RwLock;

use cmx_api_types::{Error, Result};

// ============================================================================
// 坐标类型
// ============================================================================

/// 部分 DAM 坐标（三段可任意缺失，用于缩小全局反查范围）。
#[derive(Debug, Clone, Default)]
pub struct DamPartial {
    pub domain: Option<String>,
    pub application: Option<String>,
    pub module: Option<String>,
}

/// 解析出的完整 DAM 坐标（Ord 保证冲突候选列表字典序稳定，跨副本一致）。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DamCoord {
    pub domain: String,
    pub application: String,
    pub module: String,
}

impl DamCoord {
    pub fn display(&self) -> String {
        format!("{}/{}/{}", self.domain, self.application, self.module)
    }
}

// ============================================================================
// 脏值归一（沿用 resolve_doc_file_smart 惯例：""/"undefined"/"null" = 缺失）
// ============================================================================

/// owned 版脏值归一：`""` / `"undefined"` / `"null"` → `None`。
pub fn clean_opt(v: Option<String>) -> Option<String> {
    v.filter(|s| !s.is_empty() && s != "undefined" && s != "null")
}

/// 借用版脏值归一（同 [`clean_opt`]）。
pub fn clean_str(v: Option<&str>) -> Option<&str> {
    v.filter(|s| !s.is_empty() && *s != "undefined" && *s != "null")
}

// ============================================================================
// 候选过滤与唯一性判定（纯逻辑）
// ============================================================================

/// 按部分 DAM 缩小候选坐标集（缺的段不过滤）。
pub fn filter_coords(candidates: &[DamCoord], partial: &DamPartial) -> Vec<DamCoord> {
    candidates
        .iter()
        .filter(|c| {
            partial.domain.as_deref().is_none_or(|d| d == c.domain)
                && partial.application.as_deref().is_none_or(|a| a == c.application)
                && partial.module.as_deref().is_none_or(|m| m == c.module)
        })
        .cloned()
        .collect()
}

/// 唯一性判定：0 → business_error；1 → Ok；多 → Conflict（候选字典序枚举行）。
///
/// `label` 取值："字典" / "单据 moduleCode" / "定义文件"（用于错误文案）。
// TODO(DAM 可选化 Task 2/3)：DCT/DOC 咽喉点接入后移除本 allow（届时 decide 即有调用方）。
#[allow(dead_code)]
pub(crate) fn decide(mut candidates: Vec<DamCoord>, label: &str, code: &str) -> Result<DamCoord> {
    candidates.sort();
    candidates.dedup();
    match candidates.len() {
        0 => Err(Error::business_error(format!(
            "{label} {code} 未在任何 DAM 下找到；请确认编码或显式传入 domain/application/module"
        ))),
        1 => Ok(candidates.pop().expect("len == 1")),
        _ => {
            let list = candidates
                .iter()
                .map(|c| format!("  - {}", c.display()))
                .collect::<Vec<_>>()
                .join("\n");
            Err(Error::Conflict(format!(
                "{label} {code} 在多个 DAM 下存在，无法自动定位，请显式传入 domain/application/module 消除歧义：\n{list}"
            )))
        }
    }
}

// ============================================================================
// 编码索引（纯构建逻辑；全局缓存与反查入口见本文件后段）
// ============================================================================

/// 编码 → DAM 坐标集索引（懒构建、常驻、代数失配重建）。
#[derive(Debug)]
pub struct CodeIndex {
    /// 构建时的定义树代数（与 [`definitions_generation`] 比对决定是否重建）。
    pub generation: u64,
    /// dictCode → DAM 坐标集。
    pub dct_by_code: HashMap<String, Vec<DamCoord>>,
    /// tableName → DAM 坐标集（仅当全局无 dictCode 命中时回退，防伪冲突）。
    pub dct_by_table: HashMap<String, Vec<DamCoord>>,
    /// moduleCode → DAM 坐标集。
    pub doc_by_code: HashMap<String, Vec<DamCoord>>,
    /// (kind, 文件名) → DAM 坐标集（file 反查用）。
    pub files: HashMap<(String, String), Vec<DamCoord>>,
}

impl CodeIndex {
    pub fn new(generation: u64) -> Self {
        Self {
            generation,
            dct_by_code: HashMap::new(),
            dct_by_table: HashMap::new(),
            doc_by_code: HashMap::new(),
            files: HashMap::new(),
        }
    }

    /// 全部坐标向量排序去重（构建收尾调用，保证冲突候选确定性）。
    pub fn sort_dedup(&mut self) {
        for v in self.dct_by_code.values_mut()
            .chain(self.dct_by_table.values_mut())
            .chain(self.doc_by_code.values_mut())
            .chain(self.files.values_mut())
        {
            v.sort();
            v.dedup();
        }
    }
}

/// 把一份定义 JSON 的编码累积进索引（纯函数，可单测）。
///
/// kind 仅处理 "DCT"/"DOC"；DCT 取 `dictionaryTables[].dictMeta.{dictCode,tableName}`，
/// DOC 取 `moduleMeta.moduleCode`。空编码跳过。
pub fn index_one(idx: &mut CodeIndex, kind: &str, coord: &DamCoord, doc: &Value) {
    match kind {
        "DCT" => {
            let Some(arr) = doc.get("dictionaryTables").and_then(|v| v.as_array()) else {
                return;
            };
            for t in arr {
                let Some(m) = t.get("dictMeta") else { continue };
                if let Some(c) = m.get("dictCode").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
                    idx.dct_by_code.entry(c.to_string()).or_default().push(coord.clone());
                }
                if let Some(tn) = m.get("tableName").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
                    idx.dct_by_table.entry(tn.to_string()).or_default().push(coord.clone());
                }
            }
        }
        "DOC" => {
            if let Some(c) = doc
                .get("moduleMeta")
                .and_then(|m| m.get("moduleCode"))
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
            {
                idx.doc_by_code.entry(c.to_string()).or_default().push(coord.clone());
            }
        }
        _ => {}
    }
}

// ============================================================================
// 定义树"代数"（generation）——带外文件变更自动感知
// ============================================================================
//
// 指纹 = (json 文件数, 最大 mtime 纳秒)；stat 遍历不读内容、不解析 JSON。
// 两条失效通道：
// 1. 进程内写（store.rs 三写路径）→ bump_generation() 即时 +1；
// 2. 带外变更（手动改文件 / git pull）→ 节流扫描发现指纹变化 → +1。
// 各定义派生缓存（resolve.rs file cache / DocMetaView / TableSpec / CODE_INDEX）
// 访问前比对代数，失配即清/重建——多节点各自扫描、独立自愈（AGENTS §五合规）。

/// 节流扫描间隔（代码常量；定义是准静态配置，2s 收敛窗口业务可接受）。
pub const GEN_SCAN_INTERVAL: Duration = Duration::from_secs(2);

/// 定义树指纹（文件数 + 最大 mtime 纳秒）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fingerprint {
    pub count: u64,
    pub max_mtime_nanos: i64,
}

struct GenState {
    /// 上次扫描时刻（节流用）。
    last_scan: Instant,
    /// 上次采样指纹（None=尚未采样）。
    fingerprint: Option<Fingerprint>,
    /// 当前代数（字段名规避 `gen` 保留字，与 CodeIndex.generation 一致）。
    generation: u64,
}

static GEN_STATE: OnceLock<RwLock<GenState>> = OnceLock::new();

fn gen_state() -> &'static RwLock<GenState> {
    GEN_STATE.get_or_init(|| {
        RwLock::new(GenState {
            last_scan: Instant::now() - GEN_SCAN_INTERVAL, // 首次访问立即采样
            fingerprint: None,
            generation: 0,
        })
    })
}

/// stat 遍历目录树计算指纹（只数 `*.json` + 取最大 mtime，不读内容；微秒级）。
pub async fn scan_fingerprint(root: &Path) -> Fingerprint {
    let mut count: u64 = 0;
    let mut max_mtime: i64 = 0;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(mut rd) = tokio::fs::read_dir(&dir).await else {
            continue;
        };
        while let Ok(Some(e)) = rd.next_entry().await {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            let Ok(ft) = e.file_type().await else { continue };
            if ft.is_dir() {
                stack.push(e.path());
                continue;
            }
            if ft.is_file() && name.ends_with(".json") {
                count += 1;
                if let Ok(md) = e.metadata().await
                    && let Ok(t) = md.modified()
                {
                    let nanos = t
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_nanos() as i64)
                        .unwrap_or(0);
                    if nanos > max_mtime {
                        max_mtime = nanos;
                    }
                }
            }
        }
    }
    Fingerprint { count, max_mtime_nanos: max_mtime }
}

/// 当前定义树代数（节流：窗口内复用上次结果，仅一次锁读）。
pub async fn definitions_generation() -> u64 {
    let mut g = gen_state().write().await;
    if g.fingerprint.is_some() && g.last_scan.elapsed() < GEN_SCAN_INTERVAL {
        return g.generation;
    }
    g.last_scan = Instant::now();
    let fp = scan_fingerprint(&crate::config::data_path(["meta", "definitions"])).await;
    if g.fingerprint != Some(fp) {
        g.fingerprint = Some(fp);
        g.generation += 1;
    }
    g.generation
}

/// 进程内写路径成功后调用：代数即时 +1（0 延迟通道）。
///
/// 同时重采样指纹并重置节流窗口——避免 bump 自身引起的文件变化在下次扫描被二次计数。
pub async fn bump_generation() {
    let mut g = gen_state().write().await;
    g.generation += 1;
    g.fingerprint = Some(scan_fingerprint(&crate::config::data_path(["meta", "definitions"])).await);
    g.last_scan = Instant::now();
}

// ============================================================================
// 全局编码索引与反查入口
// ============================================================================

static CODE_INDEX: OnceLock<RwLock<Option<std::sync::Arc<CodeIndex>>>> = OnceLock::new();

fn code_index() -> &'static RwLock<Option<std::sync::Arc<CodeIndex>>> {
    CODE_INDEX.get_or_init(|| RwLock::new(None))
}

/// 纯查询内核（注入索引，可单测）：DCT 先 dictCode、全局无命中才回退 tableName。
fn lookup(idx: &CodeIndex, kind: &str, code: &str, partial: &DamPartial) -> Result<DamCoord> {
    let empty = Vec::new();
    let candidates: &Vec<DamCoord> = match kind {
        "DCT" => idx
            .dct_by_code
            .get(code)
            .filter(|v| !v.is_empty())
            .or_else(|| idx.dct_by_table.get(code))
            .unwrap_or(&empty),
        "DOC" => idx.doc_by_code.get(code).unwrap_or(&empty),
        _ => &empty,
    };
    let label = if kind == "DCT" { "字典" } else { "单据 moduleCode" };
    decide(filter_coords(candidates, partial), label, code)
}

/// 纯查询内核：按定义文件名反查（file_map）。
fn lookup_file(idx: &CodeIndex, kind: &str, file: &str, partial: &DamPartial) -> Result<DamCoord> {
    let empty = Vec::new();
    let candidates = idx
        .files
        .get(&(kind.to_string(), file.to_string()))
        .unwrap_or(&empty);
    decide(filter_coords(candidates, partial), "定义文件", file)
}

/// 全量构建索引：一次 list_definitions 全扫 + 逐文件提取编码（此后常驻）。
async fn build_index(generation: u64) -> Result<std::sync::Arc<CodeIndex>> {
    let items = super::store::list_definitions(None, None, None, None).await?;
    let mut idx = CodeIndex::new(generation);
    for it in &items {
        let kind = it.get("kind").and_then(|v| v.as_str()).unwrap_or("");
        if kind != "DCT" && kind != "DOC" {
            continue; // BASE 域无 DAM 语义；UNKNOWN 损坏文件跳过
        }
        let domain = it.get("domain").and_then(|v| v.as_str()).unwrap_or("");
        let app = it.get("application").and_then(|v| v.as_str()).unwrap_or("");
        let module = it.get("module").and_then(|v| v.as_str()).unwrap_or("");
        let file = it.get("file").and_then(|v| v.as_str()).unwrap_or("");
        if domain.is_empty() || app.is_empty() || module.is_empty() || file.is_empty() {
            continue;
        }
        let coord = DamCoord {
            domain: domain.to_string(),
            application: app.to_string(),
            module: module.to_string(),
        };
        // 文件名级索引不需要读内容
        idx.files
            .entry((kind.to_string(), file.to_string()))
            .or_default()
            .push(coord.clone());
        // 编码级索引需要读定义内容；单文件失败不阻断整体构建
        let dref = super::store::DefRef {
            domain: Some(domain.to_string()),
            application: Some(app.to_string()),
            app: Some(app.to_string()),
            module: Some(module.to_string()),
            file: Some(file.to_string()),
            id: None,
            kind: None,
        };
        let Ok(doc) = super::store::get_definition(&dref).await else {
            continue;
        };
        index_one(&mut idx, kind, &coord, &doc);
    }
    idx.sort_dedup();
    Ok(std::sync::Arc::new(idx))
}

/// 取索引（代数失配或首次 → 重建；写锁内二次检查防并发重复构建）。
async fn get_index() -> Result<std::sync::Arc<CodeIndex>> {
    let generation = definitions_generation().await;
    if let Some(idx) = code_index().read().await.as_ref()
        && idx.generation == generation
    {
        return Ok(idx.clone());
    }
    let mut slot = code_index().write().await;
    if let Some(idx) = slot.as_ref()
        && idx.generation == generation
    {
        return Ok(idx.clone());
    }
    let idx = build_index(generation).await?;
    *slot = Some(idx.clone());
    Ok(idx)
}

/// 全局按业务编码反查 DAM 坐标（DAM 缺失/部分时的补全入口）。
///
/// kind："DCT"（dictCode 优先，全局无命中回退 tableName）/ "DOC"（moduleCode）。
pub async fn resolve_dam_by_code(kind: &str, code: &str, partial: &DamPartial) -> Result<DamCoord> {
    let idx = get_index().await?;
    lookup(&idx, kind, code, partial)
}

/// 全局按定义文件名反查 DAM 坐标（仅传 file 无 DAM 的场景）。
pub async fn resolve_dam_by_file(kind: &str, file: &str, partial: &DamPartial) -> Result<DamCoord> {
    let idx = get_index().await?;
    lookup_file(&idx, kind, file, partial)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn coord(d: &str, a: &str, m: &str) -> DamCoord {
        DamCoord { domain: d.into(), application: a.into(), module: m.into() }
    }

    #[test]
    fn clean_treats_dirty_as_missing() {
        assert_eq!(clean_opt(Some("".into())), None);
        assert_eq!(clean_opt(Some("undefined".into())), None);
        assert_eq!(clean_opt(Some("null".into())), None);
        assert_eq!(clean_opt(Some("basic".into())), Some("basic".into()));
        assert_eq!(clean_opt(None), None);
        assert_eq!(clean_str(Some("")), None);
        assert_eq!(clean_str(Some("fi")), Some("fi"));
    }

    #[test]
    fn filter_coords_narrows_by_present_segments() {
        let all = vec![coord("basic", "dataplatform", "mdm"), coord("fi", "cmxfico", "gl")];
        let only_basic = filter_coords(&all, &DamPartial { domain: Some("basic".into()), ..Default::default() });
        assert_eq!(only_basic, vec![coord("basic", "dataplatform", "mdm")]);
        let none = filter_coords(&all, &DamPartial { domain: Some("hr".into()), ..Default::default() });
        assert!(none.is_empty());
        let both = filter_coords(&all, &DamPartial::default());
        assert_eq!(both.len(), 2);
    }

    #[test]
    fn decide_zero_one_many() {
        // 0 → business_error
        let e = decide(vec![], "字典", "ghost").unwrap_err();
        assert!(matches!(e, Error::BusinessError(_)));
        // 1 → Ok
        let c = decide(vec![coord("basic", "dataplatform", "mdm")], "字典", "supplier").unwrap();
        assert_eq!(c.display(), "basic/dataplatform/mdm");
        // 多 → Conflict 且候选字典序
        let e = decide(
            vec![coord("fi", "cmxfico", "gl"), coord("basic", "dataplatform", "mdm")],
            "字典", "supplier",
        ).unwrap_err();
        match e {
            Error::Conflict(msg) => {
                let basic_at = msg.find("basic/dataplatform/mdm").unwrap();
                let fi_at = msg.find("fi/cmxfico/gl").unwrap();
                assert!(basic_at < fi_at, "候选必须字典序：{msg}");
            }
            other => panic!("应为 Conflict：{other:?}"),
        }
    }

    #[test]
    fn decide_dedups_same_coord() {
        let c = decide(
            vec![coord("basic", "dataplatform", "mdm"), coord("basic", "dataplatform", "mdm")],
            "字典", "supplier",
        ).unwrap();
        assert_eq!(c.display(), "basic/dataplatform/mdm");
    }

    #[test]
    fn index_one_extracts_dct_and_doc_codes() {
        let mut idx = CodeIndex::new(1);
        let c = coord("basic", "dataplatform", "mdm");
        let dct = json!({
            "dictionaryTables": [
                { "dictMeta": { "dictCode": "supplier", "tableName": "cf_supplier" } },
                { "dictMeta": { "dictCode": "", "tableName": "cf_anon" } }
            ]
        });
        index_one(&mut idx, "DCT", &c, &dct);
        assert_eq!(idx.dct_by_code.get("supplier").unwrap(), &vec![c.clone()]);
        assert_eq!(idx.dct_by_table.get("cf_supplier").unwrap(), &vec![c.clone()]);
        // 空 dictCode 跳过，tableName 仍收
        assert!(!idx.dct_by_code.contains_key(""));
        assert_eq!(idx.dct_by_table.get("cf_anon").unwrap(), &vec![c.clone()]);

        let doc = json!({ "moduleMeta": { "moduleCode": "cmxfico" } });
        index_one(&mut idx, "DOC", &c, &doc);
        assert_eq!(idx.doc_by_code.get("cmxfico").unwrap(), &vec![c.clone()]);
        // 未知 kind 忽略
        index_one(&mut idx, "BASE", &c, &doc);
    }

    #[test]
    fn sort_dedup_makes_coords_deterministic() {
        let mut idx = CodeIndex::new(1);
        let v = idx.dct_by_code.entry("x".into()).or_default();
        v.push(coord("fi", "a", "m"));
        v.push(coord("basic", "a", "m"));
        v.push(coord("fi", "a", "m"));
        idx.sort_dedup();
        assert_eq!(
            idx.dct_by_code.get("x").unwrap(),
            &vec![coord("basic", "a", "m"), coord("fi", "a", "m")]
        );
    }

    #[tokio::test]
    async fn scan_fingerprint_counts_json_and_detects_change() {
        let dir = std::env::temp_dir().join(format!("cmx_coord_fp_{}", std::process::id()));
        let sub = dir.join("d/a/m");
        tokio::fs::create_dir_all(&sub).await.unwrap();
        tokio::fs::write(sub.join("x_v1.json"), "{}").await.unwrap();
        tokio::fs::write(dir.join("skip.txt"), "no").await.unwrap();

        let fp1 = scan_fingerprint(&dir).await;
        assert_eq!(fp1.count, 1); // 只数 json

        tokio::fs::write(sub.join("y_v1.json"), "{}").await.unwrap();
        let fp2 = scan_fingerprint(&dir).await;
        assert_eq!(fp2.count, 2);
        assert!(fp2.max_mtime_nanos >= fp1.max_mtime_nanos);

        tokio::fs::remove_dir_all(&dir).await.unwrap();
    }

    #[test]
    fn lookup_prefers_dictcode_over_tablename() {
        let mut idx = CodeIndex::new(1);
        let c1 = coord("basic", "dataplatform", "mdm");
        let c2 = coord("fi", "cmxfico", "gl");
        // supplier 的 dictCode 只在 basic；但 fi 的某表 tableName 恰好叫 supplier
        idx.dct_by_code.entry("supplier".into()).or_default().push(c1.clone());
        idx.dct_by_table.entry("supplier".into()).or_default().push(c2.clone());
        idx.sort_dedup();

        let got = super::lookup(&idx, "DCT", "supplier", &DamPartial::default()).unwrap();
        assert_eq!(got, c1, "dictCode 命中优先，不回退 tableName");

        // dictCode 全局无命中、仅 tableName 命中时回退成功
        let c3 = coord("hr", "cmxhr", "org");
        idx.dct_by_table.entry("dept".into()).or_default().push(c3.clone());
        idx.sort_dedup();
        let got = super::lookup(&idx, "DCT", "dept", &DamPartial::default()).unwrap();
        assert_eq!(got, c3, "dictCode 未命中时回退 tableName");

        // dct_by_code 与 dct_by_table 均未命中 → Err
        let got = super::lookup(&idx, "DCT", "cf_anon_never", &DamPartial::default());
        assert!(got.is_err());
    }

    #[test]
    fn lookup_conflict_lists_all_candidates_sorted() {
        let mut idx = CodeIndex::new(1);
        idx.doc_by_code.entry("cmxfico".into()).or_default().push(coord("fi", "cmxfico", "gl"));
        idx.doc_by_code.entry("cmxfico".into()).or_default().push(coord("basic", "dataplatform", "mdm"));
        idx.sort_dedup();
        let e = super::lookup(&idx, "DOC", "cmxfico", &DamPartial::default()).unwrap_err();
        assert!(matches!(e, Error::Conflict(_)));
    }

    #[test]
    fn lookup_partial_filter_applies_before_decide() {
        let mut idx = CodeIndex::new(1);
        idx.dct_by_code.entry("supplier".into()).or_default().push(coord("basic", "dataplatform", "mdm"));
        idx.dct_by_code.entry("supplier".into()).or_default().push(coord("fi", "cmxfico", "gl"));
        idx.sort_dedup();
        let got = super::lookup(
            &idx, "DCT", "supplier",
            &DamPartial { domain: Some("fi".into()), ..Default::default() },
        ).unwrap();
        assert_eq!(got.display(), "fi/cmxfico/gl");
    }

    #[test]
    fn lookup_file_map() {
        let mut idx = CodeIndex::new(1);
        idx.files.entry(("DOC".into(), "cmxfico_doc_meta_v1.json".into())).or_default()
            .push(coord("fi", "cmxfico", "gl"));
        let got = super::lookup_file(&idx, "DOC", "cmxfico_doc_meta_v1.json", &DamPartial::default()).unwrap();
        assert_eq!(got.display(), "fi/cmxfico/gl");
        assert!(super::lookup_file(&idx, "DOC", "ghost.json", &DamPartial::default()).is_err());
    }
}
