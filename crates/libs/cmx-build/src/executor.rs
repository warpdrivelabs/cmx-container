//! 构建执行器（W1 last-mile）—— 后台异步跑 Pipeline，日志经 broadcast 扇出供 SSE。
//!
//! **守铁律**：编译在**后台 tokio task**（`tokio::spawn`）里跑，绝不在 HTTP 请求线程里 spawn cargo。
//! 端点提交作业 → [`BuildExecutor::submit`] 立刻返回 job_id 并在后台启动流水线；每行编译日志经
//! per-job broadcast channel 扇出，SSE 端点用 [`BuildExecutor::subscribe`] 订阅实时消费。
//!
//! 载体简单（进程内 DashMap<job_id, Sender>）；集群级可后续换 cmx-job JobHandler（有暂停/恢复/心跳）。

use std::sync::Arc;

use tokio::sync::broadcast;

use crate::model::BuildRequest;
use crate::pipeline::{BuildPipeline, PipelineResult};
use crate::quota::{QuotaConfig, QuotaGuard};

/// 一条构建日志事件。
#[derive(Debug, Clone)]
pub enum BuildLogEvent {
    /// 一行编译日志。
    Line(String),
    /// 终止（携带最终状态字符串）。
    Done { status: String, error: Option<String> },
}

/// 提交结果。
pub enum SubmitOutcome {
    /// 已受理，后台构建中；返回初始日志订阅端。
    Accepted(broadcast::Receiver<BuildLogEvent>),
    /// 被配额拒绝。
    Denied(String),
}

/// 后台构建执行器。
#[derive(Clone)]
pub struct BuildExecutor {
    pipeline: Arc<BuildPipeline>,
    /// per-job 日志广播（订阅者可为 0；容量满则最旧被丢，SSE 侧容忍 Lagged）。
    channels: Arc<dashmap::DashMap<String, broadcast::Sender<BuildLogEvent>>>,
    quota: Arc<QuotaGuard>,
    cap: usize,
}

impl BuildExecutor {
    pub fn new(pipeline: Arc<BuildPipeline>) -> Self {
        Self::with_quota(pipeline, QuotaConfig::default())
    }

    /// 带配额构造。
    pub fn with_quota(pipeline: Arc<BuildPipeline>, quota: QuotaConfig) -> Self {
        Self {
            pipeline,
            channels: Arc::new(dashmap::DashMap::new()),
            quota: Arc::new(QuotaGuard::new(quota)),
            cap: 1024,
        }
    }

    /// 当前在跑构建数（诊断/大盘）。
    pub fn running(&self) -> usize {
        self.quota.running()
    }

    /// 订阅某作业的日志流（SSE 端点用）。作业未在跑则返回 None。
    pub fn subscribe(&self, job_id: &str) -> Option<broadcast::Receiver<BuildLogEvent>> {
        self.channels.get(job_id).map(|s| s.subscribe())
    }

