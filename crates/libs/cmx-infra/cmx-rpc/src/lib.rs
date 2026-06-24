//! cmx-rpc — RPC 框架核心功能
//!
//! 提供 gRPC 客户端/服务端封装、服务发现桥接、全局 RPC 客户端管理等功能。

pub mod client;
pub mod config;
pub mod discover;
pub mod error;
pub mod factory;
pub mod global;
pub mod server;
pub mod server_runner;

pub use client::VoloGrpcClient;
pub use config::{GrpcConfig, HttpRestConfig, RpcConfig};
pub use discover::RegistryAwareDiscover;
pub use error::RpcFrameworkError;
pub use factory::create_rpc_client;
pub use global::{GlobalRpcClient, GlobalRpcClientAlreadySetError};
pub use cmx_traits::plugin::PluginDataImporter;
pub use server::CmxOrchestratorServiceImpl;
pub use server_runner::start_grpc_server;
