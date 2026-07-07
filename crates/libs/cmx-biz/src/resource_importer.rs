//! 插件数据导入器实现 — `ResourceDataImporterImpl`。
//!
//! 实现 `cmx_traits::resource::ResourceDataImporter` trait，按 `ResourceDataCategory`
//! 路由到对应的定义导入器。支持 Perm(权限)/Form(表单)/Menu(菜单) 类别。
//!
//! # 归属说明
//!
//! 本类型是**多类别路由器**，不属于任何单一业务域，放在 cmx-biz（平台基础业务层）
//! 是因为 Form/Menu 的 Local 实现在此 crate。Perm 类别通过 `PermissionZipImporter`
//! trait 对象注入（由 cmx-iam 的 PermissionServiceImpl 实现），无需依赖 cmx-iam。
//!
//! HTTP 端点和 gRPC 服务端均通过此 trait 调用，统一路径与缓存失效逻辑。

use std::sync::Arc;

use async_trait::async_trait;
use cmx_core::model::module::{FormDefinition, MenuDefinition};
use cmx_traits::error::TraitError;
use cmx_traits::resource::{
    FormDefinitionImporter, MenuDefinitionImporter, PermissionDefinitionImporter,
    PermissionZipImporter,
};
use cmx_traits::resource::{
    ResourceDataCategory, ResourceDataCleanupRequest, ResourceDataImportRequest,
    ResourceDataImportResult, ResourceDataImporter, ResourceDataListResult,
};

/// 插件数据导入器实现（多类别路由器）。
///
/// 持有三类 trait 对象:
/// - `perm_zip_importer`: Perm 类别的 ZIP 格式导入/清理(由 cmx-iam 实现)
/// - `perm_def_importer`: Perm 类别的结构化定义导入/导出(由 cmx-iam 实现)
/// - `form_importer` / `menu_importer`: Form/Menu 类别的结构化定义(由 cmx-biz 本地实现)
pub struct ResourceDataImporterImpl {
    /// 权限 ZIP 导入器(Perm 类别的 ZIP permdata 格式,含 diff/审计/缓存)
    perm_zip_importer: Arc<dyn PermissionZipImporter>,
    /// 权限定义导入器(Perm 类别的结构化 apply/list)
    perm_def_importer: Arc<dyn PermissionDefinitionImporter>,
    /// 表单定义导入器(可选,支持 Form 类别)
    form_importer: Option<Arc<dyn FormDefinitionImporter>>,
    /// 菜单定义导入器(可选,支持 Menu 类别)
    menu_importer: Option<Arc<dyn MenuDefinitionImporter>>,
}

impl ResourceDataImporterImpl {
    /// 创建新的插件数据导入器。
    ///
    /// # Arguments
    ///
    /// * `perm_zip_importer` - 权限 ZIP 导入器(通常为 PermissionServiceImpl)
    /// * `perm_def_importer` - 权限定义导入器(通常为同一 PermissionServiceImpl 实例)
    pub fn new(
        perm_zip_importer: Arc<dyn PermissionZipImporter>,
        perm_def_importer: Arc<dyn PermissionDefinitionImporter>,
    ) -> Self {
        Self {
            perm_zip_importer,
            perm_def_importer,
            form_importer: None,
            menu_importer: None,
        }
    }

    /// 注入表单定义导入器(Builder 模式)。
    pub fn with_form_importer(mut self, importer: Arc<dyn FormDefinitionImporter>) -> Self {
        self.form_importer = Some(importer);
        self
    }

    /// 注入菜单定义导入器(Builder 模式)。
    pub fn with_menu_importer(mut self, importer: Arc<dyn MenuDefinitionImporter>) -> Self {
        self.menu_importer = Some(importer);
        self
    }

