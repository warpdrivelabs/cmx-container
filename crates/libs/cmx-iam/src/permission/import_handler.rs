//! 插件数据导入器实现 — `PluginDataImporterImpl`。
//!
//! 实现 `cmx_traits::plugin::PluginDataImporter` trait，按 `PluginDataCategory`
//! 路由到对应的定义导入器。支持 Perm(权限)/Form(表单)/Menu(菜单) 类别。
//!
//! HTTP 端点和 gRPC 服务端均通过此 trait 调用，统一路径与缓存失效逻辑。
//! Form/Menu 类别经注入的 `FormDefinitionImporter` / `MenuDefinitionImporter` 处理,
//! 接收端解压 ZIP → 解析 JSON → 调用 Local 实现(与单体共用同一套 Local 代码)。

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use cmx_core::SVRContext;
use cmx_core::model::module::{FormDefinition, MenuDefinition};
use cmx_traits::error::TraitError;
use cmx_traits::module::{FormDefinitionImporter, MenuDefinitionImporter};
use cmx_traits::iam::PermissionDefinitionImporter;
use cmx_traits::plugin::{
    PluginDataCategory, PluginDataCleanupRequest, PluginDataImportRequest, PluginDataImportResult,
    PluginDataImporter, PluginDataListResult,
};

use super::service::PermissionServiceImpl;

/// 插件数据导入器实现。
///
/// 持有 `PermissionServiceImpl` 的具体类型引用（非 trait 对象），
/// 以便调用其固有方法 `import_permissions` / `cleanup_permissions`。
/// 另持有可选的 Form/Menu 定义导入器(支持跨服务的表单/菜单远程导入)。
pub struct PluginDataImporterImpl {
    /// 权限服务实现（具体类型）。
    permission_service: Arc<PermissionServiceImpl>,
    /// 表单定义导入器(可选,支持 Form 类别远程导入)
    form_importer: Option<Arc<dyn FormDefinitionImporter>>,
    /// 菜单定义导入器(可选,支持 Menu 类别远程导入)
    menu_importer: Option<Arc<dyn MenuDefinitionImporter>>,
}

impl PluginDataImporterImpl {
    /// 构造函数。
    ///
    /// # Arguments
    ///
    /// * `permission_service` - 权限服务实现的具体类型实例。
    pub fn new(permission_service: Arc<PermissionServiceImpl>) -> Self {
        Self {
            permission_service,
            form_importer: None,
            menu_importer: None,
        }
    }

    /// 注入表单定义导入器(Builder 模式)。
    pub fn with_form_importer(
        mut self,
        importer: Arc<dyn FormDefinitionImporter>,
    ) -> Self {
        self.form_importer = Some(importer);
        self
    }

    /// 注入菜单定义导入器(Builder 模式)。
    pub fn with_menu_importer(
        mut self,
        importer: Arc<dyn MenuDefinitionImporter>,
    ) -> Self {
        self.menu_importer = Some(importer);
        self
    }

    /// 构造 `SVRContext`（系统调用上下文，无 HTTP 请求头信息）。
    fn build_svr_ctx() -> SVRContext {
        SVRContext::new(
            serde_json::Value::Null,
            HashMap::new(),
            Utc::now(),
            cmx_utils::id::snowflake_id_str(),
        )
    }

    /// 从 ZIP 字节中提取所有 JSON 文件内容(文件名 → 内容)。
    ///
    /// 供 Form/Menu 远程导入接收端复用:解压 ZIP → 读取每个 .json 文件内容。
    fn extract_json_files_from_zip(zip_data: &[u8]) -> Result<Vec<(String, Vec<u8>)>, TraitError> {
        let cursor = std::io::Cursor::new(zip_data);
        let mut archive = zip::ZipArchive::new(cursor)
            .map_err(|e| TraitError::Business(format!("解压 ZIP 失败: {e}")))?;
        let mut files = Vec::new();
        for i in 0..archive.len() {
            let mut entry = archive
                .by_index(i)
                .map_err(|e| TraitError::Business(format!("读取 ZIP 条目 {i} 失败: {e}")))?;
            let name = entry.name().to_string();
            if !name.ends_with(".json") {
                continue;
            }
            let mut buf = Vec::new();
            std::io::Read::read_to_end(&mut entry, &mut buf)
                .map_err(|e| TraitError::Business(format!("读取 ZIP 文件 {name} 内容失败: {e}")))?;
            files.push((name, buf));
        }
        Ok(files)
    }
}

