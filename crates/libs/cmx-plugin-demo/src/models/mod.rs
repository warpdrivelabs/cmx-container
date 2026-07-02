//! 业务模型定义。
//!
//! 按实体组织模型，每个文件对应一个业务领域。

// 重导出 SDK 核心类型，方便业务代码直接 use crate::models::*
pub use cmx_plugin_sdk::{
    CacheGetRequest, CacheResponse, CacheSetRequest, CallServiceRequest, CallServiceResponse,
    DbRequest, DbResponse, FunctionInput, FunctionOutput, PluginFunCallResponse, PluginFunRequest,
    SVRContext, WasmCheckResult, WasmEffectivePermissions, WasmUserDetails,
};

pub mod common;
pub mod order;

pub use common::*;
pub use order::*;
