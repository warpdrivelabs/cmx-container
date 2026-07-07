//! cmx-rpc — RPC 框架核心功能。
//!
//! 提供 gRPC 客户端/服务端封装、服务发现桥接、全局 RPC 客户端管理等功能。
//!
//! # 架构（Bundle 模式）
//!
//! - [`bundle`]：领域 Bundle trait + [`bundle::default_bundles`]，实现 OCP。
//! - [`client`]：按领域拆分的 gRPC 客户端（[`orchestrator_client`] / [`resource_data_client`]）。
//! - [`server`]：按领域拆分的 gRPC 服务端实现。
//! - [`factory`]：迭代 Bundle 初始化客户端。
//! - [`server_runner`]：迭代 Bundle 注册服务端。

pub mod bundle;
pub mod client;
pub mod config;
pub mod discover;
pub mod error;
pub mod factory;
pub mod global;
pub mod server;
pub mod server_runner;

// 领域客户端访问器（调用方入口）
pub use client::orchestrator_client;
pub use client::resource_data_client;

// 共享类型
pub use cmx_traits::resource::ResourceDataImporter;
pub use config::{GrpcConfig, HttpRestConfig, RpcConfig};
pub use discover::RegistryAwareDiscover;
pub use error::RpcFrameworkError;
pub use factory::{ClientInitError, init_rpc_clients};
pub use global::{GlobalRpcClient, GlobalRpcClientAlreadySetError};
pub use server::AuthVerifier;
pub use server_runner::start_grpc_server;
