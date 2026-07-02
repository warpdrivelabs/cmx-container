use anyhow::Result;
use extism::PluginBuilder;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;
use std::time::Instant;

pub mod plugin;

lazy_static! {
    pub static ref DEBUG_SESSIONS: Mutex<HashMap<String, DebugSession>> =
        Mutex::new(HashMap::new());
}

static CLEANUP_RUNNING: AtomicBool = AtomicBool::new(false);

fn start_cleanup_thread() {
    if CLEANUP_RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }

    thread::spawn(|| {
        loop {
            thread::sleep(Duration::from_millis(500));
            cleanup_dead_sessions();
        }
    });
}

#[derive(Debug, Clone)]
pub struct DebugSession {
    pub id: String,
    pub plugin_id: String,
    pub plugin_name: String,
    pub plugin_version: String,
    pub function_name: String,
    pub wasm_path: String,
    pub source_path: String,
    pub cmx_pid: u32,
    pub start_time: Instant,
    pub is_active: bool,
    pub is_protected: bool,
    pub previous_output: JsonValue,
    pub initial_input: JsonValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebugRequest {
    pub function: String,
    pub args: Vec<serde_json::Value>,
    pub data: serde_json::Value,
    #[serde(rename = "self")]
    pub is_self: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct DebugResponse {
    pub code: i32,
    pub source_path: String,
    pub wasm_path: String,
    pub code_server_url: Option<String>,
    pub plugin: String,
    pub functions: Vec<WasmFunctionInfo>,
    pub cmx_pid: u32,
    pub debug_function: String,
    pub message: Option<String>,
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct WasmFunctionInfo {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvokeResponse {
    pub code: i32,
    pub result: Option<JsonValue>,
    pub error: Option<String>,
}

pub fn get_session(session_id: &str) -> Option<DebugSession> {
    let sessions = DEBUG_SESSIONS.lock().unwrap();
    sessions.get(session_id).cloned()
}

pub fn create_session(session: DebugSession) {
    let mut sessions = DEBUG_SESSIONS.lock().unwrap();
    sessions.insert(session.id.clone(), session);
}

pub fn remove_session(session_id: &str) -> Option<DebugSession> {
    let mut sessions = DEBUG_SESSIONS.lock().unwrap();
    sessions.remove(session_id)
}

pub fn get_active_session() -> Option<DebugSession> {
    let sessions = DEBUG_SESSIONS.lock().unwrap();
    sessions.values().find(|s| s.is_active).cloned()
}

pub fn clear_all_sessions() {
    let mut sessions = DEBUG_SESSIONS.lock().unwrap();
    sessions.clear();
}

pub fn is_debugger_attached(target_pid: u32) -> bool {
    let lldb_pids = if let Ok(output) = std::process::Command::new("pgrep")
        .args(["-f", "codelldb|lldb"])
        .output()
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        stdout
            .trim()
            .split('\n')
            .filter(|s| !s.is_empty())
            .map(|s| s.trim().to_string())
            .collect::<Vec<_>>()
    } else {
        vec![]
    };

    if lldb_pids.is_empty() {
        return false;
    }

    tracing::info!("[cmx-debug] Found lldb/codelldb PIDs: {:?}", lldb_pids);

    for pid in &lldb_pids {
        if let Ok(lsof_output) = std::process::Command::new("lsof")
            .args(["-p", pid])
            .output()
        {
            let lsof_str = String::from_utf8_lossy(&lsof_output.stdout);
            if lsof_str.contains("cmx-container") {
                tracing::info!("[cmx-debug] lldb进程 {} 通过lsof附加到了cmx-container", pid);
                return true;
            }
        }
        if let Ok(fds) = std::fs::read_dir(format!("/proc/{}/fd", pid)) {
            for fd in fds.flatten() {
                if let Ok(link) = std::fs::read_link(fd.path()) {
                    let link_str = link.to_string_lossy();
                    if link_str.contains(&target_pid.to_string()) {
                        tracing::info!(
                            "[cmx-debug] lldb进程 {} 通过fd附加到目标进程 {}",
                            pid,
                            target_pid
                        );
                        return true;
                    }
                }
            }
        }
    }

    tracing::info!("[cmx-debug] lldb进程存在但未附加到目标进程，保守返回true");
    true
}

pub fn cleanup_dead_sessions() {
    let mut sessions = DEBUG_SESSIONS.lock().unwrap();
    let active_count = sessions.len();
    // log::info!(
    //     "[cmx-debug] cleanup_dead_sessions called, active sessions: {}",
    //     active_count
    // );
    if active_count > 10 {
        // log::warn!(
        //     "[cmx-debug] Many active sessions ({}), cleanup may be needed",
        //     active_count
        // );
    }
    sessions.retain(|_session_id, session| {
        let attached = is_debugger_attached(session.cmx_pid);
        if session.is_protected {
            if attached {
                // log::info!(
                //     "[cmx-debug] Session {} is protected and debugger is attached, unprotecting",
                //     session_id
                // );
                session.is_protected = false;
                return true;
            } else {
                // log::info!(
                //     "[cmx-debug] Session {} is protected (debugger starting), keeping",
                //     session_id
                // );
                return true;
            }
        }
        // log::info!(
        //     "[cmx-debug] Session {} (cmx_pid={}), is_debugger_attached={}",
        //     session_id,
        //     session.cmx_pid,
        //     attached
        // );
        if !attached {
            // log::info!(
            //     "[cmx-debug] No debugger attached for {}, cleaning up session",
            //     session_id
            // );
            return false;
        }
        //log::info!("[cmx-debug] Session {} is alive, keeping", session_id);
        true
    });
}

pub fn init() {
    start_cleanup_thread();
    tracing::info!("[cmx-debug] Debug session manager initialized");
}

pub fn get_code_server_url() -> String {
    std::env::var("CODE_SERVER_URL")
        .unwrap_or_else(|_| "https://dev.cloudmatrix.one:18080".to_string())
}

pub async fn get_code_server_url_async() -> String {
    if let Ok(url) = std::env::var("CODE_SERVER_URL")
        && !url.is_empty()
    {
        tracing::info!("[cmx-debug] Using CODE_SERVER_URL from env: {}", url);
        return url;
    }

    let plugin_port = std::env::var("PLUGIN_PORT").unwrap_or_else(|_| "9000".to_string());
    let url = format!("http://localhost:{}/config", plugin_port);

    tracing::info!("[cmx-debug] Fetching code_server_url from: {}", url);

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(2))
        .build();

    if let Ok(client) = client
        && let Ok(resp) = client.get(&url).send().await
        && let Ok(json) = resp.json::<serde_json::Value>().await
        && let Some(code_server_url) = json.get("code_server_url").and_then(|v| v.as_str())
    {
        tracing::info!(
            "[cmx-debug] Got code_server_url from config: {}",
            code_server_url
        );
        return code_server_url.to_string();
    }

    tracing::warn!("[cmx-debug] Failed to get code_server_url, using default");
    "https://dev.cloudmatrix.one:18080".to_string()
}

pub fn call_plugin_function(
    wasm_bytes: &[u8],
    func_name: &str,
    input: &JsonValue,
) -> Result<JsonValue> {
    // SAFETY: `std::env::set_var` 在多线程环境下可能引发数据竞争，此处安全的前提是：
    // 当前进程内没有其他线程并发读写 `EXTISM_DEBUG` 环境变量。该变量仅在下方
    // `PluginBuilder::build()` 期间被 extism 运行时读取，设置后立即构建并在构建完成后移除，
    // 假设调用方未并发触发同一调试流程。
    unsafe {
        std::env::set_var("EXTISM_DEBUG", "1");
    }
    let mut plugin = PluginBuilder::new(wasm_bytes).with_wasi(true).build()?;
    // SAFETY: `std::env::remove_var` 在多线程环境下可能引发数据竞争，此处安全的前提是：
    // 与上方 `set_var` 配对，且没有其他线程并发读写 `EXTISM_DEBUG`。
    // 插件构建已完成，移除该变量以避免影响后续操作。
    unsafe {
        std::env::remove_var("EXTISM_DEBUG");
    }

    let input_bytes = serde_json::to_vec(input)?;
    let result = plugin.call::<&[u8], &[u8]>(func_name, &input_bytes)?;
    let result_str = String::from_utf8_lossy(result);

    if result_str.is_empty() {
        return Ok(serde_json::json!({ "success": true, "data": { "result": null } }));
    }

    match serde_json::from_str::<JsonValue>(&result_str) {
        Ok(json) => Ok(json),
        Err(_) => Ok(serde_json::json!({
            "success": true,
            "data": { "result": result_str.to_string() }
        })),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartDebugRequest {
    pub function: String,
    pub args: Vec<JsonValue>,
    pub data: JsonValue,
}

#[allow(clippy::too_many_arguments)]
pub fn start_debug_session(
    plugin_id: String,
    plugin_name: String,
    plugin_version: String,
    function_name: String,
    wasm_path: String,
    source_path: String,
    wasm_functions: Vec<WasmFunctionInfo>,
    previous_output: JsonValue,
    initial_input: JsonValue,
) -> DebugResponse {
    let cmx_pid = std::process::id();
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .expect("系统时钟异常: 当前时间早于 UNIX_EPOCH")
        .as_millis();
    let session_id = format!("debug_{}_{}", plugin_name, timestamp);

    let session = DebugSession {
        id: session_id.clone(),
        plugin_id,
        plugin_name: plugin_name.clone(),
        plugin_version: plugin_version.clone(),
        function_name: function_name.clone(),
        wasm_path: wasm_path.clone(),
        source_path: source_path.clone(),
        cmx_pid,
        start_time: std::time::Instant::now(),
        is_active: true,
        is_protected: true,
        previous_output,
        initial_input,
    };

    create_session(session);

    DebugResponse {
        code: 0,
        source_path,
        wasm_path,
        code_server_url: Some(get_code_server_url()),
        plugin: plugin_name,
        functions: wasm_functions,
        cmx_pid,
        debug_function: function_name,
        message: None,
        session_id: Some(session_id),
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn start_debug_session_async(
    plugin_id: String,
    plugin_name: String,
    plugin_version: String,
    function_name: String,
    wasm_path: String,
    source_path: String,
    wasm_functions: Vec<WasmFunctionInfo>,
    previous_output: JsonValue,
    initial_input: JsonValue,
) -> DebugResponse {
    let cmx_pid = std::process::id();
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .expect("系统时钟异常: 当前时间早于 UNIX_EPOCH")
        .as_millis();
    let session_id = format!("debug_{}_{}", plugin_name, timestamp);

    let session = DebugSession {
        id: session_id.clone(),
        plugin_id,
        plugin_name: plugin_name.clone(),
        plugin_version: plugin_version.clone(),
        function_name: function_name.clone(),
        wasm_path: wasm_path.clone(),
        source_path: source_path.clone(),
        cmx_pid,
        start_time: std::time::Instant::now(),
        is_active: true,
        is_protected: true,
        previous_output,
        initial_input,
    };

    create_session(session);

    DebugResponse {
        code: 0,
        source_path,
        wasm_path,
        code_server_url: Some(get_code_server_url_async().await),
        plugin: plugin_name,
        functions: wasm_functions,
        cmx_pid,
        debug_function: function_name,
        message: None,
        session_id: Some(session_id),
    }
}
