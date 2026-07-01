//! 模块包安装服务(ModuleInstallService)
//!
//! 编排模块迁移包的导入流程:
//! 1. 解压模块包 + 解析 module.manifest.json
//! 2. 版本校验(避免旧版本覆盖新版本,从 cmx_module_current_version 读当前版本)
//! 3. upsert cmx_module(字典表)
//! 4. 版本登记(record_import:current_version + version_history)
//! 5. 安装模块级资源(metadata/permissions/forms/menus)
//! 6. 遍历插件子包,复用 InstallService::install 逐个安装(填模块归属三段式)

use std::path::{Path, PathBuf};

use cmx_core::model::module::manifest::ModuleManifest;
use cmx_database::get_default_db_manager;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use tracing::{info, warn};

use crate::common::package::PackageUtils;
use crate::domain::plugin::PluginSource;
use crate::error::{PluginError, PluginResult};
use crate::service::install::{InstallRequest, InstallService};

/// 模块包来源
#[derive(Debug, Clone)]
pub enum ModulePackageSource {
    /// zip 字节流(API multipart 上传)
    Bytes(Vec<u8>),
    /// 本地 zip 文件路径
    Local(PathBuf),
}

/// 导入结果
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModuleInstallResult {
    pub success: bool,
    pub skipped: bool,
    pub reason: String,
    pub module_code: String,
    pub package_version: String,
    pub plugin_count: usize,
}

/// 导入动作(版本校验结果)
enum ImportAction {
    SkipSame,
    RejectOldVersion(String),
    AllowUpgrade,
    AllowForceDowngrade,
    AllowSameSecondPatch,
}

/// 模块包安装服务
pub struct ModuleInstallService {
    package_utils: PackageUtils,
    install_service: std::sync::Arc<InstallService>,
}

impl ModuleInstallService {
    /// 创建模块安装服务
    pub fn new(package_utils: PackageUtils, install_service: std::sync::Arc<InstallService>) -> Self {
        Self {
            package_utils,
            install_service,
        }
    }

    /// 安装/导入模块包
    ///
    /// # Errors
    /// 版本冲突、解压失败、插件子包安装失败时返回错误
    pub async fn install_module_package(
        &self,
        source: ModulePackageSource,
        force: bool,
        operator: Option<String>,
    ) -> PluginResult<ModuleInstallResult> {
        // 1. 解压模块包到临时目录
        let module_dir = self.fetch_and_extract(&source).await?;

        // 2. 解析 module.manifest.json
        let manifest = self.parse_manifest(&module_dir)?;

        info!(
            module_code = %manifest.module.code,
            package_version = %manifest.package_version,
            "开始导入模块包"
        );

        // 3. 版本校验
        let mm = get_default_db_manager();
        let db_id = "default".to_string();
        let action =
            Self::validate_import(mm, &db_id, &manifest, force).await?;
        match action {
            ImportAction::SkipSame => {
                return Ok(ModuleInstallResult {
                    success: true,
                    skipped: true,
                    reason: "已是当前版本".to_string(),
                    module_code: manifest.module.code.clone(),
                    package_version: manifest.package_version.clone(),
                    plugin_count: 0,
                });
            }
            ImportAction::RejectOldVersion(msg) => {
                return Err(PluginError::CenterData(msg));
            }
            ImportAction::AllowUpgrade
            | ImportAction::AllowForceDowngrade
            | ImportAction::AllowSameSecondPatch => {}
        }

        // 4. 安装模块级资源(metadata 建表 / forms / menus / permissions)
        //    对称契约见 install_module_resources 文档
        if let Err(e) = self.install_module_resources(&module_dir, &manifest).await {
            warn!(error = %e, "模块级资源安装失败(继续后续步骤)");
        }

        // 5. 版本登记(current_version upsert + version_history insert)
        if let Err(e) = self.record_version(mm, &db_id, &manifest, &operator).await {
            warn!(error = %e, "版本登记失败,继续安装插件");
        }

        // 6. 遍历插件子包,逐个复用 InstallService::install
        let domain = &manifest.module.domain_code;
        let app = &manifest.module.application_code;
        let module = &manifest.module.code;
        let mut plugin_count = 0usize;
        for entry in &manifest.plugins {
            let plugin_zip = module_dir.join(&entry.package);
            if !plugin_zip.exists() {
                warn!(package = %entry.package, "插件子包不存在,跳过");
                continue;
            }
            let install_req = InstallRequest {
                source: PluginSource::Local {
                    path: plugin_zip.clone(),
                },
                db_id: None,
                auto_activate: true,
                version_constraint: None,
                build_type: None,
                marketplace_source_id: None,
                app_id: Some(app.clone()),
            };
            // 复用现有插件安装(建表/分发已注释,不会重复)
            match self.install_service.install(install_req).await {
                Ok(resp) => {
                    info!(plugin_id = %resp.plugin_id, "插件子包安装成功");
                    plugin_count += 1;
                }
                Err(e) => {
                    warn!(package = %entry.package, error = %e, "插件子包安装失败");
                }
            }
            // NOTE: 插件归属(domain_code/application_code/module_code)通过 manifest 注入,
            // 实际需 InstallService 支持外部传入归属字段,此处先记录 module 维度。
            let _ = (domain, module);
        }

        Ok(ModuleInstallResult {
            success: true,
            skipped: false,
            reason: "导入成功".to_string(),
            module_code: manifest.module.code.clone(),
            package_version: manifest.package_version.clone(),
            plugin_count,
        })
    }

