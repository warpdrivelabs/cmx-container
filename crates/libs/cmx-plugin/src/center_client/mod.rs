//! 基础服务中心客户端模块(精简版)。
//!
//! 原先的 sender/dispatcher/http_sender/grpc_sender/mock_sender 已被上层的
//! Remote 定义导入器(`service::remote_importers`)取代 —— 后者直接复用
//! `cmx_resource_rpc::resource_data_client()` 经 gRPC 传输,不再需要 sender 中间层。
//!
//! 本模块保留仍在使用的部分:
//! - [`config`]: `CenterClientConfig`(加载 `[center_client]` 配置;urls 与 discovery.services
//!   均为**自由键值表**,新增微服务只在 toml 加一行,配置层零代码改动)
//! - [`upstream`]: 反代目标定位(`proxy_upstream` 按 mode 分派解析目标;`ProxyUpstream::resolve`
//!   动态选例,反代与导入器共用同一套负载均衡语义)
//! - [`packer`]: ZIP 打包工具(`pack_definitions_to_zip` / `pack_payload_to_zip`,供 Remote 导入器复用)
//! - [`types`]: `DataCategory` 枚举(Menu/Perm/Form/Flow,供服务名路由)
//!
//! 详见方案文档:`20260703_cmx-plugin_模块资源导入导出统一抽象方案.md`、
//! `20260822_cmx-center-client_服务定位map化与微服务接入Nacos方案.md`

pub mod config;
pub mod packer;
pub mod types;
pub mod upstream;

pub use config::CenterClientConfig;
pub use types::DataCategory;
pub use upstream::{ProxyUpstream, log_center_client_snapshot, proxy_upstream, warm_proxy_upstreams};
