//! 基础服务中心客户端模块(精简版)。
//!
//! 原先的 sender/dispatcher/http_sender/grpc_sender/mock_sender 已被上层的
//! Remote 定义导入器(`service::remote_importers`)取代 —— 后者直接复用
//! `cmx_rpc::plugin_data_client()` 经 gRPC 传输,不再需要 sender 中间层。
//!
//! 本模块保留仍在使用的部分:
//! - [`config`]: `CenterClientConfig`(加载 `[center_client]` 配置,解析各 category 服务名)
//! - [`packer`]: ZIP 打包工具(`pack_definitions_to_zip` / `pack_payload_to_zip`,供 Remote 导入器复用)
//! - [`types`]: `DataCategory` 枚举(Menu/Perm/Form/Flow,供服务名路由)
//!
//! 详见方案文档:`20260703_cmx-plugin_模块资源导入导出统一抽象方案.md`

pub mod config;
pub mod packer;
pub mod types;

pub use config::CenterClientConfig;
pub use types::DataCategory;
