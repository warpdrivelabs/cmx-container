//! Builder（W1 核心）—— 在隔离目录跑 `cargo build --target wasm32-wasip1` 编译插件 wasm。
//!
//! **可测性**：命令执行经 [`CommandRunner`] trait 注入——生产用 [`TokioCommandRunner`]（真跑
//! cargo，流式日志），单测用 mock（不碰真 cargo/网络）。Builder 本身只负责"组 argv → 跑 → 定位
//! 产物 → 哈希"，与"进程怎么跑"解耦。
//!
//! **不在平台运行时进程内**：Builder 由独立 Build Service/worker 持有；平台运行时永不 spawn cargo。

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::model::{BuildArtifact, BuildRequest};

/// 构建错误。
#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    #[error("编译失败（exit={code:?}）: {tail}")]
    Compile { code: Option<i32>, tail: String },
    #[error("编译超时（{0:?}）")]
    Timeout(Duration),
    #[error("未找到产物 wasm: {0}")]
    ArtifactNotFound(String),
    #[error("IO 错误: {0}")]
    Io(String),
}

/// 命令执行结果。
pub struct RunOutput {
    pub exit_code: Option<i32>,
    /// 合并的 stdout+stderr 全量日志。
    pub log: String,
}

/// 命令执行器（依赖注入接缝，便于单测）。
#[async_trait]
pub trait CommandRunner: Send + Sync {
    /// 在 `cwd` 执行 `program args...`，注入 `envs` 环境变量，`timeout` 内完成；行级日志经 `on_line`。
    async fn run(
        &self,
        program: &str,
        args: &[String],
        cwd: &str,
        envs: &[(String, String)],
        timeout: Duration,
        on_line: Arc<dyn Fn(String) + Send + Sync>,
    ) -> Result<RunOutput, BuildError>;
}

/// 生产实现：tokio 子进程，逐行捕获 stdout+stderr，支持超时 kill。
pub struct TokioCommandRunner;

#[async_trait]
impl CommandRunner for TokioCommandRunner {
    async fn run(
        &self,
        program: &str,
        args: &[String],
        cwd: &str,
        envs: &[(String, String)],
        timeout: Duration,
        on_line: Arc<dyn Fn(String) + Send + Sync>,
    ) -> Result<RunOutput, BuildError> {
        use tokio::process::Command;
        let mut cmd = Command::new(program);
        cmd.args(args)
            .current_dir(cwd)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (k, v) in envs {
            cmd.env(k, v);
        }
        let mut child = cmd
            .spawn()
            .map_err(|e| BuildError::Io(format!("启动 {program} 失败: {e}")))?;

        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let mut log = String::new();

        let mut out_lines = BufReader::new(stdout).lines();
        let mut err_lines = BufReader::new(stderr).lines();

        let pump = async {
            loop {
                tokio::select! {
                    line = out_lines.next_line() => match line {
                        Ok(Some(l)) => { on_line(l.clone()); log.push_str(&l); log.push('\n'); }
                        _ => break,
                    },
                    line = err_lines.next_line() => {
                        if let Ok(Some(l)) = line { on_line(l.clone()); log.push_str(&l); log.push('\n'); }
                    },
                }
            }
            // 排空 stderr 剩余。
            while let Ok(Some(l)) = err_lines.next_line().await {
                on_line(l.clone());
                log.push_str(&l);
                log.push('\n');
            }
        };

        let status = tokio::time::timeout(timeout, async {
            pump.await;
            child.wait().await
        })
        .await;

        match status {
            Err(_) => {
                let _ = child.start_kill();
                Err(BuildError::Timeout(timeout))
            }
            Ok(Ok(st)) => Ok(RunOutput {
                exit_code: st.code(),
                log,
            }),
            Ok(Err(e)) => Err(BuildError::Io(format!("等待子进程失败: {e}"))),
        }
    }
}

/// 构建缓存配置 —— 共享 CARGO_HOME + target 目录，加速增量构建。
#[derive(Debug, Clone, Default)]
pub struct CacheConfig {
    /// 共享 `CARGO_HOME`（registry/git 缓存）。None=不注入（用系统默认）。
    pub cargo_home: Option<String>,
    /// 共享 `CARGO_TARGET_DIR`（编译产物缓存）。None=各工程自带 target/。
    pub target_dir: Option<String>,
    /// `sccache` 等编译缓存包装器路径（设则注入 `RUSTC_WRAPPER`）。
    pub rustc_wrapper: Option<String>,
}

impl CacheConfig {
    /// 展开为环境变量对（供 CommandRunner 注入）。
    pub fn to_envs(&self) -> Vec<(String, String)> {
        let mut envs = Vec::new();
        if let Some(h) = &self.cargo_home {
            envs.push(("CARGO_HOME".to_string(), h.clone()));
        }
        if let Some(t) = &self.target_dir {
            envs.push(("CARGO_TARGET_DIR".to_string(), t.clone()));
        }
        if let Some(w) = &self.rustc_wrapper {
            envs.push(("RUSTC_WRAPPER".to_string(), w.clone()));
        }
        envs
    }

