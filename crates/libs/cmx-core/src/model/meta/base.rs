//! 表/列定义从 JSON 加载，以及数据库创建/升级表接口预留

use std::collections::{HashMap, VecDeque};
use std::path::Path;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::model::cell::{ColumnDefine, FieldType, TableDefine};

// ==========================================
// 错误类型
// ==========================================

#[derive(Error, Debug)]
pub enum BaseError {
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON 解析错误: {0}")]
    Json(#[from] serde_json::Error),
    #[error("未实现: {0}")]
    Unimplemented(String),
    #[error("配置未找到: {0}")]
    ConfigNotFound(String),
    #[error("配置依赖错误: {0}")]
    ConfigDependency(String),
}

// ==========================================
// 从 JSON 读取表/列定义
// ==========================================

/// 从 JSON 字符串解析单个表定义
pub fn table_define_from_str(s: &str) -> Result<TableDefine, BaseError> {
    let define: TableDefine = serde_json::from_str(s)?;
    Ok(define)
}

/// 从 JSON 文件路径读取单个表定义  
/// JSON 格式与 `TableDefine` 一致：`{ "table_name", "display_name", "columns": [ ColumnDefine, ... ] }`
pub fn load_table_define_from_path(path: &Path) -> Result<TableDefine, BaseError> {
    let s = std::fs::read_to_string(path)?;
    table_define_from_str(&s)
}

/// 支持“多表”的 JSON 根结构（可选）
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum TableDefinesRoot {
    Single(TableDefine),
    Multi { tables: Vec<TableDefine> },
    Array(Vec<TableDefine>),
}

/// 从 JSON 字符串解析多个表定义  
/// 支持三种根格式：单个 `TableDefine` 对象、`{ "tables": [ ... ] }`、或顶层数组 `[ TableDefine, ... ]`
pub fn table_defines_from_str(s: &str) -> Result<Vec<TableDefine>, BaseError> {
    let root: TableDefinesRoot = serde_json::from_str(s)?;
    Ok(match root {
        TableDefinesRoot::Single(t) => vec![t],
        TableDefinesRoot::Multi { tables } => tables,
        TableDefinesRoot::Array(arr) => arr,
    })
}

/// 从 JSON 文件路径读取多个表定义  
/// 文件格式见 `docs/table_defines_example.json`：根对象为 `{ "tables": [ TableDefine, ... ] }` 或根数组 `[ TableDefine, ... ]`
pub fn load_table_defines_from_path(path: &Path) -> Result<Vec<TableDefine>, BaseError> {
    let s = std::fs::read_to_string(path)?;
    table_defines_from_str(&s)
}

// ==========================================
// 建表 JSON 配置文件（多文件列表）
// ==========================================

/// 建表 JSON 配置文件：描述一组表定义文件（如拆分后的 oracle_tables_01.json … oracle_tables_22.json）。
/// JSON 格式：`{ "name", "description?", "files": [...], "depends_on"?:[...], "priority"? N }`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableDefinesConfig {
    /// 配置名称，用于区分多套表定义（如 "oracle_tables"、"sys_tables"）
    pub name: String,
    /// 可选说明
    #[serde(default)]
    pub description: Option<String>,
    /// 表定义 JSON 文件列表（相对路径或文件名，由加载时 base_path 解析）
    pub files: Vec<String>,
    /// 依赖的配置名称列表；被依赖的配置会先于本配置加载（被依赖的在前）
    #[serde(default)]
    pub depends_on: Vec<String>,
    /// 优先级，数值越小越先加载；同层级或无依赖关系时按此排序
    #[serde(default)]
    pub priority: Option<i32>,
}

