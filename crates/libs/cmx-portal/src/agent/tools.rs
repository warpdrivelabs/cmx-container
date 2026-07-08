//! agent 工具实现（复刻 `agentRoutes.js` 的 tool* 函数）。
//!
//! 路径穿越保护：所有文件操作限制在 rootDir 内。命令白名单：npm run lint/build -w cmx-portal-manager。

use std::path::{Path, PathBuf};

use serde_json::{Map, Value, json};

use crate::error::{PortalError, PortalResult};

const MAX_FILE_BYTES: u64 = 180_000;
const MAX_COMMAND_OUTPUT: usize = 60_000;
const MAX_PATCH_BYTES: u64 = 120_000;
const MAX_TEXT_REPLACEMENTS: usize = 200;
const TEXT_FILE_EXTS: &[&str] = &["json", "html", "mjs", "cjs", "css", "md", "ts", "js"];
const MAX_GENERIC_OUTPUT: usize = 80_000;

fn bad(msg: impl Into<String>) -> PortalError {
    PortalError::bad_request(msg)
}

/// 把相对路径解析为 rootDir 内的绝对路径，防穿越。
fn resolve_inside_root(root: &Path, input: &str) -> PortalResult<PathBuf> {
    let raw = input.trim();
    if raw.is_empty() {
        return Err(bad("缺少文件路径"));
    }
    let cleaned = raw.trim_start_matches('/');
    let abs = root.join(cleaned);
    // 规范化判断：用 components 消除 .. 后必须仍在 root 下
    let normalized = normalize_path(&abs);
    if normalized != root && !normalized.starts_with(root) {
        return Err(bad("文件路径超出允许范围"));
    }
    Ok(normalized)
}

/// 纯词法路径规范化（不触盘）：消除 `.` / `..`。
fn normalize_path(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in p.components() {
        match comp {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

fn relative_from_root(root: &Path, abs: &Path) -> String {
    abs.strip_prefix(root)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|_| abs.to_string_lossy().to_string())
}

fn has_text_ext(name: &str) -> bool {
    let lower = name.to_lowercase();
    TEXT_FILE_EXTS
        .iter()
        .any(|e| lower.ends_with(&format!(".{e}")))
}

fn opt_str<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

fn app_arg(args: &Value) -> Option<&str> {
    opt_str(args, "app").or_else(|| opt_str(args, "application"))
}

fn limit_arg(args: &Value, default: usize, max: usize) -> usize {
    args.get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(default as u64)
        .clamp(1, max as u64) as usize
}

fn fc_ref_from_args(args: &Value) -> crate::flexible_combination::store::FcRef {
    crate::flexible_combination::store::FcRef {
        domain: opt_str(args, "domain").map(str::to_string),
        app: app_arg(args).map(str::to_string),
        module: opt_str(args, "module").map(str::to_string),
        scenario: opt_str(args, "scenario").map(str::to_string),
    }
}

fn anchor_from_args(args: &Value) -> Map<String, Value> {
    args.get("anchor")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default()
}

fn repo_root(root: &Path) -> PathBuf {
    if root.join("Cargo.toml").exists()
        || root.join("package.json").exists()
        || root.join(".git").exists()
    {
        return root.to_path_buf();
    }
    if root.join("cmx-container").join("Cargo.toml").exists() || root.join("package.json").exists()
    {
        return root.to_path_buf();
    }
    root.parent().unwrap_or(root).to_path_buf()
}

fn cargo_root(root: &Path) -> PathBuf {
    if root.join("Cargo.toml").exists() {
        root.to_path_buf()
    } else if root.join("cmx-container").join("Cargo.toml").exists() {
        root.join("cmx-container")
    } else {
        repo_root(root)
    }
}

fn npm_root(root: &Path) -> PathBuf {
    if root.join("package.json").exists() {
        root.to_path_buf()
    } else if root
        .parent()
        .map(|p| p.join("package.json").exists())
        .unwrap_or(false)
    {
        root.parent().unwrap_or(root).to_path_buf()
    } else {
        repo_root(root)
    }
}

async fn run_process(
    cwd: &Path,
    command: &str,
    argv: &[String],
    timeout_ms: u64,
) -> PortalResult<Value> {
    let timeout_ms = timeout_ms.clamp(1000, 300000);
    let cmd_str = std::iter::once(command.to_string())
        .chain(argv.iter().cloned())
        .collect::<Vec<_>>()
        .join(" ");
    let mut cmd = tokio::process::Command::new(command);
    cmd.args(argv)
        .current_dir(cwd)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let child = cmd.spawn();
    let output = match child {
        Ok(c) => match tokio::time::timeout(
            std::time::Duration::from_millis(timeout_ms),
            c.wait_with_output(),
        )
        .await
        {
            Ok(Ok(o)) => o,
            Ok(Err(e)) => {
                return Ok(
                    json!({ "command": cmd_str, "cwd": cwd, "exitCode": 1, "stdout": "", "stderr": e.to_string(), "timedOut": false }),
                );
            }
            Err(_) => {
                return Ok(
                    json!({ "command": cmd_str, "cwd": cwd, "exitCode": 1, "stdout": "", "stderr": "命令执行超时", "timedOut": true }),
                );
            }
        },
        Err(e) => {
            return Ok(
                json!({ "command": cmd_str, "cwd": cwd, "exitCode": 1, "stdout": "", "stderr": e.to_string(), "timedOut": false }),
            );
        }
    };
    Ok(json!({
        "command": cmd_str,
        "cwd": cwd.to_string_lossy(),
        "exitCode": output.status.code().unwrap_or(1),
        "stdout": tail_str(&String::from_utf8_lossy(&output.stdout), MAX_GENERIC_OUTPUT),
        "stderr": tail_str(&String::from_utf8_lossy(&output.stderr), MAX_GENERIC_OUTPUT),
        "timedOut": false,
    }))
}

async fn run_process_with_stdin(
    cwd: &Path,
    command: &str,
    argv: &[String],
    stdin: &str,
    timeout_ms: u64,
) -> PortalResult<Value> {
    use tokio::io::AsyncWriteExt;

    let timeout_ms = timeout_ms.clamp(1000, 300000);
    let cmd_str = std::iter::once(command.to_string())
        .chain(argv.iter().cloned())
        .collect::<Vec<_>>()
        .join(" ");
    let mut cmd = tokio::process::Command::new(command);
    cmd.args(argv)
        .current_dir(cwd)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return Ok(
                json!({ "command": cmd_str, "cwd": cwd, "exitCode": 1, "stdout": "", "stderr": e.to_string(), "timedOut": false }),
            );
        }
    };
    if let Some(mut child_stdin) = child.stdin.take() {
        child_stdin
            .write_all(stdin.as_bytes())
            .await
            .map_err(PortalError::Io)?;
    }
    let output = match tokio::time::timeout(
        std::time::Duration::from_millis(timeout_ms),
        child.wait_with_output(),
    )
    .await
    {
        Ok(Ok(o)) => o,
        Ok(Err(e)) => {
            return Ok(
                json!({ "command": cmd_str, "cwd": cwd, "exitCode": 1, "stdout": "", "stderr": e.to_string(), "timedOut": false }),
            );
        }
        Err(_) => {
            return Ok(
                json!({ "command": cmd_str, "cwd": cwd, "exitCode": 1, "stdout": "", "stderr": "命令执行超时", "timedOut": true }),
            );
        }
    };
    Ok(json!({
        "command": cmd_str,
        "cwd": cwd.to_string_lossy(),
        "exitCode": output.status.code().unwrap_or(1),
        "stdout": tail_str(&String::from_utf8_lossy(&output.stdout), MAX_GENERIC_OUTPUT),
        "stderr": tail_str(&String::from_utf8_lossy(&output.stderr), MAX_GENERIC_OUTPUT),
        "timedOut": false,
    }))
}

