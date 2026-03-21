//! 依赖检查工具模块
//!
//! 提供插件依赖检查、依赖者查找等通用操作。

use std::sync::Arc;

use crate::domain::dependency::{DependencyCheckResult, DependencyConflict, MissingDependency};
use crate::domain::plugin::PluginFilter;
use crate::error::{PluginError, PluginResult};
use crate::infrastructure::database::repository::PluginRepository;

/// 依赖检查工具依赖
pub struct DependencyUtilsDeps {
    /// 数据仓库
    pub repository: Arc<PluginRepository>,
}

/// 依赖检查工具
pub struct DependencyUtils {
    deps: DependencyUtilsDeps,
}

impl DependencyUtils {
    /// 创建新的依赖检查工具
    pub fn new(deps: DependencyUtilsDeps) -> Self {
        Self { deps }
    }

    /// 检查依赖此插件的其他插件
    ///
    /// 查询所有插件，检查它们的依赖列表中是否包含当前插件。
    pub async fn check_dependents(&self, plugin_id: &str) -> PluginResult<Vec<String>> {
        let all_plugins = self
            .deps
            .repository
            .list_plugins(&PluginFilter::default())
            .await?;
        let mut dependents = Vec::new();

        for plugin in all_plugins {
            if let Some(ref metadata) = plugin.metadata {
                if let Some(deps) = metadata.get("dependencies").and_then(|d| d.as_array()) {
                    for dep in deps {
                        if let Some(dep_id) = dep.get("plugin_id").and_then(|id| id.as_str()) {
                            if dep_id == plugin_id {
                                dependents.push(plugin.plugin_id.clone());
                                break;
                            }
                        }
                    }
                }
            }
        }

        Ok(dependents)
    }

    /// 检查已激活的依赖此插件的其他插件
    ///
    /// 查询所有已激活的插件，检查它们的依赖列表中是否包含当前插件。
    pub async fn check_active_dependents(&self, plugin_id: &str) -> PluginResult<Vec<String>> {
        let all_plugins = self
            .deps
            .repository
            .list_plugins(&PluginFilter::default())
            .await?;
        let mut dependents = Vec::new();

        for plugin in all_plugins {
            if plugin.status != "activated" {
                continue;
            }

            if let Some(ref metadata) = plugin.metadata {
                if let Some(deps) = metadata.get("dependencies").and_then(|d| d.as_array()) {
                    for dep in deps {
                        if let Some(dep_id) = dep.get("plugin_id").and_then(|id| id.as_str()) {
                            if dep_id == plugin_id {
                                dependents.push(plugin.plugin_id.clone());
                                break;
                            }
                        }
                    }
                }
            }
        }

        Ok(dependents)
    }

    /// 检查依赖是否已激活
    ///
    /// 获取插件的依赖列表，检查每个依赖是否已激活。
    pub async fn check_dependencies_activated(
        &self,
        plugin_id: &str,
    ) -> PluginResult<Vec<String>> {
        let plugin = self
            .deps
            .repository
            .find_plugin(plugin_id)
            .await?
            .ok_or_else(|| PluginError::plugin_not_found(plugin_id))?;

        let mut inactive_deps = Vec::new();

        if let Some(ref metadata) = plugin.metadata {
            if let Some(deps) = metadata.get("dependencies").and_then(|d| d.as_array()) {
                for dep in deps {
                    if let Some(dep_id) = dep.get("plugin_id").and_then(|id| id.as_str()) {
                        if let Some(dep_plugin) = self.deps.repository.find_plugin(dep_id).await? {
                            if dep_plugin.status != "activated" {
                                inactive_deps.push(dep_id.to_string());
                            }
                        } else {
                            inactive_deps.push(dep_id.to_string());
                        }
                    }
                }
            }
        }

        Ok(inactive_deps)
    }

    /// 检查插件依赖是否满足
    ///
    /// 验证插件的所有依赖是否已安装且版本满足约束。
    pub async fn check_plugin_dependencies(
        &self,
        plugin_def: &cmx_core::model::meta::plugin::PluginDefinition,
        get_plugin_info: impl Fn(&str) -> PluginResult<Option<crate::domain::plugin::PluginInfo>>,
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

            if let Some(ref constraint_str) = dep.version_constraint {
                if let Ok(constraint) = crate::domain::version::VersionConstraint::parse(constraint_str)
                {
                    if let Some(plugin_info) = get_plugin_info(&dep.plugin_id)? {
                        if let Ok(installed_version) =
                            crate::domain::version::SemanticVersion::parse(&plugin_info.version)
                        {
                            if !constraint.satisfies(&installed_version) {
                                result.add_conflict(DependencyConflict {
                                    plugin_id: dep.plugin_id.clone(),
                                    constraints: vec![(plugin_def.id.clone(), constraint)],
                                });
                            }
                        }
                    }
                }
            }
        }

        Ok(result)
    }
}
