//! cmx-biz 模块
//!
//! 平台基础业务模型层，包含 Domain/Application/Module/SysDatasource 等实体的
//! Entity/BMC/Filter/Service 定义，以及 function_invoker/service_executor 共享逻辑。
//! 另含 ResourceDataImporterImpl（多类别数据导入路由器，Form/Menu/Perm 统一接收端）。

pub mod application;
pub mod datasource;
pub mod domain;
pub mod module;

// DAM 资产文件服务（模块资源目录创建/改名搬移/引用校验）
pub mod dam_asset_service;

pub mod form;
pub mod menu;

// 资源数据导入路由器（Form/Menu/Perm 多类别接收端,Perm 经 trait 对象注入）
pub mod resource_importer;

// 插件函数调用核心逻辑（协议无关）
pub mod function_invoker;

// 服务编排执行核心逻辑（协议无关）
pub mod service_executor;

pub mod error;
pub use error::{api_err, api_err_db, pg_detail, BizError, Result};

// 统一错误码 + 错误信息库（落库校验 / 约束翻译）
pub mod errcode;
// 落库前列级校验（列规范 + 缓存 + 校验器）
pub mod validation;
