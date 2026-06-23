//! RPC 领域 trait 抽象。
//!
//! 定义跨实例 RPC 调用统一接口。

pub mod client;

pub use client::{RpcClient, RpcError, FunctionCallResult};
