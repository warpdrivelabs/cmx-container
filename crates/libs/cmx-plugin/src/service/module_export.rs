//! 模块包导出服务(ModuleExportService)
//!
//! 从数据库 + 文件系统聚合模块数据,导出为单一聚合 zip。
//! 版本号(package_version)为导出时刻自动生成的时间戳 yyyyMMddHHmmSS。
//!
//! 对称性契约(与 ModuleInstallService::install_module_resources 严格对称):
//! - forms/{stem}.json:经 bundle.form.list_form_definitions 查询,definition 整体透传
//! - menus/{stem}.json:经 bundle.menu.list_menu_definitions 查询(根菜单)
//! - metadata/tables/*.json:经 bundle.table.list_table_definitions 查询
//! - permissions/{module}_permissions.json:经 bundle.permission.list_permission_definitions 查询
//! - plugins/{plugin_id}.zip:从 cmx_plugin 查询安装目录打包

use std::path::{Path, PathBuf};
use std::sync::Arc;

use cmx_core::model::cell::DataValue;
use cmx_core::model::module::manifest::{
    ModuleInfo, ModuleManifest, ModulePluginEntry, ModuleResources, ModuleStats,
};
use cmx_database::DatabaseManager;
use cmx_traits::module::DefinitionImporterBundle;
use cmx_utils::zip::ZipCompressor;
use tracing::{info, warn};

use crate::error::{PluginError, PluginResult};

/// 模块导出服务
pub struct ModuleExportService {
    plugin_root: PathBuf,
    /// 资源定义导入器集合(导出时用 list_* 方法,消除内联 SQL)
    importers: Option<Arc<DefinitionImporterBundle>>,
}

impl ModuleExportService {
    /// 创建导出服务
    pub fn new(plugin_root: PathBuf) -> Self {
        Self {
            plugin_root,
            importers: None,
        }
    }

    /// 注入资源定义导入器集合(Builder 模式)。
    ///
    /// 注入后,导出时的表单/菜单/元数据/权限查询统一委托给 bundle 的 list_* 方法,
    /// 消除本文件内的内联 SQL。为 None 时导出这些资源会跳过(向后兼容)。
    pub fn with_definition_importers(
        mut self,
        importers: Arc<DefinitionImporterBundle>,
    ) -> Self {
        self.importers = Some(importers);
        self
    }

