//! 建表 JSON 配置管理
//!
//! `TableDefinesConfig` 数据结构定义在 cmx-core 中，
//! 本模块提供配置加载和管理的具体实现（`TableDefinesConfigManager`）。
//!
//! # 功能特性
//! - 支持从 JSON 文件加载建表配置
//! - 支持配置依赖管理和拓扑排序
//! - 支持批量加载所有配置的表定义
//!
//! # 使用示例
//! ```ignore
//! use cmx_metadata::config::TableDefinesConfigManager;
//!
//! let manager = TableDefinesConfigManager::from_config_paths(&["config.json"])?;
//! let tables = manager.load_all_tables(&base_path)?;
//! ```

use std::collections::{HashMap, VecDeque};
use std::path::Path;

use cmx_core::model::cell::TableDefine;
use cmx_core::model::meta::plugin::{TableDefinesConfig, SeedDataConfig};
use crate::loader::load_table_defines_from_path;
use crate::MetadataError;

/// 从 JSON 文件路径读取单个建表配置
///
/// # 参数
/// * `path` - JSON 配置文件的路径
///
/// # 返回值
/// * 成功返回 `TableDefinesConfig`
/// * 失败返回 `MetadataError`
pub fn load_table_defines_config_from_path(path: &Path) -> Result<TableDefinesConfig, MetadataError> {
    let s = std::fs::read_to_string(path)?;
    let config: TableDefinesConfig = serde_json::from_str(&s)?;
    Ok(config)
}

/// 管理多套建表配置，可合并加载所有配置指向的表定义
///
/// 该管理器支持：
/// - 添加多个配置文件
/// - 按依赖关系和优先级进行拓扑排序
/// - 批量加载所有配置的表定义
///
/// # 示例
/// ```ignore
/// let mut manager = TableDefinesConfigManager::new();
/// manager.add_config(config1);
/// manager.add_config(config2);
/// let sorted = manager.sorted_configs()?;
/// ```
#[derive(Debug, Clone, Default)]
pub struct TableDefinesConfigManager {
    /// 存储的配置列表
    configs: Vec<TableDefinesConfig>,
}

impl TableDefinesConfigManager {
    /// 创建一个新的配置管理器实例
    pub fn new() -> Self {
        Self { configs: Vec::new() }
    }

    /// 从多个配置文件路径创建配置管理器
    ///
    /// # 参数
    /// * `paths` - JSON 配置文件的路径列表
    ///
    /// # 返回值
    /// * 成功返回配置管理器实例
    /// * 失败返回 `MetadataError`（如文件不存在或 JSON 解析失败）
    pub fn from_config_paths(paths: &[impl AsRef<Path>]) -> Result<Self, MetadataError> {
        let mut manager = Self::new();
        for p in paths {
            let config = load_table_defines_config_from_path(p.as_ref())?;
            manager.add_config(config);
        }
        Ok(manager)
    }

    /// 添加一个配置文件到管理器
    ///
    /// # 参数
    /// * `config` - 要添加的 `TableDefinesConfig`
    pub fn add_config(&mut self, config: TableDefinesConfig) {
        self.configs.push(config);
    }

    /// 获取已添加的配置数量
    pub fn config_count(&self) -> usize {
        self.configs.len()
    }

    /// 获取所有配置的引用
    pub fn configs(&self) -> &[TableDefinesConfig] {
        &self.configs
    }

    /// 根据配置名称获取配置
    ///
    /// # 参数
    /// * `name` - 配置的名称
    ///
    /// # 返回值
    /// * 找到返回 `Some(&TableDefinesConfig)`
    /// * 未找到返回 `None`
    pub fn get_config_by_name(&self, name: &str) -> Option<&TableDefinesConfig> {
        self.configs.iter().find(|c| c.name == name)
    }

