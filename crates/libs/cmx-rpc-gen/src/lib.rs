//! cmx-rpc-gen — volo-build 生成的 gRPC 代码重导出

pub mod cmx {
    pub mod cmx_service_orchestrator {
        include!(concat!(env!("OUT_DIR"), "/cmx_service_orchestrator.rs"));
    }

    pub mod cmx_resource_data_service {
        include!(concat!(env!("OUT_DIR"), "/cmx_resource_data_service.rs"));
    }
}
