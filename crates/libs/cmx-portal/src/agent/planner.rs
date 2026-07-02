//! LocalRulePlanner —— 正则意图抽取（复刻 `agentPlanner.js` 的 LocalRulePlanner）。
//!
//! 把用户消息转成 decision：analysis（只读工具组合）或 approval（写文件/跑命令）。
//! LlmPlanner（CMX_AGENT_PLANNER=llm）暂不实现，默认走本地规则。

use serde_json::{Value, json};

use crate::error::PortalResult;

const TEXT_FILE_EXT_PATTERN: &str = "json|html|mjs|cjs|css|md|ts|js";

/// 值转字符串（数字/布尔也转，对象/数组保持 JSON）。
fn value_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn default_plan() -> Value {
    json!([
        "理解请求与当前工作区上下文",
        "选择只读工具收集证据",
        "汇总下一步建议或定位结果"
    ])
}

/// 取最近一条 user 消息文本。
pub fn latest_user_text(messages: &[Value]) -> String {
    for m in messages.iter().rev() {
        if m.get("role").and_then(|v| v.as_str()) == Some("user") {
            return m
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
        }
    }
    String::new()
}

/// 猜测搜索关键词（引号 > 路径 > 末尾 token）。
fn guess_search_query(text: &str) -> String {
    // 引号内 2-120 字符
    let quoted = regex::Regex::new(r#"["'`“”‘’]([^"'`“”‘’]{2,120})["'`“”‘’]"#).unwrap();
    if let Some(c) = quoted.captures(text) {
        return c[1].trim().to_string();
    }
    let path_like = regex::Regex::new(&format!(
        r"([a-zA-Z0-9_.@/-]+\.(?:{TEXT_FILE_EXT_PATTERN}))"
    ))
    .unwrap();
    if let Some(c) = path_like.captures(text) {
        return c[1].trim().to_string();
    }
    let stop = regex::Regex::new(r"^(请|帮我|如何|怎么|一下|实现|方案|这个|那个)$").unwrap();
    let cleaned = regex::Regex::new(r"[，。！？；：、]")
        .unwrap()
        .replace_all(text, " ");
    let tokens: Vec<&str> = cleaned
        .split_whitespace()
        .map(|s| s.trim())
        .filter(|s| s.chars().count() >= 2 && !stop.is_match(s))
        .collect();
    if tokens.is_empty() {
        text.chars().take(80).collect()
    } else {
        tokens
            .iter()
            .rev()
            .take(4)
            .rev()
            .cloned()
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// 宽松解析值（JSON / bool / null / number / 去引号字符串）。
fn parse_loose_value(raw: &str) -> Value {
    let text = raw.trim();
    if text.is_empty() {
        return json!("");
    }
    if let Ok(v) = serde_json::from_str::<Value>(text) {
        return v;
    }
    let lower = text.to_lowercase();
    if lower == "true" {
        return json!(true);
    }
    if lower == "false" {
        return json!(false);
    }
    if lower == "null" {
        return Value::Null;
    }
    if regex::Regex::new(r"^-?\d+(\.\d+)?$")
        .unwrap()
        .is_match(text)
    {
        if let Ok(n) = text.parse::<f64>() {
            return json!(n);
        }
    }
    json!(text.trim_matches(|c| "\"'“”‘’".contains(c)))
}

fn strip_portal_prefix(s: &str) -> String {
    s.strip_prefix("CMXPortalManager/").unwrap_or(s).to_string()
}

/// 抽取 JSON 补丁请求（file + pointer + value）。
fn extract_json_patch_request(text: &str) -> Option<Value> {
    let file = regex::Regex::new(r"([a-zA-Z0-9_.@/-]+\.json)")
        .unwrap()
        .captures(text)
        .map(|c| strip_portal_prefix(&c[1]))?;
    let pointer =
        regex::Regex::new(r"(?i)(?:pointer|路径|字段|json\s*pointer)\s*[:：]?\s*(/[^\s，。；]+)")
            .unwrap()
            .captures(text)
            .map(|c| c[1].to_string())
            .or_else(|| {
                regex::Regex::new(r"(?i)(/[a-zA-Z0-9_~/-]+)\s*(?:改为|设置为|set\s+to)\s*")
                    .unwrap()
                    .captures(text)
                    .map(|c| c[1].to_string())
            })?;
    let value_str = regex::Regex::new(r"(?i)(?:值|value)\s*[:：]\s*([\s\S]+)$")
        .unwrap()
        .captures(text)
        .map(|c| c[1].to_string())
        .or_else(|| {
            regex::Regex::new(r"(?i)(?:改为|设置为|set\s+to)\s*([\s\S]+)$")
                .unwrap()
                .captures(text)
                .map(|c| c[1].to_string())
        })?;
    Some(json!({ "path": file, "pointer": pointer, "value": parse_loose_value(&value_str) }))
}

/// 抽取引号片段（最多 6 个，按出现顺序）。
///
/// Rust `regex` 不支持反向引用 `\1`，故对三种引号各跑一次非贪婪匹配，再按起始位置排序合并，
/// 等价于 Node 的 `(["'`])([\s\S]*?)\1`。
fn extract_quoted_parts(text: &str) -> Vec<String> {
    let normalized = text.replace(['“', '”'], "\"").replace(['‘', '’'], "'");
    let mut found: Vec<(usize, String)> = Vec::new();
    for re in [
        regex::Regex::new(r#""([\s\S]*?)""#).unwrap(),
        regex::Regex::new(r#"'([\s\S]*?)'"#).unwrap(),
        regex::Regex::new(r#"`([\s\S]*?)`"#).unwrap(),
    ] {
        for c in re.captures_iter(&normalized) {
            let m = c.get(0).unwrap();
            let inner = c.get(1).map(|x| x.as_str().to_string()).unwrap_or_default();
            if !inner.is_empty() {
                found.push((m.start(), inner));
            }
        }
    }
    found.sort_by_key(|(pos, _)| *pos);
    found.into_iter().map(|(_, s)| s).take(6).collect()
}

/// 抽取文本替换请求。
fn extract_text_replace_request(text: &str) -> Option<Value> {
    if !regex::Regex::new(r"(?i)(替换|replace|改成|改为)")
        .unwrap()
        .is_match(text)
    {
        return None;
    }
    let file = regex::Regex::new(&format!(
        r"([a-zA-Z0-9_.@/-]+\.(?:{TEXT_FILE_EXT_PATTERN}))"
    ))
    .unwrap()
    .captures(text)
    .map(|c| strip_portal_prefix(&c[1]))?;
    let all = regex::Regex::new(r"(?iu)全部|所有|all|global")
        .unwrap()
        .is_match(text);
    let occurrence = if all { "all" } else { "first" };
    let quoted = extract_quoted_parts(text);
    if quoted.len() >= 2 {
        return Some(
            json!({ "path": file, "oldText": quoted[0], "newText": quoted[1], "occurrence": occurrence }),
        );
    }
    let m = regex::Regex::new(r"把\s+([\s\S]+?)\s*(?:替换为|替换成|改成|改为)\s*([\s\S]+)$")
        .unwrap()
        .captures(text)?;
    let new_text = regex::Regex::new(&format!(
        r"(?i)\s*(?:在|到)\s*[a-zA-Z0-9_.@/-]+\.(?:{TEXT_FILE_EXT_PATTERN})\s*$"
    ))
    .unwrap()
    .replace(m[2].trim(), "")
    .to_string();
    Some(
        json!({ "path": file, "oldText": m[1].trim(), "newText": new_text.trim(), "occurrence": occurrence }),
    )
}

/// 推断命令审批（lint / build）。
fn infer_command_approval(text: &str) -> Option<Value> {
    if regex::Regex::new(r"(?i)lint|eslint|代码检查|静态检查")
        .unwrap()
        .is_match(text)
    {
        return Some(json!({
            "title": "运行 CMXPortalManager lint",
            "risk": "只读检查命令，会读取源码并输出诊断，不写业务文件。",
            "args": { "command": "npm", "args": ["run", "lint", "-w", "cmx-portal-manager"] }
        }));
    }
    if regex::Regex::new(r"(?i)build|构建|打包")
        .unwrap()
        .is_match(text)
    {
        return Some(json!({
            "title": "构建 CMXPortalManager",
            "risk": "构建命令可能写入 dist 等构建产物，耗时也更长。",
            "args": { "command": "npm", "args": ["run", "build", "-w", "cmx-portal-manager"], "timeoutMs": 120000 }
        }));
    }
    None
}

fn wants_html_page_context(text: &str) -> bool {
    regex::Regex::new(r"(?i)自定义页面|html\s*page|html页面|页面设计|设计器|html_pages|页面资产")
        .unwrap()
        .is_match(text)
}

fn extract_html_page_id(text: &str) -> String {
    if let Some(c) = regex::Regex::new(
        r"(?i)(?:页面\s*ID|html\s*page\s*id|pageId|id)\s*[:：=]\s*([a-zA-Z0-9._-]{1,128})",
    )
    .unwrap()
    .captures(text)
    {
        return c[1].to_string();
    }
    let stop =
        regex::Regex::new(r"(?i)^(json|html|css|js|ts|md|lint|build|agent|deepseek)$").unwrap();
    for c in regex::Regex::new(r"\b([a-zA-Z0-9_-]+(?:\.[a-zA-Z0-9_-]+){0,5})\b")
        .unwrap()
        .captures_iter(text)
    {
        let s = c[1].to_string();
        if stop.is_match(&s) {
            continue;
        }
        if s.contains('.') || s.contains('-') || s.contains('_') {
            return s;
        }
    }
    String::new()
}

/// 计划 decision：CMX_AGENT_PLANNER=llm 时走 LLM 规划（失败回退本地规则），否则纯本地规则。
pub async fn plan(messages: &[Value], root: &std::path::Path) -> Value {
    if std::env::var("CMX_AGENT_PLANNER").ok().as_deref() == Some("llm")
        && crate::ai::is_configured()
    {
        match llm_plan(messages).await {
            Ok(decision) => return decision,
            Err(e) => {
                tracing::warn!("[agentPlanner] LLM 规划失败，回退本地规则：{e}");
            }
        }
    }
    local_plan(messages, root).await
}

/// 本地规则规划（LocalRulePlanner）。`root` 用于判断 readFile 候选路径是否存在。
pub async fn local_plan(messages: &[Value], root: &std::path::Path) -> Value {
    let text = latest_user_text(messages);

    if let Some(json_patch) = extract_json_patch_request(&text) {
        return json!({
            "kind": "approval", "action": "apply_json_patch", "args": json_patch,
            "intro": "我已根据你的描述生成 JSON 补丁预览，确认后才会写入文件。",
            "plan": ["解析目标 JSON 文件与字段路径", "生成修改前后 diff", "等待用户审批后写入文件"],
            "title": null,
            "risk": "审批通过后会写入项目文件；写入前已确认目标文件是合法 JSON。",
            "outro": "请在审批卡片中查看 diff。确认无误后点同意，我再应用补丁。",
        });
    }
    if let Some(text_replace) = extract_text_replace_request(&text) {
        return json!({
            "kind": "approval", "action": "apply_text_replace", "args": text_replace,
            "intro": "我已生成文本替换补丁预览，确认后才会写入文件。",
            "plan": ["定位目标文本文件", "生成替换前后 diff", "等待用户审批后写入文件"],
            "title": null, "risk": null,
            "outro": "请在审批卡片中查看 diff。确认无误后点同意，我再应用文本补丁。",
        });
    }
    if let Some(command) = infer_command_approval(&text) {
        return json!({
            "kind": "approval", "action": "run_command", "args": command.get("args").cloned().unwrap_or(json!({})),
            "intro": "这一步需要执行本地命令，我先生成审批请求，确认后再运行。",
            "plan": ["确认命令与影响范围", "等待用户审批", "执行命令并回填输出"],
            "title": command.get("title").cloned().unwrap_or(Value::Null),
            "risk": command.get("risk").cloned().unwrap_or(Value::Null),
            "outro": "请在审批卡片中选择同意或拒绝。当前仅允许预置安全命令，不会执行任意 shell 文本。",
        });
    }

    // 只读分析：抽取可读路径（存在才用）
    let readable_path = extract_readable_path(&text, root).await;
    let html_id = if wants_html_page_context(&text) {
        extract_html_page_id(&text)
    } else {
        String::new()
    };
    let wants_html = wants_html_page_context(&text);
    json!({
        "kind": "analysis",
        "intro": "我先按只读方式查看项目上下文，尽量把定位结果和下一步动作说清楚。",
        "plan": default_plan(),
        "wantsDefinitions": regex::Regex::new(r"(?i)定义|字典|单据|metadata|meta|definition").unwrap().is_match(&text),
        "wantsHtmlPages": wants_html,
        "htmlPagesFilter": if wants_html { json!({ "page": 1, "pageSize": 20 }) } else { Value::Null },
        "readHtmlPage": if !html_id.is_empty() { json!({ "id": html_id }) } else { Value::Null },
        "wantsValidate": regex::Regex::new(r"(?i)校验|验证|validate|检查").unwrap().is_match(&text),
        "readFile": if !readable_path.is_empty() { json!({ "path": readable_path }) } else { Value::Null },
        "search": if readable_path.is_empty() { json!({ "query": guess_search_query(&text), "limit": 20 }) } else { Value::Null },
    })
}

async fn extract_readable_path(text: &str, root: &std::path::Path) -> String {
    let re = regex::Regex::new(&format!(
        r"([a-zA-Z0-9_.@/-]+\.(?:{TEXT_FILE_EXT_PATTERN}))"
    ))
    .unwrap();
    let Some(c) = re.captures(text) else {
        return String::new();
    };
    let candidate = strip_portal_prefix(&c[1]);
    let abs = root.join(candidate.trim_start_matches('/'));
    if tokio::fs::metadata(&abs).await.is_ok() {
        candidate
    } else {
        String::new()
    }
}

/// 本地总结（无 LLM 时）。
pub fn build_local_summary(events: &[Value], context: &Value) -> String {
    let results: Vec<&Value> = events
        .iter()
        .filter(|e| e.get("type").and_then(|v| v.as_str()) == Some("tool_result"))
        .collect();
    let failed: Vec<&&Value> = results
        .iter()
        .filter(|e| e.get("status").and_then(|v| v.as_str()) == Some("error"))
        .collect();
    let ok: Vec<&&Value> = results
        .iter()
        .filter(|e| e.get("status").and_then(|v| v.as_str()) != Some("error"))
        .collect();
    let ctx_title = context
        .get("workspaceTitle")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|t| format!("当前工作区：{t}。\n"))
        .unwrap_or_default();
    if !failed.is_empty() && ok.is_empty() {
        let msgs: Vec<String> = failed
            .iter()
            .map(|e| {
                e.get("summary")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string()
            })
            .collect();
        return format!("{ctx_title}工具调用失败：{}。", msgs.join("；"));
    }
    let lines: Vec<String> = ok
        .iter()
        .map(|e| {
            format!(
                "- {}",
                e.get("summary").and_then(|v| v.as_str()).unwrap_or("")
            )
        })
        .collect();
    format!(
        "{ctx_title}已完成这轮只读分析：\n{}\n\n目前这个 Agent Gateway 已具备对话协议、计划、工具调用和结果展示；写文件、运行命令、审批流可以在这个协议上继续扩展。",
        lines.join("\n")
    )
}

// ── LlmPlanner（CMX_AGENT_PLANNER=llm）─────────────────────────────────

/// Planner 系统提示词（复刻 Node buildPlannerSystemPrompt，含工具 schema + 安全规则）。
fn planner_system_prompt() -> String {
    let tools = crate::agent::schemas::public_tool_schemas();
    format!(
        r#"你是 CMXPortalManager 网页 Agent 的 Planner。你只输出 JSON，不要输出 Markdown。

你的任务是把用户请求转换为一个 decision。不要执行工具，不要编造工具结果。

允许的 JSON 格式：
1. 只读分析：
{{
  "kind": "analysis",
  "intro": "简短中文说明",
  "plan": ["步骤1", "步骤2"],
  "wantsDefinitions": false,
  "wantsHtmlPages": false,
  "htmlPagesFilter": {{"domain": "fi", "app":"cmxfico","module":"gl", "page": 1, "pageSize": 20}} 或 null,
  "readHtmlPage": {{"id": "page.id"}} 或 null,
  "wantsValidate": false,
  "readFile": {{"path": "relative/file.js"}} 或 null,
  "search": {{"query": "keyword", "limit": 20}} 或 null
}}

2. 需要审批的操作：
{{
  "kind": "approval",
  "action": "run_command" | "apply_json_patch" | "apply_text_replace",
  "args": {{}},
  "intro": "简短中文说明",
  "plan": ["步骤1", "步骤2"],
  "title": "审批标题",
  "risk": "风险说明",
  "outro": "提示用户审批"
}}

安全规则：
- 写文件只能使用 apply_json_patch 或 apply_text_replace。
- 命令只能使用 run_command，且只能请求 npm run lint/build -w cmx-portal-manager。
- 不要请求任意 shell，不要请求删除文件，不要越过审批。
- 文件路径必须是相对 CMXPortalManager 根目录的相对路径。
- 用户提到自定义页面、HTML 页面、页面设计器、html_pages 时，优先使用 wantsHtmlPages/readHtmlPage。
- 不确定时选择 analysis + search。

可用工具 schema：
{tools}"#,
        tools = serde_json::to_string_pretty(&tools).unwrap_or_default()
    )
}

/// 从 LLM 文本里抽取 JSON（容错：截首个 `{` 到末个 `}`）。
fn parse_planner_json(content: &str) -> Option<Value> {
    let text = content.trim();
    if let Ok(v) = serde_json::from_str::<Value>(text) {
        return Some(v);
    }
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if end > start {
        serde_json::from_str::<Value>(&text[start..=end]).ok()
    } else {
        None
    }
}

fn string_or(v: Option<&Value>, fallback: &str) -> String {
    let s = v.map(value_to_string).unwrap_or_default();
    let s = s.trim();
    if s.is_empty() {
        fallback.to_string()
    } else {
        s.to_string()
    }
}

fn string_array_or(v: Option<&Value>, fallback: Value) -> Value {
    match v.and_then(|x| x.as_array()) {
        Some(arr) => {
            let items: Vec<Value> = arr
                .iter()
                .filter_map(|x| x.as_str().map(|s| s.trim()))
                .filter(|s| !s.is_empty())
                .take(8)
                .map(|s| json!(s))
                .collect();
            if items.is_empty() {
                fallback
            } else {
                Value::Array(items)
            }
        }
        None => fallback,
    }
}

/// 可选对象：required 键须为非空字符串，否则 null。
fn normalize_optional_object(v: Option<&Value>, required: &[&str]) -> Value {
    let Some(obj) = v.filter(|x| x.is_object()) else {
        return Value::Null;
    };
    for key in required {
        let ok = obj
            .get(*key)
            .map(value_to_string)
            .map(|s| !s.trim().is_empty())
            .unwrap_or(false);
        if !ok {
            return Value::Null;
        }
    }
    obj.clone()
}

fn passthrough_object(args: &Value, allowed_keys: &[&str]) -> Value {
    let mut out = serde_json::Map::new();
    for key in allowed_keys {
        if let Some(v) = args.get(*key) {
            out.insert((*key).to_string(), v.clone());
        }
    }
    Value::Object(out)
}

/// 校验审批 args（命令白名单 / patch 必填字段）；非法时 Err。
fn normalize_approval_args(action: &str, args: Option<&Value>) -> Result<Value, String> {
    let args = args
        .filter(|v| v.is_object())
        .ok_or_else(|| "approval args must be an object".to_string())?;
    match action {
        "run_command" => {
            let command = args.get("command").and_then(|v| v.as_str()).unwrap_or("");
            let argv: Vec<String> = args
                .get("args")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().map(value_to_string).collect())
                .unwrap_or_default();
            let joined = std::iter::once(command.to_string())
                .chain(argv.clone())
                .collect::<Vec<_>>()
                .join(" ");
            if joined == "npm run lint -w cmx-portal-manager" {
                Ok(json!({ "command": command, "args": argv }))
            } else if joined == "npm run build -w cmx-portal-manager" {
                Ok(json!({ "command": command, "args": argv, "timeoutMs": 120000 }))
            } else if matches!(
                joined.as_str(),
                "npm run build:runtime"
                    | "npm run build:portal"
                    | "npm run build:html"
                    | "npm run build:apps"
                    | "cargo check"
                    | "cargo build"
                    | "cargo test"
                    | "cargo clippy -- -D warnings"
                    | "git status --short"
            ) {
                Ok(
                    json!({ "command": command, "args": argv, "timeoutMs": args.get("timeoutMs").cloned().unwrap_or(Value::Null) }),
                )
            } else {
                Err(format!("command is not allowed: {joined}"))
            }
        }
        "apply_json_patch" => {
            let path = args
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            let pointer = args
                .get("pointer")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
            if path.is_empty() || !pointer.starts_with('/') {
                return Err("invalid json patch args".to_string());
            }
            Ok(
                json!({ "path": path, "pointer": pointer, "value": args.get("value").cloned().unwrap_or(Value::Null) }),
            )
        }
        "apply_text_replace" => {
            let path = args
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim()
                .to_string();
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
            if path.is_empty() || old_text.is_empty() {
                return Err("invalid text replace args".to_string());
            }
            let occ = if args.get("occurrence").and_then(|v| v.as_str()) == Some("all") {
                "all"
            } else {
                "first"
            };
            Ok(json!({ "path": path, "oldText": old_text, "newText": new_text, "occurrence": occ }))
        }
        "cargo_check" | "cargo_build" | "cargo_test" | "cargo_clippy" => {
            Ok(passthrough_object(args, &["package", "test", "timeoutMs"]))
        }
        "npm_test" | "npm_build_workspace" => Ok(passthrough_object(
            args,
            &["workspace", "script", "timeoutMs"],
        )),
        "run_playwright" => Ok(passthrough_object(args, &["project", "grep", "timeoutMs"])),
        "capture_page_screenshot" | "inspect_dom" => {
            let url = args
                .get("url")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            if url.is_empty() || !(url.starts_with("http://") || url.starts_with("https://")) {
                return Err("browser tool requires http(s) url".to_string());
            }
            Ok(passthrough_object(
                args,
                &["url", "output", "selector", "timeoutMs"],
            ))
        }
        "check_accessibility" => Ok(passthrough_object(args, &["url", "timeoutMs"])),
        "apply_file_patch" => {
            let patch = args.get("patch").and_then(|v| v.as_str()).unwrap_or("");
            if patch.trim().is_empty() {
                return Err("apply_file_patch requires patch".to_string());
            }
            Ok(json!({ "patch": patch }))
        }
        "format_file" => {
            let path = args
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            if path.is_empty() {
                return Err("format_file requires path".to_string());
            }
            Ok(passthrough_object(args, &["path", "timeoutMs"]))
        }
        "create_file" => {
            let path = args
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            if path.is_empty() {
                return Err("create_file requires path".to_string());
            }
            Ok(passthrough_object(args, &["path", "content", "overwrite"]))
        }
        "rename_file" => {
            let from = args
                .get("from")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            let to = args.get("to").and_then(|v| v.as_str()).unwrap_or("").trim();
            if from.is_empty() || to.is_empty() {
                return Err("rename_file requires from/to".to_string());
            }
            Ok(passthrough_object(args, &["from", "to"]))
        }
        "call_plugin_function" => {
            let plugin_id = args
                .get("pluginId")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            let function_name = args
                .get("functionName")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            if plugin_id.is_empty() || function_name.is_empty() {
                return Err("call_plugin_function requires pluginId/functionName".to_string());
            }
            Ok(passthrough_object(
                args,
                &["serviceName", "pluginId", "functionName", "input"],
            ))
        }
        "call_service_flow" => {
            let service_key = args
                .get("serviceKey")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            if service_key.is_empty() {
                return Err("call_service_flow requires serviceKey".to_string());
            }
            Ok(passthrough_object(
                args,
                &["serviceName", "serviceKey", "input", "timeoutMs"],
            ))
        }
        other => Err(format!("unsupported action: {other}")),
    }
}

/// 归一 LLM decision（防止越权 action / 缺字段）。
fn normalize_decision(raw: &Value) -> Result<Value, String> {
    let kind = raw.get("kind").and_then(|v| v.as_str()).unwrap_or("");
    match kind {
        "analysis" => Ok(json!({
            "kind": "analysis",
            "intro": string_or(raw.get("intro"), "我先按只读方式查看项目上下文。"),
            "plan": string_array_or(raw.get("plan"), default_plan()),
            "wantsDefinitions": raw.get("wantsDefinitions").and_then(|v| v.as_bool()).unwrap_or(false),
            "wantsHtmlPages": raw.get("wantsHtmlPages").and_then(|v| v.as_bool()).unwrap_or(false),
            "htmlPagesFilter": normalize_optional_object(raw.get("htmlPagesFilter"), &[]),
            "readHtmlPage": normalize_optional_object(raw.get("readHtmlPage"), &["id"]),
            "wantsValidate": raw.get("wantsValidate").and_then(|v| v.as_bool()).unwrap_or(false),
            "readFile": normalize_optional_object(raw.get("readFile"), &["path"]),
            "search": normalize_optional_object(raw.get("search"), &["query"]),
        })),
        "approval" => {
            let action = raw.get("action").and_then(|v| v.as_str()).unwrap_or("");
            if ![
                "run_command",
                "apply_json_patch",
                "apply_text_replace",
                "apply_file_patch",
                "format_file",
                "create_file",
                "rename_file",
                "cargo_check",
                "cargo_build",
                "cargo_test",
                "cargo_clippy",
                "npm_test",
                "npm_build_workspace",
                "run_playwright",
                "capture_page_screenshot",
                "inspect_dom",
                "check_accessibility",
                "call_plugin_function",
                "call_service_flow",
            ]
            .contains(&action)
            {
                return Err(format!("unsafe planner action: {action}"));
            }
            let args = normalize_approval_args(action, raw.get("args"))?;
            Ok(json!({
                "kind": "approval",
                "action": action,
                "args": args,
                "intro": string_or(raw.get("intro"), "这一步需要审批。"),
                "plan": string_array_or(raw.get("plan"), json!(["确认操作", "等待审批", "执行并回填结果"])),
                "title": raw.get("title").filter(|v| !v.is_null()).map(value_to_string).map(Value::String).unwrap_or(Value::Null),
                "risk": raw.get("risk").filter(|v| !v.is_null()).map(value_to_string).map(Value::String).unwrap_or(Value::Null),
                "outro": string_or(raw.get("outro"), "请在审批卡片中选择同意或拒绝。"),
            }))
        }
        other => Err(format!("unsupported planner decision kind: {other}")),
    }
}

/// LLM 规划：调 DeepSeek 出 decision JSON，归一后返回（失败 Err 让上层回退）。
async fn llm_plan(messages: &[Value]) -> PortalResult<Value> {
    let safe_messages: Vec<Value> = messages
        .iter()
        .rev()
        .take(12)
        .rev()
        .map(|m| json!({ "role": m.get("role").and_then(|v| v.as_str()).unwrap_or("user"), "content": m.get("content").and_then(|v| v.as_str()).unwrap_or("").chars().take(2000).collect::<String>() }))
        .collect();
    let user_prompt =
        serde_json::to_string_pretty(&json!({ "messages": safe_messages })).unwrap_or_default();
    let req_messages = json!([
        { "role": "system", "content": planner_system_prompt() },
        { "role": "user", "content": user_prompt },
    ]);
    let content = crate::ai::raw_chat_completion(req_messages, true, 0.1).await?;
    let raw = parse_planner_json(&content)
        .ok_or_else(|| crate::error::PortalError::business("LLM planner 未返回 JSON"))?;
    normalize_decision(&raw).map_err(crate::error::PortalError::business)
}

/// LLM 总结：基于工具事件用 DeepSeek 出简洁中文总结（失败回退本地总结）。
///
/// `on_delta` 在每个 token 增量到达时被调用，用于逐字流式输出（emit `assistant_delta`）。
pub async fn build_summary<F>(
    events: &[Value],
    context: &Value,
    messages: &[Value],
    on_delta: F,
) -> String
where
    F: FnMut(&str),
{
    if std::env::var("CMX_AGENT_PLANNER").ok().as_deref() == Some("llm")
        && crate::ai::is_configured()
    {
        match llm_summary(events, context, messages, on_delta).await {
            Ok(s) => return s,
            Err(e) => tracing::warn!("[agentPlanner] LLM 总结失败，回退本地：{e}"),
        }
    }
    build_local_summary(events, context)
}

async fn llm_summary<F>(
    events: &[Value],
    context: &Value,
    messages: &[Value],
    on_delta: F,
) -> PortalResult<String>
where
    F: FnMut(&str),
{
    // 压缩工具事件（截断大字段）
    let tool_events: Vec<Value> = events
        .iter()
        .filter(|e| matches!(e.get("type").and_then(|v| v.as_str()), Some("tool_call" | "tool_result")))
        .map(|e| {
            let mut data = e.get("data").cloned().unwrap_or(Value::Null);
            if let Some(obj) = data.as_object_mut() {
                for (k, max) in [("content", 12000usize), ("html", 12000), ("stdout", 12000), ("stderr", 12000)] {
                    if let Some(s) = obj.get(k).and_then(|v| v.as_str()) {
                        obj.insert(k.to_string(), json!(s.chars().take(max).collect::<String>()));
                    }
                }
            }
            json!({ "type": e.get("type"), "name": e.get("name"), "status": e.get("status"), "summary": e.get("summary"), "args": e.get("args"), "data": data })
        })
        .collect();
    let recent: Vec<Value> = messages.iter().rev().take(8).rev().map(|m| json!({ "role": m.get("role"), "content": m.get("content").and_then(|v| v.as_str()).unwrap_or("").chars().take(2000).collect::<String>() })).collect();
    let user = serde_json::to_string_pretty(
        &json!({ "context": context, "messages": recent, "toolEvents": tool_events }),
    )
    .unwrap_or_default();
    let req = json!([
        { "role": "system", "content": "你是 CMXPortalManager 网页 Agent。请基于工具结果用简洁中文总结，指出关键文件/发现/下一步。不要编造工具结果，不要要求用户复制文件。" },
        { "role": "user", "content": user },
    ]);
    let content = crate::ai::stream_chat_completion(req, 0.2, on_delta).await?;
    Ok(content.trim().chars().take(6000).collect())
}