    /// 是否配置了任一缓存项。
    pub fn is_enabled(&self) -> bool {
        self.cargo_home.is_some() || self.target_dir.is_some() || self.rustc_wrapper.is_some()
    }
}

/// Builder 配置。
pub struct BuilderConfig {
    /// 编译超时。
    pub timeout: Duration,
    /// 日志尾部保留字节数。
    pub log_tail_bytes: usize,
    /// 缓存配置（共享 CARGO_HOME / target / sccache）。
    pub cache: CacheConfig,
}

impl Default for BuilderConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(600),
            log_tail_bytes: 8 * 1024,
            cache: CacheConfig::default(),
        }
    }
}

/// 插件 wasm 构建器。
pub struct Builder {
    runner: Arc<dyn CommandRunner>,
    cfg: BuilderConfig,
}

impl Builder {
    pub fn new(runner: Arc<dyn CommandRunner>) -> Self {
        Self {
            runner,
            cfg: BuilderConfig::default(),
        }
    }

    pub fn with_config(runner: Arc<dyn CommandRunner>, cfg: BuilderConfig) -> Self {
        Self { runner, cfg }
    }

    /// 组 cargo argv（暴露供测试断言）。
    pub fn cargo_args(req: &BuildRequest) -> Vec<String> {
        let mut args = vec![
            "build".to_string(),
            "--target".to_string(),
            req.target.clone(),
        ];
        if req.profile == "release" {
            args.push("--release".to_string());
        }
        if !req.features.is_empty() {
            args.push("--features".to_string());
            args.push(req.features.join(","));
        }
        args
    }

    /// 执行一次构建。`on_line` 流式日志回调（SSE/WS 前端消费）。成功返回产物 + 内容哈希。
    pub async fn build(
        &self,
        req: &BuildRequest,
        on_line: Arc<dyn Fn(String) + Send + Sync>,
    ) -> Result<BuildArtifact, BuildError> {
        let args = Self::cargo_args(req);
        let envs = self.cfg.cache.to_envs();
        let out = self
            .runner
            .run("cargo", &args, &req.plugin_path, &envs, self.cfg.timeout, on_line)
            .await?;

        if out.exit_code != Some(0) {
            return Err(BuildError::Compile {
                code: out.exit_code,
                tail: tail(&out.log, self.cfg.log_tail_bytes),
            });
        }

        // 产物根：配了 CARGO_TARGET_DIR 则用它，否则 <plugin_path>/target。
        let target_root = self
            .cfg
            .cache
            .target_dir
            .clone()
            .unwrap_or_else(|| format!("{}/target", req.plugin_path.trim_end_matches('/')));
        let wasm = locate_wasm(&target_root, &req.target, &req.profile)
            .ok_or_else(|| BuildError::ArtifactNotFound(req.plugin_path.clone()))?;
        let bytes = std::fs::read(&wasm).map_err(|e| BuildError::Io(format!("读产物失败: {e}")))?;
        let rev = content_rev(&bytes);

        Ok(BuildArtifact {
            wasm_path: wasm.to_string_lossy().to_string(),
            rev,
            log_tail: tail(&out.log, self.cfg.log_tail_bytes),
        })
    }
}

/// 定位产物 wasm（首个 .wasm 文件）。`target_root` 是 target 目录（含缓存重定向后的路径）。
fn locate_wasm(target_root: &str, target: &str, profile: &str) -> Option<PathBuf> {
    let profile_dir = if profile == "release" { "release" } else { "debug" };
    let dir = PathBuf::from(target_root).join(target).join(profile_dir);
    let entries = std::fs::read_dir(&dir).ok()?;
    for e in entries.flatten() {
        let p = e.path();
        if p.extension().map(|x| x == "wasm").unwrap_or(false) {
            return Some(p);
        }
    }
    None
}