// ── lineDiff（复刻 Node lineDiff）─────────────────────────────────

/// 生成简易行级 diff（前后各保留 3 行上下文）。
pub fn line_diff(before: &str, after: &str) -> String {
    let a: Vec<&str> = before.split('\n').collect();
    let b: Vec<&str> = after.split('\n').collect();
    let mut start = 0;
    while start < a.len() && start < b.len() && a[start] == b[start] {
        start += 1;
    }
    let mut end_a = a.len() as isize - 1;
    let mut end_b = b.len() as isize - 1;
    while end_a >= start as isize
        && end_b >= start as isize
        && a[end_a as usize] == b[end_b as usize]
    {
        end_a -= 1;
        end_b -= 1;
    }
    let from = start.saturating_sub(3);
    let to_a = ((end_a + 3).max(0) as usize).min(a.len().saturating_sub(1));
    let to_b = ((end_b + 3).max(0) as usize).min(b.len().saturating_sub(1));
    let mut out: Vec<String> = Vec::new();
    out.push(format!(
        "@@ {},{} -> {},{} @@",
        from + 1,
        (to_a as isize - from as isize + 1).max(0),
        from + 1,
        (to_b as isize - from as isize + 1).max(0)
    ));
    let max = to_a.max(to_b);
    let change_end = (end_a.max(end_b)).max(0) as usize;
    for i in from..=max {
        let old_line = if i <= to_a { Some(a[i]) } else { None };
        let new_line = if i <= to_b { Some(b[i]) } else { None };
        if i < start || i > change_end {
            if let Some(ol) = old_line {
                out.push(format!(" {ol}"));
            }
            continue;
        }
        if old_line == new_line {
            if let Some(ol) = old_line {
                out.push(format!(" {ol}"));
            }
        } else {
            if let Some(ol) = old_line {
                out.push(format!("-{ol}"));
            }
            if let Some(nl) = new_line {
                out.push(format!("+{nl}"));
            }
        }
    }
    out.join("\n")
}

// ── JSON Pointer（复刻 setJsonPointer）────────────────────────────

fn json_pointer_parts(pointer: &str) -> PortalResult<Vec<String>> {
    let p = pointer.trim();
    if !p.starts_with('/') {
        return Err(bad("JSON Pointer 必须以 / 开头"));
    }
    Ok(p.split('/')
        .skip(1)
        .map(|seg| seg.replace("~1", "/").replace("~0", "~"))
        .collect())
}

fn set_json_pointer(doc: &mut Value, pointer: &str, value: Value) -> PortalResult<()> {
    let parts = json_pointer_parts(pointer)?;
    if parts.is_empty() {
        *doc = value;
        return Ok(());
    }
    let mut cur = doc;
    for i in 0..parts.len() - 1 {
        let key = &parts[i];
        let next_is_index = parts[i + 1].chars().all(|c| c.is_ascii_digit());
        match cur {
            Value::Object(obj) => {
                if !obj.contains_key(key) {
                    obj.insert(
                        key.clone(),
                        if next_is_index { json!([]) } else { json!({}) },
                    );
                }
                cur = obj.get_mut(key).unwrap();
            }
            Value::Array(arr) => {
                let idx: usize = key.parse().map_err(|_| {
                    bad(format!(
                        "JSON Pointer 数组下标非法：/{}",
                        parts[..=i].join("/")
                    ))
                })?;
                if idx >= arr.len() {
                    return Err(bad(format!(
                        "JSON Pointer 中间节点不存在：/{}",
                        parts[..=i].join("/")
                    )));
                }
                cur = &mut arr[idx];
            }
            _ => {
                return Err(bad(format!(
                    "JSON Pointer 中间节点不存在：/{}",
                    parts[..=i].join("/")
                )));
            }
        }
    }
    let last = &parts[parts.len() - 1];
    match cur {
        Value::Object(obj) => {
            obj.insert(last.clone(), value);
            Ok(())
        }
        Value::Array(arr) => {
            if last == "-" {
                arr.push(value);
                return Ok(());
            }
            let idx: usize = last
                .parse()
                .map_err(|_| bad(format!("JSON Pointer 父节点不可写：{pointer}")))?;
            if idx < arr.len() {
                arr[idx] = value;
            } else if idx == arr.len() {
                arr.push(value);
            } else {
                return Err(bad(format!("JSON Pointer 数组下标越界：{pointer}")));
            }
            Ok(())
        }
        _ => Err(bad(format!("JSON Pointer 父节点不可写：{pointer}"))),
    }
}

// ── 工具：搜索 ────────────────────────────────────────────────────

/// search_files：优先 ripgrep（若可用），回退目录遍历。
pub async fn search_files(root: &Path, args: &Value) -> PortalResult<Value> {
    let query = args
        .get("query")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if query.is_empty() {
        return Err(bad("search_files 需要 query"));
    }
    let limit = args
        .get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(20)
        .clamp(1, 50) as usize;
    // 直接走目录遍历（不依赖外部 rg，跨平台稳定）
    let results = walk_search(root, &query, limit).await?;
    Ok(Value::Array(results))
}

