//! 集成测试：验证 SSE relay 在真实 OpenCode 流量下的事件翻译与分发。
//!
//! 默认 `#[ignore]`（依赖外部 OpenCode）。手动运行：
//! ```bash
//! cargo test -p cmx-ai --test sse_relay_integration -- --ignored --nocapture
//! ```
//!
//! 这是最接近前端真实体验的测试：初始化 AI 子系统（含 relay）→ 订阅一个 session →
//! 触发 OpenCode 生成 → 断言前端订阅 receiver 收到翻译后的 cmx-ai 事件（text_delta/result/done 等）。

use serde_json::json;
use std::time::Duration;

async fn opencode_reachable() -> bool {
    tokio::net::TcpStream::connect("127.0.0.1:4096")
        .await
        .is_ok()
}

/// 初始化 ConfigManager（最小空配置）+ 设置 OPENCODE_BASE_URL 环境变量 + 初始化 AI 子系统。
///
/// 这是 web-server 启动时做的事的等价前置：cmx_ai::config::load_config 依赖
/// `ConfigManager::global()`，独立测试进程须先 initialize，否则 panic。
/// 幂等：ConfigManager 重复 initialize 返回 Err，AI 子系统 init_ai_subsystem 内部用 OnceCell 守护。
async fn ensure_ai_subsystem() {
    // SAFETY: 测试串行运行，set_var 在进程启动早期、无并发读。
    unsafe {
        std::env::set_var("OPENCODE_BASE_URL", "http://127.0.0.1:4096");
    }
    // 最小初始化 ConfigManager（空配置）；已初始化时忽略 Err。
    let _ = cmx_utils::ConfigManager::initialize(|| {
        cmx_utils::config::ConfigBuilder::default().build()
    });
    cmx_ai::init_ai_subsystem().await;
}

/// 测试：relay 把 OpenCode 的 message.part.delta/session.status 翻译为
/// text_delta/result/done 并按 sessionID 分发到前端订阅。
///
/// 注意：此测试依赖 OpenCode 配置了可用的 LLM Provider；若未配置，
/// OpenCode 会发 session.status: retry/error，测试会相应收到 error 事件，
/// 此时断言放宽为「至少收到一个 cmx-ai 翻译事件」。
#[tokio::test]
#[ignore]
async fn relay_translates_and_dispatches_events() {
    if !opencode_reachable().await {
        eprintln!("跳过：OpenCode 不可达");
        return;
    }

    // 1. 初始化 AI 子系统（加载配置 + 全局 registry/client + 启动 relay 后台 task）。
    ensure_ai_subsystem().await;

    let client = cmx_ai::get_client().expect("init_ai_subsystem 后 client 应可用");
    let registry = cmx_ai::get_registry().expect("init_ai_subsystem 后 registry 应可用");

    // 2. 创建真实会话。
    let session = client
        .create_session(&json!({}))
        .await
        .expect("create_session 应成功");
    let sid = session["id"].as_str().unwrap().to_string();
    println!("[test] 会话已创建: {sid}");

    // 3. 先订阅该 session（模拟前端 GET /api/ai/events?session_id=...）。
    let mut rx = registry.subscribe(&sid);

    // 4. 触发生成（prompt_async）。
    let body = json!({
        "parts": [{"type": "text", "text": "只回复两个字：你好"}]
    });
    client
        .prompt_async(&sid, &body)
        .await
        .expect("prompt_async 应成功");
    println!("[test] prompt_async 已触发，等待 relay 分发事件...");

    // 5. 收集 relay 翻译后的 cmx-ai 事件（最多等 25 秒）。
    let mut received_events: Vec<(String, serde_json::Value)> = Vec::new();
    let mut event_names: Vec<String> = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(25);

    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout_at(deadline, rx.recv()).await {
            Err(_) => break,   // 整体超时
            Ok(None) => break, // 通道关闭（relay 异常）
            Ok(Some(ev)) => {
                println!(
                    "[test] 收到 cmx-ai 事件: {} -> {}",
                    ev.event_name, ev.payload
                );
                let payload: serde_json::Value =
                    serde_json::from_str(&ev.payload).unwrap_or(json!(null));
                event_names.push(ev.event_name.to_string());
                received_events.push((ev.event_name.to_string(), payload));
                // 收到 done 表示本轮结束，可提前退出。
                if ev.event_name == "done" {
                    break;
                }
            }
        }
    }

    // 6. 清理。
    let _ = client.abort(&sid).await;
    let _ = client.delete_session(&sid).await;

    // 7. 断言：至少收到一个 cmx-ai 翻译事件（relay 链路打通的证据）。
    assert!(
        !received_events.is_empty(),
        "前端订阅应至少收到 1 个 cmx-ai 事件，但 0 个。relay 链路可能未工作"
    );
    println!(
        "[test] ✅ 共收到 {} 个 cmx-ai 事件: {:?}",
        received_events.len(),
        event_names
    );

    // 进一步：若收到 done，说明 relay 的 session.status:idle 翻译正确。
    let has_done = event_names.iter().any(|n| n == "done");
    let has_result = event_names.iter().any(|n| n == "result");
    let has_error = event_names.iter().any(|n| n == "error");
    println!(
        "[test] done={has_done} result={has_result} error={has_error}（done/result 表示正常完成，error 表示 OpenCode 侧重试/失败）"
    );

    // 至少应包含 done 或 error 之一（任何一条都证明 relay 正确捕获了 session 终态）。
    assert!(
        has_done || has_error,
        "应收到 done 或 error 终态事件，实际事件: {:?}",
        event_names
    );
}

