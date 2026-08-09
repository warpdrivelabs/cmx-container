//! 拓扑活体探测集成测试。
//!
//! 直接驱动真实的 `set_topology_provider` + `spawn_topology_prober` + `topology_snapshot`，
//! 对一个真实存活的下游 HTTP 服务做活体探测。需要环境变量 `PROBE_TARGET` 指向一个暴露
//! `/_mon/tech-stats` 的服务（如本地 flow-server:8091）；未设则跳过（不误报失败）。
//!
//! 运行：`PROBE_TARGET=http://127.0.0.1:8091 cargo test -p cmx-web-monitor --test topology_probe -- --nocapture`
//!
//! 注意：`set_topology_provider` 是进程级 `OnceLock`，故全测试文件共用**一个** provider（返回
//! 一条 proxy + 一条 embedded），在单个测试里一并断言两类能力的快照形态。

use cmx_web_monitor::{ServiceDep, set_topology_provider, spawn_topology_prober, topology_snapshot};

/// 一条 proxy（flow，target 来自 PROBE_TARGET）+ 一条 embedded（report，无 target）。
fn provider() -> Vec<ServiceDep> {
    vec![
        ServiceDep {
            key: "flow".into(),
            label: "流程引擎".into(),
            mode: "proxy".into(),
            target: std::env::var("PROBE_TARGET").ok(),
            proxiable: true,
        },
        ServiceDep {
            key: "report".into(),
            label: "报表引擎".into(),
            mode: "embedded".into(),
            target: None,
            proxiable: false,
        },
    ]
}

#[tokio::test]
async fn probes_live_downstream_and_skips_embedded() {
    let Some(target) = std::env::var("PROBE_TARGET").ok().filter(|s| !s.is_empty()) else {
        eprintln!("PROBE_TARGET 未设，跳过活体探测集成测试");
        return;
    };

    set_topology_provider(provider);
    spawn_topology_prober();

    // 探测器每 10s 一轮，interval 首 tick 即刻。给足几秒完成一次探测。
    let mut snap = None;
    for _ in 0..15 {
        tokio::time::sleep(std::time::Duration::from_millis(800)).await;
        let s = topology_snapshot().await;
        // flow 是第一条（proxy）。等它出探测结果。
        if !s["services"][0]["probe"].is_null() {
            snap = Some(s);
            break;
        }
    }
    let snap = snap.expect("探测器应在几秒内产出一次探测结果");
    let services = snap["services"].as_array().expect("services 应为数组");
    eprintln!("拓扑快照: {}", serde_json::to_string_pretty(&snap).unwrap());

    // —— proxy 能力（flow）——
    let flow = &services[0];
    assert_eq!(flow["key"], "flow");
    assert_eq!(flow["mode"], "proxy");
    assert_eq!(flow["target"], target);
    let probe = &flow["probe"];
    assert_eq!(probe["reachable"], true, "下游应可达（探测: {probe}）");
    assert!(probe["latencyMs"].as_u64().is_some(), "可达时应有延迟毫秒");
    assert!(
        probe["remoteService"].as_str().is_some(),
        "应从对端 /_mon/tech-stats 解析出 service.name"
    );

    // —— embedded 能力（report）不探测 ——
    let report = &services[1];
    assert_eq!(report["key"], "report");
    assert_eq!(report["mode"], "embedded");
    assert!(
        report["probe"].is_null(),
        "embedded 能力不应有探测结果（就在本进程，恒可达）"
    );
}
