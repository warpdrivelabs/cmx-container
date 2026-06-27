//! 业务模型定义。
//!
//! 按实体组织模型，每个文件对应一个业务领域。

// 重导出 SDK 核心类型，方便业务代码直接 use crate::models::*
pub use cmx_plugin_sdk::{
    FunctionInput, FunctionOutput, SVRContext,
    DbRequest, DbResponse,
    CacheGetRequest, CacheSetRequest, CacheResponse,
    PluginFunRequest, PluginFunCallResponse, CallServiceRequest, CallServiceResponse,
    WasmUserDetails, WasmEffectivePermissions,
};

pub mod order;
pub mod common;

pub use order::*;
pub use common::*;
