//! 模块包导出服务(ModuleExportService)
//!
//! 从数据库 + 文件系统聚合模块数据,导出为单一聚合 zip。
//! 版本号(package_version)为导出时刻自动生成的时间戳 yyyyMMddHHmmSS。
//!
//! 对称性契约(与 ModuleInstallService::install_module_resources 严格对称):
//! - forms/{stem}.json:从 cmx_form.definition 取出原始 JSON,stem = code 去掉 `{module}:` 前缀
//! - menus/{stem}.json:从 cmx_menu.extension(根菜单)取出原始 JSON
//! - metadata/tables/*.json:从 cmx_meta_table_define 查询,重新组装 `{ "tables": [TableDefine] }`
//! - permissions/{module}_permissions.json:从 cmx_permission 查询,组装 PermissionFile 结构
//! - plugins/{plugin_id}.zip:从 cmx_plugin 查询安装目录,取 manifest+servicedata+wasm+wit+api+seeddata 打包

use std::path::{Path, PathBuf};

use cmx_core::model::cell::DataValue;
use cmx_core::model::module::manifest::{
    ModuleInfo, ModuleManifest, ModulePluginEntry, ModuleResources, ModuleStats,
};
use cmx_database::DatabaseManager;
use cmx_utils::zip::ZipCompressor;
use tracing::{info, warn};

use crate::error::{PluginError, PluginResult};

/// 模块导出服务
pub struct ModuleExportService {
    plugin_root: PathBuf,
}

