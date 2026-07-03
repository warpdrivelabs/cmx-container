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
    /// 权限定义导入器(由 cmx-iam 注入,复用两阶段 upsert 逻辑)。
    /// 为 None 时跳过权限安装(向后兼容,如测试场景)。
    permission_importer: Option<std::sync::Arc<dyn cmx_traits::iam::PermissionDefinitionImporter>>,
}

impl ModuleInstallService {
    /// 创建模块安装服务
    pub fn new(package_utils: PackageUtils, deploy_service: std::sync::Arc<DeployService>) -> Self {
        Self {
            package_utils,
            deploy_service,
            permission_importer: None,
        }
    }

    /// 注入权限定义导入器(Builder 模式)。
    ///
    /// 注入后,模块导入时的权限安装会委托给 cmx-iam 的统一实现,
    /// 消除本文件内重复的两阶段 upsert SQL。
    pub fn with_permission_importer(
        mut self,
        importer: std::sync::Arc<dyn cmx_traits::iam::PermissionDefinitionImporter>,
    ) -> Self {
        self.permission_importer = Some(importer);
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

        // 4. 安装模块级资源(metadata 建表用 biz_db_id,其余用 default_db_id)
        if let Err(e) = self
            .install_module_resources(mm, &default_db_id, &biz_db_id, &module_dir, &manifest)
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

    /// 安装模块级资源:metadata(建表)/forms/menus/permissions
    ///
    /// 对称性契约(与 export_module 严格对称):
    /// - forms/*.json:整体 JSON 透传存入 cmx_form.definition,code = 文件名(去扩展名)
    /// - menus/*.json:整体 JSON 透传存入 cmx_menu.definition(根菜单,树形字段自动计算)
    /// - metadata/tables/*.json:用 PgTableDefineExecutor 建表/升级
    /// - permissions/*.json:解析 PermissionDefinition,SQL upsert 到 cmx_permission
    async fn install_module_resources(
        &self,
        mm: &cmx_database::DatabaseManager,
        default_db_id: &str,
        biz_db_id: &str,
        module_dir: &Path,
        manifest: &ModuleManifest,
    ) -> PluginResult<()> {
        let domain = manifest.module.domain_code.as_str();
        let app = manifest.module.application_code.as_str();
        let module = manifest.module.code.as_str();
        // app_id 取配置值(当前设计下 app_id ≡ module_code,与导入守卫校验一致)
        let app_id = cmx_utils::ConfigManager::global().get_app_id();

        // 1. 表单:forms/*.json 整体透传存入 cmx_form(存 default 库)
        self.install_forms(mm, default_db_id, module_dir, domain, app, module)
            .await;

        // 2. 菜单:menus/*.json 整体透传存入 cmx_menu.definition(存 default 库)
        self.install_menus(mm, default_db_id, module_dir, domain, app, module)
            .await;

        // 3. 元数据:metadata/tables/*.json 建表(建到 biz 库,元数据登记存 default 库)
        self.install_metadata(mm, default_db_id, biz_db_id, module_dir, domain, app, module, &app_id)
            .await;

        // 4. 权限:permissions/*.json 委托注入的 importer upsert cmx_permission
        self.install_permissions(module_dir, domain, app, module)
            .await;

        Ok(())
    }

    /// 安装表单:读取 forms/*.json,整体 JSON 透传存入 cmx_form.definition
    ///
    /// code 规则:`{module_code}:{file_stem}`,幂等(先删后建,基于 code 唯一约束)
    async fn install_forms(
        &self,
        mm: &cmx_database::DatabaseManager,
        db_id: &str,
        module_dir: &Path,
        domain: &str,
        app: &str,
        module: &str,
    ) {
        for (code, name, definition) in Self::read_definition_files(module_dir, "forms", module) {
            let dto = cmx_biz::form::FormForCreate {
                code: code.clone(),
                name,
                description: None,
                definition: Some(definition),
                domain_code: domain.to_string(),
                application_code: app.to_string(),
                module_code: module.to_string(),
            };
            // 幂等:先删同 code 记录,再创建
            let _ = cmx_biz::form::FormService::delete_by_code(mm, db_id, &code).await;
            if let Err(e) = cmx_biz::form::FormService::create(mm, db_id, dto).await {
                warn!(form = %code, error = %e, "表单安装失败");
            } else {
                info!(form = %code, "表单安装成功");
            }
        }
    }

    /// 安装菜单:读取 menus/*.json,整体 JSON 透传存入 cmx_menu.definition(根菜单)
    ///
    /// 每个 menus 文件创建一个根菜单,code = `{module}:{file_stem}`,
    /// 完整 JSON 内容存入 definition 字段,前端从 definition 解析 items/children 树。
    async fn install_menus(
        &self,
        mm: &cmx_database::DatabaseManager,
        db_id: &str,
        module_dir: &Path,
        domain: &str,
        app: &str,
        module: &str,
    ) {
        for (code, name, menu_json) in Self::read_definition_files(module_dir, "menus", module) {
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
                definition: Some(menu_json.clone()), // 整体透传
                ext_attributes: None,
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

    /// 读取定义文件目录的通用 helper(forms/menus 共用)。
    ///
    /// 遍历 `module_dir/{subdir}/*.json`,解析每个文件为 JSON,
    /// 返回 `(code, name, definition)` 三元组列表:
    /// - `code` = `{module}:{file_stem}`
    /// - `name` 取 JSON 的 name 字段,fallback 到 file_stem
    /// - `definition` 为完整 JSON 内容(整体透传)
    ///
    /// 目录不存在或读取失败时返回空 Vec(不阻断安装)。
    fn read_definition_files(
        module_dir: &Path,
        subdir: &str,
        module: &str,
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
            let code = format!("{module}:{stem}");
            result.push((code, name, definition));
        }
        result
    }

    /// 安装元数据:读取 metadata/tables/*.json 表定义,用 PgTableDefineExecutor 建表到业务库
    #[allow(clippy::too_many_arguments)]
    ///
    /// 表定义文件格式:`{ "tables": [TableDefine, ...] }`(对齐 cmx-metadata TableDefine)。
    /// 建表幂等(create_or_upgrade_table 会判断表是否存在)。
    async fn install_metadata(
        &self,
        mm: &cmx_database::DatabaseManager,
        default_db_id: &str,
        biz_db_id: &str,
        module_dir: &Path,
        domain: &str,
        app: &str,
        module: &str,
        app_id: &str,
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

        // 用 PgTableDefineExecutor 建表到业务库(无需分布式锁,模块安装是低频操作)
        use cmx_metadata::TableDefineDbExecutor;
        let executor = cmx_metadata::executor::PgTableDefineExecutor::new(biz_db_id, None);
        for table_def in &all_tables {
            match executor.create_or_upgrade_table(table_def).await {
                Ok(_) => info!(table = %table_def.table_name, "建表/升级成功(业务库)"),
                Err(e) => warn!(table = %table_def.table_name, error = %e, "建表失败"),
            }
            // 记录表元数据到 cmx_meta_table_define(登记存 default 库,记录 db_id 标记 biz 库)
            let _ = Self::save_table_metadata(mm, default_db_id, biz_db_id, table_def, domain, app, module, app_id).await;
        }
    }

    /// 安装权限:读取 permissions/*.json,委托注入的 PermissionDefinitionImporter 执行两阶段 upsert
    ///
    /// 对称契约:JSON 格式为 PermissionFile(name/version/description + permissions[]),
    /// 权限条目字段对齐 cmx-core 的 PermissionDefinition。
    /// 两阶段 upsert 逻辑统一收敛到 cmx-iam(经 cmx-traits trait 注入),消除本文件内的重复 SQL。
    async fn install_permissions(
        &self,
        module_dir: &Path,
        domain: &str,
        app: &str,
        module: &str,
    ) {
        let Some(importer) = &self.permission_importer else {
            warn!("未注入 PermissionDefinitionImporter,跳过权限安装");
            return;
        };

        let perms_dir = module_dir.join("permissions");
        if !perms_dir.exists() {
            return;
        }

        // 解析所有 permissions/*.json,收集 PermissionDefinition(使用 cmx-core 统一契约)
        use cmx_core::model::iam::{PermissionDefinition, PermissionFile};
        let mut all_defs: Vec<PermissionDefinition> = Vec::new();
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
            let file: PermissionFile = serde_json::from_str(&content).unwrap_or(PermissionFile {
                name: String::new(),
                version: String::new(),
                description: String::new(),
                permissions: vec![],
            });
            all_defs.extend(file.permissions);
        }

        if all_defs.is_empty() {
            return;
        }

        // 委托 cmx-iam 的统一实现(两阶段 upsert + parent 回填 + is_leaf 重算)
        match importer
            .apply_permission_definitions(domain, app, module, &all_defs)
            .await
        {
            Ok(n) => info!(count = n, "权限安装完成(委托 importer)"),
            Err(e) => warn!(error = %e, "权限安装失败(委托 importer)"),
        }
    }

    /// 保存表元数据到 cmx_meta_table_define + cmx_meta_table_define_version
    ///
    /// SQL 执行库用 default_db_id(元数据表在默认库),
    /// 记录的 db_id 列登记 biz_db_id(标记业务表所在库)。
    #[allow(clippy::too_many_arguments)]
    async fn save_table_metadata(
        mm: &cmx_database::DatabaseManager,
        default_db_id: &str,
        biz_db_id: &str,
        table_def: &cmx_core::model::meta::table::TableDefine,
        domain: &str,
        app: &str,
        module: &str,
        app_id: &str,
    ) -> PluginResult<()> {
        use cmx_core::model::cell::DataValue;
        let metadata_json = serde_json::to_string(table_def)
            .map_err(|e| crate::error::PluginError::Config(format!("序列化表定义失败: {e}")))?;

        // 主表:先查 table_name 是否存在(cmx_meta_table_define 无 table_name 唯一约束)
        let check_sql = "SELECT id FROM cmx_meta_table_define WHERE table_name = $1";
        let check_ds = mm
            .query_sql_with_datavalues(
                default_db_id,
                None,
                check_sql,
                vec![DataValue::String(table_def.table_name.clone())],
                "save_meta_check",
            )
            .await;
        let existing_id = check_ds
            .ok()
            .and_then(|ds| serde_json::to_value(&ds).ok())
            .and_then(|j| j.get("rows").and_then(|r| r.as_array()).and_then(|rows| rows.first()).cloned())
            .and_then(|row| row.get("id").and_then(|v| v.as_str()).map(String::from));

        if let Some(eid) = existing_id {
            // 已存在 → UPDATE
            let upd_sql = "UPDATE cmx_meta_table_define SET \
                           display_name = $1, domain_code = $2, application_code = $3, \
                           module_code = $4, db_id = $5, version = '1', ddl_status = 'completed', \
                           update_time = CURRENT_TIMESTAMP WHERE id = $6";
            let _ = mm
                .execute_sql_with_datavalues(
                    default_db_id,
                    None,
                    upd_sql,
                    vec![
                        DataValue::String(table_def.display_name.clone()),
                        DataValue::String(domain.to_string()),
                        DataValue::String(app.to_string()),
                        DataValue::String(module.to_string()),
                        DataValue::String(biz_db_id.to_string()),
                        DataValue::String(eid),
                    ],
                )
                .await;
        } else {
            // 不存在 → INSERT(db_id 列登记 biz_db_id，标记业务表所在库)
            let id = uuid::Uuid::new_v4().to_string();
            let ins_sql = "INSERT INTO cmx_meta_table_define \
                           (id, table_name, display_name, db_id, plugin_id, version, app_id, \
                            ddl_status, domain_code, application_code, module_code, archived) \
                           VALUES ($1, $2, $3, $4, NULL, '1', $5, 'completed', \
                                   $6, $7, $8, 0)";
            let _ = mm
                .execute_sql_with_datavalues(
                    default_db_id,
                    None,
                    ins_sql,
                    vec![
                        DataValue::String(id),
                        DataValue::String(table_def.table_name.clone()),
                        DataValue::String(table_def.display_name.clone()),
                        DataValue::String(biz_db_id.to_string()),
                        DataValue::String(app_id.to_string()),
                        DataValue::String(domain.to_string()),
                        DataValue::String(app.to_string()),
                        DataValue::String(module.to_string()),
                    ],
                )
                .await;
        }

        // version 表存完整 TableDefine JSON(供导出对称读取)
        let vid = uuid::Uuid::new_v4().to_string();
        let version_sql = "INSERT INTO cmx_meta_table_define_version \
                           (id, table_name, display_name, db_id, plugin_id, version, app_id, \
                            domain_code, application_code, module_code, metadata, archived) \
                           VALUES ($1, $2, $3, $4, NULL, '1', $5, $6, $7, $8, $9::jsonb, 0)";
        let _ = mm
            .execute_sql_with_datavalues(
                default_db_id,
                None,
                version_sql,
                vec![
                    DataValue::String(vid),
                    DataValue::String(table_def.table_name.clone()),
                    DataValue::String(table_def.display_name.clone()),
                    DataValue::String(biz_db_id.to_string()),
                    DataValue::String(app_id.to_string()),
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
