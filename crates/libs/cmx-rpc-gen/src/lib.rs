//! cmx-rpc-gen — volo-build 生成的 gRPC 代码重导出（proto 契约集中 crate）。
//!
//! proto 按业务域放在 `idl/<域>/` 子目录；`volo.yml` 的 `filename` 字段决定生成文件名。
//! 生成类型的完整路径较深（`cmx_rpc_gen::cmx::<service>::<service>::cmx::*`），
//! 消费方优先使用便捷别名模块 [`orchestrator_proto`] / [`resource_data_proto`]。

pub mod cmx {
    pub mod cmx_service_orchestrator {
        include!(concat!(env!("OUT_DIR"), "/cmx_service_orchestrator.rs"));
    }

    pub mod cmx_resource_data_service {
        include!(concat!(env!("OUT_DIR"), "/cmx_resource_data_service.rs"));
    }
}

/// 服务编排域生成类型便捷别名。
///
/// 替代深路径 `cmx::cmx_service_orchestrator::cmx_service_orchestrator::cmx::*`，
/// 供 `cmx-orchestrator-rpc` 皮肤 crate 使用。
pub mod orchestrator_proto {
    pub use super::cmx::cmx_service_orchestrator::cmx_service_orchestrator::cmx::*;
}

/// 资源数据管理域生成类型便捷别名。
///
/// 替代深路径 `cmx::cmx_resource_data_service::cmx_resource_data_service::cmx::*`，
/// 供 `cmx-resource-rpc` 皮肤 crate 使用。
pub mod resource_data_proto {
    pub use super::cmx::cmx_resource_data_service::cmx_resource_data_service::cmx::*;
}
