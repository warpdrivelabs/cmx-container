//! 平台资源数据导入导出统一抽象。
//!
//! 包含四类资源(表单/菜单/表结构/权限)的定义导入器 trait,
//! 资源数据交换 DTO, 以及多类别路由器 trait。

use std::sync::Arc;

pub mod category;
pub mod dto;
pub mod form;
pub mod importer;
pub mod menu;
pub mod permission;
pub mod table;

pub use category::ResourceDataCategory;
pub use dto::{
    ResourceDataCleanupRequest, ResourceDataImportRequest, ResourceDataImportResult,
    ResourceDataListResult,
};
pub use form::FormDefinitionImporter;
pub use importer::ResourceDataImporter;
pub use menu::MenuDefinitionImporter;
pub use permission::{PermissionDefinitionImporter, PermissionZipImporter};
pub use table::TableDefinitionImporter;

/// 资源定义导入器集合(四类资源统一注入)。
///
/// 装配时根据部署模式(mode=local/grpc)填充 Local 或 Remote 实现,
/// 调用方(ModuleInstallService / ModuleExportService)持有 `Arc<DefinitionImporterBundle>`,
/// 无论本地/远程,调用代码完全一致。
#[derive(Clone)]
pub struct DefinitionImporterBundle {
    /// 表单定义导入器
    pub form: Arc<dyn FormDefinitionImporter>,
    /// 菜单定义导入器
    pub menu: Arc<dyn MenuDefinitionImporter>,
    /// 表结构定义导入器
    pub table: Arc<dyn TableDefinitionImporter>,
    /// 权限定义导入器
    pub permission: Arc<dyn PermissionDefinitionImporter>,
}