async fn walk_search(root: &Path, query: &str, limit: usize) -> PortalResult<Vec<Value>> {
    let q = query.to_lowercase();
    let skip = ["node_modules", "dist", ".git", "target"];
    let mut out: Vec<Value> = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if out.len() >= limit {
            break;
        }
        let mut rd = match tokio::fs::read_dir(&dir).await {
            Ok(rd) => rd,
            Err(_) => continue,
        };
        while let Some(entry) = rd.next_entry().await.map_err(PortalError::Io)? {
            if out.len() >= limit {
                break;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if skip.contains(&name.as_str()) {
                continue;
            }
            let ft = entry.file_type().await.map_err(PortalError::Io)?;
            if ft.is_dir() {
                stack.push(entry.path());
            } else if ft.is_file() && has_text_ext(&name) {
                if name.to_lowercase().contains(&q) {
                    out.push(json!({ "file": relative_from_root(root, &entry.path()), "line": 1, "text": "文件名匹配" }));
                    continue;
                }
                let content = match tokio::fs::read_to_string(entry.path()).await {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                if let Some((idx, line)) = content
                    .lines()
                    .enumerate()
                    .find(|(_, l)| l.to_lowercase().contains(&q))
                {
                    let text: String = line.trim().chars().take(240).collect();
                    out.push(json!({ "file": relative_from_root(root, &entry.path()), "line": idx + 1, "text": text }));
                }
            }
        }
    }
    Ok(out)
}

// ── 工具：读文件 / 数据列举 ────────────────────────────────────────

pub async fn read_file(root: &Path, args: &Value) -> PortalResult<Value> {
    let p = resolve_inside_root(
        root,
        args.get("path").and_then(|v| v.as_str()).unwrap_or(""),
    )?;
    let meta = tokio::fs::metadata(&p)
        .await
        .map_err(|_| bad("只能读取文件"))?;
    if !meta.is_file() {
        return Err(bad("只能读取文件"));
    }
    if meta.len() > MAX_FILE_BYTES {
        return Err(bad(format!("文件过大，当前限制 {MAX_FILE_BYTES} bytes")));
    }
    let content = tokio::fs::read_to_string(&p)
        .await
        .map_err(PortalError::Io)?;
    Ok(json!({ "path": relative_from_root(root, &p), "bytes": meta.len(), "content": content }))
}

/// list_definitions：复用 definitions store。
pub async fn list_definitions(args: &Value) -> PortalResult<Value> {
    let kind = args
        .get("kind")
        .and_then(|v| v.as_str())
        .map(|s| s.to_uppercase());
    let domain = args.get("domain").and_then(|v| v.as_str());
    let module = args.get("module").and_then(|v| v.as_str());
    let limit = args
        .get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(60)
        .clamp(1, 100) as usize;
    let items =
        crate::definitions::store::list_definitions(kind.as_deref(), domain, None, module).await?;
    Ok(Value::Array(items.into_iter().take(limit).collect()))
}

/// list_html_pages：复用 html store（裁剪字段）。
pub async fn list_html_pages(args: &Value) -> PortalResult<Value> {
    let page = args
        .get("page")
        .and_then(|v| v.as_i64())
        .unwrap_or(1)
        .max(1);
    let page_size = args
        .get("pageSize")
        .or_else(|| args.get("limit"))
        .and_then(|v| v.as_i64())
        .unwrap_or(20)
        .clamp(1, 50);
    let domain = args.get("domain").and_then(|v| v.as_str());
    let app = args.get("app").and_then(|v| v.as_str());
    let module = args.get("module").and_then(|v| v.as_str());
    let out =
        crate::pages::html::list_html_pages_paged(Some(page), Some(page_size), domain, app, module)
            .await?;
    // 裁剪 items 字段
    let items: Vec<Value> = out
        .get("items")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .map(|it| {
                    json!({
                        "id": it.get("id"), "name": it.get("name"), "details": it.get("details"),
                        "domain": it.get("domain"), "app": it.get("app"), "module": it.get("module"),
                        "relPath": it.get("relPath"),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let mut res = out;
    res.as_object_mut()
        .unwrap()
        .insert("items".to_string(), Value::Array(items));
    Ok(res)
}

/// read_html_page：复用 html store（截断 html 到 24000 字符）。
pub async fn read_html_page(args: &Value) -> PortalResult<Value> {
    let id = args
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    if id.is_empty() {
        return Err(bad("read_html_page 需要 id"));
    }
    let page = crate::pages::html::get_html_page_by_id(&id).await?;
    let html = page.get("html").and_then(|v| v.as_str()).unwrap_or("");
    let bytes = html.len();
    let truncated: String = html.chars().take(24000).collect();
    Ok(json!({
        "id": page.get("id"), "name": page.get("name"), "details": page.get("details"),
        "domain": page.get("domain"), "app": page.get("app"), "module": page.get("module"),
        "relPath": page.get("relPath"), "bytes": bytes, "html": truncated,
    }))
}

// ── 工具：门户业务查询 ──────────────────────────────────────────────

pub async fn list_modules_tool(args: &Value) -> PortalResult<Value> {
    let limit = limit_arg(args, 80, 200);
    let items =
        crate::meta::modules::list_module_manifests(opt_str(args, "domain"), app_arg(args)).await?;
    Ok(json!({ "items": items.into_iter().take(limit).collect::<Vec<_>>() }))
}

pub async fn get_module_manifest_tool(args: &Value) -> PortalResult<Value> {
    let domain = opt_str(args, "domain").ok_or_else(|| bad("get_module_manifest 需要 domain"))?;
    let app = app_arg(args).ok_or_else(|| bad("get_module_manifest 需要 app/application"))?;
    let module = opt_str(args, "module").ok_or_else(|| bad("get_module_manifest 需要 module"))?;
    crate::meta::modules::load_module_manifest(domain, app, module).await
}

pub async fn get_module_resource_tool(args: &Value) -> PortalResult<Value> {
    let domain = opt_str(args, "domain").ok_or_else(|| bad("get_module_resource 需要 domain"))?;
    let app = app_arg(args).ok_or_else(|| bad("get_module_resource 需要 app/application"))?;
    let module = opt_str(args, "module").ok_or_else(|| bad("get_module_resource 需要 module"))?;
    let res_type = opt_str(args, "type").ok_or_else(|| bad("get_module_resource 需要 type"))?;
    crate::meta::modules::resolve_module_resource(domain, app, module, res_type).await
}

pub async fn list_dict_schemas_tool(args: &Value) -> PortalResult<Value> {
    let limit = limit_arg(args, 200, 500);
    let schemas = crate::dict::schema::list_schemas_json().await?;
    let items = schemas
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .take(limit)
        .collect::<Vec<_>>();
    Ok(json!({ "schemas": items }))
}

pub async fn dict_search_tool(args: &Value) -> PortalResult<Value> {
    let dict_id = opt_str(args, "dictId").ok_or_else(|| bad("dict_search 需要 dictId"))?;
    let mut body = args
        .get("body")
        .filter(|v| v.is_object())
        .cloned()
        .unwrap_or_else(|| json!({}));
    if let Some(q) = opt_str(args, "q").or_else(|| opt_str(args, "query")) {
        body.as_object_mut()
            .unwrap()
            .insert("q".to_string(), json!(q));
    }
    if let Some(limit) = args.get("limit").and_then(|v| v.as_u64()) {
        body.as_object_mut()
            .unwrap()
            .insert("limit".to_string(), json!(limit));
    }
    crate::dict::api::search_endpoint(dict_id, &body).await
}

pub async fn dict_suggest_tool(args: &Value) -> PortalResult<Value> {
    let dict_id = opt_str(args, "dictId").ok_or_else(|| bad("dict_suggest 需要 dictId"))?;
    crate::dict::api::suggest_endpoint(dict_id, opt_str(args, "q").unwrap_or("")).await
}

pub async fn list_facts_tool(args: &Value) -> PortalResult<Value> {
    let limit = limit_arg(args, 100, 500);
    let q = crate::fact::store::FactQuery {
        domain: opt_str(args, "domain").map(str::to_string),
        app: app_arg(args).map(str::to_string),
        module: opt_str(args, "module").map(str::to_string),
    };
    let items = crate::fact::store::list_facts(&q).await?;
    let values = items
        .into_iter()
        .take(limit)
        .map(|x| serde_json::to_value(x).unwrap_or(Value::Null))
        .collect::<Vec<_>>();
    Ok(json!({ "items": values }))
}

pub async fn get_fact_tool(args: &Value) -> PortalResult<Value> {
    let r = crate::fact::store::FactRef {
        domain: opt_str(args, "domain")
            .ok_or_else(|| bad("get_fact 需要 domain"))?
            .to_string(),
        app: app_arg(args)
            .ok_or_else(|| bad("get_fact 需要 app/application"))?
            .to_string(),
        module: opt_str(args, "module")
            .ok_or_else(|| bad("get_fact 需要 module"))?
            .to_string(),
        file: opt_str(args, "file")
            .ok_or_else(|| bad("get_fact 需要 file"))?
            .to_string(),
    };
    crate::fact::store::get_fact(&r).await
}

pub async fn service_catalog_list_tool(args: &Value) -> PortalResult<Value> {
    let limit = limit_arg(args, 80, 200);
    let services = crate::service_catalog::store::list_services(
        opt_str(args, "domain"),
        app_arg(args),
        opt_str(args, "module"),
    )
    .await?;
    Ok(json!({ "services": services.into_iter().take(limit).collect::<Vec<_>>() }))
}

pub async fn service_catalog_get_tool(args: &Value) -> PortalResult<Value> {
    let id = opt_str(args, "id").ok_or_else(|| bad("service_catalog_get 需要 id"))?;
    match crate::service_catalog::store::get_service_by_id(id).await? {
        Some(svc) => Ok(svc),
        None => Err(PortalError::not_found(format!("服务不存在：{id}"))),
    }
}

pub async fn flexible_combination_list_tool(args: &Value) -> PortalResult<Value> {
    let limit = limit_arg(args, 80, 200);
    let items = crate::flexible_combination::store::list_flexible_combinations(
        opt_str(args, "domain"),
        app_arg(args),
        opt_str(args, "module"),
    )
    .await?;
    Ok(json!({ "items": items.into_iter().take(limit).collect::<Vec<_>>() }))
}

pub async fn flexible_combination_get_tool(args: &Value) -> PortalResult<Value> {
    crate::flexible_combination::store::get_flexible_combination(&fc_ref_from_args(args)).await
}

pub async fn flexible_combination_validate_tool(args: &Value) -> PortalResult<Value> {
    let body = args.get("combination").cloned().unwrap_or_else(|| json!({}));
    crate::flexible_combination::api::validate(&body, &fc_ref_from_args(args)).await
}

pub async fn flexible_combination_preview_tool(args: &Value) -> PortalResult<Value> {
    let mut body = args.get("combination").cloned().unwrap_or_else(|| json!({}));
    if let Some(anchor) = args.get("anchor").filter(|v| v.is_object()) {
        if !body.is_object() {
            body = json!({});
        }
        body.as_object_mut()
            .unwrap()
            .insert("anchor".to_string(), anchor.clone());
    }
    crate::flexible_combination::api::preview(&body, &fc_ref_from_args(args)).await
}

pub async fn flexible_combination_resolve_tool(args: &Value) -> PortalResult<Value> {
    crate::flexible_combination::api::resolve(&fc_ref_from_args(args), &anchor_from_args(args)).await
}

pub async fn flexible_combination_rule_tool(args: &Value) -> PortalResult<Value> {
    crate::flexible_combination::api::rule(&fc_ref_from_args(args), &anchor_from_args(args)).await
}

/// validate_metadata：递归校验 JSON 可解析性。
pub async fn validate_metadata(root: &Path, args: &Value) -> PortalResult<Value> {
    let target = match args.get("path").and_then(|v| v.as_str()) {
        Some(p) if !p.trim().is_empty() => resolve_inside_root(root, p)?,
        _ => root
            .join("cmx-node-server")
            .join("data")
            .join("meta")
            .join("definitions"),
    };
    let mut files: Vec<PathBuf> = Vec::new();
    let mut stack = vec![target];
    while let Some(p) = stack.pop() {
        let meta = match tokio::fs::metadata(&p).await {
            Ok(m) => m,
            Err(_) => continue,
        };
        if meta.is_file() {
            if p.extension().and_then(|e| e.to_str()) == Some("json") {
                files.push(p);
            }
        } else if meta.is_dir() {
            let mut rd = match tokio::fs::read_dir(&p).await {
                Ok(rd) => rd,
                Err(_) => continue,
            };
            while let Some(entry) = rd.next_entry().await.map_err(PortalError::Io)? {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') {
                    continue;
                }
                stack.push(entry.path());
            }
        }
    }
    let mut diagnostics: Vec<Value> = Vec::new();
    for file in files.iter().take(200) {
        if let Ok(content) = tokio::fs::read_to_string(file).await
            && let Err(e) = serde_json::from_str::<Value>(&content) {
                diagnostics.push(
                    json!({ "file": relative_from_root(root, file), "error": e.to_string() }),
                );
            }
    }
    Ok(json!({ "checked": files.len(), "errors": diagnostics }))
}

// ── 工具：Git / 插件发现 ───────────────────────────────────────────

pub async fn git_status_tool(root: &Path, _args: &Value) -> PortalResult<Value> {
    run_process(
        &repo_root(root),
        "git",
        &["status".to_string(), "--short".to_string()],
        30_000,
    )
    .await
}

pub async fn git_diff_tool(root: &Path, args: &Value) -> PortalResult<Value> {
    let mut argv = vec!["diff".to_string()];
    if args
        .get("staged")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
    {
        argv.push("--staged".to_string());
    }
    if let Some(path) = opt_str(args, "path") {
        // 只允许 repo 内相对路径作为 pathspec。
        let _ = resolve_inside_root(root, path)?;
        argv.push("--".to_string());
        argv.push(path.trim_start_matches('/').to_string());
    }
    let mut out = run_process(&repo_root(root), "git", &argv, 30_000).await?;
    let max = args
        .get("maxBytes")
        .and_then(|v| v.as_u64())
        .unwrap_or(80_000)
        .clamp(1_000, 200_000) as usize;
    if let Some(stdout) = out
        .get("stdout")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
    {
        out.as_object_mut()
            .unwrap()
            .insert("stdout".to_string(), json!(tail_str(&stdout, max)));
    }
    Ok(out)
}

pub async fn git_log_tool(root: &Path, args: &Value) -> PortalResult<Value> {
    let limit = limit_arg(args, 10, 50).to_string();
    run_process(
        &repo_root(root),
        "git",
        &[
            "log".to_string(),
            "--oneline".to_string(),
            "-n".to_string(),
            limit,
        ],
        30_000,
    )
    .await
}

pub async fn list_local_plugins(root: &Path, args: &Value) -> PortalResult<Value> {
    let limit = limit_arg(args, 80, 300);
    let mut items = Vec::new();
    let mut stack = vec![repo_root(root)];
    let skip = ["target", "node_modules", ".git", "dist"];
    while let Some(dir) = stack.pop() {
        if items.len() >= limit {
            break;
        }
        let mut rd = match tokio::fs::read_dir(&dir).await {
            Ok(rd) => rd,
            Err(_) => continue,
        };
        while let Some(entry) = rd.next_entry().await.map_err(PortalError::Io)? {
            if items.len() >= limit {
                break;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if skip.contains(&name.as_str()) {
                continue;
            }
            let ft = entry.file_type().await.map_err(PortalError::Io)?;
            if ft.is_dir() {
                stack.push(entry.path());
                continue;
            }
            if !ft.is_file() || name != "manifest.json" {
                continue;
            }
            let path = entry.path();
            let content = match tokio::fs::read_to_string(&path).await {
                Ok(c) => c,
                Err(_) => continue,
            };
            let doc: Value = serde_json::from_str(&content).unwrap_or(Value::Null);
            let parent = path.parent().unwrap_or(&path);
            let has_mcpdata = parent.join("mcpdata").exists();
            let has_agents = parent.join(".agents").exists();
            items.push(json!({
                "path": relative_from_root(root, &path),
                "pluginId": doc.get("plugin_id").or_else(|| doc.get("pluginId")).or_else(|| doc.get("id")),
                "name": doc.get("name"),
                "version": doc.get("version"),
                "hasMcpData": has_mcpdata,
                "hasAgents": has_agents,
            }));
        }
    }
    Ok(json!({ "plugins": items }))
}

pub async fn inspect_plugin_manifest(root: &Path, args: &Value) -> PortalResult<Value> {
    if let Some(path) = opt_str(args, "path") {
        return read_file(root, &json!({ "path": path }))
            .await
            .and_then(|v| {
                let content = v.get("content").and_then(|v| v.as_str()).unwrap_or("");
                let manifest: Value = serde_json::from_str(content)?;
                Ok(json!({ "path": v.get("path"), "manifest": manifest }))
            });
    }
    let plugin_id = opt_str(args, "pluginId")
        .ok_or_else(|| bad("inspect_plugin_manifest 需要 path 或 pluginId"))?;
    let list = list_local_plugins(root, &json!({ "limit": 300 })).await?;
    let hit = list
        .get("plugins")
        .and_then(|v| v.as_array())
        .and_then(|arr| {
            arr.iter().find(|p| {
                p.get("pluginId")
                    .map(|v| {
                        v.as_str().map(|s| s == plugin_id).unwrap_or_else(|| {
                            v.get("plugin_id").and_then(|x| x.as_str()) == Some(plugin_id)
                        })
                    })
                    .unwrap_or(false)
            })
        })
        .and_then(|p| p.get("path").and_then(|v| v.as_str()))
        .ok_or_else(|| PortalError::not_found(format!("未找到插件 manifest：{plugin_id}")))?;
    read_file(root, &json!({ "path": hit }))
        .await
        .and_then(|v| {
            let content = v.get("content").and_then(|v| v.as_str()).unwrap_or("");
            let manifest: Value = serde_json::from_str(content)?;
            Ok(json!({ "path": v.get("path"), "manifest": manifest }))
        })
}

pub async fn call_plugin_function_tool(_root: &Path, args: &Value) -> PortalResult<Value> {
    let plugin_id =
        opt_str(args, "pluginId").ok_or_else(|| bad("call_plugin_function 需要 pluginId"))?;
    let function_name = opt_str(args, "functionName")
        .ok_or_else(|| bad("call_plugin_function 需要 functionName"))?;
    Ok(json!({
        "configured": false,
        "pluginId": plugin_id,
        "functionName": function_name,
        "serviceName": opt_str(args, "serviceName"),
        "input": args.get("input").cloned().unwrap_or(Value::Null),
        "message": "cmx-portal Agent 工具已声明该能力，但当前 crate 未持有 RuntimeInvoker/ServiceOrchestrationClient 句柄；需要在 API/AppState 层注入运行时桥接后启用真实调用。",
    }))
}

pub async fn call_service_flow_tool(_root: &Path, args: &Value) -> PortalResult<Value> {
    let service_key =
        opt_str(args, "serviceKey").ok_or_else(|| bad("call_service_flow 需要 serviceKey"))?;
    Ok(json!({
        "configured": false,
        "serviceKey": service_key,
        "serviceName": opt_str(args, "serviceName"),
        "input": args.get("input").cloned().unwrap_or(Value::Null),
        "message": "cmx-portal Agent 工具已声明该能力，但当前 crate 未持有服务编排客户端；需要在 API/AppState 层注入 ServiceOrchestrationClient 后启用真实调用。",
    }))
}

pub async fn generate_api_doc_tool(_root: &Path, args: &Value) -> PortalResult<Value> {
    Ok(json!({
        "configured": false,
        "pluginId": opt_str(args, "pluginId"),
        "version": opt_str(args, "version"),
        "hasOrchestration": args.get("orchestration").is_some(),
        "installPath": opt_str(args, "installPath"),
        "message": "cmx-plugin::ApiDocGenerator 需要 PluginQuery、插件安装根目录和 ServiceOrchestration 上下文；当前 agent 工具入口已预留，后续应在持有插件管理器的层完成桥接。",
    }))
}

// ── 补丁预览/应用 ────────────────────────────────────────────────

/// 文本替换补丁预览（不写盘）。
pub async fn prepare_text_replace(root: &Path, args: &Value) -> PortalResult<Value> {
    let p = resolve_inside_root(
        root,
        args.get("path").and_then(|v| v.as_str()).unwrap_or(""),
    )?;
    let meta = tokio::fs::metadata(&p)
        .await
        .map_err(|_| bad("只能修改文件"))?;
    if !meta.is_file() {
        return Err(bad("只能修改文件"));
    }
    if meta.len() > MAX_PATCH_BYTES {
        return Err(bad(format!(
            "文件过大，当前补丁限制 {MAX_PATCH_BYTES} bytes"
        )));
    }
    let old_text = args
        .get("oldText")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let new_text = args
        .get("newText")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if old_text.is_empty() {
        return Err(bad("文本替换补丁需要 oldText"));
    }
    let before = tokio::fs::read_to_string(&p)
        .await
        .map_err(PortalError::Io)?;
    let count = before.matches(&old_text).count();
    if count == 0 {
        return Err(bad("未找到要替换的文本"));
    }
    let occurrence = if args.get("occurrence").and_then(|v| v.as_str()) == Some("all") {
        "all"
    } else {
        "first"
    };
    if occurrence == "all" && count > MAX_TEXT_REPLACEMENTS {
        return Err(bad(format!(
            "匹配过多，当前限制 {MAX_TEXT_REPLACEMENTS} 处"
        )));
    }
    let replacements = if occurrence == "all" {
        count.min(MAX_TEXT_REPLACEMENTS)
    } else {
        1
    };
    let after = if occurrence == "all" {
        before.replace(&old_text, &new_text)
    } else {
        before.replacen(&old_text, &new_text, 1)
    };
    Ok(json!({
        "path": relative_from_root(root, &p), "oldText": old_text, "newText": new_text,
        "occurrence": occurrence, "replacements": replacements,
        "before": before, "after": after, "diff": line_diff(&before, &after),
    }))
}

/// 应用文本替换补丁（写盘）。
pub async fn apply_text_replace(root: &Path, args: &Value) -> PortalResult<Value> {
    let preview = prepare_text_replace(root, args).await?;
    let rel = preview.get("path").and_then(|v| v.as_str()).unwrap_or("");
    let after = preview.get("after").and_then(|v| v.as_str()).unwrap_or("");
    let abs = resolve_inside_root(root, rel)?;
    tokio::fs::write(&abs, after)
        .await
        .map_err(PortalError::Io)?;
    Ok(json!({
        "path": rel, "occurrence": preview.get("occurrence"), "replacements": preview.get("replacements"),
        "bytes": after.len(), "diff": preview.get("diff"),
    }))
}

/// JSON 补丁预览。
pub async fn prepare_json_patch(root: &Path, args: &Value) -> PortalResult<Value> {
    let p = resolve_inside_root(
        root,
        args.get("path").and_then(|v| v.as_str()).unwrap_or(""),
    )?;
    let meta = tokio::fs::metadata(&p)
        .await
        .map_err(|_| bad("只能修改文件"))?;
    if !meta.is_file() {
        return Err(bad("只能修改文件"));
    }
    if meta.len() > MAX_PATCH_BYTES {
        return Err(bad(format!(
            "文件过大，当前补丁限制 {MAX_PATCH_BYTES} bytes"
        )));
    }
    let before = tokio::fs::read_to_string(&p)
        .await
        .map_err(PortalError::Io)?;
    let mut doc: Value =
        serde_json::from_str(&before).map_err(|_| bad("当前仅支持可解析的 JSON 文件补丁"))?;
    let pointer = args.get("pointer").and_then(|v| v.as_str()).unwrap_or("");
    let value = args.get("value").cloned().unwrap_or(Value::Null);
    set_json_pointer(&mut doc, pointer, value.clone())?;
    let after = format!("{}\n", serde_json::to_string_pretty(&doc)?);
    Ok(json!({
        "path": relative_from_root(root, &p), "pointer": pointer, "value": value,
        "before": before, "after": after, "diff": line_diff(&before, &after),
    }))
}

/// 应用 JSON 补丁（写盘）。
pub async fn apply_json_patch(root: &Path, args: &Value) -> PortalResult<Value> {
    let preview = prepare_json_patch(root, args).await?;
    let rel = preview.get("path").and_then(|v| v.as_str()).unwrap_or("");
    let after = preview.get("after").and_then(|v| v.as_str()).unwrap_or("");
    let abs = resolve_inside_root(root, rel)?;
    tokio::fs::write(&abs, after)
        .await
        .map_err(PortalError::Io)?;
    Ok(
        json!({ "path": rel, "pointer": preview.get("pointer"), "bytes": after.len(), "diff": preview.get("diff") }),
    )
}

/// 创建文本文件。
pub async fn create_file(root: &Path, args: &Value) -> PortalResult<Value> {
    let path = opt_str(args, "path").ok_or_else(|| bad("create_file 需要 path"))?;
    let abs = resolve_inside_root(root, path)?;
    if tokio::fs::metadata(&abs).await.is_ok()
        && !args
            .get("overwrite")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    {
        return Err(bad("目标文件已存在；如需覆盖请设置 overwrite=true"));
    }
    if let Some(parent) = abs.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(PortalError::Io)?;
    }
    let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");
    if content.len() as u64 > MAX_PATCH_BYTES {
        return Err(bad(format!(
            "文件内容过大，当前限制 {MAX_PATCH_BYTES} bytes"
        )));
    }
    tokio::fs::write(&abs, content)
        .await
        .map_err(PortalError::Io)?;
    Ok(json!({ "path": relative_from_root(root, &abs), "bytes": content.len(), "created": true }))
}

/// 重命名/移动文件。
pub async fn rename_file(root: &Path, args: &Value) -> PortalResult<Value> {
    let from = resolve_inside_root(
        root,
        opt_str(args, "from").ok_or_else(|| bad("rename_file 需要 from"))?,
    )?;
    let to = resolve_inside_root(
        root,
        opt_str(args, "to").ok_or_else(|| bad("rename_file 需要 to"))?,
    )?;
    let meta = tokio::fs::metadata(&from)
        .await
        .map_err(|_| bad("源文件不存在"))?;
    if !meta.is_file() {
        return Err(bad("当前仅允许移动文件"));
    }
    if tokio::fs::metadata(&to).await.is_ok() {
        return Err(bad("目标路径已存在"));
    }
    if let Some(parent) = to.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(PortalError::Io)?;
    }
    tokio::fs::rename(&from, &to)
        .await
        .map_err(PortalError::Io)?;
    Ok(
        json!({ "from": relative_from_root(root, &from), "to": relative_from_root(root, &to), "renamed": true }),
    )
}

/// 应用 unified diff patch（stdin 传给 git apply）。
pub async fn apply_file_patch(root: &Path, args: &Value) -> PortalResult<Value> {
    let patch = args.get("patch").and_then(|v| v.as_str()).unwrap_or("");
    if patch.trim().is_empty() {
        return Err(bad("apply_file_patch 需要 patch"));
    }
    if patch.len() as u64 > MAX_PATCH_BYTES * 4 {
        return Err(bad("patch 过大"));
    }
    let cwd = repo_root(root);
    let check = run_process_with_stdin(
        &cwd,
        "git",
        &["apply".to_string(), "--check".to_string()],
        patch,
        60_000,
    )
    .await?;
    if check.get("exitCode").and_then(|v| v.as_i64()) != Some(0) {
        return Ok(json!({ "applied": false, "check": check }));
    }
    let result = run_process_with_stdin(
        &cwd,
        "git",
        &["apply".to_string(), "--whitespace=nowarn".to_string()],
        patch,
        60_000,
    )
    .await?;
    Ok(
        json!({ "applied": result.get("exitCode").and_then(|v| v.as_i64()) == Some(0), "check": check, "result": result }),
    )
}

/// 格式化单个文件。
pub async fn format_file(root: &Path, args: &Value) -> PortalResult<Value> {
    let path = opt_str(args, "path").ok_or_else(|| bad("format_file 需要 path"))?;
    let abs = resolve_inside_root(root, path)?;
    let ext = abs.extension().and_then(|e| e.to_str()).unwrap_or("");
    let timeout_ms = args
        .get("timeoutMs")
        .and_then(|v| v.as_u64())
        .unwrap_or(60_000);
    if ext == "rs" {
        run_process(
            &cargo_root(root),
            "rustfmt",
            &[abs.to_string_lossy().to_string()],
            timeout_ms,
        )
        .await
    } else if ["js", "ts", "json", "css", "html", "md"].contains(&ext) {
        run_process(
            &npm_root(root),
            "npx",
            &[
                "prettier".to_string(),
                "--write".to_string(),
                abs.to_string_lossy().to_string(),
            ],
            timeout_ms,
        )
        .await
    } else {
        Err(bad(format!("暂不支持格式化 .{ext} 文件")))
    }
}

pub async fn cargo_check(root: &Path, args: &Value) -> PortalResult<Value> {
    let mut argv = vec!["check".to_string()];
    if let Some(pkg) = opt_str(args, "package") {
        argv.extend(["-p".to_string(), pkg.to_string()]);
    }
    run_process(
        &cargo_root(root),
        "cargo",
        &argv,
        args.get("timeoutMs")
            .and_then(|v| v.as_u64())
            .unwrap_or(120_000),
    )
    .await
}

pub async fn cargo_build(root: &Path, args: &Value) -> PortalResult<Value> {
    let mut argv = vec!["build".to_string()];
    if let Some(pkg) = opt_str(args, "package") {
        argv.extend(["-p".to_string(), pkg.to_string()]);
    }
    run_process(
        &cargo_root(root),
        "cargo",
        &argv,
        args.get("timeoutMs")
            .and_then(|v| v.as_u64())
            .unwrap_or(180_000),
    )
    .await
}

pub async fn cargo_test(root: &Path, args: &Value) -> PortalResult<Value> {
    let mut argv = vec!["test".to_string()];
    if let Some(pkg) = opt_str(args, "package") {
        argv.extend(["-p".to_string(), pkg.to_string()]);
    }
    if let Some(test) = opt_str(args, "test") {
        argv.push(test.to_string());
    }
    run_process(
        &cargo_root(root),
        "cargo",
        &argv,
        args.get("timeoutMs")
            .and_then(|v| v.as_u64())
            .unwrap_or(180_000),
    )
    .await
}

pub async fn cargo_clippy(root: &Path, args: &Value) -> PortalResult<Value> {
    let mut argv = vec!["clippy".to_string()];
    if let Some(pkg) = opt_str(args, "package") {
        argv.extend(["-p".to_string(), pkg.to_string()]);
    }
    argv.extend(["--".to_string(), "-D".to_string(), "warnings".to_string()]);
    run_process(
        &cargo_root(root),
        "cargo",
        &argv,
        args.get("timeoutMs")
            .and_then(|v| v.as_u64())
            .unwrap_or(180_000),
    )
    .await
}

pub async fn npm_test(root: &Path, args: &Value) -> PortalResult<Value> {
    let mut argv = vec!["test".to_string()];
    if let Some(workspace) = opt_str(args, "workspace") {
        argv.extend(["-w".to_string(), workspace.to_string()]);
    }
    run_process(
        &npm_root(root),
        "npm",
        &argv,
        args.get("timeoutMs")
            .and_then(|v| v.as_u64())
            .unwrap_or(120_000),
    )
    .await
}

pub async fn npm_build_workspace(root: &Path, args: &Value) -> PortalResult<Value> {
    let script = opt_str(args, "script").unwrap_or("build");
    if ![
        "build",
        "build:runtime",
        "build:portal",
        "build:html",
        "build:apps",
    ]
    .contains(&script)
    {
        return Err(bad("npm_build_workspace 仅允许 build/build:* 预置脚本"));
    }
    let mut argv = vec!["run".to_string(), script.to_string()];
    if let Some(workspace) = opt_str(args, "workspace") {
        argv.extend(["-w".to_string(), workspace.to_string()]);
    }
    run_process(
        &npm_root(root),
        "npm",
        &argv,
        args.get("timeoutMs")
            .and_then(|v| v.as_u64())
            .unwrap_or(180_000),
    )
    .await
}

pub async fn run_playwright(root: &Path, args: &Value) -> PortalResult<Value> {
    let mut argv = vec!["playwright".to_string(), "test".to_string()];
    if let Some(project) = opt_str(args, "project") {
        argv.extend(["--project".to_string(), project.to_string()]);
    }
    if let Some(grep) = opt_str(args, "grep") {
        argv.extend(["--grep".to_string(), grep.to_string()]);
    }
    run_process(
        &npm_root(root),
        "npx",
        &argv,
        args.get("timeoutMs")
            .and_then(|v| v.as_u64())
            .unwrap_or(180_000),
    )
    .await
}

pub async fn capture_page_screenshot(root: &Path, args: &Value) -> PortalResult<Value> {
    let url = opt_str(args, "url").ok_or_else(|| bad("capture_page_screenshot 需要 url"))?;
    let output = opt_str(args, "output").unwrap_or("agent-screenshot.png");
    let out_abs = resolve_inside_root(root, output)?;
    if let Some(parent) = out_abs.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(PortalError::Io)?;
    }
    let script = r#"
const { chromium } = require('playwright');
(async () => {
  const [url, output] = process.argv.slice(1);
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
  await page.goto(url, { waitUntil: 'networkidle', timeout: 30000 });
  await page.screenshot({ path: output, fullPage: true });
  await browser.close();
})().catch((e) => { console.error(e && e.stack || e); process.exit(1); });
"#;
    let res = run_process(
        &npm_root(root),
        "node",
        &[
            "-e".to_string(),
            script.to_string(),
            url.to_string(),
            out_abs.to_string_lossy().to_string(),
        ],
        args.get("timeoutMs")
            .and_then(|v| v.as_u64())
            .unwrap_or(60_000),
    )
    .await?;
    Ok(json!({ "output": relative_from_root(root, &out_abs), "result": res }))
}

pub async fn inspect_dom(root: &Path, args: &Value) -> PortalResult<Value> {
    let url = opt_str(args, "url").ok_or_else(|| bad("inspect_dom 需要 url"))?;
    let selector = opt_str(args, "selector").unwrap_or("body");
    let script = r#"
const { chromium } = require('playwright');
(async () => {
  const [url, selector] = process.argv.slice(1);
  const browser = await chromium.launch();
  const page = await browser.newPage();
  await page.goto(url, { waitUntil: 'networkidle', timeout: 30000 });
  const title = await page.title();
  const text = await page.locator(selector).first().innerText({ timeout: 5000 }).catch(() => '');
  console.log(JSON.stringify({ title, selector, text: text.slice(0, 12000) }));
  await browser.close();
})().catch((e) => { console.error(e && e.stack || e); process.exit(1); });
"#;
    run_process(
        &npm_root(root),
        "node",
        &[
            "-e".to_string(),
            script.to_string(),
            url.to_string(),
            selector.to_string(),
        ],
        args.get("timeoutMs")
            .and_then(|v| v.as_u64())
            .unwrap_or(60_000),
    )
    .await
}

pub async fn check_accessibility(root: &Path, args: &Value) -> PortalResult<Value> {
    let url = opt_str(args, "url").unwrap_or("");
    if url.is_empty() {
        return run_playwright(root, &json!({ "grep": "accessibility", "timeoutMs": args.get("timeoutMs").cloned().unwrap_or(json!(180000)) })).await;
    }
    let script = r#"
const { chromium } = require('playwright');
(async () => {
  const [url] = process.argv.slice(1);
  const browser = await chromium.launch();
  const page = await browser.newPage();
  await page.goto(url, { waitUntil: 'networkidle', timeout: 30000 });
  const result = await page.evaluate(() => {
    const imgsMissingAlt = [...document.images].filter((img) => !img.hasAttribute('alt')).length;
    const unnamedButtons = [...document.querySelectorAll('button,[role="button"]')].filter((el) => !(el.textContent || '').trim() && !el.getAttribute('aria-label') && !el.getAttribute('title')).length;
    const inputsMissingLabel = [...document.querySelectorAll('input,textarea,select')].filter((el) => !el.id || !document.querySelector(`label[for="${CSS.escape(el.id)}"]`)).length;
    return { imgsMissingAlt, unnamedButtons, inputsMissingLabel };
  });
  console.log(JSON.stringify(result));
  await browser.close();
})().catch((e) => { console.error(e && e.stack || e); process.exit(1); });
"#;
    run_process(
        &npm_root(root),
        "node",
        &["-e".to_string(), script.to_string(), url.to_string()],
        args.get("timeoutMs")
            .and_then(|v| v.as_u64())
            .unwrap_or(60_000),
    )
    .await
}

// ── run_command（白名单）─────────────────────────────────────────

/// 命令白名单校验，返回 (command, args)。
fn normalize_command(args: &Value) -> PortalResult<(String, Vec<String>)> {
    let command = args
        .get("command")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let argv: Vec<String> = args
        .get("args")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .map(|x| x.as_str().unwrap_or("").to_string())
                .collect()
        })
        .unwrap_or_default();
    let allowed: &[(&str, &[&str])] = &[
        ("npm", &["run", "lint", "-w", "cmx-portal-manager"]),
        ("npm", &["run", "build", "-w", "cmx-portal-manager"]),
        ("npm", &["run", "build:runtime"]),
        ("npm", &["run", "build:portal"]),
        ("npm", &["run", "build:html"]),
        ("npm", &["run", "build:apps"]),
        ("cargo", &["check"]),
        ("cargo", &["build"]),
        ("cargo", &["test"]),
        ("cargo", &["clippy", "--", "-D", "warnings"]),
        ("git", &["status", "--short"]),
    ];
    let hit = allowed.iter().any(|(c, a)| {
        *c == command && a.len() == argv.len() && a.iter().zip(&argv).all(|(x, y)| *x == y)
    });
    if !hit {
        let joined = std::iter::once(command.clone())
            .chain(argv.clone())
            .collect::<Vec<_>>()
            .join(" ");
        return Err(bad(format!(
            "命令不在允许列表中：{}",
            if joined.trim().is_empty() {
                "(empty)".to_string()
            } else {
                joined
            }
        )));
    }
    Ok((command, argv))
}

