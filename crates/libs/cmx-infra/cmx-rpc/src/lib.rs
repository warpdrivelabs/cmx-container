//! cmx-rpc — RPC 基础设施核心（纯共享设施层）。
//!
//! 提供 gRPC 跨服务调用的共享能力：服务发现桥接、重试、出/入站鉴权、
//! 客户端共享基础设施、Bundle 装配接口、gRPC Server 启动器与全局初始化守卫。
//!
//! # 架构（契约中心化 · 实现归域 · 装配显式）
//!
//! - proto 契约集中在 `cmx-rpc-gen`（单一 contract crate）。
//! - 具体领域皮肤（client / server impl / Bundle）在 `cmx-rpcs/*`（如
//!   `cmx-orchestrator-rpc`、`cmx-resource-rpc`），依赖本 crate 的共享设施与
//!   [`bundle::RpcServiceBundle`] 接口。
//! - 组装层（cmx-platform-app）显式收集皮肤 Bundle 列表传入
//!   [`factory::init_rpc_clients`]——**主应用提供哪些 RPC 服务由此决定**。
//! - 新增一个 gRPC 服务的标准步骤见 cmx-rpc/README.md。
//!
//! # 模块
//!
//! - [`bundle`]：领域 Bundle trait + `ServerDeps`（OCP 装配接口）。
//! - [`client`]：客户端共享设施（`GrpcInfrastructure` / `with_retry` / `apply_auth_metadata`）。
//! - [`server`]：服务端共享设施（`AuthVerifier` 鉴权）。
//! - [`factory`]：迭代外部传入的 Bundle 初始化客户端。
//! - [`server_runner`]：迭代 Bundle 注册服务端并启动。

pub mod bundle;
pub mod client;
pub mod config;
pub mod discover;
pub mod error;
pub mod factory;
pub mod global;
pub mod server;
pub mod server_runner;

// 共享类型
pub use config::{GrpcConfig, HttpRestConfig, RpcConfig};
pub use discover::RegistryAwareDiscover;
pub use error::RpcFrameworkError;
pub use factory::{ClientInitError, init_rpc_clients};
pub use global::{GlobalRpcClient, GlobalRpcClientAlreadySetError};
pub use server_runner::start_grpc_server;

// 共享设施便捷 re-export（供 cmx-rpcs/* 皮肤 crate 以短路径使用）
pub use client::auth_outbound::apply_auth_metadata;
pub use client::infra::GrpcInfrastructure;
pub use client::retry::{RetryStats, with_retry};
pub use client::safe_parse_json;
pub use server::{AuthVerifier, VerifiedAuth, verify_request};
