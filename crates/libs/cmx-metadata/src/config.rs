//! 建表 JSON 配置管理
//!
//! `TableDefinesConfig` 数据结构定义在 cmx-core 中，
//! 本模块提供配置加载和管理的具体实现（`TableDefinesConfigManager`）。

use std::collections::{HashMap, VecDeque};
use std::path::Path;

use cmx_core::model::cell::TableDefine;
use cmx_core::model::meta::base::{TableDefineDbExecutor, TableDefinesConfig};
use crate::loader::load_table_defines_from_path;
use crate::MetadataError;

/// 从 JSON 文件路径读取单个建表配置
pub fn load_table_defines_config_from_path(path: &Path) -> Result<TableDefinesConfig, MetadataError> {
    let s = std::fs::read_to_string(path)?;
    let config: TableDefinesConfig = serde_json::from_str(&s)?;
    Ok(config)
}

/// 管理多套建表配置，可合并加载所有配置指向的表定义
#[derive(Debug, Clone, Default)]
pub struct TableDefinesConfigManager {
    configs: Vec<TableDefinesConfig>,
}

impl TableDefinesConfigManager {
    pub fn new() -> Self {
        Self { configs: Vec::new() }
    }

    pub fn from_config_paths(paths: &[impl AsRef<Path>]) -> Result<Self, MetadataError> {
        let mut manager = Self::new();
        for p in paths {
            let config = load_table_defines_config_from_path(p.as_ref())?;
            manager.add_config(config);
        }
        Ok(manager)
    }

    pub fn add_config(&mut self, config: TableDefinesConfig) {
        self.configs.push(config);
    }

    pub fn config_count(&self) -> usize {
        self.configs.len()
    }

    pub fn configs(&self) -> &[TableDefinesConfig] {
        &self.configs
    }

    pub fn get_config_by_name(&self, name: &str) -> Option<&TableDefinesConfig> {
        self.configs.iter().find(|c| c.name == name)
    }

    /// 按依赖与优先级排序（拓扑排序）
    pub fn sorted_configs(&self) -> Result<Vec<&TableDefinesConfig>, MetadataError> {
        let name_to_index: HashMap<&str, usize> = self
            .configs
            .iter()
            .enumerate()
            .map(|(i, c)| (c.name.as_str(), i))
            .collect();
        for c in &self.configs {
            for d in &c.depends_on {
                if !name_to_index.contains_key(d.as_str()) {
                    return Err(MetadataError::ConfigDependency(format!(
                        "配置 \"{}\" 依赖不存在的配置 \"{}\"",
                        c.name, d
                    )));
                }
            }
        }
        let n = self.configs.len();
        let mut in_degree: Vec<usize> = vec![0; n];
        let mut successors: Vec<Vec<usize>> = vec![Vec::new(); n];
        for (i, c) in self.configs.iter().enumerate() {
            in_degree[i] = c.depends_on.len();
            for d in &c.depends_on {
                let &j = name_to_index.get(d.as_str()).unwrap();
                successors[j].push(i);
            }
        }
        let priority = |i: usize| self.configs[i].priority.unwrap_or(0);
        let mut queue: VecDeque<usize> = (0..n).filter(|&i| in_degree[i] == 0).collect();
        queue.make_contiguous().sort_by_key(|&i| priority(i));
        let mut out: Vec<usize> = Vec::with_capacity(n);
        while let Some(i) = queue.pop_front() {
            out.push(i);
            let mut next_batch: Vec<usize> = Vec::new();
            for &j in &successors[i] {
                in_degree[j] -= 1;
                if in_degree[j] == 0 {
                    next_batch.push(j);
                }
            }
            next_batch.sort_by_key(|&j| priority(j));
            for j in next_batch {
                queue.push_back(j);
            }
        }
        if out.len() != n {
            return Err(MetadataError::ConfigDependency(
                "配置之间存在循环依赖".to_string(),
            ));
        }
        Ok(out.into_iter().map(|i| &self.configs[i]).collect())
    }

    pub fn load_all_tables(&self, base_path: &Path) -> Result<Vec<TableDefine>, MetadataError> {
        let mut all = Vec::new();
        for config in self.sorted_configs()? {
            for file in &config.files {
                let path = base_path.join(file);
                let tables = load_table_defines_from_path(&path)?;
                all.extend(tables);
            }
        }
        Ok(all)
    }

    pub fn load_tables_by_config_name(
        &self,
        base_path: &Path,
        config_name: &str,
    ) -> Result<Vec<TableDefine>, MetadataError> {
        let config = self
            .get_config_by_name(config_name)
            .ok_or_else(|| MetadataError::ConfigNotFound(config_name.to_string()))?;
        let mut all = Vec::new();
        for file in &config.files {
            let path = base_path.join(file);
            let tables = load_table_defines_from_path(&path)?;
            all.extend(tables);
        }
        Ok(all)
    }
}

/// 从 JSON 文件读取所有表定义，并依次对每张表执行"创建或升级"
pub fn load_and_apply_table_defines_from_path(
    path: &Path,
    executor: &dyn TableDefineDbExecutor,
) -> Result<(), MetadataError> {
    let defines = load_table_defines_from_path(path)?;
    for define in &defines {
        executor
            .create_or_upgrade_table(define)
            .map_err(|e| MetadataError::DdlGeneration(e.to_string()))?;
    }
    Ok(())
}
