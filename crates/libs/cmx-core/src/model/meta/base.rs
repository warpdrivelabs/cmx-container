//! 表/列定义从 JSON 加载，以及数据库创建/升级表接口预留
//!
//! DDL 生成/解析、增量 DDL diff 等高级功能已迁移至 cmx-metadata crate。

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
    #[error("DDL 生成错误: {0}")]
    DdlGeneration(String),
    #[error("DDL 解析错误: {0}")]
    DdlParse(String),
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
pub fn load_table_define_from_path(path: &Path) -> Result<TableDefine, BaseError> {
    let s = std::fs::read_to_string(path)?;
    table_define_from_str(&s)
}

/// 支持"多表"的 JSON 根结构（可选）
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum TableDefinesRoot {
    Single(TableDefine),
    Multi { tables: Vec<TableDefine> },
    Array(Vec<TableDefine>),
}

/// 从 JSON 字符串解析多个表定义
pub fn table_defines_from_str(s: &str) -> Result<Vec<TableDefine>, BaseError> {
    let root: TableDefinesRoot = serde_json::from_str(s)?;
    Ok(match root {
        TableDefinesRoot::Single(t) => vec![t],
        TableDefinesRoot::Multi { tables } => tables,
        TableDefinesRoot::Array(arr) => arr,
    })
}

/// 从 JSON 文件路径读取多个表定义
pub fn load_table_defines_from_path(path: &Path) -> Result<Vec<TableDefine>, BaseError> {
    let s = std::fs::read_to_string(path)?;
    table_defines_from_str(&s)
}

// ==========================================
// 建表 JSON 配置文件（多文件列表）
// ==========================================

/// 建表 JSON 配置文件：描述一组表定义文件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableDefinesConfig {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    pub files: Vec<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
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
    pub fn new() -> Self {
        Self { configs: Vec::new() }
    }

    pub fn from_config_paths(paths: &[impl AsRef<Path>]) -> Result<Self, BaseError> {
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

/// 根据基础表定义生成多语言伴生表定义（表名后缀 `_i18n`）
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
        length: None, precision: None, scale: None,
        db_type: None, ordinal: None,
        created_at: None, updated_at: None,
        is_foreign_key: false,
        foreign_key_table: None, foreign_key_column: None,
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
        length: None, precision: None, scale: None,
        db_type: None, ordinal: None,
        created_at: None, updated_at: None,
        is_foreign_key: false,
        foreign_key_table: None, foreign_key_column: None,
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

// ==========================================
// 数据库创建/升级表接口（预留）
// ==========================================

/// 表定义在数据库中执行"创建或升级"的接口，后续由具体数据库实现（如 PostgreSQL / MySQL）。
pub trait TableDefineDbExecutor: Send + Sync {
    fn create_table(&self, define: &TableDefine) -> Result<(), BaseError>;
    fn upgrade_table(&self, define: &TableDefine) -> Result<(), BaseError>;

    fn create_or_upgrade_table(&self, define: &TableDefine) -> Result<(), BaseError> {
        if self.create_table(define).is_ok() {
            return Ok(());
        }
        self.upgrade_table(define)
    }
}