    /// 从 ZIP 字节中提取所有 JSON 文件内容(文件名 → 内容)。
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
impl ResourceDataImporter for ResourceDataImporterImpl {
    /// 导入插件数据。按 `ResourceDataCategory` 路由。
    async fn import_data(
        &self,
        request: ResourceDataImportRequest,
    ) -> Result<ResourceDataImportResult, TraitError> {
        match request.category {
            ResourceDataCategory::Perm => {
                self.perm_zip_importer
                    .import_permissions_zip(
                        &request.domain_code,
                        &request.application_code,
                        &request.module_code,
                        &request.zip_data,
                    )
                    .await
            }
            ResourceDataCategory::Form => {
                let Some(importer) = &self.form_importer else {
                    return Err(TraitError::Business(
                        "未注入 FormDefinitionImporter,不支持远程表单导入".to_string(),
                    ));
                };
                let files = Self::extract_json_files_from_zip(&request.zip_data)?;
                let mut defs = Vec::new();
                for (_name, content) in files {
                    let def: FormDefinition = serde_json::from_slice(&content)
                        .map_err(|e| TraitError::Business(format!("解析表单定义 JSON 失败: {e}")))?;
                    defs.push(def);
                }
                let count = importer
                    .apply_form_definitions(
                        &request.domain_code,
                        &request.application_code,
                        &request.module_code,
                        &defs,
                        None, // RPC 接收端:跨服务无共享事务,暂不开事务
                    )
                    .await?;
                Ok(ResourceDataImportResult {
                    success: true,
                    message: format!("远程表单导入完成: {count} 条"),
                    created_count: count as u32,
                    updated_count: 0,
                    deleted_count: 0,
                })
            }
            ResourceDataCategory::Menu => {
                let Some(importer) = &self.menu_importer else {
                    return Err(TraitError::Business(
                        "未注入 MenuDefinitionImporter,不支持远程菜单导入".to_string(),
                    ));
                };
                let files = Self::extract_json_files_from_zip(&request.zip_data)?;
                let mut defs = Vec::new();
                for (_name, content) in files {
                    let def: MenuDefinition = serde_json::from_slice(&content)
                        .map_err(|e| TraitError::Business(format!("解析菜单定义 JSON 失败: {e}")))?;
                    defs.push(def);
                }
                let count = importer
                    .apply_menu_definitions(
                        &request.domain_code,
                        &request.application_code,
                        &request.module_code,
                        &defs,
                        None, // RPC 接收端:跨服务无共享事务,暂不开事务
                    )
                    .await?;
                Ok(ResourceDataImportResult {
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

    /// 清理插件数据。仅支持 Perm。
    async fn cleanup_data(
        &self,
        request: ResourceDataCleanupRequest,
    ) -> Result<ResourceDataImportResult, TraitError> {
        match request.category {
            ResourceDataCategory::Perm => {
                self.perm_zip_importer
                    .cleanup_permissions_zip(
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
    async fn list_data(
        &self,
        request: ResourceDataImportRequest,
    ) -> Result<ResourceDataListResult, TraitError> {
        match request.category {
            ResourceDataCategory::Form => {
                let Some(importer) = &self.form_importer else {
                    return Err(TraitError::Business(
                        "未注入 FormDefinitionImporter,不支持远程表单导出".to_string(),
                    ));
                };
                let defs = importer.list_form_definitions(&request.module_code).await?;
                let json_data = serde_json::to_vec(&defs)
                    .map_err(|e| TraitError::Business(format!("序列化表单定义失败: {e}")))?;
                Ok(ResourceDataListResult {
                    success: true,
                    message: format!("查询到 {} 条表单定义", defs.len()),
                    json_data,
                })
            }
            ResourceDataCategory::Menu => {
                let Some(importer) = &self.menu_importer else {
                    return Err(TraitError::Business(
                        "未注入 MenuDefinitionImporter,不支持远程菜单导出".to_string(),
                    ));
                };
                let defs = importer.list_menu_definitions(&request.module_code).await?;
                let json_data = serde_json::to_vec(&defs)
                    .map_err(|e| TraitError::Business(format!("序列化菜单定义失败: {e}")))?;
                Ok(ResourceDataListResult {
                    success: true,
                    message: format!("查询到 {} 条菜单定义", defs.len()),
                    json_data,
                })
            }
            ResourceDataCategory::Perm => {
                let defs = self
                    .perm_def_importer
                    .list_permission_definitions(
                        &request.domain_code,
                        &request.application_code,
                        &request.module_code,
                    )
                    .await?;
                let json_data = serde_json::to_vec(&defs)
                    .map_err(|e| TraitError::Business(format!("序列化权限定义失败: {e}")))?;
                Ok(ResourceDataListResult {
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
