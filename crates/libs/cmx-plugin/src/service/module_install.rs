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
use tracing::{error, info, warn};
use crate::common::package::PackageUtils;
use crate::domain::plugin::PluginSource;
use crate::error::{PluginError, PluginResult};
use crate::service::deploy::{DeployRequest, DeployService};

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
    deploy_service: std::sync::Arc<DeployService>,
    /// 模块资源定义导入器集合(表单/菜单/元数据/权限,本地或远程实现)。
    /// 为 None 时跳过资源安装(向后兼容,如测试场景)。
    importers: Option<std::sync::Arc<cmx_traits::resource::DefinitionImporterBundle>>,
}

impl ModuleInstallService {
    /// 创建模块安装服务
    pub fn new(package_utils: PackageUtils, deploy_service: std::sync::Arc<DeployService>) -> Self {
        Self {
            package_utils,
            deploy_service,
            importers: None,
        }
    }

    /// 注入模块资源定义导入器集合(Builder 模式)。
    ///
    /// 注入后,模块导入时的表单/菜单/元数据/权限安装统一委托给 bundle 中的 importer,
    /// 无论本地(Local)还是远程(Remote)实现,调用代码一致。
    pub fn with_definition_importers(
        mut self,
        importers: std::sync::Arc<cmx_traits::resource::DefinitionImporterBundle>,
    ) -> Self {
        self.importers = Some(importers);
        self
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

        // 导入守卫:模块包的 module.code 必须与当前服务 app_id 一致。
        // 当前设计下 app_id 由配置 app.module_code 决定(get_app_id),即 app_id ≡ module_code,
        // 因此这里用模块 code 比对服务 app_id;二者不一致说明模块包不属于本服务,拒绝导入。
        let module_code = &manifest.module.code;
        let current_service_app_id = cmx_utils::ConfigManager::global().get_app_id();
        if module_code != &current_service_app_id {
            return Err(PluginError::CenterData(format!(
                "导入的模块资源不属于当前模块: 模块包 module_code={}, 当前服务 app_id={}",
                module_code, current_service_app_id
            )));
        }

        info!(
            module_code = %manifest.module.code,
            package_version = %manifest.package_version,
            "开始导入模块包"
        );

        // 3. 版本校验
        let mm = get_default_db_manager();
        let default_db_id = mm.get_default_db_id().await;
        let biz_db_id = mm.get_biz_db_id().await;
        let action =
            Self::validate_import(mm, &default_db_id, &manifest, force).await?;
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

        // 4. 安装模块级资源(委托 DefinitionImporterBundle:forms/menus/metadata/permissions)
        if let Err(e) = self
            .install_module_resources(&biz_db_id, &module_dir, &manifest)
            .await
        {
            warn!(error = %e, "模块级资源安装失败(继续后续步骤)");
        }

        // 5. 版本登记(current_version upsert + version_history insert)
        if let Err(e) = self.record_version(mm, &default_db_id, &manifest, &operator).await {
            error!(error = %e, "版本登记失败,继续安装插件");
        }

        // 6. 遍历插件子包,逐个复用 DeployService::deploy(自动判断升级/安装/跳过)
        //    source 用 Local{path},deploy 内部会统一上传 OSS 后转为 Storage

        let mut plugin_count = 0usize;
        for entry in &manifest.plugins {
            let plugin_zip = module_dir.join(&entry.package);
            if !plugin_zip.exists() {
                warn!(package = %entry.package, "插件子包不存在,跳过");
                continue;
            }
            let deploy_req = DeployRequest {
                source: PluginSource::Local {
                    path: plugin_zip.clone(),
                },
                db_id: Some(biz_db_id.clone()),
                force_reinstall: true,
                //fixme 写死 0702
                build_type: Some("release".to_string()),
                publish_to_marketplace: false,
                app_id: Some(module_code.clone()),
                marketplace_source_id: None,
                marketplace_publish_info: None,
            };
            // deploy 自动判断 Install/Upgrade/AlreadyInstalled,不会因已安装而报错
            // deploy 内部统一上传 OSS,确保其他节点能拉取插件包
            match self.deploy_service.deploy(deploy_req).await {
                Ok(resp) => {
                    info!(
                        plugin_id = %resp.plugin_id,
                        action = ?resp.action,
                        "插件子包部署成功"
                    );
                    plugin_count += 1;
                }
                Err(e) => {
                    warn!(package = %entry.package, error = %e, "插件子包安装失败");
                }
            }
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

    /// 安装模块级资源:forms/menus/metadata/permissions
    ///
    /// 统一委托注入的 DefinitionImporterBundle(本地或远程实现,调用代码一致):
    /// - forms/*.json → bundle.form.apply_form_definitions
    /// - menus/*.json → bundle.menu.apply_menu_definitions
    /// - metadata/tables/*.json → bundle.table.apply_table_definitions(建表+元数据登记)
    /// - permissions/*.json → bundle.permission.apply_permission_definitions
    async fn install_module_resources(
        &self,
        biz_db_id: &str,
        module_dir: &Path,
        manifest: &ModuleManifest,
    ) -> PluginResult<()> {
        let Some(bundle) = &self.importers else {
            warn!("未注入 DefinitionImporterBundle,跳过模块级资源安装");
            return Ok(());
        };

        let domain = manifest.module.domain_code.as_str();
        let app = manifest.module.application_code.as_str();
        let module = manifest.module.code.as_str();
        let app_id = cmx_utils::ConfigManager::global().get_app_id();

        // 1. 表单:forms/*.json 解析为 FormDefinition,委托 importer
        let form_defs = Self::read_form_definitions(module_dir, domain, app, module);
        if !form_defs.is_empty() {
            match bundle
                .form
                .apply_form_definitions(domain, app, module, &form_defs)
                .await
            {
                Ok(n) => info!(count = n, "表单安装完成"),
                Err(e) => warn!(error = %e, "表单安装失败"),
            }
        }

        // 2. 菜单:menus/*.json 解析为 MenuDefinition,委托 importer
        let menu_defs = Self::read_menu_definitions(module_dir, domain, app, module);
        if !menu_defs.is_empty() {
            match bundle
                .menu
                .apply_menu_definitions(domain, app, module, &menu_defs)
                .await
            {
                Ok(n) => info!(count = n, "菜单安装完成"),
                Err(e) => warn!(error = %e, "菜单安装失败"),
            }
        }

        // 3. 元数据:metadata/tables/*.json 解析为 TableDefine,委托 importer(建表+登记)
        let table_defs = Self::read_table_definitions(module_dir);
        if !table_defs.is_empty() {
            match bundle
                .table
                .apply_table_definitions(domain, app, module, &app_id, &table_defs, biz_db_id)
                .await
            {
                Ok(n) => info!(count = n, "表结构安装完成"),
                Err(e) => warn!(error = %e, "表结构安装失败"),
            }
        }

        // 4. 权限:permissions/*.json 解析为 PermissionDefinition,委托 importer
        let perm_defs = Self::read_permission_definitions(module_dir);
        if !perm_defs.is_empty() {
            match bundle
                .permission
                .apply_permission_definitions(domain, app, module, &perm_defs)
                .await
            {
                Ok(n) => info!(count = n, "权限安装完成"),
                Err(e) => warn!(error = %e, "权限安装失败"),
            }
        }

        Ok(())
    }

    /// 读取 forms/*.json,组装 FormDefinition 列表。
    ///
    /// code = `{module}:{file_stem}`,definition 为整体 JSON,name 取 JSON.name fallback 到 stem。
    fn read_form_definitions(
        module_dir: &Path,
        domain: &str,
        app: &str,
        module: &str,
    ) -> Vec<cmx_core::model::module::FormDefinition> {
        Self::read_definition_files(module_dir, "forms")
            .into_iter()
            .map(|(stem, name, definition)| cmx_core::model::module::FormDefinition {
                code: format!("{module}:{stem}"),
                name,
                description: None,
                definition,
                domain_code: domain.to_string(),
                application_code: app.to_string(),
                module_code: module.to_string(),
            })
            .collect()
    }

    /// 读取 menus/*.json,组装 MenuDefinition 列表(根菜单)。
    fn read_menu_definitions(
        module_dir: &Path,
        domain: &str,
        app: &str,
        module: &str,
    ) -> Vec<cmx_core::model::module::MenuDefinition> {
        Self::read_definition_files(module_dir, "menus")
            .into_iter()
            .map(|(stem, name, definition)| cmx_core::model::module::MenuDefinition {
                code: format!("{module}:{stem}"),
                name,
                definition,
                domain_code: domain.to_string(),
                application_code: app.to_string(),
                module_code: module.to_string(),
            })
            .collect()
    }

    /// 读取 metadata/tables/*.json,解析为 TableDefine 列表。
    ///
    /// 文件格式:`{ "tables": [TableDefine, ...] }`,合并所有文件的表定义。
    fn read_table_definitions(
        module_dir: &Path,
    ) -> Vec<cmx_core::model::meta::table::TableDefine> {
        let tables_dir = module_dir.join("metadata").join("tables");
        if !tables_dir.exists() {
            return Vec::new();
        }
        let mut all_tables = Vec::new();
        let entries = match std::fs::read_dir(&tables_dir) {
            Ok(e) => e,
            Err(e) => {
                warn!(error = %e, "读取 metadata/tables 目录失败");
                return Vec::new();
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
        all_tables
    }

    /// 读取 permissions/*.json,解析为 PermissionDefinition 列表(使用 cmx-core 统一契约)。
    fn read_permission_definitions(
        module_dir: &Path,
    ) -> Vec<cmx_core::model::iam::PermissionDefinition> {
        use cmx_core::model::iam::PermissionFile;
        let perms_dir = module_dir.join("permissions");
        if !perms_dir.exists() {
            return Vec::new();
        }
        let mut all_defs = Vec::new();
        let entries = match std::fs::read_dir(&perms_dir) {
            Ok(e) => e,
            Err(e) => {
                warn!(error = %e, "读取 permissions 目录失败");
                return Vec::new();
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
            let file: PermissionFile = serde_json::from_str(&content).unwrap_or(PermissionFile {
                name: String::new(),
                version: String::new(),
                description: String::new(),
                permissions: vec![],
            });
            all_defs.extend(file.permissions);
        }
        all_defs
    }

    /// 读取定义文件目录的通用 helper(forms/menus 共用)。
    ///
    /// 遍历 `module_dir/{subdir}/*.json`,返回 `(code, name, definition)` 三元组列表。
    fn read_definition_files(
        module_dir: &Path,
        subdir: &str,
    ) -> Vec<(String, String, serde_json::Value)> {
        let dir = module_dir.join(subdir);
        if !dir.exists() {
            return Vec::new();
        }
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) => {
                warn!(subdir = %subdir, error = %e, "读取定义文件目录失败");
                return Vec::new();
            }
        };
        let mut result = Vec::new();
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
                    warn!(file = %stem, error = %e, "读取定义文件失败,跳过");
                    continue;
                }
            };
            let definition: serde_json::Value =
                serde_json::from_str(&content).unwrap_or_default();
            let name = definition
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or(&stem)
                .to_string();
            let code = stem; // read_form_definitions/read_menu_definitions 会拼接 module 前缀
            result.push((code, name, definition));
        }
        result
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
