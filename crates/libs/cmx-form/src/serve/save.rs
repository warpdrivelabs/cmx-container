//! html 页面写路径（F3-save）：引擎微服务侧 upsert，落自有 ui-html 资产根。
//!
//! 供设计器保存业务域页：门户按 id 归属把 `POST /api/html-pages` 反代到属主引擎，
//! 引擎把源文件 + v2 分片行写进自己的 assets 工作区（真源），与 serve 读端点同根，
//! 读写天然一致。与门户本地 save（[`crate::pages::html`]，数据根 `html-pages/**`）
//! 的差异只在目录布局；行字段语义一致。
//!
//! 坐标污染防护（与 check-asset-ownership.py 固化同一教训）：id 归属前缀（如
//! `portal.model`）不是业务坐标。行字段三级回退：
//! - `domain/app/module/doc` = 显式入参 > 既有行 > id 命名空间推导（兜底）；
//! - `relPath` = 既有行 > id 推导（更新已有页不迁移源文件路径）。

use std::path::Path;

use serde_json::{Value, json};

use crate::fsutil::{write_json_atomic, write_text_atomic};
use crate::pages::html::{HtmlPageInput, parse_page_namespace};
use crate::util::{is_safe_segment, write_lock};

use super::error::PageServeError;
use super::loader::safe_join;

/// 读 manifest 声明的域清单（`index.json` 缺失 / 坏解析 → 空清单）。
fn read_domains(html_dir: &Path) -> Vec<String> {
    std::fs::read_to_string(html_dir.join("index.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| {
            v.get("domains")
                .and_then(|d| d.as_array())
                .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
        })
        .unwrap_or_default()
}

/// 读某域分片的 pages 数组（分片缺失 / 坏解析 → 空数组）。
fn read_shard_rows(html_dir: &Path, domain: &str) -> Vec<Value> {
    std::fs::read_to_string(html_dir.join("index").join(format!("{domain}.pages.json")))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v.get("pages").and_then(|p| p.as_array()).cloned())
        .unwrap_or_default()
}

/// 跨分片按 id 找既有行，返回（所在分片域, 行 JSON）；未命中返回 `None`。
fn find_row(html_dir: &Path, id: &str) -> Option<(String, Value)> {
    for dom in read_domains(html_dir) {
        let row = read_shard_rows(html_dir, &dom).into_iter().find(|r| {
            r.get("id").and_then(Value::as_str) == Some(id)
        });
        if let Some(row) = row {
            return Some((dom, row));
        }
    }
    None
}

/// 取显式入参（trim 后非空才算显式提供）。
fn explicit(v: &Option<String>) -> Option<String> {
    v.as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
}