/// 从 JSON 文件路径读取单个建表配置
pub fn load_table_defines_config_from_path(path: &Path) -> Result<TableDefinesConfig, BaseError> {
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
    /// 空管理器
    pub fn new() -> Self {
        Self { configs: Vec::new() }
    }

    /// 从多个配置文件路径加载并合并为同一管理器
    pub fn from_config_paths(paths: &[impl AsRef<Path>]) -> Result<Self, BaseError> {
        let mut manager = Self::new();
        for p in paths {
            let config = load_table_defines_config_from_path(p.as_ref())?;
            manager.add_config(config);
        }
        Ok(manager)
    }

    /// 添加一套配置
    pub fn add_config(&mut self, config: TableDefinesConfig) {
        self.configs.push(config);
    }

    /// 当前管理的配置数量
    pub fn config_count(&self) -> usize {
        self.configs.len()
    }

    /// 所有配置的只读切片
    pub fn configs(&self) -> &[TableDefinesConfig] {
        &self.configs
    }

    /// 按名称查找配置
    pub fn get_config_by_name(&self, name: &str) -> Option<&TableDefinesConfig> {
        self.configs.iter().find(|c| c.name == name)
    }

    /// 按依赖与优先级排序：被依赖的在前，同层级按 priority 升序（数值小的先）。
    /// 若存在循环依赖或依赖了不存在的配置名则返回 `ConfigDependency` 错误。
    pub fn sorted_configs(&self) -> Result<Vec<&TableDefinesConfig>, BaseError> {
        let name_to_index: HashMap<&str, usize> = self
            .configs
            .iter()
            .enumerate()
            .map(|(i, c)| (c.name.as_str(), i))
            .collect();
        for c in &self.configs {
            for d in &c.depends_on {
                if !name_to_index.contains_key(d.as_str()) {
                    return Err(BaseError::ConfigDependency(format!(
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
            return Err(BaseError::ConfigDependency(
                "配置之间存在循环依赖".to_string(),
            ));
        }
        Ok(out.into_iter().map(|i| &self.configs[i]).collect())
    }

    /// 根据当前所有配置，在 `base_path` 下解析并加载全部表定义文件，合并为一个列表。
    /// 加载顺序按依赖与优先级：被依赖的配置先加载，同层级按 priority 升序。
    pub fn load_all_tables(&self, base_path: &Path) -> Result<Vec<TableDefine>, BaseError> {
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

    /// 对指定名称的配置，在 `base_path` 下加载其表定义
    pub fn load_tables_by_config_name(
        &self,
        base_path: &Path,
        config_name: &str,
    ) -> Result<Vec<TableDefine>, BaseError> {
        let config = self
            .get_config_by_name(config_name)
            .ok_or_else(|| BaseError::ConfigNotFound(config_name.to_string()))?;
        let mut all = Vec::new();
        for file in &config.files {
            let path = base_path.join(file);
            let tables = load_table_defines_from_path(&path)?;
            all.extend(tables);
        }
        Ok(all)
    }
}

/// 根据基础表定义生成多语言伴生表定义（表名后缀 `_i18n`，含 ref_id、locale 及所有 i18n 列）
pub fn derive_i18n_table_define(base: &TableDefine) -> Option<TableDefine> {
    if !base.i18n {
        return None;
    }
    let i18n_columns: Vec<ColumnDefine> = base
        .columns
        .iter()
        .filter(|c| c.i18n)
        .map(|c| ColumnDefine {
            name: c.name.clone(),
            label: c.label.clone(),
            field_type: c.field_type.clone(),
            is_primary_key: false,
            is_nullable: c.is_nullable,
            default_value: c.default_value.clone(),
            i18n: false,
            length: c.length,
            precision: c.precision,
            scale: c.scale,
            db_type: c.db_type.clone(),
            ordinal: c.ordinal,
            created_at: c.created_at,
            updated_at: c.updated_at,
            is_foreign_key: false,
            foreign_key_table: None,
            foreign_key_column: None,
            extensions: c.extensions.clone(),
        })
        .collect();
    if i18n_columns.is_empty() {
        return None;
    }
    let ref_col = ColumnDefine {
        name: "ref_id".to_string(),
        label: "主表ID".to_string(),
        field_type: FieldType::Int,
        is_primary_key: false,
        is_nullable: false,
        default_value: None,
        i18n: false,
        length: None,
        precision: None,
        scale: None,
        db_type: None,
        ordinal: None,
        created_at: None,
        updated_at: None,
        is_foreign_key: false,
        foreign_key_table: None,
        foreign_key_column: None,
        extensions: Default::default(),
    };
    let locale_col = ColumnDefine {
        name: "locale".to_string(),
        label: "语言".to_string(),
        field_type: FieldType::String,
        is_primary_key: false,
        is_nullable: false,
        default_value: None,
        i18n: false,
        length: None,
        precision: None,
        scale: None,
        db_type: None,
        ordinal: None,
        created_at: None,
        updated_at: None,
        is_foreign_key: false,
        foreign_key_table: None,
        foreign_key_column: None,
        extensions: Default::default(),
    };
    let mut columns = vec![ref_col, locale_col];
    columns.extend(i18n_columns);
    let table_name = format!("{}_i18n", base.table_name);
    let display_name = format!("{}（多语言）", base.display_name);
    Some(TableDefine {
        table_name,
        display_name,
        columns,
        primary_keys: vec!["ref_id".to_string(), "locale".to_string()],
        indexes: vec![],
        version: base.version,
        created_at: base.created_at,
        updated_at: base.updated_at,
        i18n: false,
        comment: None,
        schema: base.schema.clone(),
        tablespace: base.tablespace.clone(),
        is_partitioned: false,
        partition_type: None,
        partition_columns: vec![],
        extensions: base.extensions.clone(),
    })
}

/// 从 JSON 文件读取所有表定义，并依次对每张表执行“创建或升级”（调用传入的 executor）
pub fn load_and_apply_table_defines_from_path(
    path: &Path,
    executor: &dyn TableDefineDbExecutor,
) -> Result<(), BaseError> {
    let defines = load_table_defines_from_path(path)?;
    for define in &defines {
        executor.create_or_upgrade_table(define)?;
    }
    Ok(())
}

// ==========================================
// 数据库创建/升级表接口（预留）
// ==========================================

/// 表定义在数据库中执行“创建或升级”的接口，后续由具体数据库实现（如 PostgreSQL / MySQL）。
pub trait TableDefineDbExecutor: Send + Sync {
    /// 根据表定义在数据库中创建新表；若表已存在则报错或由实现决定行为。
    fn create_table(&self, define: &TableDefine) -> Result<(), BaseError>;

    /// 根据表定义升级已有表（如增加列、修改类型等），具体策略由实现决定。
    fn upgrade_table(&self, define: &TableDefine) -> Result<(), BaseError>;

    /// 若表不存在则创建，若存在则尝试升级（默认实现：先尝试创建，失败再调用 upgrade_table）。
    fn create_or_upgrade_table(&self, define: &TableDefine) -> Result<(), BaseError> {
        if self.create_table(define).is_ok() {
            return Ok(());
        }
        self.upgrade_table(define)
    }

    /// 根据表定义创建多语言伴生表（仅当 define.i18n 为 true 且存在 i18n 列时有效）；后续由具体数据库实现。
    fn create_i18n_table(&self, define: &TableDefine) -> Result<(), BaseError> {
        if let Some(i18n_define) = derive_i18n_table_define(define) {
            self.create_or_upgrade_table(&i18n_define)
        } else {
            Ok(())
        }
    }
}
