//! gRPC 设施（吸收自退役的 cmx-rpc；feature 门控）。
//!
//! - `grpc-client`：客户端设施——服务发现桥接（[`RegistryAwareDiscover`）、共享
//!   [`GrpcInfrastructure`]、重试（[`with_retry`]）、出站鉴权（[`apply_auth_metadata`]）、
//!   全局初始化守卫（[`GlobalRpcClient`]）；
//! - `grpc-server`（蕴含 grpc-client）：服务端设施——领域 Bundle 装配接口
//!   （[`RpcServiceBundle`]）、gRPC Server 启动器（[`start_grpc_server`]）、
//!   入站鉴权（[`AuthVerifier`]）。
//!
//! proto 契约集中在 `cmx-rpc-gen`（单一 contract crate），领域皮肤在 `cmx-rpcs/*`
//! （如 `cmx-orchestrator-rpc`、`cmx-resource-rpc`），依赖本模块的共享设施；
//! 组装层（cmx-platform-app）显式收集皮肤 Bundle 列表传入 [`init_rpc_clients`]。
//!
//! 纯 gRPC 消费方不开 feature 时，本模块与 volo 依赖树完全不进编译图。

pub mod error;

#[cfg(feature = "grpc-client")]
pub mod client;
#[cfg(feature = "grpc-client")]
pub mod discover;
#[cfg(feature = "grpc-client")]
pub mod global;

#[cfg(feature = "grpc-server")]
pub mod bundle;
#[cfg(feature = "grpc-server")]
pub mod factory;
#[cfg(feature = "grpc-server")]
pub mod server;
#[cfg(feature = "grpc-server")]
pub mod server_runner;

// 共享类型（与 cmx-rpc 时代同名的 re-export，皮肤 crate 以短路径使用）
pub use error::RpcFrameworkError;

#[cfg(feature = "grpc-client")]
pub use client::auth_outbound::apply_auth_metadata;
#[cfg(feature = "grpc-client")]
pub use client::infra::GrpcInfrastructure;
#[cfg(feature = "grpc-client")]
pub use client::retry::{RetryStats, with_retry};
#[cfg(feature = "grpc-client")]
pub use client::safe_parse_json;
#[cfg(feature = "grpc-client")]
pub use discover::RegistryAwareDiscover;
#[cfg(feature = "grpc-client")]
pub use global::{GlobalRpcClient, GlobalRpcClientAlreadySetError};

#[cfg(feature = "grpc-server")]
pub use bundle::{RpcServiceBundle, ServerDeps, ServerRegistration};
#[cfg(feature = "grpc-server")]
pub use factory::{ClientInitError, init_rpc_clients};
#[cfg(feature = "grpc-server")]
pub use server::{AuthVerifier, VerifiedAuth, verify_request};
#[cfg(feature = "grpc-server")]
pub use server_runner::start_grpc_server;
