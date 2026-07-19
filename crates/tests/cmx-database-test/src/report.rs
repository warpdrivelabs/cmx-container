//! 计时、统计与报告表格。

use std::time::Duration;

/// 对一组数取中位数（不要求已排序）。
pub fn median(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut v = values.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    }
}

/// 变异系数（stddev/mean，百分比）——衡量多轮测量的离散程度。
fn cv_pct(values: &[f64]) -> f64 {
    let n = values.len();
    if n < 2 {
        return 0.0;
    }
    let mean = values.iter().sum::<f64>() / n as f64;
    if mean == 0.0 {
        return 0.0;
    }
    let var = values.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n as f64 - 1.0);
    (var.sqrt() / mean) * 100.0
}

/// 单次测量结果。
#[derive(Debug, Clone)]
pub struct Measure {
    /// 场景标签（如 "insert/row-by-row"）。
    pub scenario: String,
    /// 驱动名（"sqlx" / "tokio-postgres"）。
    pub driver: String,
    /// 处理的行数。
    pub rows: u64,
    /// 耗时。
    pub elapsed: Duration,
}

impl Measure {
    pub fn new(
        scenario: impl Into<String>,
        driver: impl Into<String>,
        rows: u64,
        elapsed: Duration,
    ) -> Self {
        Self {
            scenario: scenario.into(),
            driver: driver.into(),
            rows,
            elapsed,
        }
    }

    /// 吞吐（行/秒）。
    pub fn rows_per_sec(&self) -> f64 {
        if self.elapsed.as_secs_f64() <= 0.0 {
            0.0
        } else {
            self.rows as f64 / self.elapsed.as_secs_f64()
        }
    }

    pub fn ms(&self) -> f64 {
        self.elapsed.as_secs_f64() * 1000.0
    }
}

/// 多轮聚合结果：同一 (场景, 驱动) 跑 N 轮后的中位数 / 最快 / 变异系数。
#[derive(Debug, Clone)]
pub struct AggMeasure {
    pub scenario: String,
    pub driver: String,
    pub rows: u64,
    pub rounds: usize,
    /// 中位吞吐（行/秒）——主报告值。
    pub median_rps: f64,
    /// 最快一轮吞吐（行/秒）。
    pub best_rps: f64,
    /// 中位耗时（ms）。
    pub median_ms: f64,
    /// 吞吐的变异系数（%）——衡量稳定性。
    pub cv_pct: f64,
}

impl AggMeasure {
    /// 从同一场景多轮 `Measure` 聚合（要求 scenario/driver/rows 一致）。
    pub fn from_runs(runs: &[Measure]) -> Self {
        assert!(!runs.is_empty(), "AggMeasure::from_runs 需要至少一轮");
        let first = &runs[0];
        let rps: Vec<f64> = runs.iter().map(|m| m.rows_per_sec()).collect();
        let ms: Vec<f64> = runs.iter().map(|m| m.ms()).collect();
        Self {
            scenario: first.scenario.clone(),
            driver: first.driver.clone(),
            rows: first.rows,
            rounds: runs.len(),
            median_rps: median(&rps),
            best_rps: rps.iter().cloned().fold(0.0_f64, f64::max),
            median_ms: median(&ms),
            cv_pct: cv_pct(&rps),
        }
    }
}

/// 延迟分布测量（点查场景）。
#[derive(Debug, Clone)]
pub struct LatencyStats {
    pub scenario: String,
    pub driver: String,
    pub samples: usize,
    pub p50_us: f64,
    pub p95_us: f64,
    pub p99_us: f64,
    pub mean_us: f64,
}

impl LatencyStats {
    /// 从一组单次延迟（微秒）计算分位数。
    pub fn from_micros(
        scenario: impl Into<String>,
        driver: impl Into<String>,
        mut micros: Vec<f64>,
    ) -> Self {
        micros.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let n = micros.len();
        let pct = |p: f64| -> f64 {
            if n == 0 {
                return 0.0;
            }
            let idx = ((p / 100.0) * (n as f64 - 1.0)).round() as usize;
            micros[idx.min(n - 1)]
        };
        let mean = if n == 0 {
            0.0
        } else {
            micros.iter().sum::<f64>() / n as f64
        };
        Self {
            scenario: scenario.into(),
            driver: driver.into(),
            samples: n,
            p50_us: pct(50.0),
            p95_us: pct(95.0),
            p99_us: pct(99.0),
            mean_us: mean,
        }
    }
}

/// 打印吞吐对比表（多轮聚合：中位吞吐 + 最快 + 变异系数；同场景 sqlx vs tokio-postgres 相邻）。
pub fn print_throughput_table(measures: &[AggMeasure]) -> String {
    let mut out = String::new();
    out.push_str("\n## 吞吐对比（插入 / 查询，多轮取中位数）\n\n");
    out.push_str("| 场景 | 驱动 | 行数 | 轮数 | 中位耗时(ms) | 中位吞吐(行/秒) | 最快(行/秒) | 波动CV | 相对 |\n");
    out.push_str("|------|------|------|------|--------------|-----------------|-------------|--------|------|\n");

    // 按 scenario 分组，组内 sqlx / tokio-postgres 相邻
    let mut scenarios: Vec<String> = Vec::new();
    for m in measures {
        if !scenarios.contains(&m.scenario) {
            scenarios.push(m.scenario.clone());
        }
    }

    for sc in &scenarios {
        let group: Vec<&AggMeasure> = measures.iter().filter(|m| &m.scenario == sc).collect();
        // 以 sqlx 的中位吞吐为基准算相对倍数
        let base = group
            .iter()
            .find(|m| m.driver == "sqlx")
            .map(|m| m.median_rps)
            .filter(|v| *v > 0.0);
        for m in &group {
            let rel = match base {
                Some(b) if b > 0.0 => format!("{:.2}x", m.median_rps / b),
                _ => "-".to_string(),
            };
            out.push_str(&format!(
                "| {} | {} | {} | {} | {:.1} | {:.0} | {:.0} | {:.1}% | {} |\n",
                m.scenario,
                m.driver,
                m.rows,
                m.rounds,
                m.median_ms,
                m.median_rps,
                m.best_rps,
                m.cv_pct,
                rel
            ));
        }
    }
    out
}

/// 打印延迟对比表。
pub fn print_latency_table(stats: &[LatencyStats]) -> String {
    let mut out = String::new();
    out.push_str("\n## 点查延迟对比（微秒）\n\n");
    out.push_str("| 场景 | 驱动 | 样本 | 均值 | P50 | P95 | P99 |\n");
    out.push_str("|------|------|------|------|-----|-----|-----|\n");
    for s in stats {
        out.push_str(&format!(
            "| {} | {} | {} | {:.1} | {:.1} | {:.1} | {:.1} |\n",
            s.scenario, s.driver, s.samples, s.mean_us, s.p50_us, s.p95_us, s.p99_us
        ));
    }
    out
}