/// 保存（upsert）一个 html 页面到 ui-html 资产根。
///
/// 写源文件（`<html_dir>/<relPath>`，原子写）+ 域分片行 upsert + manifest 域维护，
/// 全程持全局写锁串行化。既有行在别的域分片且显式改域时，从旧分片摘除（防双写）。
///
/// # Arguments
///
/// * `html_dir` - html 资产根（manifest `index.json` 所在目录）。
/// * `input` - 保存入参（与门户 save_html_page 同构）。
///
/// # Returns
///
/// 返回写后的行 JSON（`{id,name,details,domain,app,module,page,doc,relPath}`）。
///
/// # Errors
///
/// id 非法 / html 缺失 / domain 段非法 / relPath 越界 → [`PageServeError::BadRequest`]；
/// 落盘失败 → [`PageServeError::Io`]。
pub(crate) async fn save_html_upsert(
    html_dir: &Path,
    input: &HtmlPageInput,
) -> Result<Value, PageServeError> {
    // ① id 校验 + 命名空间推导（复用门户同一解析器，两侧 relPath 推导一致）
    let ns = parse_page_namespace(&input.id)
        .map_err(|e| PageServeError::BadRequest(e.to_string()))?;
    let html = input
        .html
        .clone()
        .ok_or_else(|| PageServeError::BadRequest("html 必须为字符串".into()))?;

    // ② 既有行（更新场景）；字段三级回退见模块文档
    let existing = find_row(html_dir, &ns.id);
    let old_row = existing.as_ref().map(|(_, r)| r.clone());
    let old_field = |k: &str| {
        old_row
            .as_ref()
            .and_then(|r| r.get(k).and_then(Value::as_str))
            .filter(|s| !s.is_empty())
            .map(String::from)
    };

    let domain = explicit(&input.domain)
        .or_else(|| old_field("domain"))
        .unwrap_or_else(|| ns.domain.clone());
    let app = explicit(&input.app)
        .or_else(|| old_field("app"))
        .unwrap_or_else(|| ns.app.clone());
    let module = explicit(&input.module)
        .or_else(|| old_field("module"))
        .unwrap_or_else(|| ns.module.clone());
    let doc = explicit(&input.doc).or_else(|| old_field("doc")).unwrap_or_default();
    let rel_path = old_field("relPath").unwrap_or_else(|| ns.rel_path.clone());
    // 分片文件名由 domain 决定，须为安全段（防分片名注入）
    if !is_safe_segment(&domain) {
        return Err(PageServeError::BadRequest(format!(
            "domain 段非法：\"{domain}\"（仅允许字母、数字、_-）"
        )));
    }

    let row = json!({
        "id": ns.id,
        "name": input.name.clone().unwrap_or_default(),
        "details": input.details.clone().unwrap_or_default(),
        "domain": domain, "app": app, "module": module, "page": ns.page,
        "doc": if doc.is_empty() { Value::Null } else { json!(doc) },
        "relPath": rel_path,
    });

    // ③ 写盘（全局写锁串行化，防并发互踩分片）
    let _guard = write_lock().lock().await;

    // 源文件：relPath 相对 html_dir 安全拼接（防越界）
    let src = safe_join(html_dir, &rel_path)
        .ok_or_else(|| PageServeError::BadRequest(format!("relPath 非法: {rel_path}")))?;
    write_text_atomic(&src, &html)
        .await
        .map_err(|e| PageServeError::Io(format!("写源文件失败 {}: {e}", src.display())))?;

    // 目标分片 upsert
    let shard_path = html_dir.join("index").join(format!("{domain}.pages.json"));
    let mut rows = read_shard_rows(html_dir, &domain);
    match rows
        .iter()
        .position(|r| r.get("id").and_then(Value::as_str) == Some(ns.id.as_str()))
    {
        Some(i) => rows[i] = row.clone(),
        None => rows.push(row.clone()),
    }
    write_json_atomic(
        &shard_path,
        &json!({ "version": 1, "domain": domain, "pages": rows }),
        true,
    )
    .await
    .map_err(|e| PageServeError::Io(format!("写分片失败（域 {domain}）: {e}")))?;

    // 既有行在别的域分片（显式改域）→ 从旧分片摘除，避免同 id 双分片
    if let Some((old_dom, _)) = &existing
        && old_dom != &domain
    {
        let mut old_rows = read_shard_rows(html_dir, old_dom);
        old_rows.retain(|r| r.get("id").and_then(Value::as_str) != Some(ns.id.as_str()));
        write_json_atomic(
            &html_dir.join("index").join(format!("{old_dom}.pages.json")),
            &json!({ "version": 1, "domain": old_dom, "pages": old_rows }),
            true,
        )
        .await
        .map_err(|e| PageServeError::Io(format!("摘除旧分片失败（域 {old_dom}）: {e}")))?;
    }

    // manifest 维护目标域（新域排序去重后落盘）
    let mut domains = read_domains(html_dir);
    if !domains.iter().any(|d| d == &domain) {
        domains.push(domain.clone());
        domains.sort();
        domains.dedup();
        write_json_atomic(
            &html_dir.join("index.json"),
            &json!({ "version": 2, "domains": domains }),
            true,
        )
        .await
        .map_err(|e| PageServeError::Io(format!("写 manifest 失败: {e}")))?;
    }

    Ok(row)
}