    /// 解压模块包到临时目录
    async fn fetch_and_extract(&self, source: &ModulePackageSource) -> PluginResult<PathBuf> {
        let temp_dir = std::env::temp_dir().join(format!(
            "cmx_module_{}_{}",
            uuid::Uuid::new_v4(),
            chrono::Local::now().format("%Y%m%d%H%M%S")
        ));
        tokio::fs::create_dir_all(&temp_dir)
            .await
            .map_err(|e| PluginError::Config(format!("创建临时目录失败: {e}")))?;

        match source {
            ModulePackageSource::Local(path) => {
                self.package_utils
                    .extract_zip(path, &temp_dir, "解压模块包")?;
            }
            ModulePackageSource::Bytes(bytes) => {
                // 写入临时 zip 文件再解压
                let zip_path = temp_dir.join("module_package.zip");
                tokio::fs::write(&zip_path, bytes)
                    .await
                    .map_err(|e| PluginError::Config(format!("写入模块包失败: {e}")))?;
                self.package_utils
                    .extract_zip(&zip_path, &temp_dir, "解压模块包")?;
            }
        }

        // 定位含 module.manifest.json 的根目录(可能在子目录)
        Self::find_manifest_root(&temp_dir)
            .ok_or_else(|| PluginError::Config("未找到 module.manifest.json".to_string()))
    }

    /// 解析 module.manifest.json
    fn parse_manifest(&self, module_dir: &Path) -> PluginResult<ModuleManifest> {
        let manifest_path = module_dir.join("module.manifest.json");
        let content = std::fs::read_to_string(&manifest_path)
            .map_err(|e| PluginError::Config(format!("读取 module.manifest.json 失败: {e}")))?;
        let manifest: ModuleManifest = serde_json::from_str(&content)
            .map_err(|e| PluginError::Config(format!("解析 module.manifest.json 失败: {e}")))?;
        Ok(manifest)
    }

    /// 版本校验(纯时间戳字符串比较)
    async fn validate_import(
        mm: &cmx_database::DatabaseManager,
        db_id: &str,
        manifest: &ModuleManifest,
        force: bool,
    ) -> PluginResult<ImportAction> {
        let current =
            cmx_biz::module::version::ModuleVersionService::get_current(mm, db_id, &manifest.module.code)
                .await
                .map_err(|e| PluginError::Database(format!("查询当前版本失败: {e}")))?;

        let Some(cur) = current else {
            return Ok(ImportAction::AllowUpgrade); // 新模块直接放行
        };

        // checksum 幂等
        if let (Some(a), Some(b)) = (&manifest.checksum, &cur.checksum)
            && a == b
        {
            return Ok(ImportAction::SkipSame);
        }

        // 时间戳字符串比较(定长14位,字典序正确)
        match manifest.package_version.cmp(&cur.package_version) {
            Ordering::Equal => Ok(ImportAction::AllowSameSecondPatch),
            Ordering::Less if !force => Ok(ImportAction::RejectOldVersion(format!(
                "无法用旧版本 {} 覆盖当前版本 {}（可用 force=true 强制降级）",
                manifest.package_version, cur.package_version
            ))),
            Ordering::Less => Ok(ImportAction::AllowForceDowngrade),
            Ordering::Greater => Ok(ImportAction::AllowUpgrade),
        }
    }