/// 内容哈希（FNV-1a 64，十六进制；不引 crypto 依赖，仅作幂等/版本对齐）。
fn content_rev(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

/// 取日志尾部（按字节，尽量对齐字符边界）。
fn tail(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    let start = s.len() - max_bytes;
    // 向后找到 char 边界。
    let mut i = start;
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    format!("…（省略前段）\n{}", &s[i..])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::BuildRequest;
    use std::sync::Mutex;

    fn req() -> BuildRequest {
        BuildRequest {
            workspace_id: "w1".into(),
            plugin_path: "/tmp/nonexistent-plugin".into(),
            target: "wasm32-wasip1".into(),
            features: vec!["extism".into()],
            profile: "release".into(),
            auto_publish: false,
            tenant_id: None,
            submitted_by: None,
        }
    }

    #[test]
    fn cargo_args_release_with_features() {
        let a = Builder::cargo_args(&req());
        assert_eq!(
            a,
            vec![
                "build".to_string(),
                "--target".into(),
                "wasm32-wasip1".into(),
                "--release".into(),
                "--features".into(),
                "extism".into(),
            ]
        );
    }

    #[test]
    fn cargo_args_debug_no_features() {
        let mut r = req();
        r.profile = "debug".into();
        r.features = vec![];
        let a = Builder::cargo_args(&r);
        assert_eq!(a, vec!["build", "--target", "wasm32-wasip1"]);
    }

    #[test]
    fn content_rev_deterministic_and_distinct() {
        assert_eq!(content_rev(b"hello"), content_rev(b"hello"));
        assert_ne!(content_rev(b"hello"), content_rev(b"world"));
    }

    #[test]
    fn tail_truncates() {
        let s = "x".repeat(100);
        let t = tail(&s, 10);
        assert!(t.len() < 100);
        assert!(t.contains("省略前段"));
    }

    // mock runner：不跑真 cargo，回放预设 exit/log。
    struct MockRunner {
        exit: Option<i32>,
        lines: Vec<String>,
        seen: Mutex<Vec<String>>,
    }
    #[async_trait]
    impl CommandRunner for MockRunner {
        async fn run(
            &self,
            program: &str,
            args: &[String],
            _cwd: &str,
            envs: &[(String, String)],
            _timeout: Duration,
            on_line: Arc<dyn Fn(String) + Send + Sync>,
        ) -> Result<RunOutput, BuildError> {
            self.seen.lock().unwrap().push(format!("{program} {}", args.join(" ")));
            // 记录收到的缓存 env（供 cache 测试断言）。
            for (k, v) in envs {
                self.seen.lock().unwrap().push(format!("ENV {k}={v}"));
            }
            let mut log = String::new();
            for l in &self.lines {
                on_line(l.clone());
                log.push_str(l);
                log.push('\n');
            }
            Ok(RunOutput {
                exit_code: self.exit,
                log,
            })
        }
    }

    #[tokio::test]
    async fn build_compile_failure_reports_tail() {
        let runner = Arc::new(MockRunner {
            exit: Some(101),
            lines: vec!["error[E0433]: boom".into()],
            seen: Mutex::new(Vec::new()),
        });
        let b = Builder::new(runner.clone());
        let logs = Arc::new(Mutex::new(Vec::<String>::new()));
        let lg = logs.clone();
        let on_line: Arc<dyn Fn(String) + Send + Sync> =
            Arc::new(move |l| lg.lock().unwrap().push(l));
        let err = b.build(&req(), on_line).await.unwrap_err();
        assert!(matches!(err, BuildError::Compile { code: Some(101), .. }));
        // 命令确实是 cargo build --target ...
        assert!(runner.seen.lock().unwrap()[0].starts_with("cargo build --target wasm32-wasip1"));
        // 流式日志确有回调。
        assert!(logs.lock().unwrap().iter().any(|l| l.contains("boom")));
    }

    #[tokio::test]
    async fn build_success_but_no_artifact_errors() {
        // exit=0 但目录无 wasm（路径不存在）→ ArtifactNotFound。
        let runner = Arc::new(MockRunner {
            exit: Some(0),
            lines: vec!["Compiling ok".into(), "Finished release".into()],
            seen: Mutex::new(Vec::new()),
        });
        let b = Builder::new(runner);
        let on_line: Arc<dyn Fn(String) + Send + Sync> = Arc::new(|_| {});
        let err = b.build(&req(), on_line).await.unwrap_err();
        assert!(matches!(err, BuildError::ArtifactNotFound(_)));
    }

    #[test]
    fn cache_config_to_envs() {
        let c = CacheConfig {
            cargo_home: Some("/cache/cargo".into()),
            target_dir: Some("/cache/target".into()),
            rustc_wrapper: Some("sccache".into()),
        };
        let envs = c.to_envs();
        assert!(c.is_enabled());
        assert!(envs.contains(&("CARGO_HOME".into(), "/cache/cargo".into())));
        assert!(envs.contains(&("CARGO_TARGET_DIR".into(), "/cache/target".into())));
        assert!(envs.contains(&("RUSTC_WRAPPER".into(), "sccache".into())));
        assert!(!CacheConfig::default().is_enabled());
    }

    #[tokio::test]
    async fn build_injects_cache_envs_into_runner() {
        let runner = Arc::new(MockRunner {
            exit: Some(101), // 让它编译失败即返回，不必真有产物。
            lines: vec![],
            seen: Mutex::new(Vec::new()),
        });
        let cfg = BuilderConfig {
            cache: CacheConfig {
                cargo_home: Some("/shared/cargo".into()),
                target_dir: Some("/shared/target".into()),
                rustc_wrapper: None,
            },
            ..Default::default()
        };
        let b = Builder::with_config(runner.clone(), cfg);
        let _ = b.build(&req(), Arc::new(|_| {})).await; // 忽略结果，只看 env 注入。
        let seen = runner.seen.lock().unwrap();
        assert!(seen.iter().any(|s| s == "ENV CARGO_HOME=/shared/cargo"), "应注入 CARGO_HOME");
        assert!(seen.iter().any(|s| s == "ENV CARGO_TARGET_DIR=/shared/target"), "应注入 CARGO_TARGET_DIR");
    }
}
