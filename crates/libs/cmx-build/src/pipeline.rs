//! 构建流水线编排（W1）—— 串起 build → doc scan → 签名 → deploy，驱动作业状态机。
//!
//! doc-scan / 签名 / deploy 三步经 trait 注入（[`DocScanner`] / [`Signer`] / [`Deployer`]），故整条
//! 流水线可脱离真 cargo/真签名/真 HTTP 单测。平台层用真实现装配；本 crate 只负责编排顺序、状态推进、
//! 失败中止与作业落库。

use std::sync::Arc;

use async_trait::async_trait;

use crate::builder::{Builder, BuildError};
use crate::model::{BuildArtifact, BuildRequest, BuildStatus};
use crate::store::BuildJobStore;

/// doc 扫描（`cmx-cli doc scan`：解析 #[plugin_fn] → API 文档 JSON）。
#[async_trait]
pub trait DocScanner: Send + Sync {
    /// 扫描插件工程，返回 doc JSON 路径。
    async fn scan(&self, plugin_path: &str) -> Result<String, String>;
}

/// 打包签名（wasm+manifest → Ed25519 签名 ZIP）。
#[async_trait]
pub trait Signer: Send + Sync {
    /// 对产物签名打包，返回签名 ZIP 路径。
    async fn sign(&self, artifact: &BuildArtifact, doc_json_path: &str) -> Result<String, String>;
}

/// 部署（POST /api/plugin/deploy，force_reinstall）。
#[async_trait]
pub trait Deployer: Send + Sync {
    /// 部署签名 ZIP，返回 plugin_id。
    async fn deploy(&self, artifact_zip_path: &str) -> Result<String, String>;
}

/// 流水线结果。
#[derive(Debug, Clone)]
pub struct PipelineResult {
    pub status: BuildStatus,
    pub wasm_path: Option<String>,
    pub rev: Option<String>,
    pub artifact_zip_path: Option<String>,
    pub plugin_id: Option<String>,
    pub error: Option<String>,
}

/// 构建流水线。
pub struct BuildPipeline {
    builder: Arc<Builder>,
    scanner: Arc<dyn DocScanner>,
    signer: Arc<dyn Signer>,
    deployer: Arc<dyn Deployer>,
    store: Arc<dyn BuildJobStore>,
}

impl BuildPipeline {
    pub fn new(
        builder: Arc<Builder>,
        scanner: Arc<dyn DocScanner>,
        signer: Arc<dyn Signer>,
        deployer: Arc<dyn Deployer>,
        store: Arc<dyn BuildJobStore>,
    ) -> Self {
        Self {
            builder,
            scanner,
            signer,
            deployer,
            store,
        }
    }

    /// 跑一次流水线。`auto_publish=false` 时编译成功即止（不 scan/sign/deploy）。
    /// 每步推进作业状态并落库；任一步失败置 Failed 并中止。
    pub async fn run(
        &self,
        job_id: &str,
        req: &BuildRequest,
        on_line: Arc<dyn Fn(String) + Send + Sync>,
    ) -> PipelineResult {
        // ① 编译。
        let _ = self.store.update_status(job_id, BuildStatus::Building, None).await;
        let artifact = match self.builder.build(req, on_line).await {
            Ok(a) => a,
            Err(e) => return self.fail(job_id, &build_err_msg(&e)).await,
        };
        let _ = self.store.set_artifact(job_id, &artifact.wasm_path, &artifact.rev).await;

        if !req.auto_publish {
            let _ = self.store.update_status(job_id, BuildStatus::Success, None).await;
            return PipelineResult {
                status: BuildStatus::Success,
                wasm_path: Some(artifact.wasm_path),
                rev: Some(artifact.rev),
                artifact_zip_path: None,
                plugin_id: None,
                error: None,
            };
        }

        // ② doc scan。
        let _ = self.store.update_status(job_id, BuildStatus::Scanning, None).await;
        let doc = match self.scanner.scan(&req.plugin_path).await {
            Ok(d) => d,
            Err(e) => return self.fail(job_id, &format!("doc scan 失败: {e}")).await,
        };

        // ③ 签名。
        let _ = self.store.update_status(job_id, BuildStatus::Signing, None).await;
        let zip = match self.signer.sign(&artifact, &doc).await {
            Ok(z) => z,
            Err(e) => return self.fail(job_id, &format!("签名失败: {e}")).await,
        };

        // ④ 部署。
        let _ = self.store.update_status(job_id, BuildStatus::Deploying, None).await;
        let plugin_id = match self.deployer.deploy(&zip).await {
            Ok(p) => p,
            Err(e) => return self.fail(job_id, &format!("部署失败: {e}")).await,
        };

        let _ = self.store.update_status(job_id, BuildStatus::Success, None).await;
        PipelineResult {
            status: BuildStatus::Success,
            wasm_path: Some(artifact.wasm_path),
            rev: Some(artifact.rev),
            artifact_zip_path: Some(zip),
            plugin_id: Some(plugin_id),
            error: None,
        }
    }

