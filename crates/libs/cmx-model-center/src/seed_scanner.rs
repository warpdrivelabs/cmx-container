//! 种子数据 / 菜单文件扫描器与 checksum 计算。
//!
//! 设计要点：
//! - 扫描结果包含原始内容，调用方可直接拿去执行/适配，无需重复读盘
//! - checksum 用 SHA256；模块级聚合 hash 按文件路径排序后拼接计算（顺序无关）

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
    /// 文件内容 SHA256 hex
    pub checksum: String,
    /// SEED: JSON 数组元素数；MENU: items 树递归节点数
    pub row_count: usize,
}

/// 扫描指定模块下的 seed/*.json
/// 路径：data/meta/definitions/<domain>/<app>/<module>/seed/
pub fn scan_seed_files(domain: &str, app: &str, module: &str) -> Vec<ScannedFile> {
    let dir = Path::new("data/meta/definitions").join(domain).join(app).join(module).join("seed");
    let prefix = format!("{domain}/{app}/{module}/seed");
    scan_seed_files_in_dir_with_prefix(&dir, &prefix)
}

/// 扫描指定模块下的 menu-pages JSON
/// 路径：data/menu-pages/<domain>/<app>/<module>/
pub fn scan_menu_files(domain: &str, app: &str, module: &str) -> Vec<ScannedFile> {
    let dir = Path::new("data/menu-pages").join(domain).join(app).join(module);
    let prefix = format!("{domain}/{app}/{module}");
    scan_menu_files_in_dir_with_prefix(&dir, &prefix)
}

// 以下两个带 _in_dir 的版本专供测试使用（不依赖工作目录）

pub fn scan_seed_files_in_dir(dir: &Path) -> Vec<ScannedFile> {
    scan_seed_files_in_dir_with_prefix(dir, "")
}

pub fn scan_menu_files_in_dir(dir: &Path) -> Vec<ScannedFile> {
    scan_menu_files_in_dir_with_prefix(dir, "")
}

fn scan_seed_files_in_dir_with_prefix(dir: &Path, prefix: &str) -> Vec<ScannedFile> {
    let mut out = Vec::new();
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return out, // 目录不存在 → 空清单（db_state 据此返回 none）
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.extension().and_then(|s| s.to_str()) != Some("json") { continue; }
        let table_name = match p.file_stem().and_then(|s| s.to_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };
        let content = match fs::read_to_string(&p) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let row_count = count_json_array_elements(&content).unwrap_or(0);
        let checksum = sha256_hex(content.as_bytes());
        let rel_path = if prefix.is_empty() {
            format!("{table_name}.json")
        } else {
            format!("{prefix}/{table_name}.json")
        };
        out.push(ScannedFile { table_name, rel_path, content, checksum, row_count });
    }
    // 按 table_name 排序，保证输出稳定
    out.sort_by(|a, b| a.table_name.cmp(&b.table_name));
    out
}

fn scan_menu_files_in_dir_with_prefix(dir: &Path, prefix: &str) -> Vec<ScannedFile> {
    let mut out = Vec::new();
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return out,
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.extension().and_then(|s| s.to_str()) != Some("json") { continue; }
        let file_name = match p.file_name().and_then(|s| s.to_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };
        let content = match fs::read_to_string(&p) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let row_count = count_menu_nodes(&content).unwrap_or(0);
        let checksum = sha256_hex(content.as_bytes());
        let rel_path = if prefix.is_empty() {
            file_name.clone()
        } else {
            format!("{prefix}/{file_name}")
        };
        out.push(ScannedFile {
            table_name: String::new(), // MENU 不用
            rel_path,
            content,
            checksum,
            row_count,
        });
    }
    out.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));
    out
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

fn count_json_array_elements(content: &str) -> Option<usize> {
    let v: serde_json::Value = serde_json::from_str(content).ok()?;
    v.as_array().map(|a| a.len())
}

/// 递归统计 menu-pages items 节点数
fn count_menu_nodes(content: &str) -> Option<usize> {
    let v: serde_json::Value = serde_json::from_str(content).ok()?;
    let items = v.get("items")?.as_array()?;
    let mut count = 0usize;
    for root in items {
        count += 1 + count_children(root);
    }
    Some(count)
}

fn count_children(node: &serde_json::Value) -> usize {
    let mut n = 0usize;
    if let Some(children) = node.get("children").and_then(|c| c.as_array()) {
        for child in children {
            n += 1 + count_children(child);
        }
    }
    n
}
