//! 系统技术指标采集（CPU / 内存 / 网络 / 磁盘），基于 sysinfo。
//!
//! 设计：**绝不在请求路径里调 sysinfo::refresh**（CPU% 需两次采样间隔、开销大）。而是一个后台
//! tokio 任务持有常驻 `System`/`Networks`/`Disks`，每 3s 刷新一次并算好快照写进程级 OnceLock；
//! [`system_snapshot`] 只克隆快照。网络字节按采样间隔算每秒速率（相邻两次累计值之差）。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::Serialize;

/// 单块磁盘（挂载点容量）。
#[derive(Debug, Clone, Default, Serialize)]
pub struct DiskInfo {
    pub mount: String,
    pub total: u64,
    pub available: u64,
}

/// 系统指标快照（后台采样器周期性刷新）。
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemMetrics {
    /// 本进程常驻内存（字节）。
    pub proc_mem_bytes: u64,
    /// 本进程 CPU 占用（%，单核归一后 sysinfo 语义）。
    pub proc_cpu_pct: f32,
    /// 本进程运行时长（秒）。
    pub proc_uptime_secs: u64,
    /// 主机内存总量（字节）。
    pub host_mem_total: u64,
    /// 主机已用内存（字节）。
    pub host_mem_used: u64,
    /// 主机 CPU 总占用（%）。
    pub host_cpu_pct: f32,
    /// CPU 核数。
    pub cpu_count: usize,
    /// 负载均值（1/5/15 分钟）。
    pub load_one: f64,
    pub load_five: f64,
    pub load_fifteen: f64,
    /// 主机开机时长（秒）。
    pub host_uptime_secs: u64,
    /// 网络累计接收 / 发送（字节）。
    pub net_rx_bytes: u64,
    pub net_tx_bytes: u64,
    /// 网络每秒速率（按采样间隔算）。
    pub net_rx_per_sec: u64,
    pub net_tx_per_sec: u64,
    /// 磁盘列表。
    pub disks: Vec<DiskInfo>,
    /// 相对进程启动的采样时刻（毫秒）。
    pub sampled_at_ms: u64,
}

fn snapshot_cell() -> &'static Mutex<SystemMetrics> {
    static SNAP: OnceLock<Mutex<SystemMetrics>> = OnceLock::new();
    SNAP.get_or_init(|| Mutex::new(SystemMetrics::default()))
}

/// 读当前系统指标快照（后台未起或首刷未完成时返回默认值）。
pub fn system_snapshot() -> SystemMetrics {
    snapshot_cell()
        .lock()
        .map(|m| m.clone())
        .unwrap_or_default()
}

/// 启动后台系统采样器（幂等；服务启动时调一次）。
pub fn spawn_system_sampler() {
    static STARTED: AtomicBool = AtomicBool::new(false);
    if STARTED.swap(true, Ordering::SeqCst) {
        return; // 已启动
    }
    tokio::spawn(async move {
        run_sampler().await;
    });
}

async fn run_sampler() {
    use sysinfo::{Disks, Networks, System};

    let started = Instant::now();
    let interval_secs = 3u64;
    let mut sys = System::new_all();
    let mut networks = Networks::new_with_refreshed_list();
    let mut disks = Disks::new_with_refreshed_list();

    // 首刷 CPU 需要一个间隔才有意义。
    sys.refresh_cpu_all();
    tokio::time::sleep(Duration::from_millis(
        sysinfo::MINIMUM_CPU_UPDATE_INTERVAL.as_millis() as u64 + 50,
    ))
    .await;

    let pid = sysinfo::get_current_pid().ok();
    let mut ticker = tokio::time::interval(Duration::from_secs(interval_secs));

    // 上一轮网络累计值（算每秒速率）。
    let mut prev_rx: u64 = 0;
    let mut prev_tx: u64 = 0;
    let mut primed = false;

    loop {
        ticker.tick().await;

        sys.refresh_cpu_all();
        sys.refresh_memory();
        if let Some(pid) = pid {
            sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[pid]), true);
        }
        networks.refresh(true);
        disks.refresh(true);

        // 进程指标。
        let (proc_mem_bytes, proc_cpu_pct, proc_uptime_secs) = pid
            .and_then(|pid| sys.process(pid))
            .map(|p| (p.memory(), p.cpu_usage(), p.run_time()))
            .unwrap_or((0, 0.0, 0));

        // 主机指标。
        let host_mem_total = sys.total_memory();
        let host_mem_used = sys.used_memory();
        let host_cpu_pct = sys.global_cpu_usage();
        let cpu_count = sys.cpus().len();
        let load = System::load_average();
        let host_uptime_secs = System::uptime();

        // 网络累计 + 速率。
        let (mut rx, mut tx) = (0u64, 0u64);
        for (_iface, data) in networks.iter() {
            rx = rx.saturating_add(data.total_received());
            tx = tx.saturating_add(data.total_transmitted());
        }
        let (net_rx_per_sec, net_tx_per_sec) = if primed {
            (
                rx.saturating_sub(prev_rx) / interval_secs,
                tx.saturating_sub(prev_tx) / interval_secs,
            )
        } else {
            (0, 0)
        };
        prev_rx = rx;
        prev_tx = tx;
        primed = true;

        // 磁盘。
        let disk_list: Vec<DiskInfo> = disks
            .iter()
            .map(|d| DiskInfo {
                mount: d.mount_point().to_string_lossy().to_string(),
                total: d.total_space(),
                available: d.available_space(),
            })
            .collect();

        let metrics = SystemMetrics {
            proc_mem_bytes,
            proc_cpu_pct,
            proc_uptime_secs,
            host_mem_total,
            host_mem_used,
            host_cpu_pct,
            cpu_count,
            load_one: load.one,
            load_five: load.five,
            load_fifteen: load.fifteen,
            host_uptime_secs,
            net_rx_bytes: rx,
            net_tx_bytes: tx,
            net_rx_per_sec,
            net_tx_per_sec,
            disks: disk_list,
            sampled_at_ms: started.elapsed().as_millis() as u64,
        };

        if let Ok(mut slot) = snapshot_cell().lock() {
            *slot = metrics;
        }
    }
}
