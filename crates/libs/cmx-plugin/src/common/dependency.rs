//! 依赖检查工具模块
//!
//! 提供插件依赖检查功能，用于验证插件的所有依赖是否已安装且版本满足约束。

use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::RwLock;
use crate::core::registry::PluginRegistry;
use crate::domain::dependency::{DependencyCheckResult, DependencyConflict, MissingDependency};
use crate::domain::plugin::{PluginInfo, PluginSource, PluginStatus};
use crate::error::PluginResult;
use crate::infrastructure::database::repository::PluginRepository;

/// 依赖检查工具依赖
///
/// 包含执行依赖检查所需的数据仓库和插件注册表。
#[derive(Clone)]
pub struct DependencyUtilsDeps {
    /// 数据仓库，用于查询插件是否存在
    pub repository: Arc<PluginRepository>,
    /// 插件注册表，用于获取已激活插件的详细信息
    pub registry: Arc<RwLock<PluginRegistry>>,
}

/// 依赖检查工具
///
/// 提供插件依赖验证功能，检查所有依赖是否已安装且版本满足约束。
#[derive(Clone)]
pub struct DependencyUtils {
    /// 依赖工具内部依赖
    deps: DependencyUtilsDeps,
}

impl DependencyUtils {
    /// 创建新的依赖检查工具
    ///
    /// # 参数
    /// * `deps` - 依赖工具依赖，包含 repository 和 registry
    ///
    /// # 返回值
    /// 新的 DependencyUtils 实例
    pub fn new(deps: DependencyUtilsDeps) -> Self {
        Self { deps }
    }

    /// 检查插件依赖是否满足
    ///
    /// 验证插件的所有依赖是否已安装且版本满足约束。
    /// 对于可选依赖（optional=true）会跳过检查。
    ///
    /// # 参数
    /// * `plugin_def` - 插件定义，包含依赖列表信息
    ///
    /// # 返回值
    /// * `PluginResult<DependencyCheckResult>` - 依赖检查结果
    ///
    /// # 检查流程
    /// 1. 遍历插件的所有依赖（跳过可选依赖）
    /// 2. 检查依赖插件是否已安装（通过 repository.plugin_exists）
    /// 3. 如果依赖有版本约束，检查已安装版本是否满足约束
    /// 4. 返回检查结果，包含缺失的依赖和版本冲突信息
    pub async fn check_plugin_dependencies(
        &self,
        plugin_def: &cmx_core::model::meta::plugin::PluginDefinition,
    ) -> PluginResult<DependencyCheckResult> {
        let mut result = DependencyCheckResult::new();

        for dep in &plugin_def.dependencies {
            if dep.optional {
                continue;
            }

            let installed = self.deps.repository.plugin_exists(&dep.plugin_id).await?;

            if !installed {
                let version_constraint = dep
                    .version_constraint
                    .as_ref()
                    .and_then(|v| crate::domain::version::VersionConstraint::parse(v).ok());

                result.add_missing(MissingDependency {
                    plugin_id: dep.plugin_id.clone(),
                    version_constraint,
                    required_by: plugin_def.id.clone(),
                });
                continue;
            }

            if let Some(ref constraint_str) = dep.version_constraint
                && let Ok(constraint) = crate::domain::version::VersionConstraint::parse(constraint_str)
                    && let Some(plugin_info) = get_plugin_info(&dep.plugin_id, self.deps.registry.clone(), self.deps.repository.clone()).await?
                        && let Ok(installed_version) =
                            crate::domain::version::SemanticVersion::parse(&plugin_info.version)
                            && !constraint.satisfies(&installed_version) {
                                result.add_conflict(DependencyConflict {
                                    plugin_id: dep.plugin_id.clone(),
                                    constraints: vec![(plugin_def.id.clone(), constraint)],
                                });
                            }
        }

        Ok(result)
    }
}

/// 从 registry 或 repository 获取插件信息
///
/// 优先从插件注册表查找已激活的插件信息，如果未找到则从数据库仓库查询。
/// 用于在依赖检查时获取依赖插件的详细信息以验证版本约束。
///
/// # 参数
/// * `plugin_id` - 插件ID
/// * `registry` - 插件注册表
/// * `repository` - 插件仓库
///
/// # 返回值
/// 插件信息（如果存在），优先返回注册表中的信息
///
/// # 查询顺序
/// 1. 从 registry 查找（已激活的插件）
/// 2. 从 repository 查找（数据库中已安装的插件）
/// 3. 都不存在则返回 None
async fn get_plugin_info(
    plugin_id: &str,
    registry: Arc<RwLock<PluginRegistry>>,
    repository: Arc<PluginRepository>,
) -> PluginResult<Option<PluginInfo>> {
    {
        let registry = registry.read().await;
        if let Some(info) = registry.get(plugin_id) {
            return Ok(Some(info.clone()));
        }
    }
    if let Some(record) = repository.find_plugin(plugin_id).await? {
        let info = PluginInfo {
            id: record.plugin_id,
            name: record.name,
            version: record.version,
            description: record.description,
            author: record.vendor_name,
            source: PluginSource::Local {
                path: PathBuf::from(&record.install_path),
            },
            status: PluginStatus::Installed,
            installed_at: Some(record.create_time),
            updated_at: Some(record.update_time),
            install_path: PathBuf::from(&record.install_path),
            domain_code: record.domain_code.unwrap_or_default(),
            application_code: record.application_code.unwrap_or_default(),
            module_code: record.module_code.unwrap_or_default(),
            plugin_type: record.plugin_type.clone().unwrap_or_default(),
            source_path: record.source_path.clone(),
        };
        return Ok(Some(info));
    }
    Ok(None)
}