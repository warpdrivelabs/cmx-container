//! 种子数据 / 菜单文件扫描器与 checksum 计算。
//!
//! 设计要点：
//! - 扫描结果包含原始内容，调用方可直接拿去执行/适配，无需重复读盘
//! - checksum 用 SHA256；模块级聚合 hash 按文件路径排序后拼接计算（顺序无关）

use cmx_model::config::data_path;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

/// 一个被扫描到的种子/菜单文件
#[derive(Debug, Clone)]
pub struct ScannedFile {
    /// SEED: 物理表名（取文件名 stem，如 "cf_gl_account"）
    /// MENU: 始终为空字符串（菜单不需要表名维度）
    pub table_name: String,
    /// 相对路径（用于 cmx_model_module_kind.def_source 等），如 "fi/cmxfico/gl/seed/cf_gl_account.json"
    pub rel_path: String,
    /// 文件原始文本内容
    pub content: String,
    /// 文件内容 SHA256 hex（drift 判断依据）
    pub checksum: String,
    /// SEED: JSON 数组元素数；MENU: items 树递归节点数
    pub row_count: usize,
    /// 文件修改日期（YYYY-MM-DD，给用户看的版本号；同一天多次修改只算一个版本）
    pub modified_date: Option<String>,
}

/// 扫描指定模块下的 seed/*.json
/// 路径：<data_root>/meta/definitions/<domain>/<app>/<module>/seed/
/// data_root 解析优先级：portal.data_root 配置 → CMX_PORTAL_DATA_ROOT 环境变量 → ./data
pub fn scan_seed_files(domain: &str, app: &str, module: &str) -> Vec<ScannedFile> {
    let dir = data_path(["meta", "definitions", domain, app, module, "seed"]);
    let prefix = format!("{domain}/{app}/{module}/seed");
    scan_seed_files_in_dir_with_prefix(&dir, &prefix)
}

/// 扫描指定模块下的 menu-pages JSON
/// 路径：<data_root>/menu-pages/<domain>/<app>/<module>/
/// data_root 解析优先级：portal.data_root 配置 → CMX_PORTAL_DATA_ROOT 环境变量 → ./data
pub fn scan_menu_files(domain: &str, app: &str, module: &str) -> Vec<ScannedFile> {
    let dir = data_path(["menu-pages", domain, app, module]);
    let prefix = format!("{domain}/{app}/{module}");
    scan_menu_files_in_dir_with_prefix(&dir, &prefix)
}

/// 枚举 `data/menu-pages/` 目录下所有 `(domain, app, module)` 三段式目录。
///
/// 仅当 module 目录里**至少含一个** `*.json` 文件时才返回该 key（避免把空目录误判为模块）。
/// 返回结果按字典序排序，结果稳定。
///
/// # 用途
///
/// `db_state` 之前以 DCT/DOC 定义列表为模块发现的唯一来源，导致"只有 menu（无 DCT/DOC）"的模块
/// 从矩阵里消失。本函数配合 lib.rs db_state 的"menu-only 补全循环"使用，让这类轻量模块也能进矩阵。
pub fn scan_all_menu_module_keys() -> Vec<(String, String, String)> {
    use std::collections::BTreeSet;
    let root = data_path(["menu-pages"]);
    let mut keys: BTreeSet<(String, String, String)> = BTreeSet::new();
    let domains = match std::fs::read_dir(&root) {
        Ok(rd) => rd,
        Err(_) => return Vec::new(), // 根目录缺失视为空集合
    };
    for d in domains.flatten() {
        let dname = match d.file_name().to_str() {
            Some(s) if !s.starts_with('.') => s.to_string(),
            _ => continue,
        };
        if !d.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let apps = match std::fs::read_dir(d.path()) {
            Ok(rd) => rd,
            Err(_) => continue,
        };
        for a in apps.flatten() {
            let aname = match a.file_name().to_str() {
                Some(s) if !s.starts_with('.') => s.to_string(),
                _ => continue,
            };
            if !a.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            let mods = match std::fs::read_dir(a.path()) {
                Ok(rd) => rd,
                Err(_) => continue,
            };
            for m in mods.flatten() {
                let mname = match m.file_name().to_str() {
                    Some(s) if !s.starts_with('.') => s.to_string(),
                    _ => continue,
                };
                if !m.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    continue;
                }
                // module 目录里至少含一个 *.json 才计为"有效模块"（避免空目录干扰）
                let has_json = std::fs::read_dir(m.path())
                    .map(|rd| {
                        rd.flatten().any(|e| {
                            e.path().extension().and_then(|s| s.to_str()) == Some("json")
                        })
                    })
                    .unwrap_or(false);
                if has_json {
                    keys.insert((dname.clone(), aname.clone(), mname));
                }
            }
        }
    }
    keys.into_iter().collect()
}

// 以下两个带 _in_dir 的版本专供测试使用（不依赖工作目录）

pub fn scan_seed_files_in_dir(dir: &Path) -> Vec<ScannedFile> {
    scan_seed_files_in_dir_with_prefix(dir, "")
}

pub fn scan_menu_files_in_dir(dir: &Path) -> Vec<ScannedFile> {
    scan_menu_files_in_dir_with_prefix(dir, "")
}