    /// 导出模块为迁移包 zip 字节
    ///
    /// # Errors
    /// 模块不存在、资源查询、打包失败时返回错误
    pub async fn export_module(
        &self,
        mm: &DatabaseManager,
        db_id: &str,
        domain_code: &str,
        application_code: &str,
        module_code: &str,
    ) -> PluginResult<Vec<u8>> {
        info!(module_code = %module_code, "开始导出模块包");

        // 创建临时导出目录
        let export_dir = std::env::temp_dir().join(format!(
            "cmx_export_{}_{}",
            module_code,
            chrono::Local::now().format("%Y%m%d%H%M%S")
        ));
        tokio::fs::create_dir_all(&export_dir)
            .await
            .map_err(|e| PluginError::Config(format!("创建导出临时目录失败: {e}")))?;

        let mut resources = ModuleResources::default();

        // 资源定义经 bundle 的 list_* 方法查询(消除内联 SQL)
        let (form_count, menu_count, table_count, perm_count) = if let Some(bundle) = &self.importers {
            // 1. 表单
            let form_defs = bundle
                .form
                .list_form_definitions(module_code)
                .await
                .map_err(|e| PluginError::Database(format!("导出表单失败: {e}")))?;
            let form_count = form_defs.len();
            if form_count > 0 {
                let forms_dir = export_dir.join("forms");
                tokio::fs::create_dir_all(&forms_dir).await.ok();
                for (i, def) in form_defs.iter().enumerate() {
                    Self::write_json(&forms_dir.join(format!("form_{i}.json")), &def.definition)?;
                }
                for i in 0..form_count {
                    resources.forms.push(format!("forms/form_{i}.json"));
                }
            }

            // 2. 菜单
            let menu_defs = bundle
                .menu
                .list_menu_definitions(module_code)
                .await
                .map_err(|e| PluginError::Database(format!("导出菜单失败: {e}")))?;
            let menu_count = menu_defs.len();
            if menu_count > 0 {
                let menus_dir = export_dir.join("menus");
                tokio::fs::create_dir_all(&menus_dir).await.ok();
                for (i, def) in menu_defs.iter().enumerate() {
                    Self::write_json(&menus_dir.join(format!("menu_{i}.json")), &def.definition)?;
                }
                for i in 0..menu_count {
                    resources.menus.push(format!("menus/menu_{i}.json"));
                }
            }

            // 3. 元数据
            let table_defs = bundle
                .table
                .list_table_definitions(application_code, module_code)
                .await
                .map_err(|e| PluginError::Database(format!("导出表元数据失败: {e}")))?;
            let table_count = table_defs.len();
            if table_count > 0 {
                let tables_dir = export_dir.join("metadata").join("tables");
                tokio::fs::create_dir_all(&tables_dir).await.ok();
                let tables_doc = serde_json::json!({ "tables": table_defs });
                Self::write_json(&tables_dir.join("module_tables.json"), &tables_doc)?;
                resources.metadata.push("metadata/tables/".to_string());
            }

            // 4. 权限
            let perm_defs = bundle
                .permission
                .list_permission_definitions(domain_code, application_code, module_code)
                .await
                .map_err(|e| PluginError::Database(format!("导出权限失败: {e}")))?;
            let perm_count = perm_defs.len();
            if perm_count > 0 {
                let perms_dir = export_dir.join("permissions");
                tokio::fs::create_dir_all(&perms_dir).await.ok();
                let perm_file = cmx_core::model::iam::PermissionFile {
                    name: format!("{}_permissions", module_code),
                    version: "1.0.0".to_string(),
                    description: format!("模块 {} 权限定义", module_code),
                    permissions: perm_defs,
                };
                Self::write_json(
                    &perms_dir.join(format!("{module_code}_permissions.json")),
                    &serde_json::to_value(&perm_file)?,
                )?;
                resources
                    .permissions
                    .push(format!("permissions/{module_code}_permissions.json"));
            }

            (form_count, menu_count, table_count, perm_count)
        } else {
            warn!("未注入 DefinitionImporterBundle,跳过表单/菜单/元数据/权限导出");
            (0, 0, 0, 0)
        };

        // 5. 导出插件子包 → plugins/{plugin_id}.zip
        // app_id 取配置值(当前设计下 app_id ≡ module_code),避免把 application_code 误当 app_id
        let app_id = cmx_utils::ConfigManager::global().get_app_id();
        let plugin_entries =
            Self::export_plugins(mm, db_id, &app_id, module_code, &export_dir, &self.plugin_root).await?;

        // 6. 组装 module.json + module.manifest.json
        let module_info = ModuleInfo {
            code: module_code.to_string(),
            name: module_code.to_string(),
            domain_code: domain_code.to_string(),
            application_code: application_code.to_string(),
            description: None,
        };
        Self::write_json(
            &export_dir.join("module.json"),
            &serde_json::to_value(&module_info)?,
        )?;

        let package_version = chrono::Local::now().format("%Y%m%d%H%M%S").to_string();
        let manifest = ModuleManifest {
            manifest_version: "1.0".to_string(),
            module: module_info,
            package_version: package_version.clone(),
            resources,
            plugins: plugin_entries.clone(),
            stats: ModuleStats {
                form_count,
                menu_count,
                permission_count: perm_count,
                table_count,
                plugin_count: plugin_entries.len(),
            },
            checksum: None,
            signature_algorithm: None,
            signature: None,
            signer_key_id: None,
        };
        Self::write_json(
            &export_dir.join("module.manifest.json"),
            &serde_json::to_value(&manifest)?,
        )?;

        // 7. 打成单一 zip
        let zip_bytes = ZipCompressor::compress_dir_to_memory(&export_dir, 6)
            .map_err(|e| PluginError::Config(format!("打包模块 zip 失败: {e}")))?;

        // 8. 清理临时目录
        let _ = tokio::fs::remove_dir_all(&export_dir).await;

        info!(
            module_code = %module_code,
            package_version = %package_version,
            size = zip_bytes.len(),
            "模块包导出成功"
        );
        Ok(zip_bytes)
    }