/// run_command：执行白名单命令（cwd = rootDir 的父目录，与 Node 一致）。
pub async fn run_command(root: &Path, args: &Value) -> PortalResult<Value> {
    let (command, argv) = normalize_command(args)?;
    let timeout_ms = args
        .get("timeoutMs")
        .and_then(|v| v.as_u64())
        .unwrap_or(60000)
        .clamp(1000, 120000);
    let cwd = match command.as_str() {
        "cargo" => cargo_root(root),
        "git" => repo_root(root),
        _ => npm_root(root),
    };
    let cmd_str = std::iter::once(command.clone())
        .chain(argv.clone())
        .collect::<Vec<_>>()
        .join(" ");

    let mut cmd = tokio::process::Command::new(&command);
    cmd.args(&argv)
        .current_dir(&cwd)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let child = cmd.spawn();
    let output = match child {
        Ok(c) => {
            match tokio::time::timeout(
                std::time::Duration::from_millis(timeout_ms),
                c.wait_with_output(),
            )
            .await
            {
                Ok(Ok(o)) => o,
                Ok(Err(e)) => {
                    return Ok(
                        json!({ "command": cmd_str, "exitCode": 1, "stdout": "", "stderr": e.to_string(), "diagnostics": [], "timedOut": false }),
                    );
                }
                Err(_) => {
                    return Ok(
                        json!({ "command": cmd_str, "exitCode": 1, "stdout": "", "stderr": "命令执行超时", "diagnostics": [], "timedOut": true }),
                    );
                }
            }
        }
        Err(e) => {
            return Ok(
                json!({ "command": cmd_str, "exitCode": 1, "stdout": "", "stderr": e.to_string(), "diagnostics": [], "timedOut": false }),
            );
        }
    };
    let stdout = tail_str(&String::from_utf8_lossy(&output.stdout), MAX_COMMAND_OUTPUT);
    let stderr = tail_str(&String::from_utf8_lossy(&output.stderr), MAX_COMMAND_OUTPUT);
    let exit_code = output.status.code().unwrap_or(1);
    let combined = format!("{stdout}\n{stderr}");
    Ok(json!({
        "command": cmd_str, "exitCode": exit_code, "stdout": stdout, "stderr": stderr,
        "diagnostics": parse_lint_diagnostics(&cmd_str, &combined),
    }))
}

