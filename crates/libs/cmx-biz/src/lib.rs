//! cmx-biz 模块
//!
//! 平台基础业务模型层，包含 Domain/Application/Module/SysDatasource 等实体的
//! Entity/BMC/Filter/Service 定义，以及 function_invoker/service_executor 共享逻辑。
//! 另含 PluginDataImporterImpl（多类别数据导入路由器，Form/Menu/Perm 统一接收端）。

pub mod domain;
pub mod application;
pub mod module;
pub mod datasource;
pub mod form;
pub mod menu;

// 插件数据导入路由器（Form/Menu/Perm 多类别接收端,Perm 经 trait 对象注入）
pub mod plugin_data_importer;

// 插件函数调用核心逻辑（协议无关）
pub mod function_invoker;

// 服务编排执行核心逻辑（协议无关）
pub mod service_executor;

pub mod error;
pub use error::{BizError, Result};
