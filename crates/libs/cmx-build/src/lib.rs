//! cmx-build —— 服务端编译流水线核心（W1）。
//!
//! 提供"源码 → wasm"的隔离编译机制：[`Builder`] 在独立目录跑 `cargo build --target wasm32-wasip1`，
//! 流式回传日志、超时熔断、定位产物并哈希。命令执行经 [`CommandRunner`] 注入，故 [`Builder`] 可脱离
//! 真 cargo 单测。作业模型 [`BuildJob`] / 状态机 [`BuildStatus`] / 存储契约 [`BuildJobStore`] 供平台层
//! 装配（排队、落库、串链 doc→签名→deploy）。
//!
//! **边界**：本 crate 零 extism / 零平台运行时依赖；`cargo` 子进程只在持有 [`Builder`] 的独立
//! Build Service/worker 里 spawn，**绝不进平台运行时进程**。

pub mod builder;
pub mod executor;
pub mod global;
pub mod model;
pub mod pipeline;
pub mod quota;
pub mod store;

pub use builder::{BuildError, Builder, BuilderConfig, CacheConfig, CommandRunner, RunOutput, TokioCommandRunner};
pub use executor::{BuildExecutor, BuildLogEvent, SubmitOutcome};
pub use model::{BuildArtifact, BuildJob, BuildRequest, BuildStatus};
pub use pipeline::{BuildPipeline, Deployer, DocScanner, PipelineResult, Signer};
pub use quota::{BuildPermit, QuotaConfig, QuotaDenied, QuotaGuard};
pub use store::BuildJobStore;