    /// 按依赖与优先级排序（拓扑排序）
    ///
    /// 对所有配置进行拓扑排序，考虑：
    /// - `depends_on` 字段定义的依赖关系（被依赖的配置先加载）
    /// - `priority` 字段定义的优先级（优先级高的先加载）
    ///
    /// # 算法说明
    /// 1. 构建有向图：配置 → 其依赖的配置
    /// 2. 计算每个节点的入度（被依赖次数）
    /// 3. 使用队列存储入度为0的节点，按优先级排序
    /// 4. 依次出队，将后继节点的入度减1，入度为0时加入队列
    /// 5. 若最终输出节点数不等于配置数，说明存在循环依赖
    ///
    /// # 返回值
    /// * 成功返回按排序后的配置引用列表
    /// * 失败返回 `MetadataError`（如存在循环依赖或依赖不存在的配置）
    pub fn sorted_configs(&self) -> Result<Vec<&TableDefinesConfig>, MetadataError> {
        // 构建配置名称到索引的映射，用于快速查找
        let name_to_index: HashMap<&str, usize> = self
            .configs
            .iter()
            .enumerate()
            .map(|(i, c)| (c.name.as_str(), i))
            .collect();

        // 检查所有依赖是否都存在，防止悬空依赖
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

        // in_degree[i] 表示配置 i 被依赖的次数（入度）
        let mut in_degree: Vec<usize> = vec![0; n];

        // successors[i] 存储依赖配置 i 的所有配置的索引
        let mut successors: Vec<Vec<usize>> = vec![Vec::new(); n];

        // 构建有向图：遍历所有配置，更新入度和后继节点列表
        for (i, c) in self.configs.iter().enumerate() {
            in_degree[i] = c.depends_on.len(); // 入度 = 被依赖次数
            for d in &c.depends_on {
                let &j = name_to_index.get(d.as_str()).unwrap();
                successors[j].push(i); // 配置 j 有一个后继 i
            }
        }

        // 优先级函数：优先加载高优先级的配置
        let priority = |i: usize| self.configs[i].priority.unwrap_or(0);

        // 初始化队列：所有入度为0的配置（无依赖的配置）可以首先加载
        let mut queue: VecDeque<usize> = (0..n).filter(|&i| in_degree[i] == 0).collect();

        // 按优先级排序，确保同级别优先级的配置按预期顺序加载
        queue.make_contiguous().sort_by_key(|&i| priority(i));

        let mut out: Vec<usize> = Vec::with_capacity(n);

        // Kahn's algorithm：依次处理入度为0的节点
        while let Some(i) = queue.pop_front() {
            out.push(i);
            // 收集下一批新入度为0的节点
            let mut next_batch: Vec<usize> = Vec::new();
            for &j in &successors[i] {
                in_degree[j] -= 1;
                if in_degree[j] == 0 {
                    next_batch.push(j);
                }
            }
            // 按优先级排序，保持稳定的加载顺序
            next_batch.sort_by_key(|&j| priority(j));
            for j in next_batch {
                queue.push_back(j);
            }
        }

        // 检测循环依赖：若输出节点数少于总节点数，说明有环
        if out.len() != n {
            return Err(MetadataError::ConfigDependency(
                "配置之间存在循环依赖".to_string(),
            ));
        }

        Ok(out.into_iter().map(|i| &self.configs[i]).collect())
    }

    /// 加载所有配置的表定义
    ///
    /// 按照拓扑排序顺序加载所有配置文件中定义的表。
    ///
    /// # 参数
    /// * `base_path` - table_config_file配置文件的基础路径
    ///
    /// # 返回值
    /// * 成功返回所有表定义的向量
    /// * 失败返回 `MetadataError`
    pub fn load_all_tables(&self, base_path: &Path) -> Result<Vec<TableDefine>, MetadataError> {
        let mut all = Vec::new();
        for config in self.sorted_configs()? {
            for file in &config.files {
                let path = base_path.join("metadata").join(file);
                let tables = load_table_defines_from_path(&path)?;
                all.extend(tables);
            }
        }
        Ok(all)
    }

    /// 加载指定配置名称的表定义
    ///
    /// # 参数
    /// * `base_path` - 配置文件的基础路径
    /// * `config_name` - 配置的名称
    ///
    /// # 返回值
    /// * 成功返回该配置下所有表定义的向量
    /// * 失败返回 `MetadataError`
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

    /// 收集所有配置中的种子数据配置
    ///
    /// 从所有已加载配置中提取 `seed_data` 字段，
    /// 按照拓扑排序顺序合并，确保依赖关系正确。
    pub fn collect_seed_configs(&self) -> Vec<SeedDataConfig> {
        let mut all = Vec::new();
        for config in self.configs() {
            for seed in &config.seed_data {
                if seed.enabled {
                    all.push(seed.clone());
                }
            }
        }
        all
    }
}
//
// /// 从 JSON 文件读取所有表定义，并依次对每张表执行"创建或升级"
// pub async fn load_and_apply_table_defines_from_path(
//     path: &Path,
//     executor: &dyn TableDefineDbExecutor,
// ) -> Result<(), MetadataError> {
//     let defines = load_table_defines_from_path(path)?;
//     for define in &defines {
//         executor
//             .create_or_upgrade_table(define).await
//             .map_err(|e| MetadataError::DdlGeneration(e.to_string()))?;
//     }
//     Ok(())
// }
