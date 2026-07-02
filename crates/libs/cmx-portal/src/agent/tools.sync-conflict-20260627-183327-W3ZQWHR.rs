//! agent 工具实现（复刻 `agentRoutes.js` 的 tool* 函数）。
//!
//! 路径穿越保护：所有文件操作限制在 rootDir 内。命令白名单：npm run lint/build -w cmx-portal-manager。

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::error::{PortalError, PortalResult};

const MAX_FILE_BYTES: u64 = 180_000;
const MAX_COMMAND_OUTPUT: usize = 60_000;
const MAX_PATCH_BYTES: u64 = 120_000;
const MAX_TEXT_REPLACEMENTS: usize = 200;
const TEXT_FILE_EXTS: &[&str] = &["json", "html", "mjs", "cjs", "css", "md", "ts", "js"];

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
    TEXT_FILE_EXTS.iter().any(|e| lower.ends_with(&format!(".{e}")))
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
    while end_a >= start as isize && end_b >= start as isize && a[end_a as usize] == b[end_b as usize] {
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
    Ok(p.split('/').skip(1).map(|seg| seg.replace("~1", "/").replace("~0", "~")).collect())
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
                    obj.insert(key.clone(), if next_is_index { json!([]) } else { json!({}) });
                }
                cur = obj.get_mut(key).unwrap();
            }
            Value::Array(arr) => {
                let idx: usize = key
                    .parse()
                    .map_err(|_| bad(format!("JSON Pointer 数组下标非法：/{}", parts[..=i].join("/"))))?;
                if idx >= arr.len() {
                    return Err(bad(format!("JSON Pointer 中间节点不存在：/{}", parts[..=i].join("/"))));
                }
                cur = &mut arr[idx];
            }
            _ => {
                return Err(bad(format!("JSON Pointer 中间节点不存在：/{}", parts[..=i].join("/"))));
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
            let idx: usize = last.parse().map_err(|_| bad(format!("JSON Pointer 父节点不可写：{pointer}")))?;
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
    let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    if query.is_empty() {
        return Err(bad("search_files 需要 query"));
    }
    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(20).clamp(1, 50) as usize;
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
                if let Some((idx, line)) = content.lines().enumerate().find(|(_, l)| l.to_lowercase().contains(&q)) {
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
    let p = resolve_inside_root(root, args.get("path").and_then(|v| v.as_str()).unwrap_or(""))?;
    let meta = tokio::fs::metadata(&p).await.map_err(|_| bad("只能读取文件"))?;
    if !meta.is_file() {
        return Err(bad("只能读取文件"));
    }
    if meta.len() > MAX_FILE_BYTES {
        return Err(bad(format!("文件过大，当前限制 {MAX_FILE_BYTES} bytes")));
    }
    let content = tokio::fs::read_to_string(&p).await.map_err(PortalError::Io)?;
    Ok(json!({ "path": relative_from_root(root, &p), "bytes": meta.len(), "content": content }))
}

/// list_definitions：复用 definitions store。
pub async fn list_definitions(args: &Value) -> PortalResult<Value> {
    let kind = args.get("kind").and_then(|v| v.as_str()).map(|s| s.to_uppercase());
    let domain = args.get("domain").and_then(|v| v.as_str());
    let module = args.get("module").and_then(|v| v.as_str());
    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(60).clamp(1, 100) as usize;
    let items = crate::definitions::store::list_definitions(kind.as_deref(), domain, None, module).await?;
    Ok(Value::Array(items.into_iter().take(limit).collect()))
}

/// list_html_pages：复用 html store（裁剪字段）。
pub async fn list_html_pages(args: &Value) -> PortalResult<Value> {
    let page = args.get("page").and_then(|v| v.as_i64()).unwrap_or(1).max(1);
    let page_size = args.get("pageSize").or_else(|| args.get("limit")).and_then(|v| v.as_i64()).unwrap_or(20).clamp(1, 50);
    let domain = args.get("domain").and_then(|v| v.as_str());
    let app = args.get("app").and_then(|v| v.as_str());
    let module = args.get("module").and_then(|v| v.as_str());
    let out = crate::pages::html::list_html_pages_paged(Some(page), Some(page_size), domain, app, module).await?;
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
    res.as_object_mut().unwrap().insert("items".to_string(), Value::Array(items));
    Ok(res)
}

/// read_html_page：复用 html store（截断 html 到 24000 字符）。
pub async fn read_html_page(args: &Value) -> PortalResult<Value> {
    let id = args.get("id").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
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

/// validate_metadata：递归校验 JSON 可解析性。
pub async fn validate_metadata(root: &Path, args: &Value) -> PortalResult<Value> {
    let target = match args.get("path").and_then(|v| v.as_str()) {
        Some(p) if !p.trim().is_empty() => resolve_inside_root(root, p)?,
        _ => root.join("cmx-node-server").join("data").join("meta").join("definitions"),
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
        if let Ok(content) = tokio::fs::read_to_string(file).await {
            if let Err(e) = serde_json::from_str::<Value>(&content) {
                diagnostics.push(json!({ "file": relative_from_root(root, file), "error": e.to_string() }));
            }
        }
    }
    Ok(json!({ "checked": files.len(), "errors": diagnostics }))
}

// ── 补丁预览/应用 ────────────────────────────────────────────────

/// 文本替换补丁预览（不写盘）。
pub async fn prepare_text_replace(root: &Path, args: &Value) -> PortalResult<Value> {
    let p = resolve_inside_root(root, args.get("path").and_then(|v| v.as_str()).unwrap_or(""))?;
    let meta = tokio::fs::metadata(&p).await.map_err(|_| bad("只能修改文件"))?;
    if !meta.is_file() {
        return Err(bad("只能修改文件"));
    }
    if meta.len() > MAX_PATCH_BYTES {
        return Err(bad(format!("文件过大，当前补丁限制 {MAX_PATCH_BYTES} bytes")));
    }
    let old_text = args.get("oldText").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let new_text = args.get("newText").and_then(|v| v.as_str()).unwrap_or("").to_string();
    if old_text.is_empty() {
        return Err(bad("文本替换补丁需要 oldText"));
    }
    let before = tokio::fs::read_to_string(&p).await.map_err(PortalError::Io)?;
    let count = before.matches(&old_text).count();
    if count == 0 {
        return Err(bad("未找到要替换的文本"));
    }
    let occurrence = if args.get("occurrence").and_then(|v| v.as_str()) == Some("all") { "all" } else { "first" };
    if occurrence == "all" && count > MAX_TEXT_REPLACEMENTS {
        return Err(bad(format!("匹配过多，当前限制 {MAX_TEXT_REPLACEMENTS} 处")));
    }
    let replacements = if occurrence == "all" { count.min(MAX_TEXT_REPLACEMENTS) } else { 1 };
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
    tokio::fs::write(&abs, after).await.map_err(PortalError::Io)?;
    Ok(json!({
        "path": rel, "occurrence": preview.get("occurrence"), "replacements": preview.get("replacements"),
        "bytes": after.len(), "diff": preview.get("diff"),
    }))
}

/// JSON 补丁预览。
pub async fn prepare_json_patch(root: &Path, args: &Value) -> PortalResult<Value> {
    let p = resolve_inside_root(root, args.get("path").and_then(|v| v.as_str()).unwrap_or(""))?;
    let meta = tokio::fs::metadata(&p).await.map_err(|_| bad("只能修改文件"))?;
    if !meta.is_file() {
        return Err(bad("只能修改文件"));
    }
    if meta.len() > MAX_PATCH_BYTES {
        return Err(bad(format!("文件过大，当前补丁限制 {MAX_PATCH_BYTES} bytes")));
    }
    let before = tokio::fs::read_to_string(&p).await.map_err(PortalError::Io)?;
    let mut doc: Value = serde_json::from_str(&before).map_err(|_| bad("当前仅支持可解析的 JSON 文件补丁"))?;
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
    tokio::fs::write(&abs, after).await.map_err(PortalError::Io)?;
    Ok(json!({ "path": rel, "pointer": preview.get("pointer"), "bytes": after.len(), "diff": preview.get("diff") }))
}

// ── run_command（白名单）─────────────────────────────────────────

/// 命令白名单校验，返回 (command, args)。
fn normalize_command(args: &Value) -> PortalResult<(String, Vec<String>)> {
    let command = args.get("command").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    let argv: Vec<String> = args
        .get("args")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().map(|x| x.as_str().unwrap_or("").to_string()).collect())
        .unwrap_or_default();
    let allowed: &[(&str, &[&str])] = &[
        ("npm", &["run", "lint", "-w", "cmx-portal-manager"]),
        ("npm", &["run", "build", "-w", "cmx-portal-manager"]),
    ];
    let hit = allowed.iter().any(|(c, a)| *c == command && a.len() == argv.len() && a.iter().zip(&argv).all(|(x, y)| *x == y));
    if !hit {
        let joined = std::iter::once(command.clone()).chain(argv.clone()).collect::<Vec<_>>().join(" ");
        return Err(bad(format!("命令不在允许列表中：{}", if joined.trim().is_empty() { "(empty)".to_string() } else { joined })));
    }
    Ok((command, argv))
}

/// run_command：执行白名单命令（cwd = rootDir 的父目录，与 Node 一致）。
pub async fn run_command(root: &Path, args: &Value) -> PortalResult<Value> {
    let (command, argv) = normalize_command(args)?;
    let timeout_ms = args.get("timeoutMs").and_then(|v| v.as_u64()).unwrap_or(60000).clamp(1000, 120000);
    let cwd = root.parent().unwrap_or(root).to_path_buf();
    let cmd_str = std::iter::once(command.clone()).chain(argv.clone()).collect::<Vec<_>>().join(" ");

    let mut cmd = tokio::process::Command::new(&command);
    cmd.args(&argv).current_dir(&cwd).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
    let child = cmd.spawn();
    let output = match child {
        Ok(c) => {
            match tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), c.wait_with_output()).await {
                Ok(Ok(o)) => o,
                Ok(Err(e)) => return Ok(json!({ "command": cmd_str, "exitCode": 1, "stdout": "", "stderr": e.to_string(), "diagnostics": [], "timedOut": false })),
                Err(_) => return Ok(json!({ "command": cmd_str, "exitCode": 1, "stdout": "", "stderr": "命令执行超时", "diagnostics": [], "timedOut": true })),
            }
        }
        Err(e) => return Ok(json!({ "command": cmd_str, "exitCode": 1, "stdout": "", "stderr": e.to_string(), "diagnostics": [], "timedOut": false })),
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
        if let Some(c) = re.captures(line) {
            if !current_file.is_empty() {
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
        "run_command" => run_command(root, args).await,
        "apply_json_patch" => apply_json_patch(root, args).await,
        "apply_text_replace" => apply_text_replace(root, args).await,
        other => Err(bad(format!("未知工具：{other}"))),
    }
}