/// 扫描类型：Seed 按 file_stem 取 table_name；Menu 按 file_name 取 rel_path。
enum ScanKind {
    /// 扫描 SEED 目录：table_name 取文件 stem（不含扩展名），用于匹配业务表
    Seed,
    /// 扫描 MENU 目录：table_name 始终为空（菜单不需要表名维度），按 file_name 排序
    Menu,
}

/// 通用文件扫描骨架：read_dir → 过滤 .json → 解析 → 计数 → sha256 → mtime → 排序。
fn scan_files_in_dir(dir: &Path, prefix: &str, scan_kind: ScanKind) -> Vec<ScannedFile> {
    let mut out = Vec::new();
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return out, // 目录不存在 → 空清单（db_state 据此返回 none）
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.extension().and_then(|s| s.to_str()) != Some("json") { continue; }
        let file_name = match p.file_name().and_then(|s| s.to_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };
        let table_name = match scan_kind {
            ScanKind::Seed => match p.file_stem().and_then(|s| s.to_str()) {
                Some(s) => s.to_string(),
                None => continue,
            },
            ScanKind::Menu => String::new(),
        };
        let content = match fs::read_to_string(&p) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let row_count = match scan_kind {
            ScanKind::Seed => count_json_array_elements(&content).unwrap_or(0),
            ScanKind::Menu => count_menu_nodes(&content).unwrap_or(0),
        };
        let checksum = sha256_hex(content.as_bytes());
        let modified_date = file_modified_date(&p);
        let rel_path = if prefix.is_empty() {
            file_name.clone()
        } else {
            format!("{prefix}/{file_name}")
        };
        out.push(ScannedFile { table_name, rel_path, content, checksum, row_count, modified_date });
    }
    match scan_kind {
        ScanKind::Seed => out.sort_by(|a, b| a.table_name.cmp(&b.table_name)),
        ScanKind::Menu => out.sort_by(|a, b| a.rel_path.cmp(&b.rel_path)),
    }
    out
}

fn scan_seed_files_in_dir_with_prefix(dir: &Path, prefix: &str) -> Vec<ScannedFile> {
    scan_files_in_dir(dir, prefix, ScanKind::Seed)
}

fn scan_menu_files_in_dir_with_prefix(dir: &Path, prefix: &str) -> Vec<ScannedFile> {
    scan_files_in_dir(dir, prefix, ScanKind::Menu)
}

/// 计算多个文件的模块级聚合 hash：按 rel_path 排序，拼接 (rel_path + content)，再 SHA256
pub fn aggregate_sha256(files: &[ScannedFile]) -> String {
    let mut sorted: Vec<&ScannedFile> = files.iter().collect();
    sorted.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    let mut hasher = Sha256::new();
    for f in sorted {
        hasher.update(f.rel_path.as_bytes());
        hasher.update([0u8]); // 分隔符，避免 "ab"+"c" 与 "a"+"bc" 撞 hash
        hasher.update(f.content.as_bytes());
        hasher.update([0u8]);
    }
    format!("{:x}", hasher.finalize())
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// 读取文件 mtime，格式化为 YYYY-MM-DD（给用户看的版本号）
/// 失败时返回 None（不影响 drift 判断，drift 走 checksum）
fn file_modified_date(p: &Path) -> Option<String> {
    use std::time::SystemTime;
    let meta = fs::metadata(p).ok()?;
    let mtime: SystemTime = meta.modified().ok()?;
    let dt: chrono::DateTime<chrono::Utc> = mtime.into();
    // 统一 UTC（避免不同时区对同一文件算出不同的"日期"）
    Some(dt.format("%Y-%m-%d").to_string())
}

/// 统计 JSON 顶层数组的元素数（SEED 文件约定顶层是数组）。
///
/// 失败时返回 `None`（调用方按 0 处理），不抛错：
/// - JSON 解析失败 → `None`
/// - 顶层不是数组 → `None`
fn count_json_array_elements(content: &str) -> Option<usize> {
    let v: serde_json::Value = serde_json::from_str(content).ok()?;
    v.as_array().map(|a| a.len())
}

/// 递归统计 menu-pages items 节点数（MENU 文件约定顶层有 `items: [...]` 树）。
///
/// 根节点 + 所有后代 children 节点的总和（与前端"菜单节点数"统计口径一致）。
/// 失败时返回 `None`（items 缺失 / 非数组 → 调用方按 0 处理）。
fn count_menu_nodes(content: &str) -> Option<usize> {
    let v: serde_json::Value = serde_json::from_str(content).ok()?;
    let items = v.get("items")?.as_array()?;
    let mut count = 0usize;
    for root in items {
        // 根节点 1 + 其所有 descendants
        count += 1 + count_children(root);
    }
    Some(count)
}

/// 递归统计某 node 及其所有 descendants 的节点数（不含 node 自身）。
///
/// 用于 `count_menu_nodes` 内联统计 children 数量；无 children 时直接返回 0。
fn count_children(node: &serde_json::Value) -> usize {
    let mut n = 0usize;
    if let Some(children) = node.get("children").and_then(|c| c.as_array()) {
        // 自身不含：递归 children，每个 child 算 1 + 其 descendants
        for child in children {
            n += 1 + count_children(child);
        }
    }
    n
}