#[async_trait]
impl PluginDataImporter for PluginDataImporterImpl {
    /// 导入插件数据。
    ///
    /// 按 `PluginDataCategory` 路由：
    /// - `Perm` → 调用 `PermissionServiceImpl::import_permissions`(ZIP permdata 格式)
    /// - `Form` → 解压 ZIP → 解析 FormDefinition 列表 → `form_importer.apply_form_definitions`
    /// - `Menu` → 解压 ZIP → 解析 MenuDefinition 列表 → `menu_importer.apply_menu_definitions`
    /// - 其他 → 返回不支持错误
    async fn import_data(
        &self,
        request: PluginDataImportRequest,
    ) -> Result<PluginDataImportResult, TraitError> {
        match request.category {
            PluginDataCategory::Perm => {
                let svr_ctx = Self::build_svr_ctx();
                self.permission_service
                    .import_permissions(
                        &svr_ctx,
                        &request.domain_code,
                        &request.application_code,
                        &request.module_code,
                        &request.zip_data,
                    )
                    .await
            }
            PluginDataCategory::Form => {
                let Some(importer) = &self.form_importer else {
                    return Err(TraitError::Business(
                        "未注入 FormDefinitionImporter,不支持远程表单导入".to_string(),
                    ));
                };
                // 解压 ZIP → 解析每个 form_*.json 为 FormDefinition
                let files = Self::extract_json_files_from_zip(&request.zip_data)?;
                let mut defs = Vec::new();
                for (_name, content) in files {
                    let def: FormDefinition = serde_json::from_slice(&content).map_err(|e| {
                        TraitError::Business(format!("解析表单定义 JSON 失败: {e}"))
                    })?;
                    defs.push(def);
                }
                let count = importer
                    .apply_form_definitions(
                        &request.domain_code,
                        &request.application_code,
                        &request.module_code,
                        &defs,
                    )
                    .await?;
                Ok(PluginDataImportResult {
                    success: true,
                    message: format!("远程表单导入完成: {count} 条"),
                    created_count: count as u32,
                    updated_count: 0,
                    deleted_count: 0,
                })
            }
            PluginDataCategory::Menu => {
                let Some(importer) = &self.menu_importer else {
                    return Err(TraitError::Business(
                        "未注入 MenuDefinitionImporter,不支持远程菜单导入".to_string(),
                    ));
                };
                // 解压 ZIP → 解析每个 menu_*.json 为 MenuDefinition
                let files = Self::extract_json_files_from_zip(&request.zip_data)?;
                let mut defs = Vec::new();
                for (_name, content) in files {
                    let def: MenuDefinition = serde_json::from_slice(&content).map_err(|e| {
                        TraitError::Business(format!("解析菜单定义 JSON 失败: {e}"))
                    })?;
                    defs.push(def);
                }
                let count = importer
                    .apply_menu_definitions(
                        &request.domain_code,
                        &request.application_code,
                        &request.module_code,
                        &defs,
                    )
                    .await?;
                Ok(PluginDataImportResult {
                    success: true,
                    message: format!("远程菜单导入完成: {count} 条"),
                    created_count: count as u32,
                    updated_count: 0,
                    deleted_count: 0,
                })
            }
            _ => Err(TraitError::Business(format!(
                "不支持的数据类别: {:?}",
                request.category
            ))),
        }
    }

    /// 清理插件数据。
    ///
    /// 按 `PluginDataCategory` 路由：
    /// - `Perm` → 调用 `PermissionServiceImpl::cleanup_permissions`
    /// - 其他类别 → 返回不支持错误
    async fn cleanup_data(
        &self,
        request: PluginDataCleanupRequest,
    ) -> Result<PluginDataImportResult, TraitError> {
        match request.category {
            PluginDataCategory::Perm => {
                let svr_ctx = Self::build_svr_ctx();
                self.permission_service
                    .cleanup_permissions(
                        &svr_ctx,
                        &request.domain_code,
                        &request.application_code,
                        &request.module_code,
                    )
                    .await
            }
            _ => Err(TraitError::Business(format!(
                "不支持的数据类别: {:?}",
                request.category
            ))),
        }
    }

    /// 查询（导出）插件数据，返回 JSON 序列化的定义列表。
    ///
    /// 按 `request.category` 路由:
    /// - `Form` → `form_importer.list_form_definitions` → JSON
    /// - `Menu` → `menu_importer.list_menu_definitions` → JSON
    /// - `Perm` → `permission_service.list_permission_definitions` → JSON
    async fn list_data(
        &self,
        request: PluginDataImportRequest,
    ) -> Result<PluginDataListResult, TraitError> {
        match request.category {
            PluginDataCategory::Form => {
                let Some(importer) = &self.form_importer else {
                    return Err(TraitError::Business(
                        "未注入 FormDefinitionImporter,不支持远程表单导出".to_string(),
                    ));
                };
                let defs = importer
                    .list_form_definitions(&request.module_code)
                    .await?;
                let json_data = serde_json::to_vec(&defs)
                    .map_err(|e| TraitError::Business(format!("序列化表单定义失败: {e}")))?;
                Ok(PluginDataListResult {
                    success: true,
                    message: format!("查询到 {} 条表单定义", defs.len()),
                    json_data,
                })
            }
            PluginDataCategory::Menu => {
                let Some(importer) = &self.menu_importer else {
                    return Err(TraitError::Business(
                        "未注入 MenuDefinitionImporter,不支持远程菜单导出".to_string(),
                    ));
                };
                let defs = importer
                    .list_menu_definitions(&request.module_code)
                    .await?;
                let json_data = serde_json::to_vec(&defs)
                    .map_err(|e| TraitError::Business(format!("序列化菜单定义失败: {e}")))?;
                Ok(PluginDataListResult {
                    success: true,
                    message: format!("查询到 {} 条菜单定义", defs.len()),
                    json_data,
                })
            }
            PluginDataCategory::Perm => {
                let defs = self
                    .permission_service
                    .list_permission_definitions(
                        &request.domain_code,
                        &request.application_code,
                        &request.module_code,
                    )
                    .await?;
                let json_data = serde_json::to_vec(&defs)
                    .map_err(|e| TraitError::Business(format!("序列化权限定义失败: {e}")))?;
                Ok(PluginDataListResult {
                    success: true,
                    message: format!("查询到 {} 条权限定义", defs.len()),
                    json_data,
                })
            }
            _ => Err(TraitError::Business(format!(
                "不支持的数据类别: {:?}",
                request.category
            ))),
        }
    }
}