fn tail_str(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max {
        s.to_string()
    } else {
        chars[chars.len() - max..].iter().collect()
    }
}

/// 解析 eslint 风格诊断（仅 lint 命令）。
fn parse_lint_diagnostics(cmd: &str, output: &str) -> Vec<Value> {
    if !cmd.contains("lint") {
        return vec![];
    }
    let re = regex::Regex::new(r"^\s+(\d+):(\d+)\s+(warning|error)\s+(.+?)\s+([@\w/-]+)$").unwrap();
    let file_re = regex::Regex::new(r"^/.+\.(?:js|mjs|cjs|ts|json|css|html)$").unwrap();
    let mut diagnostics = Vec::new();
    let mut current_file = String::new();
    for line in output.split('\n') {
        let trimmed = line.trim();
        if file_re.is_match(trimmed) {
            current_file = trimmed.to_string();
            continue;
        }
        if let Some(c) = re.captures(line)
            && !current_file.is_empty() {
                diagnostics.push(json!({
                    "file": current_file,
                    "line": c[1].parse::<i64>().unwrap_or(0),
                    "column": c[2].parse::<i64>().unwrap_or(0),
                    "severity": &c[3],
                    "message": c[4].trim(),
                    "rule": &c[5],
                }));
            }
    }
    diagnostics
}