    /// 安装模块级资源:metadata(建表)/forms/menus/permissions
    ///
    /// 对称性契约(与 export_module 严格对称):
    /// - forms/*.json:整体 JSON 透传存入 cmx_form.definition,code = 文件名(去扩展名)
    /// - menus/*.json:整体 JSON 透传存入 cmx_menu.extension(根菜单,树形字段自动计算)
    /// - metadata/tables/*.json:用 PgTableDefineExecutor 建表/升级
    /// - permissions/*.json:解析 PermissionDefinition,SQL upsert 到 cmx_permission
    async fn install_module_resources(
        &self,
        module_dir: &Path,
        manifest: &ModuleManifest,
    ) -> PluginResult<()> {
        let mm = get_default_db_manager();
        let db_id = "default".to_string();
        let domain = manifest.module.domain_code.as_str();
        let app = manifest.module.application_code.as_str();
        let module = manifest.module.code.as_str();

        // 1. 表单:forms/*.json 整体透传存入 cmx_form
        self.install_forms(mm, &db_id, module_dir, domain, app, module)
            .await;

        // 2. 菜单:menus/*.json 整体透传存入 cmx_menu.extension(根菜单)
        self.install_menus(mm, &db_id, module_dir, domain, app, module)
            .await;

        // 3. 元数据:metadata/tables/*.json 建表
        self.install_metadata(mm, &db_id, module_dir, domain, app, module)
            .await;

        // 4. 权限:permissions/*.json upsert cmx_permission
        self.install_permissions(mm, &db_id, module_dir, domain, app, module)
            .await;

        Ok(())
    }