impl ModuleExportService {
    /// 创建导出服务
    pub fn new(plugin_root: PathBuf) -> Self {
        Self { plugin_root }
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

        // 1. 导出表单 → forms/{stem}.json
        let form_count =
            Self::export_forms(mm, db_id, module_code, &export_dir).await?;
        for i in 0..form_count {
            resources.forms.push(format!("forms/form_{i}.json"));
        }

        // 2. 导出菜单 → menus/{stem}.json
        let menu_count =
            Self::export_menus(mm, db_id, module_code, &export_dir).await?;
        for i in 0..menu_count {
            resources.menus.push(format!("menus/menu_{i}.json"));
        }

        // 3. 导出元数据 → metadata/tables/{table_name}.json
        let table_count =
            Self::export_metadata(mm, db_id, application_code, module_code, &export_dir).await?;
        if table_count > 0 {
            resources.metadata.push("metadata/tables/".to_string());
        }

        // 4. 导出权限 → permissions/{module}_permissions.json
        let perm_count =
            Self::export_permissions(mm, db_id, domain_code, application_code, module_code, &export_dir)
                .await?;
        if perm_count > 0 {
            resources
                .permissions
                .push(format!("permissions/{module_code}_permissions.json"));
        }

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

    /// 导出表单:从 cmx_form.definition 取出原始 JSON
    ///
    /// 对称:导入时 forms/*.json 整体存入 definition,导出时整体取出。
    async fn export_forms(
        mm: &DatabaseManager,
        db_id: &str,
        module_code: &str,
        export_dir: &Path,
    ) -> PluginResult<usize> {
        let forms_dir = export_dir.join("forms");
        let sql = "SELECT code, definition FROM cmx_form WHERE module_code = $1 AND archived = 0";
        let ds = mm
            .query_sql_with_datavalues(
                db_id,
                None,
                sql,
                vec![DataValue::String(module_code.to_string())],
                "export_forms",
            )
            .await
            .map_err(|e| PluginError::Database(format!("查询表单失败: {e}")))?;
        let json = serde_json::to_value(&ds)?;
        let rows = json.get("rows").and_then(|r| r.as_array());
        let mut count = 0;
        if let Some(rows) = rows {
            tokio::fs::create_dir_all(&forms_dir).await.ok();
            for (i, row) in rows.iter().enumerate() {
                // definition 是原始 JSON(整体透传);DB JSONB 可能以字符串返回,统一归一化为对象
                let definition = row.get("definition").cloned().unwrap_or_default();
                let definition = cmx_utils::json::coerce_to_object(definition);
                let file_path = forms_dir.join(format!("form_{i}.json"));
                Self::write_json(&file_path, &definition)?;
                count += 1;
            }
        }
        Ok(count)
    }

    /// 导出菜单:从 cmx_menu.definition(根菜单)取出原始 JSON
    ///
    /// 对称:导入时 menus/*.json 整体存入 definition,导出时整体取出。
    async fn export_menus(
        mm: &DatabaseManager,
        db_id: &str,
        module_code: &str,
        export_dir: &Path,
    ) -> PluginResult<usize> {
        let menus_dir = export_dir.join("menus");
        // 只导出根菜单(parent_id IS NULL),其 definition 含完整菜单树
        let sql = "SELECT code, definition FROM cmx_menu \
                   WHERE module_code = $1 AND parent_id IS NULL AND archived = 0";
        let ds = mm
            .query_sql_with_datavalues(
                db_id,
                None,
                sql,
                vec![DataValue::String(module_code.to_string())],
                "export_menus",
            )
            .await
            .map_err(|e| PluginError::Database(format!("查询菜单失败: {e}")))?;
        let json = serde_json::to_value(&ds)?;
        let rows = json.get("rows").and_then(|r| r.as_array());
        let mut count = 0;
        if let Some(rows) = rows {
            tokio::fs::create_dir_all(&menus_dir).await.ok();
            for (i, row) in rows.iter().enumerate() {
                // definition 是 JSONB,DB 取出可能是字符串或对象,统一归一化
                let definition = row.get("definition").cloned().unwrap_or_default();
                let menu_json = cmx_utils::json::coerce_to_object(definition);
                let file_path = menus_dir.join(format!("menu_{i}.json"));
                Self::write_json(&file_path, &menu_json)?;
                count += 1;
            }
        }
        Ok(count)
    }

    /// 导出元数据:连查主表(最新版本) + version 表(完整定义 JSON)
    ///
    /// 对称:导入时 metadata/tables/*.json 被解析建表,完整 JSON 存入 version 表;
    /// 导出时通过主表 cmx_meta_table_define 定位每个表的当前版本,
    /// 再从 version 表取对应的 metadata JSON,组装 `{ "tables": [...] }`。
    /// metadata 列是 JSONB,可能以 text 字符串返回,需解析为 JSON 对象后写入文件。
    async fn export_metadata(
        mm: &DatabaseManager,
        db_id: &str,
        application_code: &str,
        module_code: &str,
        export_dir: &Path,
    ) -> PluginResult<usize> {
        let tables_dir = export_dir.join("metadata").join("tables");
        // 连查:主表取最新版本的 table_name+version,关联 version 表取完整 metadata
        // 带 app_id 过滤(多应用隔离)
        let sql = "SELECT v.table_name, v.metadata \
                   FROM cmx_meta_table_define d \
                   INNER JOIN cmx_meta_table_define_version v \
                     ON d.table_name = v.table_name AND d.version = v.version \
                     AND d.app_id = v.app_id  \
                   WHERE d.module_code = $1 AND d.application_code = $2 AND d.archived = 0 AND v.archived = 0";
        let ds = mm
            .query_sql_with_datavalues(
                db_id,
                None,
                sql,
                vec![
                    DataValue::String(module_code.to_string()),
                    DataValue::String(application_code.to_string()),
                ],
                "export_metadata",
            )
            .await
            .map_err(|e| PluginError::Database(format!("查询表元数据失败: {e}")))?;
        let json = serde_json::to_value(&ds)?;
        let rows = json.get("rows").and_then(|r| r.as_array());
        let Some(rows) = rows else {
            return Ok(0);
        };
        if rows.is_empty() {
            return Ok(0);
        }

        let mut all_tables = Vec::new();
        for row in rows {
            // metadata 是 JSONB 列,DB 可能以 text 字符串或 JSON 对象返回
            // 统一解析为 JSON 对象,确保写入文件时是格式化的 JSON 而非转义字符串
            if let Some(metadata) = row.get("metadata")
                && !metadata.is_null()
            {
                let table_def = cmx_utils::json::coerce_to_object(metadata.clone());
                all_tables.push(table_def);
            }
        }

        if all_tables.is_empty() {
            return Ok(0);
        }

        tokio::fs::create_dir_all(&tables_dir).await.ok();
        // 写入一个 tables.json(包含所有表定义,对称于导入的 { "tables": [...] })
        let tables_doc = serde_json::json!({ "tables": all_tables });
        Self::write_json(&tables_dir.join("module_tables.json"), &tables_doc)?;
        Ok(all_tables.len())
    }

    /// 导出权限:从 cmx_permission 查询,组装 PermissionFile 结构
    ///
    /// 对称:导入时 permissions/*.json 解析 PermissionDefinition upsert;
    /// 导出时查询并组装相同结构(parent_id → parent_code 重建)。
    async fn export_permissions(
        mm: &DatabaseManager,
        db_id: &str,
        domain_code: &str,
        application_code: &str,
        module_code: &str,
        export_dir: &Path,
    ) -> PluginResult<usize> {
        let perms_dir = export_dir.join("permissions");
        let sql = "SELECT code, name, resource_type, parent_id, sort_order, description, \
                   extension, status FROM cmx_permission \
                   WHERE domain_code = $1 AND app_code = $2 AND module_code = $3 AND archived = 0";
        let ds = mm
            .query_sql_with_datavalues(
                db_id,
                None,
                sql,
                vec![
                    DataValue::String(domain_code.to_string()),
                    DataValue::String(application_code.to_string()),
                    DataValue::String(module_code.to_string()),
                ],
                "export_permissions",
            )
            .await
            .map_err(|e| PluginError::Database(format!("查询权限失败: {e}")))?;
        let json = serde_json::to_value(&ds)?;
        let rows = json.get("rows").and_then(|r| r.as_array());
        let Some(rows) = rows else {
            return Ok(0);
        };
        if rows.is_empty() {
            return Ok(0);
        }

        // 构建 parent_id → code 映射(用于重建 parent_code)
        let mut id_to_code: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        // 查询 id→code 映射(包括父权限)
        let id_code_sql = "SELECT id, code FROM cmx_permission \
                           WHERE domain_code = $1 AND app_code = $2 AND module_code = $3";
        let id_ds = mm
            .query_sql_with_datavalues(
                db_id,
                None,
                id_code_sql,
                vec![
                    DataValue::String(domain_code.to_string()),
                    DataValue::String(application_code.to_string()),
                    DataValue::String(module_code.to_string()),
                ],
                "export_perm_id_code",
            )
            .await
            .map_err(|e| PluginError::Database(format!("查询权限 id→code 失败: {e}")))?;
        let id_json = serde_json::to_value(&id_ds)?;
        if let Some(id_rows) = id_json.get("rows").and_then(|r| r.as_array()) {
            for row in id_rows {
                let id = row.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let code = row.get("code").and_then(|v| v.as_str()).unwrap_or("").to_string();
                if !id.is_empty() {
                    id_to_code.insert(id, code);
                }
            }
        }

        // 组装 PermissionFile 结构(使用 cmx-core 统一契约,对称于导入端)
        use cmx_core::model::iam::{PermissionDefinition, PermissionFile};
        let mut perm_defs: Vec<PermissionDefinition> = Vec::new();
        for row in rows {
            let parent_code = row
                .get("parent_id")
                .and_then(|v| v.as_str())
                .and_then(|pid| id_to_code.get(pid))
                .cloned();
            // extension 可能是 JSON 字符串或 null
            let extension = row
                .get("extension")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let def = PermissionDefinition {
                code: row.get("code").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                name: row.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                resource_type: row
                    .get("resource_type")
                    .and_then(|v| v.as_str())
                    .map(String::from),
                parent_code,
                sort_order: row.get("sort_order").and_then(|v| v.as_i64()),
                description: row.get("description").and_then(|v| v.as_str()).map(String::from),
                extension,
                status: row.get("status").and_then(|v| v.as_i64()),
            };
            perm_defs.push(def);
        }

        let perm_file = PermissionFile {
            name: format!("{}_permissions", module_code),
            version: "1.0.0".to_string(),
            description: format!("模块 {} 权限定义", module_code),
            permissions: perm_defs.clone(),
        };
        let perm_file_json = serde_json::to_value(&perm_file)?;

        tokio::fs::create_dir_all(&perms_dir).await.ok();
        Self::write_json(
            &perms_dir.join(format!("{module_code}_permissions.json")),
            &perm_file_json,
        )?;
        Ok(perm_defs.len())
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