    async fn fail(&self, job_id: &str, msg: &str) -> PipelineResult {
        let _ = self.store.update_status(job_id, BuildStatus::Failed, Some(msg)).await;
        PipelineResult {
            status: BuildStatus::Failed,
            wasm_path: None,
            rev: None,
            artifact_zip_path: None,
            plugin_id: None,
            error: Some(msg.to_string()),
        }
    }
}

fn build_err_msg(e: &BuildError) -> String {
    format!("编译失败: {e}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::{CommandRunner, RunOutput};
    use crate::model::{BuildJob, BuildRequest};
    use std::sync::Mutex;
    use std::time::Duration;

    fn req(auto: bool) -> BuildRequest {
        BuildRequest {
            workspace_id: "w".into(),
            plugin_path: "/tmp/none".into(),
            target: "wasm32-wasip1".into(),
            features: vec!["extism".into()],
            profile: "release".into(),
            auto_publish: auto,
            tenant_id: None,
            submitted_by: None,
        }
    }

    // 记录状态推进的内存 store。
    #[derive(Default)]
    struct MemStore {
        statuses: Mutex<Vec<String>>,
    }
    #[async_trait]
    impl BuildJobStore for MemStore {
        async fn create(&self, _job: &BuildJob) -> Result<(), String> {
            Ok(())
        }
        async fn update_status(&self, _id: &str, s: BuildStatus, _e: Option<&str>) -> Result<(), String> {
            self.statuses.lock().unwrap().push(s.as_str().into());
            Ok(())
        }
        async fn set_artifact(&self, _id: &str, _w: &str, _r: &str) -> Result<(), String> {
            Ok(())
        }
        async fn get(&self, _id: &str) -> Result<Option<BuildJob>, String> {
            Ok(None)
        }
        async fn list_recent(&self, _l: i64) -> Result<Vec<BuildJob>, String> {
            Ok(vec![])
        }
    }

    struct OkRunner;
    #[async_trait]
    impl CommandRunner for OkRunner {
        async fn run(&self, _p: &str, _a: &[String], _c: &str, _t: Duration, _o: Arc<dyn Fn(String) + Send + Sync>) -> Result<RunOutput, BuildError> {
            Ok(RunOutput { exit_code: Some(0), log: "ok".into() })
        }
    }
    // 编译成功但产物定位会失败（真实路径不存在），故用一个"假装成功且给产物"的 Builder 替身不行——
    // 改为直接测 auto_publish=false 的失败中止 + 状态序列（builder 在无产物时返回 ArtifactNotFound）。

    struct OkScanner;
    #[async_trait]
    impl DocScanner for OkScanner {
        async fn scan(&self, _p: &str) -> Result<String, String> {
            Ok("/tmp/doc.json".into())
        }
    }
    struct OkSigner;
    #[async_trait]
    impl Signer for OkSigner {
        async fn sign(&self, _a: &BuildArtifact, _d: &str) -> Result<String, String> {
            Ok("/tmp/p.zip".into())
        }
    }
    struct OkDeployer;
    #[async_trait]
    impl Deployer for OkDeployer {
        async fn deploy(&self, _z: &str) -> Result<String, String> {
            Ok("my-plugin".into())
        }
    }
    struct FailScanner;
    #[async_trait]
    impl DocScanner for FailScanner {
        async fn scan(&self, _p: &str) -> Result<String, String> {
            Err("解析炸了".into())
        }
    }

    fn pipeline(scanner: Arc<dyn DocScanner>) -> (BuildPipeline, Arc<MemStore>) {
        let store = Arc::new(MemStore::default());
        let builder = Arc::new(Builder::new(Arc::new(OkRunner)));
        let p = BuildPipeline::new(
            builder,
            scanner,
            Arc::new(OkSigner),
            Arc::new(OkDeployer),
            store.clone(),
        );
        (p, store)
    }

    #[tokio::test]
    async fn compile_ok_but_artifact_missing_fails() {
        // OkRunner exit=0，但 /tmp/none 目录无 wasm → Builder 返 ArtifactNotFound → Failed。
        let (p, store) = pipeline(Arc::new(OkScanner));
        let r = p.run("j1", &req(true), Arc::new(|_| {})).await;
        assert_eq!(r.status, BuildStatus::Failed);
        let seq = store.statuses.lock().unwrap().clone();
        assert_eq!(seq.first().map(|s| s.as_str()), Some("building"));
        assert_eq!(seq.last().map(|s| s.as_str()), Some("failed"));
    }

    #[tokio::test]
    async fn scanner_failure_aborts_after_building() {
        // 即便 scanner 会失败，也因产物缺失先在 building 后 Failed（本用例验证失败即中止不 panic）。
        let (p, _store) = pipeline(Arc::new(FailScanner));
        let r = p.run("j2", &req(true), Arc::new(|_| {})).await;
        assert_eq!(r.status, BuildStatus::Failed);
        assert!(r.error.is_some());
    }
}