/// 派发工具调用。
pub async fn run_tool(root: &Path, name: &str, args: &Value) -> PortalResult<Value> {
    match name {
        "search_files" => search_files(root, args).await,
        "read_file" => read_file(root, args).await,
        "list_definitions" => list_definitions(args).await,
        "list_html_pages" => list_html_pages(args).await,
        "read_html_page" => read_html_page(args).await,
        "validate_metadata" => validate_metadata(root, args).await,
        "list_modules" => list_modules_tool(args).await,
        "get_module_manifest" => get_module_manifest_tool(args).await,
        "get_module_resource" => get_module_resource_tool(args).await,
        "list_dict_schemas" => list_dict_schemas_tool(args).await,
        "dict_search" => dict_search_tool(args).await,
        "dict_suggest" => dict_suggest_tool(args).await,
        "list_facts" => list_facts_tool(args).await,
        "get_fact" => get_fact_tool(args).await,
        "service_catalog_list" => service_catalog_list_tool(args).await,
        "service_catalog_get" => service_catalog_get_tool(args).await,
        "flexible_combination_list" => flexible_combination_list_tool(args).await,
        "flexible_combination_get" => flexible_combination_get_tool(args).await,
        "flexible_combination_validate" => flexible_combination_validate_tool(args).await,
        "flexible_combination_preview" => flexible_combination_preview_tool(args).await,
        "flexible_combination_resolve" => flexible_combination_resolve_tool(args).await,
        "flexible_combination_rule" => flexible_combination_rule_tool(args).await,
        "git_status" => git_status_tool(root, args).await,
        "git_diff" => git_diff_tool(root, args).await,
        "git_log" => git_log_tool(root, args).await,
        "list_local_plugins" => list_local_plugins(root, args).await,
        "inspect_plugin_manifest" => inspect_plugin_manifest(root, args).await,
        "call_plugin_function" => call_plugin_function_tool(root, args).await,
        "call_service_flow" => call_service_flow_tool(root, args).await,
        "generate_api_doc" => generate_api_doc_tool(root, args).await,
        "cargo_check" => cargo_check(root, args).await,
        "cargo_build" => cargo_build(root, args).await,
        "cargo_test" => cargo_test(root, args).await,
        "cargo_clippy" => cargo_clippy(root, args).await,
        "npm_test" => npm_test(root, args).await,
        "npm_build_workspace" => npm_build_workspace(root, args).await,
        "run_playwright" => run_playwright(root, args).await,
        "capture_page_screenshot" => capture_page_screenshot(root, args).await,
        "inspect_dom" => inspect_dom(root, args).await,
        "check_accessibility" => check_accessibility(root, args).await,
        "apply_file_patch" => apply_file_patch(root, args).await,
        "format_file" => format_file(root, args).await,
        "create_file" => create_file(root, args).await,
        "rename_file" => rename_file(root, args).await,
        "run_command" => run_command(root, args).await,
        "apply_json_patch" => apply_json_patch(root, args).await,
        "apply_text_replace" => apply_text_replace(root, args).await,
        other => Err(bad(format!("未知工具：{other}"))),
    }
}