    /// 提交构建（**配额门控**）：放行则后台启动流水线、返回日志订阅端；超配额则 [`SubmitOutcome::Denied`]。
    ///
    /// `quota_key` 用于频控（工作区 id / 租户）；permit 随构建生命周期持有，构建结束自动释放并发名额。
    pub fn submit(&self, job_id: String, req: BuildRequest) -> SubmitOutcome {
        let now_min = chrono::Utc::now().timestamp() / 60;
        let permit = match self.quota.try_acquire(&req.workspace_id, now_min) {
            Ok(p) => p,
            Err(reason) => return SubmitOutcome::Denied(reason.to_string()),
        };

        let (tx, rx0) = broadcast::channel::<BuildLogEvent>(self.cap);
        self.channels.insert(job_id.clone(), tx.clone());

        let pipeline = self.pipeline.clone();
        let channels = self.channels.clone();
        let jid = job_id.clone();

        tokio::spawn(async move {
            // permit 移入 task：构建期间占并发名额，task 结束自动 drop 释放。
            let _permit = permit;
            let line_tx = tx.clone();
            let on_line: Arc<dyn Fn(String) + Send + Sync> = Arc::new(move |l| {
                let _ = line_tx.send(BuildLogEvent::Line(l));
            });

            let result: PipelineResult = pipeline.run(&jid, &req, on_line).await;
            let _ = tx.send(BuildLogEvent::Done {
                status: format!("{:?}", result.status),
                error: result.error.clone(),
            });
            channels.remove(&jid);
        });
        SubmitOutcome::Accepted(rx0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::{Builder, CommandRunner, RunOutput};
    use crate::model::{BuildArtifact, BuildJob, BuildStatus};
    use crate::pipeline::{BuildPipeline, Deployer, DocScanner, Signer};
    use crate::store::BuildJobStore;
    use async_trait::async_trait;
    use std::time::Duration;

    struct OkRunner;
    #[async_trait]
    impl CommandRunner for OkRunner {
        async fn run(&self, _p: &str, _a: &[String], _c: &str, _e: &[(String, String)], _t: Duration, on_line: Arc<dyn Fn(String) + Send + Sync>) -> Result<RunOutput, crate::builder::BuildError> {
            on_line("Compiling foo".into());
            Ok(RunOutput { exit_code: Some(0), log: "Compiling foo\n".into() })
        }
    }
    struct Noop;
    #[async_trait]
    impl BuildJobStore for Noop {
        async fn create(&self, _j: &BuildJob) -> Result<(), String> { Ok(()) }
        async fn update_status(&self, _i: &str, _s: BuildStatus, _e: Option<&str>) -> Result<(), String> { Ok(()) }
        async fn set_artifact(&self, _i: &str, _w: &str, _r: &str) -> Result<(), String> { Ok(()) }
        async fn get(&self, _i: &str) -> Result<Option<BuildJob>, String> { Ok(None) }
        async fn list_recent(&self, _l: i64) -> Result<Vec<BuildJob>, String> { Ok(vec![]) }
    }
    #[async_trait]
    impl DocScanner for Noop {
        async fn scan(&self, _p: &str) -> Result<String, String> { Ok("d".into()) }
    }
    #[async_trait]
    impl Signer for Noop {
        async fn sign(&self, _a: &BuildArtifact, _d: &str) -> Result<String, String> { Ok("z".into()) }
    }
    #[async_trait]
    impl Deployer for Noop {
        async fn deploy(&self, _z: &str) -> Result<String, String> { Ok("p".into()) }
    }

    fn req() -> BuildRequest {
        BuildRequest {
            workspace_id: "w".into(),
            plugin_path: "/tmp/none-exec".into(),
            target: "wasm32-wasip1".into(),
            features: vec!["extism".into()],
            profile: "release".into(),
            auto_publish: false,
            tenant_id: None,
            submitted_by: None,
        }
    }

    #[tokio::test]
    async fn submit_streams_line_then_done() {
        let pipeline = Arc::new(BuildPipeline::new(
            Arc::new(Builder::new(Arc::new(OkRunner))),
            Arc::new(Noop),
            Arc::new(Noop),
            Arc::new(Noop),
            Arc::new(Noop),
        ));
        let exec = BuildExecutor::new(pipeline);

        // 用 submit 返回的订阅端（消除竞态）。
        let mut rx = match exec.submit("j-exec".into(), req()) {
            SubmitOutcome::Accepted(rx) => rx,
            SubmitOutcome::Denied(m) => panic!("不应被拒: {m}"),
        };

        let mut saw_line = false;
        let mut saw_done = false;
        // 最多收 10 条，够覆盖 Line + Done。
        for _ in 0..10 {
            match tokio::time::timeout(Duration::from_secs(2), rx.recv()).await {
                Ok(Ok(BuildLogEvent::Line(l))) => {
                    if l.contains("Compiling") { saw_line = true; }
                }
                Ok(Ok(BuildLogEvent::Done { status, .. })) => {
                    saw_done = true;
                    // OkRunner exit=0 但 /tmp/none-exec 无 wasm → 产物缺失 → Failed。
                    assert_eq!(status, "Failed");
                    break;
                }
                _ => break,
            }
        }
        assert!(saw_line, "应收到编译日志行");
        assert!(saw_done, "应收到 Done 事件");
    }

    #[tokio::test]
    async fn subscribe_unknown_job_is_none() {
        let pipeline = Arc::new(BuildPipeline::new(
            Arc::new(Builder::new(Arc::new(OkRunner))),
            Arc::new(Noop),
            Arc::new(Noop),
            Arc::new(Noop),
            Arc::new(Noop),
        ));
        let exec = BuildExecutor::new(pipeline);
        assert!(exec.subscribe("nope").is_none());
    }

    // 慢 runner：让首个构建占住并发名额，好测第二个被拒。
    struct SlowRunner;
    #[async_trait]
    impl CommandRunner for SlowRunner {
        async fn run(&self, _p: &str, _a: &[String], _c: &str, _e: &[(String, String)], _t: Duration, _o: Arc<dyn Fn(String) + Send + Sync>) -> Result<RunOutput, crate::builder::BuildError> {
            tokio::time::sleep(Duration::from_millis(300)).await;
            Ok(RunOutput { exit_code: Some(0), log: String::new() })
        }
    }

    #[tokio::test]
    async fn quota_denies_over_concurrency() {
        use crate::quota::QuotaConfig;
        let pipeline = Arc::new(BuildPipeline::new(
            Arc::new(Builder::new(Arc::new(SlowRunner))),
            Arc::new(Noop),
            Arc::new(Noop),
            Arc::new(Noop),
            Arc::new(Noop),
        ));
        let exec = BuildExecutor::with_quota(
            pipeline,
            QuotaConfig { max_concurrent: 1, max_per_min: 0, max_disk_bytes: 0 },
        );
        // 首个受理并占名额（慢 runner 让它还在跑）。
        let r1 = exec.submit("j1".into(), req());
        assert!(matches!(r1, SubmitOutcome::Accepted(_)));
        // 立刻提交第二个 → 并发满被拒。
        let r2 = exec.submit("j2".into(), req());
        assert!(matches!(r2, SubmitOutcome::Denied(_)), "并发上限 1，第二个应被拒");
    }
}