// ============================================================================
// 单元测试
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// 最小资产根：manifest（域 fi）+ fi 分片一行（id 命名空间与业务坐标分离的改名页）。
    fn fixture() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("index")).unwrap();
        std::fs::create_dir_all(root.join("fi/cmxfico/gl")).unwrap();
        std::fs::write(
            root.join("index.json"),
            json!({"version":2,"domains":["fi"]}).to_string(),
        )
        .unwrap();
        std::fs::write(
            root.join("index/fi.pages.json"),
            json!({"pages":[{"id":"portal.model.gl.v","name":"凭证","details":"","domain":"fi","app":"cmxfico","module":"gl","doc":null,"relPath":"fi/cmxfico/gl/v.html"}]})
                .to_string(),
        )
        .unwrap();
        std::fs::write(root.join("fi/cmxfico/gl/v.html"), "<html>old</html>").unwrap();
        dir
    }

    fn input(id: &str, html: &str) -> HtmlPageInput {
        HtmlPageInput {
            id: id.into(),
            name: Some("n".into()),
            details: Some("".into()),
            html: Some(html.into()),
            domain: None,
            app: None,
            module: None,
            doc: None,
        }
    }

    #[tokio::test]
    async fn 更新既有页_保留业务坐标与relPath() {
        let dir = fixture();
        let row = save_html_upsert(dir.path(), &input("portal.model.gl.v", "<html>new</html>"))
            .await
            .unwrap();
        // 坐标不随 id 前缀（portal.model）漂移
        assert_eq!(row["domain"], "fi");
        assert_eq!(row["app"], "cmxfico");
        assert_eq!(row["module"], "gl");
        assert_eq!(row["relPath"], "fi/cmxfico/gl/v.html");
        // 源文件在既有路径被覆盖，id 推导路径（portal/model/gl/）不产生新文件
        assert_eq!(
            std::fs::read_to_string(dir.path().join("fi/cmxfico/gl/v.html")).unwrap(),
            "<html>new</html>"
        );
        assert!(!dir.path().join("portal/model/gl/v.html").exists());
    }

    #[tokio::test]
    async fn 新页写入_分片与manifest维护() {
        let dir = fixture();
        let mut inp = input("portal.model.gl.brand-new", "<html>x</html>");
        // 新页显式带业务坐标（设计器侧三级回退的顶层来源）
        inp.domain = Some("cr".into());
        inp.app = Some("explorer".into());
        inp.module = Some("explorer-menu".into());
        let row = save_html_upsert(dir.path(), &inp).await.unwrap();
        assert_eq!(row["domain"], "cr");
        assert_eq!(row["relPath"], "portal/model/gl/brand-new.html");
        // 新域分片落盘 + manifest 增补
        assert!(dir.path().join("index/cr.pages.json").exists());
        let manifest: Value = serde_json::from_str(
            &std::fs::read_to_string(dir.path().join("index.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(manifest["domains"], json!(["cr", "fi"]));
        // 源文件按 id 推导 relPath 落盘
        assert!(dir.path().join("portal/model/gl/brand-new.html").exists());
    }

    #[tokio::test]
    async fn 显式改域_旧分片摘除() {
        let dir = fixture();
        let mut inp = input("portal.model.gl.v", "<html>moved</html>");
        inp.domain = Some("cr".into());
        save_html_upsert(dir.path(), &inp).await.unwrap();
        let fi: Value = serde_json::from_str(
            &std::fs::read_to_string(dir.path().join("index/fi.pages.json")).unwrap(),
        )
        .unwrap();
        assert!(fi["pages"].as_array().unwrap().is_empty());
        let cr: Value = serde_json::from_str(
            &std::fs::read_to_string(dir.path().join("index/cr.pages.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(cr["pages"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn 坏id与缺失html拒绝() {
        let dir = fixture();
        assert!(matches!(
            save_html_upsert(dir.path(), &input("a..b", "<html></html>")).await,
            Err(PageServeError::BadRequest(_))
        ));
        let mut no_html = input("portal.model.gl.v", "");
        no_html.html = None;
        assert!(matches!(
            save_html_upsert(dir.path(), &no_html).await,
            Err(PageServeError::BadRequest(_))
        ));
    }

    #[tokio::test]
    async fn 既有行relPath越界拒绝() {
        let dir = fixture();
        let shard = dir.path().join("index/fi.pages.json");
        let mut v: Value =
            serde_json::from_str(&std::fs::read_to_string(&shard).unwrap()).unwrap();
        v["pages"][0]["relPath"] = json!("../evil.html");
        std::fs::write(&shard, v.to_string()).unwrap();
        assert!(matches!(
            save_html_upsert(dir.path(), &input("portal.model.gl.v", "<html></html>")).await,
            Err(PageServeError::BadRequest(_))
        ));
    }
}
