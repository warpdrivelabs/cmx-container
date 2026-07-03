//! 模块资源定义导入器 trait 集合。
//!
//! 为表单/菜单/元数据(表结构)/权限四类模块资源定义统一的「定义导入器」抽象。
//!
//! # 设计目标
//!
//! - **本地/远程透明**:每个 trait 有 Local 实现(直调 Service)和 Remote 实现(gRPC),
//!   调用方代码一致。
//! - **导入/导出对称**:每个 trait 同时含 `apply_*`(导入)和 `list_*`(导出)方法。
//! - **结构化参数**:接收已解析的结构体列表(非 ZIP),消除上层重复的序列化/解析。
//!
//! 详见方案文档:`20260703_cmx-plugin_模块资源导入导出统一抽象方案.md`

use std::sync::Arc;

pub mod form;
pub mod menu;
pub mod table;

pub use form::FormDefinitionImporter;
pub use menu::MenuDefinitionImporter;
pub use table::TableDefinitionImporter;

use crate::iam::PermissionDefinitionImporter;

/// 模块资源定义导入器集合(四类资源统一注入)。
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