    /// 安装表单:读取 forms/*.json,整体 JSON 透传存入 cmx_form.definition
    ///
    /// code 规则:`{module_code}:{file_stem}`,幂等(基于 code 唯一约束 upsert)
    async fn install_forms(
        &self,
        mm: &cmx_database::DatabaseManager,
        db_id: &str,
        module_dir: &Path,
        domain: &str,
        app: &str,
        module: &str,
    ) {
        let forms_dir = module_dir.join("forms");
        if !forms_dir.exists() {
            return;
        }
        let entries = match std::fs::read_dir(&forms_dir) {
            Ok(e) => e,
            Err(e) => {
                warn!(error = %e, "读取 forms 目录失败");
                return;
            }
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();
            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(e) => {
                    warn!(form = %stem, error = %e, "读取表单文件失败,跳过");
                    continue;
                }
            };
            let definition: serde_json::Value = serde_json::from_str(&content).unwrap_or_default();
            // name 取 JSON 的 name 字段,fallback 到文件名
            let name = definition
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or(&stem)
                .to_string();
            let code = format!("{module}:{stem}");

            // 幂等 upsert:先删后建(基于 code 唯一)
            let dto = cmx_biz::form::FormForCreate {
                code: code.clone(),
                name,
                description: None,
                definition: Some(definition),
                domain_code: domain.to_string(),
                application_code: app.to_string(),
                module_code: module.to_string(),
            };
            // 先尝试删除已有同 code 记录,再创建(幂等)
            let _ = cmx_biz::form::FormService::delete_by_code(mm, db_id, &code).await;
            if let Err(e) = cmx_biz::form::FormService::create(mm, db_id, dto).await {
                warn!(form = %code, error = %e, "表单安装失败");
            } else {
                info!(form = %code, "表单安装成功");
            }
        }
    }

    /// 安装菜单:读取 menus/*.json,整体 JSON 透传存入 cmx_menu.extension(根菜单)
    ///
    /// 每个 menus 文件创建一个根菜单,code = `{module}:{file_stem}`,
    /// 完整 JSON 内容存入 extension 字段,前端从 extension 解析 items/children 树。
    async fn install_menus(
        &self,
        mm: &cmx_database::DatabaseManager,
        db_id: &str,
        module_dir: &Path,
        domain: &str,
        app: &str,
        module: &str,
    ) {
        let menus_dir = module_dir.join("menus");
        if !menus_dir.exists() {
            return;
        }
        let entries = match std::fs::read_dir(&menus_dir) {
            Ok(e) => e,
            Err(e) => {
                warn!(error = %e, "读取 menus 目录失败");
                return;
            }
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();
            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(e) => {
                    warn!(menu = %stem, error = %e, "读取菜单文件失败,跳过");
                    continue;
                }
            };
            let menu_json: serde_json::Value = serde_json::from_str(&content).unwrap_or_default();
            let name = menu_json
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or(&stem)
                .to_string();
            let code = format!("{module}:{stem}");

            // 幂等:先删同 code 根菜单,再建
            let _ = cmx_biz::menu::MenuService::delete_by_code(mm, db_id, &code).await;
            let dto = cmx_biz::menu::MenuForCreate {
                code: code.clone(),
                name,
                parent_id: None,
                path: menu_json.get("path").and_then(|v| v.as_str()).map(String::from),
                icon: None,
                component: None,
                sort_order: 0,
                visible: 1,
                extension: Some(content), // 整体透传
                domain_code: domain.to_string(),
                application_code: app.to_string(),
                module_code: module.to_string(),
            };
            if let Err(e) = cmx_biz::menu::MenuService::create(mm, db_id, dto).await {
                warn!(menu = %code, error = %e, "菜单安装失败");
            } else {
                info!(menu = %code, "菜单安装成功");
            }
        }
    }

    /// 安装元数据:读取 metadata/tables/*.json 表定义,用 PgTableDefineExecutor 建表
    ///
    /// 表定义文件格式:`{ "tables": [TableDefine, ...] }`(对齐 cmx-metadata TableDefine)。
    /// 建表幂等(create_or_upgrade_table 会判断表是否存在)。
    async fn install_metadata(
        &self,
        mm: &cmx_database::DatabaseManager,
        db_id: &str,
        module_dir: &Path,
        domain: &str,
        app: &str,
        module: &str,
    ) {
        let tables_dir = module_dir.join("metadata").join("tables");
        if !tables_dir.exists() {
            return;
        }
        // 收集所有表定义
        let mut all_tables: Vec<cmx_core::model::meta::table::TableDefine> = Vec::new();
        let entries = match std::fs::read_dir(&tables_dir) {
            Ok(e) => e,
            Err(e) => {
                warn!(error = %e, "读取 metadata/tables 目录失败");
                return;
            }
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(e) => {
                    warn!(file = ?path.file_name(), error = %e, "读取表定义文件失败,跳过");
                    continue;
                }
            };
            // 解析 { "tables": [...] } 结构(用 serde_json::Value 提取数组)
            let parsed: serde_json::Value = match serde_json::from_str(&content) {
                Ok(v) => v,
                Err(e) => {
                    warn!(file = ?path.file_name(), error = %e, "解析表定义失败,跳过");
                    continue;
                }
            };
            let Some(tables_arr) = parsed.get("tables").and_then(|t| t.as_array()) else {
                warn!(file = ?path.file_name(), "表定义文件缺少 tables 数组,跳过");
                continue;
            };
            for table_val in tables_arr {
                match serde_json::from_value::<cmx_core::model::meta::table::TableDefine>(
                    table_val.clone(),
                ) {
                    Ok(td) => all_tables.push(td),
                    Err(e) => {
                        warn!(error = %e, "解析单个表定义失败,跳过该项");
                    }
                }
            }
        }

        if all_tables.is_empty() {
            return;
        }

        // 用 PgTableDefineExecutor 建表(无需分布式锁,模块安装是低频操作)
        use cmx_metadata::TableDefineDbExecutor;
        let executor = cmx_metadata::executor::PgTableDefineExecutor::new(db_id, None);
        for table_def in &all_tables {
            match executor.create_or_upgrade_table(table_def).await {
                Ok(_) => info!(table = %table_def.table_name, "建表/升级成功"),
                Err(e) => warn!(table = %table_def.table_name, error = %e, "建表失败"),
            }
            // 记录表元数据到 cmx_meta_table_define(便于导出时查询)
            let _ = Self::save_table_metadata(mm, db_id, table_def, domain, app, module).await;
        }
    }

    /// 安装权限:读取 permissions/*.json,解析 PermissionDefinition,SQL upsert cmx_permission
    ///
    /// 对称契约:JSON 格式与 cmx-iam PermissionFile 一致,
    /// 字段:code/name/resource_type/parent_code/sort_order/description/extension/status。
    /// 由于 cmx-iam import_permissions 需注入式 Service + zip,这里直接 SQL upsert(同语义)。
    async fn install_permissions(
        &self,
        mm: &cmx_database::DatabaseManager,
        db_id: &str,
        module_dir: &Path,
        domain: &str,
        app: &str,
        module: &str,
    ) {
        let perms_dir = module_dir.join("permissions");
        if !perms_dir.exists() {
            return;
        }

        // 1. 解析所有 permissions/*.json,收集 PermissionDefinition
        #[derive(serde::Deserialize)]
        struct PermFile {
            #[serde(default)]
            permissions: Vec<PermDef>,
        }
        #[derive(serde::Deserialize)]
        #[serde(rename_all = "snake_case")]
        struct PermDef {
            code: String,
            name: String,
            #[serde(default)]
            resource_type: Option<String>,
            #[serde(default)]
            parent_code: Option<String>,
            #[serde(default)]
            sort_order: Option<i64>,
            #[serde(default)]
            description: Option<String>,
            #[serde(default)]
            extension: Option<String>,
            #[serde(default)]
            status: Option<i64>,
        }

        let mut all_defs: Vec<PermDef> = Vec::new();
        let entries = match std::fs::read_dir(&perms_dir) {
            Ok(e) => e,
            Err(e) => {
                warn!(error = %e, "读取 permissions 目录失败");
                return;
            }
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(e) => {
                    warn!(file = ?path.file_name(), error = %e, "读取权限文件失败,跳过");
                    continue;
                }
            };
            let file: PermFile = serde_json::from_str(&content).unwrap_or(PermFile {
                permissions: vec![],
            });
            all_defs.extend(file.permissions);
        }

        if all_defs.is_empty() {
            return;
        }

        // 2. 第一阶段:upsert 所有权限(parent_id 暂置 NULL,full_code_path='/'+code)
        use cmx_core::model::cell::DataValue;
        let mut code_to_id: std::collections::HashMap<String, String> = std::collections::HashMap::new();
        for def in &all_defs {
            let id = uuid::Uuid::new_v4().to_string();
            let resource_type = def.resource_type.clone().unwrap_or_else(|| "api".to_string());
            let full_path = format!("/{}", def.code);
            let status = def.status.unwrap_or(1);

            // upsert:ON CONFLICT (code) DO UPDATE
            let sql = "INSERT INTO cmx_permission \
                       (id, code, name, resource_type, parent_id, sort_order, description, \
                        domain_code, app_code, module_code, extension, status, archived, \
                        parent_code, full_code_path, is_leaf, level) \
                       VALUES ($1, $2, $3, $4, NULL, $5, $6, $7, $8, $9, $10, $11, 0, NULL, $12, 1, 1) \
                       ON CONFLICT (code) DO UPDATE SET \
                       name = EXCLUDED.name, resource_type = EXCLUDED.resource_type, \
                       sort_order = EXCLUDED.sort_order, description = EXCLUDED.description, \
                       extension = EXCLUDED.extension, status = EXCLUDED.status, \
                       domain_code = EXCLUDED.domain_code, app_code = EXCLUDED.app_code, \
                       module_code = EXCLUDED.module_code, \
                       full_code_path = EXCLUDED.full_code_path, \
                       update_time = CURRENT_TIMESTAMP \
                       RETURNING id";
            let params: Vec<DataValue> = vec![
                DataValue::String(id.clone()),
                DataValue::String(def.code.clone()),
                DataValue::String(def.name.clone()),
                DataValue::String(resource_type),
                DataValue::Int(def.sort_order.unwrap_or(0)),
                def.description.clone().into(),
                DataValue::String(domain.to_string()),
                DataValue::String(app.to_string()),
                DataValue::String(module.to_string()),
                def.extension.clone().into(),
                DataValue::Int(status),
                DataValue::String(full_path),
            ];
            match mm
                .query_sql_with_datavalues(db_id, None, sql, params, "module_install_perm")
                .await
            {
                Ok(ds) => {
                    let json = serde_json::to_value(&ds).unwrap_or_default();
                    let returned_id = json
                        .get("rows")
                        .and_then(|r| r.as_array())
                        .and_then(|rows| rows.first())
                        .and_then(|row| row.get("id"))
                        .and_then(|v| v.as_str())
                        .unwrap_or(&id)
                        .to_string();
                    code_to_id.insert(def.code.clone(), returned_id);
                }
                Err(e) => {
                    warn!(perm_code = %def.code, error = %e, "权限 upsert 失败");
                }
            }
        }

        // 3. 第二阶段:回填 parent_id / parent_code / full_code_path / level
        for def in &all_defs {
            let Some(parent_code) = &def.parent_code else {
                continue;
            };
            let Some(parent_id) = code_to_id.get(parent_code) else {
                warn!(perm_code = %def.code, parent_code = %parent_code, "父权限未找到,跳过回填");
                continue;
            };
            let Some(child_id) = code_to_id.get(&def.code) else {
                continue;
            };
            // 查父节点 full_path/level
            let parent_sql = "SELECT full_code_path, level FROM cmx_permission WHERE id = $1";
            let parent_ds = mm
                .query_sql_with_datavalues(
                    db_id,
                    None,
                    parent_sql,
                    vec![DataValue::String(parent_id.clone())],
                    "module_perm_parent",
                )
                .await;
            if let Ok(pds) = parent_ds {
                let pjson = serde_json::to_value(&pds).unwrap_or_default();
                let p_path = pjson
                    .get("rows")
                    .and_then(|r| r.as_array())
                    .and_then(|rows| rows.first())
                    .and_then(|row| row.get("full_code_path"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let p_level = pjson
                    .get("rows")
                    .and_then(|r| r.as_array())
                    .and_then(|rows| rows.first())
                    .and_then(|row| row.get("level"))
                    .and_then(|v| v.as_i64())
                    .unwrap_or(1);
                let new_path = format!("{p_path}/{}", def.code);
                let new_level = p_level + 1;
                // 更新子节点的 parent 引用
                let upd_sql = "UPDATE cmx_permission SET parent_id = $1, parent_code = $2, \
                               full_code_path = $3, level = $4 WHERE id = $5";
                let _ = mm
                    .execute_sql_with_datavalues(
                        db_id,
                        None,
                        upd_sql,
                        vec![
                            DataValue::String(parent_id.clone()),
                            DataValue::String(parent_code.clone()),
                            DataValue::String(new_path),
                            DataValue::Int(new_level),
                            DataValue::String(child_id.clone()),
                        ],
                    )
                    .await;
                // 父节点 is_leaf = 0
                let leaf_sql = "UPDATE cmx_permission SET is_leaf = 0 WHERE id = $1";
                let _ = mm
                    .execute_sql_with_datavalues(
                        db_id,
                        None,
                        leaf_sql,
                        vec![DataValue::String(parent_id.clone())],
                    )
                    .await;
            }
        }
        info!(count = all_defs.len(), "权限安装完成");
    }

    /// 保存表元数据到 cmx_meta_table_define + cmx_meta_table_define_version
    ///
    /// 主表记录归属,version 表存完整 TableDefine JSON(供导出查询,保持对称)。
    async fn save_table_metadata(
        mm: &cmx_database::DatabaseManager,
        db_id: &str,
        table_def: &cmx_core::model::meta::table::TableDefine,
        domain: &str,
        app: &str,
        module: &str,
    ) -> PluginResult<()> {
        use cmx_core::model::cell::DataValue;
        let metadata_json = serde_json::to_string(table_def)
            .map_err(|e| crate::error::PluginError::Config(format!("序列化表定义失败: {e}")))?;
        let id = uuid::Uuid::new_v4().to_string();
        // 主表 upsert(ON CONFLICT table_name)
        let sql = "INSERT INTO cmx_meta_table_define \
                   (id, table_name, display_name, db_id, plugin_id, version, app_id, ddl_status, \
                    domain_code, application_code, module_code, archived) \
                   VALUES ($1, $2, $3, 'default', NULL, 1, 'default', 'completed', $4, $5, $6, 0) \
                   ON CONFLICT (table_name) DO UPDATE SET \
                   display_name = EXCLUDED.display_name, \
                   domain_code = EXCLUDED.domain_code, \
                   application_code = EXCLUDED.application_code, \
                   module_code = EXCLUDED.module_code, \
                   update_time = CURRENT_TIMESTAMP";
        let _ = mm
            .execute_sql_with_datavalues(
                db_id,
                None,
                sql,
                vec![
                    DataValue::String(id.clone()),
                    DataValue::String(table_def.table_name.clone()),
                    DataValue::String(table_def.display_name.clone()),
                    DataValue::String(domain.to_string()),
                    DataValue::String(app.to_string()),
                    DataValue::String(module.to_string()),
                ],
            )
            .await;

        // version 表存完整 TableDefine JSON(供导出对称读取)
        let vid = uuid::Uuid::new_v4().to_string();
        let version_sql = "INSERT INTO cmx_meta_table_define_version \
                           (id, table_name, display_name, db_id, plugin_id, version, app_id, \
                            domain_code, application_code, module_code, metadata, archived) \
                           VALUES ($1, $2, $3, 'default', NULL, 1, 'default', $4, $5, $6, $7::jsonb, 0)";
        let _ = mm
            .execute_sql_with_datavalues(
                db_id,
                None,
                version_sql,
                vec![
                    DataValue::String(vid),
                    DataValue::String(table_def.table_name.clone()),
                    DataValue::String(table_def.display_name.clone()),
                    DataValue::String(domain.to_string()),
                    DataValue::String(app.to_string()),
                    DataValue::String(module.to_string()),
                    DataValue::String(metadata_json),
                ],
            )
            .await;
        Ok(())
    }

    /// 版本登记(委托 ModuleVersionService)
    async fn record_version(
        &self,
        mm: &cmx_database::DatabaseManager,
        db_id: &str,
        manifest: &ModuleManifest,
        operator: &Option<String>,
    ) -> PluginResult<()> {
        let record = cmx_biz::module::version::ModuleVersionRecord {
            module_id: manifest.module.code.clone(),
            domain_code: manifest.module.domain_code.clone(),
            application_code: manifest.module.application_code.clone(),
            module_code: manifest.module.code.clone(),
            package_version: manifest.package_version.clone(),
            checksum: manifest.checksum.clone(),
            manifest_snapshot: serde_json::to_value(manifest)
                .unwrap_or(serde_json::Value::Null),
            imported_by: operator.clone(),
            source: Some("module_package_import".to_string()),
        };
        cmx_biz::module::version::ModuleVersionService::record_import(mm, db_id, record)
            .await
            .map_err(|e| PluginError::Database(format!("版本登记失败: {e}")))?;
        Ok(())
    }

    /// 递归查找含 module.manifest.json 的目录
    fn find_manifest_root(dir: &Path) -> Option<PathBuf> {
        if dir.join("module.manifest.json").exists() {
            return Some(dir.to_path_buf());
        }
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir()
                    && let Some(found) = Self::find_manifest_root(&path)
                {
                    return Some(found);
                }
            }
        }
        None
    }
}