    /// 导出插件子包:从 cmx_plugin 查询当前版本安装目录,打包 manifest+servicedata+wasm+wit+api+seeddata
    async fn export_plugins(
        mm: &DatabaseManager,
        db_id: &str,
        app_id: &str,
        module_code: &str,
        export_dir: &Path,
        plugin_root: &Path,
    ) -> PluginResult<Vec<ModulePluginEntry>> {
        let plugins_dir = export_dir.join("plugins");
        // cmx_plugin 主表唯一约束(app_id, plugin_id)保证每个插件一行(version 为当前版本)
        let sql = "SELECT plugin_id, version, app_id FROM cmx_plugin \
                   WHERE module_code = $1 AND app_id = $2 AND archived = 0";
        let ds = mm
            .query_sql_with_datavalues(
                db_id,
                None,
                sql,
                vec![
                    DataValue::String(module_code.to_string()),
                    DataValue::String(app_id.to_string()),
                ],
                "export_plugins",
            )
            .await
            .map_err(|e| PluginError::Database(format!("查询插件失败: {e}")))?;
        let json = serde_json::to_value(&ds)?;
        let rows = json.get("rows").and_then(|r| r.as_array());
        let mut entries = Vec::new();
        let Some(rows) = rows else {
            return Ok(entries);
        };
        if rows.is_empty() {
            return Ok(entries);
        }

        tokio::fs::create_dir_all(&plugins_dir).await.ok();
        for row in rows {
            let plugin_id = row
                .get("plugin_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let version = row
                .get("version")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let app_id = row
                .get("app_id")
                .and_then(|v| v.as_str())
                .unwrap_or("default")
                .to_string();
            if plugin_id.is_empty() {
                continue;
            }

            // 安装目录:{plugin_root}/{app_id}/{plugin_id}/{version}/
            let install_path = plugin_root.join(&app_id).join(&plugin_id).join(&version);
            if !install_path.exists() {
                warn!(plugin_id = %plugin_id, "插件安装目录不存在,跳过");
                continue;
            }

            // 打成子 zip(只含 manifest+servicedata+wasm+wit+api+seeddata,整个目录压缩即可)
            let plugin_zip_name = format!("{plugin_id}.zip");
            let plugin_zip_path = plugins_dir.join(&plugin_zip_name);
            let package_rel = format!("plugins/{plugin_zip_name}");
            match ZipCompressor::compress_dir_to_memory(&install_path, 6) {
                Ok(zip_bytes) => {
                    tokio::fs::write(&plugin_zip_path, &zip_bytes)
                        .await
                        .map_err(|e| PluginError::Config(format!("写入插件子包失败: {e}")))?;
                    entries.push(ModulePluginEntry {
                        id: plugin_id.clone(),
                        version: version.clone(),
                        package: package_rel,
                    });
                    info!(plugin_id = %plugin_id, "插件子包导出成功");
                }
                Err(e) => {
                    warn!(plugin_id = %plugin_id, error = %e, "插件子包打包失败,跳过");
                }
            }
        }
        Ok(entries)
    }

    /// 写 JSON 到文件(美化格式,便于校验)
    fn write_json(path: &Path, value: &serde_json::Value) -> PluginResult<()> {
        let content = serde_json::to_string_pretty(value)
            .map_err(|e| PluginError::Config(format!("序列化 JSON 失败: {e}")))?;
        std::fs::write(path, content)
            .map_err(|e| PluginError::Config(format!("写入文件失败: {e}")))?;
        Ok(())
    }
}
