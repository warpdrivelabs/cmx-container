//! 表元数据存储模块
//!
//! 提供插件表元数据的增删改查操作，包括：
//! - cmx_meta_table_define: 表元数据主表
//! - cmx_meta_table_define_version: 表元数据版本表
//!
//! 使用 cmx_database 的 GenericCrudService 和 modql 实现

pub mod bmc;
pub mod entity;
pub mod filter;
pub mod service;

pub use bmc::{TableMetadataBmc, TableMetadataVersionBmc};
pub use entity::{
    TableMetadataDetail, TableMetadataForCreate, TableMetadataForUpdate, TableMetadataVersion,
};
pub use filter::{TableMetadataFilter, TableMetadataVersionFilter};
pub use service::TableMetadataService;
