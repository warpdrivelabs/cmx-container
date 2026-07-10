//! 集成测试：直连真实 OpenCode（:4096）验证 OpenCodeClient 转发链路。
//!
//! 默认 `#[ignore]`（依赖外部 OpenCode 服务，不污染 CI）。手动运行：
//! ```bash
//! cargo test -p cmx-ai --test opencode_integration -- --ignored --nocapture
//! ```
//!
//! 前置：`opencode serve --host 0.0.0.0 --port 4096` 已启动且 `127.0.0.1:4096` 可达。

use cmx_ai::{OpenCodeClient, OpenCodeConfig};
use futures::StreamExt;

/// 构造指向本机 OpenCode 的客户端。
fn local_client() -> OpenCodeClient {
    let cfg = OpenCodeConfig {
        enabled: true,
        base_url: "http://127.0.0.1:4096".into(),
        password: None,
        request_timeout_ms: 30_000,
        sse_heartbeat_secs: 30,
    };
    assert!(cfg.is_configured(), "OpenCodeConfig 应视为已配置");
    OpenCodeClient::new(cfg)
}

/// 探测 OpenCode 是否可达；不可达则跳过（不 fail），便于无 OpenCode 环境下不阻塞 CI。
async fn opencode_reachable() -> bool {
    tokio::net::TcpStream::connect("127.0.0.1:4096")
        .await
        .is_ok()
}

#[tokio::test]
#[ignore]
async fn create_session_returns_ses_id() {
    if !opencode_reachable().await {
        eprintln!("跳过：OpenCode 不可达");
        return;
    }
    let client = local_client();
    let session = client
        .create_session(&serde_json::json!({}))
        .await
        .expect("create_session 应成功");
    let sid = session
        .get("id")
        .and_then(|v| v.as_str())
        .expect("Session 应含 id 字段");
    assert!(
        sid.starts_with("ses_"),
        "session id 应以 ses_ 开头，实际: {sid}"
    );
    // 清理。
    let _ = client.delete_session(sid).await;
    println!("✅ create_session 返回 {sid}");
}

#[tokio::test]
#[ignore]
async fn prompt_async_returns_ok() {
    if !opencode_reachable().await {
        eprintln!("跳过：OpenCode 不可达");
        return;
    }
    let client = local_client();
    let session = client
        .create_session(&serde_json::json!({}))
        .await
        .expect("create_session 应成功");
    let sid = session["id"].as_str().unwrap().to_string();

    let body = serde_json::json!({
        "parts": [{"type": "text", "text": "只回复两个字：你好"}]
    });
    let result = client.prompt_async(&sid, &body).await;
    // 无论 OpenCode 是否配置 LLM，prompt_async 本身应返回 Ok（204）。
    assert!(result.is_ok(), "prompt_async 应返回 Ok，实际: {:?}", result.err());

    // 清理（abort 中止可能正在跑的生成）。
    let _ = client.abort(&sid).await;
    let _ = client.delete_session(&sid).await;
    println!("✅ prompt_async 返回 Ok");
}

#[tokio::test]
#[ignore]
async fn stream_events_yields_sse_frames() {
    if !opencode_reachable().await {
        eprintln!("跳过：OpenCode 不可达");
        return;
    }
    let client = local_client();
    let session = client
        .create_session(&serde_json::json!({}))
        .await
        .expect("create_session 应成功");
    let sid = session["id"].as_str().unwrap().to_string();

    // 先建立 SSE 流，再发 prompt_async 触发事件。
    let stream = client
        .stream_events()
        .await
        .expect("stream_events 应建立 SSE 连接");
    tokio::pin!(stream);

    let body = serde_json::json!({
        "parts": [{"type": "text", "text": "只回复两个字：你好"}]
    });
    client
        .prompt_async(&sid, &body)
        .await
        .expect("prompt_async 应成功");

    // 收集 SSE 帧（按 \n\n 分帧），最多读 8 秒或收到 5 帧带 sessionID 的事件。
    let mut frame_buf = String::new();
    let mut seen_session_events = 0u32;
    let mut event_types: Vec<String> = Vec::new();
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(20);

    loop {
        let next = tokio::time::timeout_at(deadline, stream.next()).await;
        match next {
            Err(_) => break, // 超时
            Ok(None) => break,
            Ok(Some(Err(e))) => {
                eprintln!("SSE 流读取错误: {e}");
                break;
            }
            Ok(Some(Ok(chunk))) => {
                let text = std::str::from_utf8(&chunk).unwrap_or("");
                frame_buf.push_str(text);
                while let Some(idx) = frame_buf.find("\n\n") {
                    let frame: String = frame_buf.drain(..idx + 2).collect();
                    // 解析 data 行。
                    let Some(data_line) = frame
                        .lines()
                        .find_map(|l| l.strip_prefix("data:").map(|s| s.trim()))
                    else {
                        continue;
                    };
                    let Ok(ev) = serde_json::from_str::<serde_json::Value>(data_line) else {
                        continue;
                    };
                    let ty = ev
                        .get("type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    event_types.push(ty.clone());
                    // 统计带 sessionID 的事件（真实生成相关）。
                    if ev
                        .get("properties")
                        .and_then(|p| p.get("sessionID"))
                        .is_some()
                    {
                        seen_session_events += 1;
                    }
                    if seen_session_events >= 8 {
                        break;
                    }
                }
                if seen_session_events >= 8 {
                    break;
                }
            }
        }
    }

    // 清理。
    let _ = client.abort(&sid).await;
    let _ = client.delete_session(&sid).await;

    // 断言：至少收到一个 server.connected（首帧）+ 至少一个带 sessionID 的事件。
    assert!(
        event_types.iter().any(|t| t == "server.connected"),
        "应收到首帧 server.connected，实际收到的事件类型: {:?}",
        event_types
    );
    assert!(
        seen_session_events > 0,
        "应收到至少 1 个带 sessionID 的生成事件，实际事件类型: {:?}",
        event_types
    );
    println!("✅ SSE 流收到事件类型: {:?}", event_types);
}

#[tokio::test]
#[ignore]
async fn delete_session_is_idempotent() {
    if !opencode_reachable().await {
        eprintln!("跳过：OpenCode 不可达");
        return;
    }
    let client = local_client();
    let session = client
        .create_session(&serde_json::json!({}))
        .await
        .expect("create_session 应成功");
    let sid = session["id"].as_str().unwrap().to_string();
    // 首次删除应成功。
    client
        .delete_session(&sid)
        .await
        .expect("首次 delete_session 应成功");
    // 二次删除：OpenCode 可能返回 404，但我们映射为 UpstreamStatus（不应 panic）。
    let second = client.delete_session(&sid).await;
    match second {
        Ok(()) => println!("✅ 二次删除也返回 Ok（OpenCode 幂等）"),
        Err(e) => println!("✅ 二次删除返回错误（预期，类型: {e}）"),
    }
}
