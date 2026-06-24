//! 基础服务中心客户端模块。
//!
//! 提供插件生命周期与外部基础服务中心（门户中心、权限中心、表单中心、流程中心）
//! 之间的数据交互能力。
//!
//! # 职责
//!
//! - 在插件安装/升级/降级时，将业务数据目录打包为 ZIP 并发送到对应中心。
//! - 在插件卸载时，通知各中心清理关联数据。
//! - 支持并行调用各中心接口，汇总成功/失败结果。
//!
//! # 架构
//!
//! - `ServiceCenterSender` trait 定义统一的发送/清理接口。
//! - `CenterDataDispatcher` 调度器负责并行分发和结果汇总。
//! - `MockServiceCenterSender` 提供 Mock 实现（默认）。
//! - `HttpServiceCenterSender` 提供 HTTP form-data 实现（`http_url`/`http_discovery` 模式）。
//! - `GrpcServiceCenterSender` 提供 gRPC 实现（`grpc` 模式）。
//! - `CenterClientConfig` 从 `dev.toml` 加载配置。

pub mod config;
pub mod dispatcher;
pub mod grpc_sender;
pub mod http_sender;
pub mod mock_sender;
pub mod packer;
pub mod sender;
pub mod types;

pub use config::CenterClientConfig;
pub use dispatcher::CenterDataDispatcher;
pub use grpc_sender::GrpcServiceCenterSender;
pub use http_sender::HttpServiceCenterSender;
pub use mock_sender::MockServiceCenterSender;
pub use sender::{CenterError, ServiceCenterSender};
pub use types::{DataCategory, DispatchContext, DispatchResult};