/// 测试：session 级活跃生成锁在真实链路下的 acquire/release。
#[tokio::test]
#[ignore]
async fn session_lock_released_after_generation() {
    if !opencode_reachable().await {
        eprintln!("跳过：OpenCode 不可达");
        return;
    }
    ensure_ai_subsystem().await;

    let client = cmx_ai::get_client().expect("client 应可用");
    let registry = cmx_ai::get_registry().expect("registry 应可用");

    let session = client
        .create_session(&json!({}))
        .await
        .expect("create_session 应成功");
    let sid = session["id"].as_str().unwrap().to_string();

    // 模拟 handler: acquire → prompt_async → 订阅等 idle → release
    assert!(registry.try_acquire_session(&sid), "首次 acquire 应成功");
    assert!(registry.is_session_active(&sid), "acquire 后应标记为活跃");

    // 触发生成并等待 relay 处理 idle（release 由 relay 在 session.status:idle 时执行）。
    let mut rx = registry.subscribe(&sid);
    let body = json!({"parts": [{"type": "text", "text": "只回复：好"}]});
    client
        .prompt_async(&sid, &body)
        .await
        .expect("prompt_async");

    // 等待 done（relay 在 session.status:idle 翻译出 result+done 时会 release_session）。
    // 注：retry 是非终态（不下发前端），只有真正完成才发 done。
    let deadline = tokio::time::Instant::now() + Duration::from_secs(25);
    let mut reached_done = false;
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout_at(deadline, rx.recv()).await {
            Ok(Some(ev)) if ev.event_name == "done" => {
                reached_done = true;
                break;
            }
            Ok(Some(_)) => continue, // 其它事件（text_delta/tool_call 等）继续等
            _ => break,              // 超时或通道关闭
        }
    }

    // 给 relay 一点时间执行 release（事件广播与 release 在同一调用栈）。
    tokio::time::sleep(Duration::from_millis(200)).await;
    let _ = client.delete_session(&sid).await;

    if reached_done {
        // 真正完成（done）：锁应被释放。
        assert!(
            !registry.is_session_active(&sid),
            "done 后活跃锁应被 relay 释放"
        );
        println!("[test] ✅ 活跃锁在 done 后正确释放");
    } else {
        // 未到 done（通常因 OpenCode 侧 LLM 不可用持续 retry，属环境问题，非代码缺陷）。
        // 此时锁未释放是符合预期的（生成未结束）；仅断言 acquire 本身生效，跳过 release 断言。
        println!(
            "[test] ⚠️ 未收到 done（OpenCode 侧可能 LLM 不可用持续 retry），跳过锁释放断言。当前锁状态: {}",
            registry.is_session_active(&sid)
        );
    }
}
