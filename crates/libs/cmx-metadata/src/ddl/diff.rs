//! 增量 DDL 生成模块
//!
//! 提供 DDL 增量比对功能，通过比对两个版本的 `TableDefine`
//! 生成 ALTER TABLE 等变更语句。
//!
//! # 功能特性
//! - 支持表级别的新增、删除、修改
//! - 支持列级别的新增、删除、修改（类型、nullable、默认值等）
//! - 支持索引的新增和删除
//! - 支持表注释的变更
//!
//! # 使用示例
//! ```ignore
//! use cmx_metadata::ddl::diff::{DdlDiff, TableChange};
//! use cmx_metadata::ddl::postgres::PostgresDdlDialect;
//!
//! let changes = DdlDiff::diff(&old_tables, &new_tables);
//! let dialect = PostgresDdlDialect::default();
//! let ddl_statements = DdlDiff::diff_to_ddl(&dialect, &old_tables, &new_tables)?;
//! ```

use super::DdlDialect;
use crate::MetadataError;
use cmx_core::model::cell::{ColumnDefine, FieldType, IndexDefine, TableDefine};
use std::collections::HashMap;
use tracing::{info, warn};

/// 列变更类型
///
/// 描述对列的变更操作：
/// - 新增列
/// - 删除列
/// - 修改列（类型、nullable、默认值等）
#[derive(Debug, Clone)]
pub enum ColumnChange {
    /// 新增列
    AddColumn(ColumnDefine),
    /// 删除列
    DropColumn(String),
    /// 修改列（包含旧列和新列定义）
    AlterColumn {
        old: ColumnDefine,
        new: Box<ColumnDefine>,
    },
}

/// 索引变更类型
///
/// 描述对索引的变更操作：
/// - 新增索引
/// - 删除索引
/// - 改名重建（内容一致仅名字不同，旧名为系统命名）
/// - 保留手工索引（不删除，仅报告提示）
#[derive(Debug, Clone)]
pub enum IndexChange {
    /// 新增索引
    AddIndex(IndexDefine),
    /// 删除索引（携带旧索引定义，name 为数据库真实索引名，columns/kind 供报告展示）
    DropIndex(IndexDefine),
    /// 改名重建：内容（列序列+类型）一致仅名字不同，且旧名为系统命名（uk_/idx_ 前缀）。
    /// 执行为 DROP 旧名 + CREATE 新名，使定义中的新名字生效；DDL 生成时打 warn 提示
    /// （内容未变仅变名，但索引重建仍有锁表开销）。
    RenameIndex {
        /// 旧索引（name 为数据库真实索引名）
        old: IndexDefine,
        /// 新索引（name 为定义期名）
        new: IndexDefine,
    },
    /// 手工创建的索引（非系统命名前缀且不在当前定义中）：**保留不删**，
    /// 不生成任何 DDL，仅随变更携带供部署计划报告提示（视为 DBA 手工创建）。
    PreservedManualIndex(IndexDefine),
}

/// 列注释变更（仅 `COMMENT ON COLUMN` 的 label 不同，列结构无变化）
///
/// 独立于 [`ColumnChange`]：列结构 diff（`column_changed`）不比较 label，
/// 避免单纯注释变化被误报为「修改列」却无结构 DDL 可执行。列注释同步通过此结构
/// 单独捕获，由 `changes_to_ddl` 生成 `COMMENT ON COLUMN`。
#[derive(Debug, Clone)]
pub struct ColumnCommentChange {
    /// 列名
    pub column: String,
    /// 旧注释（数据库现有 `col_description`，缺失时为空串）
    pub old_label: String,
    /// 新注释（设计期 `caption`，缺失时为空串）
    pub new_label: String,
}

/// 表级别的变更描述
///
/// 描述对表的变更操作：
/// - 新建表
/// - 删除表
/// - 修改表（包含列变更、索引变更、注释变更等）
#[derive(Debug, Clone)]
pub enum TableChange {
    /// 新建表
    CreateTable(TableDefine),
    /// 删除表
    DropTable(String),
    /// 修改表
    AlterTable {
        table_name: String,
        schema: Option<String>,
        column_changes: Vec<ColumnChange>,
        index_changes: Vec<IndexChange>,
        comment_change: Option<String>,
        /// 列注释变更（label 不一致的列）
        column_comment_changes: Vec<ColumnCommentChange>,
    },
}

/// 增量 DDL 工具
///
/// 提供静态方法用于：
/// - 比对两组 `TableDefine` 生成变更列表
/// - 将变更列表转换为 DDL 语句
/// - 一步到位完成比对和 DDL 生成
pub struct DdlDiff;

impl DdlDiff {
    /// 比对两组 TableDefine，生成变更列表
    ///
    /// 比对旧版本和新版本的表定义，生成增量变更。
    /// 包括：新增的表、删除的表、修改的表（列变更、索引变更、注释变更）
    ///
    /// # 比对策略
    /// - 使用 HashMap 按表名快速查找，提高比对效率
    /// - 新增表：存在于 new 但不在 old 中
    /// - 删除表：存在于 old 但不在 new 中
    /// - 修改表：同时存在于两者中，但列/索引/注释有差异
    ///
    /// # 参数
    /// * `old` - 旧版本表定义列表
    /// * `new` - 新版本表定义列表
    ///
    /// # 返回值
    /// * `Vec<TableChange>` - 变更列表
    pub fn diff(old: &[TableDefine], new: &[TableDefine]) -> Vec<TableChange> {
        // 构建按表名索引的 HashMap，加速查找
        let old_map: HashMap<&str, &TableDefine> =
            old.iter().map(|t| (t.table_name.as_str(), t)).collect();
        let new_map: HashMap<&str, &TableDefine> =
            new.iter().map(|t| (t.table_name.as_str(), t)).collect();

        let mut changes = Vec::new();

        // ============================================
        // 检测新增的表
        // ============================================
        for new_table in new {
            if !old_map.contains_key(new_table.table_name.as_str()) {
                changes.push(TableChange::CreateTable(new_table.clone()));
            }
        }

        // ============================================
        // 检测删除的表
        // ============================================
        for old_table in old {
            if !new_map.contains_key(old_table.table_name.as_str()) {
                changes.push(TableChange::DropTable(old_table.table_name.clone()));
            }
        }

        // ============================================
        // 检测修改的表（列变更、索引变更、注释变更）
        // ============================================
        for new_table in new {
            if let Some(old_table) = old_map.get(new_table.table_name.as_str()) {
                // 比对列定义差异
                let column_changes = Self::diff_columns(&old_table.columns, &new_table.columns);
                // 比对索引定义差异
                let index_changes = Self::diff_indexes(&old_table.indexes, &new_table.indexes);
                // 比对表注释变更
                let comment_change = if old_table.comment != new_table.comment {
                    new_table.comment.clone()
                } else {
                    None
                };
                // 比对列注释变更（label 不一致，独立于列结构 diff）
                let column_comment_changes =
                    Self::diff_column_comments(&old_table.columns, &new_table.columns);

                // 只有当有实质变更时才记录。
                // 注意：DropColumn（DB 多出列）不算实质变更——生产环境允许 DB 比元数据多列
                // （additive-only：只加列不删列），故 DB 多出列不应触发表的升级判定。
                // column_changes 仍完整保留 DropColumn 传给执行层（执行层仅记日志、不删列）。
                let has_substantive_column_change = column_changes
                    .iter()
                    .any(|c| !matches!(c, ColumnChange::DropColumn(_)));
                if has_substantive_column_change
                    || !index_changes.is_empty()
                    || comment_change.is_some()
                    || !column_comment_changes.is_empty()
                {
                    changes.push(TableChange::AlterTable {
                        table_name: new_table.table_name.clone(),
                        schema: new_table.schema.clone(),
                        column_changes,
                        index_changes,
                        comment_change,
                        column_comment_changes,
                    });
                }
            }
        }

        changes
    }

    /// 比对列注释（label）变更
    ///
    /// 遍历同时存在于新旧两组的列，收集 `label` 不一致的列（旧来自数据库
    /// `col_description`、新来自设计期 `caption`）。仅比较 label，不触发列结构变更，
    /// 由 `changes_to_ddl` 生成 `COMMENT ON COLUMN` 同步。
    fn diff_column_comments(
        old_cols: &[ColumnDefine],
        new_cols: &[ColumnDefine],
    ) -> Vec<ColumnCommentChange> {
        let old_map: HashMap<&str, &ColumnDefine> =
            old_cols.iter().map(|c| (c.name.as_str(), c)).collect();
        let mut changes = Vec::new();
        for new_col in new_cols {
            if let Some(old_col) = old_map.get(new_col.name.as_str())
                && old_col.label != new_col.label
            {
                changes.push(ColumnCommentChange {
                    column: new_col.name.clone(),
                    old_label: old_col.label.clone(),
                    new_label: new_col.label.clone(),
                });
            }
        }
        changes
    }

    /// 比对列变更
    ///
    /// 比对两组列定义，返回列变更列表。
    /// 包括：新增列、删除列、修改列（类型、nullable、默认值等）
    ///
    /// # 参数
    /// * `old_cols` - 旧版本列定义列表
    /// * `new_cols` - 新版本列定义列表
    ///
    /// # 返回值
    /// * `Vec<ColumnChange>` - 列变更列表
    fn diff_columns(old_cols: &[ColumnDefine], new_cols: &[ColumnDefine]) -> Vec<ColumnChange> {
        let old_map: HashMap<&str, &ColumnDefine> =
            old_cols.iter().map(|c| (c.name.as_str(), c)).collect();
        let new_map: HashMap<&str, &ColumnDefine> =
            new_cols.iter().map(|c| (c.name.as_str(), c)).collect();

        let mut changes = Vec::new();

        // 新增列
        for new_col in new_cols {
            if !old_map.contains_key(new_col.name.as_str()) {
                changes.push(ColumnChange::AddColumn(new_col.clone()));
            }
        }

        // 删除列
        for old_col in old_cols {
            if !new_map.contains_key(old_col.name.as_str()) {
                changes.push(ColumnChange::DropColumn(old_col.name.clone()));
            }
        }

        // 修改列（类型/nullable/default/length 变更）
        for new_col in new_cols {
            if let Some(old_col) = old_map.get(new_col.name.as_str())
                && Self::column_changed(old_col, new_col)
            {
                changes.push(ColumnChange::AlterColumn {
                    old: (*old_col).clone(),
                    new: Box::new(new_col.clone()),
                });
            }
        }

        changes
    }

    /// 判断列是否有实质性变更
    ///
    /// 检查列的以下属性是否发生变化：
    /// - 字段类型（field_type）
    /// - 可空性（is_nullable）
    /// - 默认值（default_value，经 [`Self::defaults_equivalent`] 语义比较，见其文档）
    /// - 长度（length）
    /// - 精度（precision）、小数位（scale）：仅对 [`FieldType::Decimal`] 比较
    ///
    /// # 精度/标度的特殊处理
    /// PostgreSQL 对 `integer`/`bigint`/`smallint`/`double precision` 等内置类型
    /// 会报告类型派生的 `numeric_precision`/`numeric_scale`（如 `bigint` 恒为 64/0），
    /// 这些并非用户定义，不应参与 diff；否则会把「设计期未定义精度」与
    /// 「数据库类型派生精度」误判为变更，产生永久假阳性。因此仅当新列类型为
    /// `Decimal`（用户显式定义精度的 `NUMERIC(p,s)`）时才比较 precision/scale。
    ///
    /// # 参数
    /// * `old` - 旧列定义
    /// * `new` - 新列定义
    ///
    /// # 返回值
    /// * `bool` - 是否发生实质性变更
    fn column_changed(old: &ColumnDefine, new: &ColumnDefine) -> bool {
        if old.field_type != new.field_type
            || old.is_nullable != new.is_nullable
            || !Self::defaults_equivalent(old.default_value.as_deref(), new.default_value.as_deref())
            || old.length != new.length
        {
            return true;
        }
        // 精度/标度仅对 Decimal 有意义：integer/bigint/smallint/double 等类型的
        // numeric_precision/scale 是 PG 类型派生属性（如 bigint 恒为 64/0），不应参与 diff。
        matches!(new.field_type, FieldType::Decimal)
            && (old.precision != new.precision || old.scale != new.scale)
    }

    /// 判断两侧列默认值表达式是否语义等价（用于 diff 消除假阳性）。
    ///
    /// # 为何不能直接字符串比较
    /// 两侧默认值的来源形态不同：
    /// - 编译侧（cmx-model-deploy `normalize_default_value`）：产出最终 SQL 表达式，
    ///   字符串/日期/JSON 带单引号定界、布尔大写（`'active'` / `TRUE` / `'{"a":1}'`）；
    /// - 内省侧（executor `clean_pg_default` 清洗 pg_attrdef.adbin）：多数已去 cast
    ///   去引号、布尔小写、jsonb 带规范化空格（`active` / `true` / `{"a": 1}`）。
    ///
    /// 直接 `!=` 比较必然不等 → 首次部署 SET DEFAULT 进库后，此后**每次部署**都重复
    /// `ALTER COLUMN ... SET DEFAULT`（无操作 DDL 噪音的永久假阳性）。此处对两侧做
    /// **同一套**归一化（[`norm_default_expr`]）后再比较，并保留 None/Some 的真差异判定。
    ///
    /// # 已知局限（知情保留）
    /// PG 对 timestamp 字面量默认值会规范化补零（`'2023-01-01'` 落库为
    /// `'2023-01-01 00:00:00'::timestamp`），两侧文本不等会保留为变更；当前存量
    /// 定义无日期类默认值，不做日期语义解析。
    fn defaults_equivalent(old: Option<&str>, new: Option<&str>) -> bool {
        match (old, new) {
            (None, None) => true,
            (Some(a), Some(b)) => {
                let (na, nb) = (norm_default_expr(a), norm_default_expr(b));
                if na == nb {
                    return true;
                }
                // jsonb 形态差异（PG 序列化加空格）：两侧都是合法 JSON 时按语义比较
                match (
                    serde_json::from_str::<serde_json::Value>(&na),
                    serde_json::from_str::<serde_json::Value>(&nb),
                ) {
                    (Ok(va), Ok(vb)) => va == vb,
                    _ => false,
                }
            }
            // None 与空串等价：内省侧 clean_pg_default 对 nextval/NULL 返回空串，
            // 编译侧无默认值为 None——两侧都表示"无默认值"。
            (a, b) => {
                let empty = |v: Option<&str>| v.is_none_or(|s| s.trim().is_empty());
                empty(a) && empty(b)
            }
        }
    }

    /// 比对索引变更
    ///
    /// 按「索引列 + 索引类型」匹配（忽略索引名），返回索引变更列表。
    /// 包括：新增索引、删除索引。
    ///
    /// # 为何按列匹配而非按名匹配
    /// 设计期索引名通常是规范化命名（如 `uk_<table>_1`），而 PostgreSQL 对列级 UNIQUE 或
    /// `ADD CONSTRAINT ... UNIQUE` 会自动命名（如 `<table>_<col>_key`）。若按名字字符串匹配，
    /// 同一组列+类型的索引会被误判为「先删后建」，产生永久假阳性。故此处只比较语义内容
    /// （columns 顺序敏感 + kind），名字仅用于执行 DDL（AddIndex 用设计期名，DropIndex 用 DB 真实名）。
    ///
    /// # INVALID 索引强制重建
    /// 内省侧对 INVALID / NOT-READY 索引（`CREATE INDEX CONCURRENTLY` 失败残留）标记
    /// `valid = false`，此处视为「内容永不匹配」：无论定义中是否有同内容索引都产生
    /// DropIndex（定义中有则再 AddIndex，先 DROP 后 CREATE 重建为有效索引），避免
    /// INVALID 索引占名导致 CREATE 撞 `already exists` 中断部署。
    ///
    /// # 改名重建
    /// 内容一致（列+类型）但名字不同的索引：旧名为系统命名（`uk_` / `idx_` 前缀）时
    /// 视为定义期改名 → 产出 [`IndexChange::RenameIndex`]（DROP 旧名 + CREATE 新名），
    /// 使定义中的新名字生效（DDL 生成时 warn 提示）；旧名非系统前缀（DBA 手工建 /
    /// PG 自动名 `_key` 等）不重建，旧索引继续服役（手工保护优先）。
    ///
    /// # 手工索引保护
    /// DB 中多余（定义中无内容匹配）的索引**并非都该删**：DBA 手工创建的索引
    /// （如性能优化临时加的）不在定义里，按「定义即真相」一刀切删除会造成误伤。
    /// 删除判定按三档：
    /// 1. INVALID 索引——留着必占名，无条件 DROP（自愈优先于保护）；
    /// 2. 系统命名（`uk_` / `idx_` 前缀，与 compile.rs `auto_index_name` /
    ///    前端 `_autoIndexName` 的前缀约定一致）或名字仍在当前定义中（自定义名
    ///    条目改列重建、删后同名重建）——本系统管理的，DROP；
    /// 3. 其余——视为手工创建，产出 [`IndexChange::PreservedManualIndex`]
    ///    保留不删（仅报告提示，不生成 DDL）。
    ///
    /// # 参数
    /// * `old_idxs` - 旧版本索引定义列表
    /// * `new_idxs` - 新版本索引定义列表
    ///
    /// # 返回值
    /// * `Vec<IndexChange>` - 索引变更列表
    fn diff_indexes(old_idxs: &[IndexDefine], new_idxs: &[IndexDefine]) -> Vec<IndexChange> {
        // 索引内容相等：双侧均有效、列名序列与类型一致（顺序敏感，与复合索引语义一致）。
        let same_content =
            |a: &IndexDefine, b: &IndexDefine| a.valid && b.valid && a.columns == b.columns && a.kind == b.kind;

        let mut changes = Vec::new();

        // 新增索引：new 中无任何 old 索引与之内容相等。
        // （改名重建场景内容相等，在此跳过——RenameIndex 在下方 DROP 循环统一产出，不重复 ADD）
        for new_idx in new_idxs {
            if !old_idxs.iter().any(|o| same_content(o, new_idx)) {
                changes.push(IndexChange::AddIndex(new_idx.clone()));
            }
        }

        // 当前定义仍使用的名字（含自定义名）：本系统管理的删除/重建不受手工保护拦
        let managed_names: std::collections::HashSet<&str> =
            new_idxs.iter().map(|n| n.name.as_str()).collect();

        // 删除索引：old 中无任何 new 索引与之内容相等（携带旧定义，name 为 DB 真实名供 DROP）。
        for old_idx in old_idxs {
            if let Some(n) = new_idxs.iter().find(|n| same_content(n, old_idx)) {
                // 内容一致：名字也一致 → 真无变更。
                if n.name == old_idx.name {
                    continue;
                }
                // 名字不同：系统命名（uk_/idx_ 前缀）→ 改名重建，定义中的新名生效；
                // 非系统前缀（DBA 手工建 / PG 自动名 _key 等）→ 手工保护，旧索引继续服役。
                if is_system_index_name(&old_idx.name) {
                    changes.push(IndexChange::RenameIndex {
                        old: old_idx.clone(),
                        new: n.clone(),
                    });
                }
                continue;
            }
            let managed = !old_idx.valid // INVALID 自愈优先：占名必撞 CREATE
                || is_system_index_name(&old_idx.name)
                || managed_names.contains(old_idx.name.as_str());
            if managed {
                changes.push(IndexChange::DropIndex(old_idx.clone()));
            } else {
                changes.push(IndexChange::PreservedManualIndex(old_idx.clone()));
            }
        }

        changes
    }

    /// 将变更列表转为 DDL 语句
    ///
    /// 将变更列表转换为对应数据库方言的 DDL 语句。
    ///
    /// # 生成的 DDL 类型
    /// - `TableChange::CreateTable`：生成 CREATE TABLE + COMMENT + INDEX
    /// - `TableChange::DropTable`：记录日志（不实际生成 DROP 语句，防止数据丢失）
    /// - `TableChange::AlterTable`：
    ///   - 列变更：ADD COLUMN / ALTER COLUMN（类型/nullable/default）
    ///   - 索引变更：CREATE INDEX / DROP INDEX
    ///   - 注释变更：COMMENT ON TABLE（表注释）/ COMMENT ON COLUMN（列注释）
    ///
    /// # 参数
    /// * `dialect` - DDL 方言实现
    /// * `changes` - 变更列表
    ///
    /// # 返回值
    /// * 成功返回 DDL 语句列表
    /// * 失败返回 `MetadataError`
    pub fn changes_to_ddl(
        dialect: &dyn DdlDialect,
        changes: &[TableChange],
    ) -> Result<Vec<String>, MetadataError> {
        let mut stmts = Vec::new();

        for change in changes {
            match change {
                // ============================================
                // 新建表：生成完整 DDL
                // ============================================
                TableChange::CreateTable(table) => {
                    stmts.push(dialect.generate_create_table(table)?);
                    stmts.extend(dialect.generate_comments(table)?);
                    stmts.extend(dialect.generate_create_indexes(table)?);
                }
                // ============================================
                // 删除表：仅记录日志，不实际执行（安全考虑）
                // ============================================
                TableChange::DropTable(name) => {
                    // stmts.push(format!("DROP TABLE IF EXISTS \"{}\" CASCADE;", name));
                    info!("表元数据更新需要删除的表: {}，实际未执行", name);
                }
                // ============================================
                // 修改表：生成列变更、索引变更、注释变更的 DDL
                // ============================================
                TableChange::AlterTable {
                    table_name,
                    schema,
                    column_changes,
                    index_changes,
                    comment_change,
                    column_comment_changes,
                } => {
                    // 处理列变更
                    for cc in column_changes {
                        match cc {
                            // 新增列
                            ColumnChange::AddColumn(col) => {
                                stmts.push(dialect.generate_add_column(
                                    table_name,
                                    schema.as_deref(),
                                    col,
                                )?);
                            }
                            // 删除列：仅记录日志，不实际执行
                            ColumnChange::DropColumn(name) => {
                                // stmts.push(dialect.generate_drop_column(
                                //     table_name,
                                //     schema.as_deref(),
                                //     name,
                                // )?);
                                info!("表元数据更新需要删除的列: {}，实际未执行", name);
                            }
                            // 修改列（类型、nullable、默认值）
                            ColumnChange::AlterColumn { old, new } => {
                                stmts.extend(dialect.generate_alter_column(
                                    table_name,
                                    schema.as_deref(),
                                    old,
                                    new,
                                )?);
                            }
                        }
                    }
                    // 处理索引变更：先 DROP 后 CREATE——
                    // ① 改列集合的旧索引先释放名字；② 防御设计期自动名与 DB 侧同名索引
                    //    （DBA 手工建 / CONCURRENTLY 失败的 INVALID 残留）撞名，CREATE 报
                    //    already exists 阻断部署而排在后面的 DROP 本可解冲突。
                    // 索引 DDL 整体位于列变更之后：新索引可引用本次新增的列。
                    for ic in index_changes {
                        // 删除索引（name 为 DB 真实索引名）；改名重建先释放旧名
                        if let IndexChange::DropIndex(idx) = ic {
                            stmts.push(format!("DROP INDEX IF EXISTS \"{}\";", idx.name));
                        }
                        if let IndexChange::RenameIndex { old, .. } = ic {
                            stmts.push(format!("DROP INDEX IF EXISTS \"{}\";", old.name));
                        }
                    }
                    for ic in index_changes {
                        // 新增索引；改名重建 CREATE 新名（内容未变仅变名，warn 提示重建开销）
                        let idx = match ic {
                            IndexChange::AddIndex(idx) => idx,
                            IndexChange::RenameIndex { old, new } => {
                                warn!(
                                    "索引改名重建: {} → {}（列 [{}]）——内容未变仅名字变更，仍执行 DROP+CREATE",
                                    old.name,
                                    new.name,
                                    new.columns.join(", ")
                                );
                                new
                            }
                            _ => continue,
                        };
                        let qualified = match schema {
                            Some(s) => format!("\"{}\".\"{}\"", s, table_name),
                            None => format!("\"{}\"", table_name),
                        };
                        let cols = idx
                            .columns
                            .iter()
                            .map(|c| format!("\"{}\"", c))
                            .collect::<Vec<_>>()
                            .join(", ");
                        let unique = match idx.kind {
                            cmx_core::model::cell::IndexKind::Unique => "UNIQUE ",
                            cmx_core::model::cell::IndexKind::Normal => "",
                        };
                        stmts.push(format!(
                            "CREATE {}INDEX \"{}\" ON {} ({});",
                            unique, idx.name, qualified, cols
                        ));
                    }
                    // 处理表注释变更
                    if let Some(comment) = comment_change {
                        let qualified = match schema {
                            Some(s) => format!("\"{}\".\"{}\"", s, table_name),
                            None => format!("\"{}\"", table_name),
                        };
                        let escaped = comment.replace('\'', "''");
                        stmts.push(format!("COMMENT ON TABLE {} IS '{}';", qualified, escaped));
                    }
                    // 处理列注释变更：对 label 不一致的列生成 COMMENT ON COLUMN。
                    // new_label 为空表示设计期清除注释 → IS NULL。
                    for cc in column_comment_changes {
                        let qualified = match schema {
                            Some(s) => format!("\"{}\".\"{}\"", s, table_name),
                            None => format!("\"{}\"", table_name),
                        };
                        if cc.new_label.is_empty() {
                            stmts.push(format!(
                                "COMMENT ON COLUMN {}.\"{}\" IS NULL;",
                                qualified, cc.column
                            ));
                        } else {
                            let escaped = cc.new_label.replace('\'', "''");
                            stmts.push(format!(
                                "COMMENT ON COLUMN {}.\"{}\" IS '{}';",
                                qualified, cc.column, escaped
                            ));
                        }
                    }
                }
            }
        }

        Ok(stmts)
    }

    /// 一步到位：比对 + 生成 DDL
    ///
    /// 一次性完成表定义比对和 DDL 语句生成。
    ///
    /// # 参数
    /// * `dialect` - DDL 方言实现
    /// * `old` - 旧版本表定义列表
    /// * `new` - 新版本表定义列表
    ///
    /// # 返回值
    /// * 成功返回 DDL 语句列表
    /// * 失败返回 `MetadataError`
    pub fn diff_to_ddl(
        dialect: &dyn DdlDialect,
        old: &[TableDefine],
        new: &[TableDefine],
    ) -> Result<Vec<String>, MetadataError> {
        let changes = Self::diff(old, new);
        Self::changes_to_ddl(dialect, &changes)
    }
}

/// 系统自动命名的索引前缀（与 compile.rs `auto_index_name`、前端 `_autoIndexName`
/// 的前缀约定一致）：带这些前缀的多余索引视为本系统产物，允许自动清理；其余视为
/// 手工创建，保留不删（见 [`DdlDiff::diff_indexes`] 的「手工索引保护」）。
fn is_system_index_name(name: &str) -> bool {
    name.starts_with("uk_") || name.starts_with("idx_")
}

/// 单侧默认值表达式的比较归一化（形态对齐，不做语义转换）。
///
/// [`DdlDiff::defaults_equivalent`] 对两侧各过一遍本函数后比较，覆盖编译侧与
/// 内省侧的形态差异：
/// - 剥离引号外的 `::type` cast 后缀（`'active'::character varying` → `'active'`）；
/// - 剥外层单引号定界并还原 `''` 转义（`'it''s'` → `it's`）；
/// - 纯布尔字面量统一小写（`TRUE` vs `true`）。
///
/// jsonb 的空格差异（`{"a": 1}` vs `{"a":1}`）由 `defaults_equivalent` 的语义级
/// 比较兜底，本函数不做 JSON 解析。
fn norm_default_expr(s: &str) -> String {
    let mut t = s.trim();
    // 剥引号外的 cast 后缀：取第一个引号外 `::` 之前的部分
    if let Some(pos) = cast_delimiter_pos(t) {
        t = t[..pos].trim();
    }
    // 剥外层单引号定界（含 '' 转义还原）
    let core = if t.len() >= 2 && t.starts_with('\'') && t.ends_with('\'') {
        t[1..t.len() - 1].replace("''", "'")
    } else {
        t.to_string()
    };
    // 纯布尔字面量统一小写（字符串默认值本身可能就是 'true' 文本，双侧对称归一无碍）
    if core.eq_ignore_ascii_case("true") {
        return "true".to_string();
    }
    if core.eq_ignore_ascii_case("false") {
        return "false".to_string();
    }
    core
}

/// 返回字符串中**第一个引号外** `::` 的字节位置（无则 None）。
///
/// 引号内（含 `''` 转义）的 `::` 不算——如 `nextval('seq'::regclass)` 的 `::`
/// 在引号内，不会被当作 cast 后缀剥除。
fn cast_delimiter_pos(s: &str) -> Option<usize> {
    let b = s.as_bytes();
    let mut in_quote = false;
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'\'' => in_quote = !in_quote,
            b':' if !in_quote && i + 1 < b.len() && b[i + 1] == b':' => return Some(i),
            _ => {}
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ddl::postgres::PostgresDdlDialect;
    use cmx_core::model::cell::{FieldType, IndexDefine, IndexKind};

    fn make_simple_table(
        name: &str,
        cols: Vec<ColumnDefine>,
        indexes: Vec<IndexDefine>,
    ) -> TableDefine {
        TableDefine {
            table_name: name.to_string(),
            display_name: name.to_string(),
            columns: cols,
            primary_keys: vec![],
            indexes,
            version: 1,
            create_time: None,
            update_time: None,
            i18n: false,
            comment: None,
            schema: None,
            tablespace: None,
            is_partitioned: false,
            partition_type: None,
            partition_columns: vec![],
            extensions: HashMap::new(),
        }
    }

    fn make_col(name: &str, ft: FieldType, nullable: bool) -> ColumnDefine {
        ColumnDefine {
            name: name.to_string(),
            label: name.to_string(),
            field_type: ft,
            is_primary_key: false,
            is_nullable: nullable,
            default_value: None,
            i18n: false,
            length: None,
            precision: None,
            scale: None,
            db_type: None,
            ordinal: None,
            create_time: None,
            update_time: None,
            is_foreign_key: false,
            foreign_key_table: None,
            foreign_key_column: None,
            extensions: HashMap::new(),
        }
    }

    #[test]
    fn test_diff_new_table() {
        let old: Vec<TableDefine> = vec![];
        let new = vec![make_simple_table("new_table", vec![], vec![])];
        let changes = DdlDiff::diff(&old, &new);
        assert_eq!(changes.len(), 1);
        assert!(matches!(&changes[0], TableChange::CreateTable(t) if t.table_name == "new_table"));
    }

    #[test]
    fn test_diff_drop_table() {
        let old = vec![make_simple_table("old_table", vec![], vec![])];
        let new: Vec<TableDefine> = vec![];
        let changes = DdlDiff::diff(&old, &new);
        assert_eq!(changes.len(), 1);
        assert!(matches!(&changes[0], TableChange::DropTable(name) if name == "old_table"));
    }

    #[test]
    fn test_diff_add_column() {
        let old = vec![make_simple_table(
            "t",
            vec![make_col("id", FieldType::Int, false)],
            vec![],
        )];
        let new = vec![make_simple_table(
            "t",
            vec![
                make_col("id", FieldType::Int, false),
                make_col("name", FieldType::String, true),
            ],
            vec![],
        )];

        let changes = DdlDiff::diff(&old, &new);
        assert_eq!(changes.len(), 1);
        if let TableChange::AlterTable { column_changes, .. } = &changes[0] {
            assert_eq!(column_changes.len(), 1);
            assert!(matches!(&column_changes[0], ColumnChange::AddColumn(c) if c.name == "name"));
        } else {
            panic!("Expected AlterTable");
        }
    }

    #[test]
    fn test_diff_drop_column() {
        // additive-only 语义：DB 比元数据多列（old 有 old_col、new 没有），
        // 不应触发表的升级判定（生产允许 DB 多列，只加不删）。
        let old = vec![make_simple_table(
            "t",
            vec![
                make_col("id", FieldType::Int, false),
                make_col("old_col", FieldType::String, true),
            ],
            vec![],
        )];
        let new = vec![make_simple_table(
            "t",
            vec![make_col("id", FieldType::Int, false)],
            vec![],
        )];

        let changes = DdlDiff::diff(&old, &new);
        assert!(
            changes.is_empty(),
            "DB 多出列不应触发表变更（additive-only）: {changes:?}"
        );
    }

    #[test]
    fn test_diff_alter_column_type() {
        let old = vec![make_simple_table(
            "t",
            vec![make_col("name", FieldType::String, true)],
            vec![],
        )];
        let new = vec![make_simple_table(
            "t",
            vec![make_col("name", FieldType::Text, true)],
            vec![],
        )];

        let changes = DdlDiff::diff(&old, &new);
        assert_eq!(changes.len(), 1);
        if let TableChange::AlterTable { column_changes, .. } = &changes[0] {
            assert!(
                column_changes
                    .iter()
                    .any(|c| matches!(c, ColumnChange::AlterColumn { .. }))
            );
        } else {
            panic!("Expected AlterTable");
        }
    }

    #[test]
    fn test_diff_add_index() {
        let old = vec![make_simple_table("t", vec![], vec![])];
        let new = vec![make_simple_table(
            "t",
            vec![],
            vec![IndexDefine {
                name: "idx_new".to_string(),
                columns: vec!["col1".to_string()],
                kind: IndexKind::Normal,
                valid: true,
            }],
        )];

        let changes = DdlDiff::diff(&old, &new);
        assert_eq!(changes.len(), 1);
        if let TableChange::AlterTable { index_changes, .. } = &changes[0] {
            assert!(matches!(&index_changes[0], IndexChange::AddIndex(i) if i.name == "idx_new"));
        } else {
            panic!("Expected AlterTable");
        }
    }

    #[test]
    fn test_diff_to_ddl() {
        let dialect = PostgresDdlDialect::default();
        let old = vec![make_simple_table(
            "t",
            vec![make_col("id", FieldType::Int, false)],
            vec![],
        )];
        let new = vec![make_simple_table(
            "t",
            vec![
                make_col("id", FieldType::Int, false),
                make_col("name", FieldType::String, true),
            ],
            vec![],
        )];

        let stmts = DdlDiff::diff_to_ddl(&dialect, &old, &new).unwrap();
        assert!(!stmts.is_empty());
        assert!(stmts[0].contains("ADD COLUMN"));
    }

    #[test]
    fn test_no_changes() {
        let tables = vec![make_simple_table(
            "t",
            vec![make_col("id", FieldType::Int, false)],
            vec![],
        )];
        let changes = DdlDiff::diff(&tables, &tables);
        assert!(changes.is_empty());
    }

    /// bigint 列经 PG 内省还原后携带派生精度（precision=64, scale=0），
    /// 设计期定义未设置精度。两者类型相同（同为 `Int`），不应判为变更。
    #[test]
    fn column_changed_ignores_int_precision() {
        let mut db_restored = make_col("sort_no", FieldType::Int, true);
        db_restored.precision = Some(64);
        db_restored.scale = Some(0);
        db_restored.db_type = Some("BIGINT".to_string());
        let desired = make_col("sort_no", FieldType::Int, true); // precision/scale = None

        assert!(
            !DdlDiff::column_changed(&db_restored, &desired),
            "bigint 派生精度不应触发列变更"
        );

        // 整张表 diff 也应无变更
        let old = vec![make_simple_table(
            "cf_client",
            vec![db_restored.clone()],
            vec![],
        )];
        let new = vec![make_simple_table("cf_client", vec![desired], vec![])];
        assert!(
            DdlDiff::diff(&old, &new).is_empty(),
            "bigint 精度假阳性：表 diff 应为空"
        );
    }

    /// Decimal 类型的精度/标度变化仍应被检出。
    #[test]
    fn column_changed_detects_decimal_precision() {
        let mut old = make_col("amount", FieldType::Decimal, true);
        old.precision = Some(10);
        old.scale = Some(2);
        let mut new = make_col("amount", FieldType::Decimal, true);
        new.precision = Some(12);
        new.scale = Some(2);
        assert!(
            DdlDiff::column_changed(&old, &new),
            "Decimal 精度变化应判为变更"
        );
    }

    /// 索引按「列+类型」匹配：设计期名 uk_t_1 与 PG 自动名 t_code_key 不同，
    /// 但列+唯一性相同，不应判为变更（消除索引名错配假阳性）。
    #[test]
    fn diff_indexes_matches_by_columns_ignoring_name() {
        // DB 还原侧（PG 真实名）
        let db_idx = IndexDefine {
            name: "cf_t_code_key".to_string(),
            columns: vec!["code".to_string()],
            kind: IndexKind::Unique,
            valid: true,
        };
        // 设计期侧（规范化名 uk_t_1）
        let design_idx = IndexDefine {
            name: "uk_cf_t_1".to_string(),
            columns: vec!["code".to_string()],
            kind: IndexKind::Unique,
            valid: true,
        };
        let old = vec![make_simple_table("cf_t", vec![], vec![db_idx.clone()])];
        let new = vec![make_simple_table("cf_t", vec![], vec![design_idx])];
        let changes = DdlDiff::diff(&old, &new);
        // 列+类型相同 → 不应产出任何 AlterTable
        assert!(
            changes.is_empty(),
            "索引名不同但列+类型相同，不应判变更: {changes:?}"
        );
    }

    /// 系统命名索引改名（内容一致仅名字不同）：RenameIndex 重建——DROP 旧名 + CREATE
    /// 新名，定义中的新名生效（DDL 生成时 warn 提示重建开销）。
    #[test]
    fn diff_indexes_rename_rebuilds_system_named() {
        let db_idx = IndexDefine {
            name: "uk_cv_docno".to_string(),
            columns: vec!["doc_no".to_string()],
            kind: IndexKind::Unique,
            valid: true,
        };
        let design_idx = IndexDefine {
            name: "uk_cv_docno1".to_string(),
            columns: vec!["doc_no".to_string()], // 列+类型一致，仅名字不同
            kind: IndexKind::Unique,
            valid: true,
        };
        let old = vec![make_simple_table("cv_t", vec![], vec![db_idx])];
        let new = vec![make_simple_table("cv_t", vec![], vec![design_idx])];
        let changes = DdlDiff::diff(&old, &new);
        assert_eq!(changes.len(), 1, "应产出 1 个 AlterTable: {changes:?}");
        if let TableChange::AlterTable { index_changes, .. } = &changes[0] {
            assert_eq!(index_changes.len(), 1, "应只有 RenameIndex: {index_changes:?}");
            assert!(
                matches!(
                    &index_changes[0],
                    IndexChange::RenameIndex { old, new }
                        if old.name == "uk_cv_docno" && new.name == "uk_cv_docno1"
                ),
                "应为 RenameIndex: {index_changes:?}"
            );
            // DDL：先 DROP 旧名再 CREATE 新名
            let dialect = PostgresDdlDialect::default();
            let ddl = DdlDiff::changes_to_ddl(&dialect, &changes).unwrap();
            assert!(
                ddl.iter().any(|s| s.contains("DROP INDEX IF EXISTS \"uk_cv_docno\"")),
                "应 DROP 旧名: {ddl:?}"
            );
            assert!(
                ddl.iter().any(|s| s.contains("CREATE UNIQUE INDEX \"uk_cv_docno1\"")),
                "应 CREATE 新名: {ddl:?}"
            );
        } else {
            panic!("Expected AlterTable");
        }
    }

    /// 索引列真变更：设计期重建新索引（AddIndex）；DB 侧旧索引的处置按命名分档——
    /// `<表>_<列>_key` 是 PG 列级 UNIQUE/ADD CONSTRAINT 的自动命名（无 uk_/idx_ 前缀、
    /// 名字不在定义中）→ 按「手工索引保护」规则保留不删（部署计划提示，如确认废弃可
    /// 手工 DROP）；系统命名（uk_/idx_）的旧索引才会被 DROP 清理（见下一个用例）。
    #[test]
    fn diff_indexes_detects_real_column_change() {
        let db_idx = IndexDefine {
            name: "cf_t_code_key".to_string(),
            columns: vec!["code".to_string()],
            kind: IndexKind::Unique,
            valid: true,
        };
        let design_idx = IndexDefine {
            name: "uk_cf_t_1".to_string(),
            columns: vec!["name".to_string()], // 列不同
            kind: IndexKind::Unique,
            valid: true,
        };
        let old = vec![make_simple_table("cf_t", vec![], vec![db_idx.clone()])];
        let new = vec![make_simple_table("cf_t", vec![], vec![design_idx.clone()])];
        let changes = DdlDiff::diff(&old, &new);
        assert_eq!(changes.len(), 1, "应产出 1 个 AlterTable");
        if let TableChange::AlterTable { index_changes, .. } = &changes[0] {
            assert_eq!(index_changes.len(), 2, "应报 AddIndex + PreservedManualIndex: {index_changes:?}");
            assert!(
                index_changes
                    .iter()
                    .any(|c| matches!(c, IndexChange::AddIndex(i) if i.name == "uk_cf_t_1")),
                "应有 AddIndex(设计期名): {index_changes:?}"
            );
            // PG 自动命名（_key）不在系统前缀与定义名中 → 手工保护，不产生 DROP
            assert!(
                index_changes
                    .iter()
                    .any(|c| matches!(c, IndexChange::PreservedManualIndex(i) if i.name == "cf_t_code_key")),
                "PG 自动命名旧索引应被保护保留: {index_changes:?}"
            );
        } else {
            panic!("Expected AlterTable");
        }
    }

    /// 系统命名（uk_/idx_ 前缀）的旧索引列真变更 → DROP + Add 正常重建清理。
    #[test]
    fn diff_indexes_system_named_rebuild_drops_old() {
        let db_idx = IndexDefine {
            name: "uk_cf_t_9".to_string(), // 系统前缀 → 允许自动清理
            columns: vec!["code".to_string()],
            kind: IndexKind::Unique,
            valid: true,
        };
        let design_idx = IndexDefine {
            name: "uk_cf_t_1".to_string(),
            columns: vec!["name".to_string()], // 列不同
            kind: IndexKind::Unique,
            valid: true,
        };
        let old = vec![make_simple_table("cf_t", vec![], vec![db_idx])];
        let new = vec![make_simple_table("cf_t", vec![], vec![design_idx])];
        let changes = DdlDiff::diff(&old, &new);
        if let TableChange::AlterTable { index_changes, .. } = &changes[0] {
            assert_eq!(index_changes.len(), 2, "{index_changes:?}");
            assert!(index_changes.iter().any(|c| matches!(c, IndexChange::DropIndex(i) if i.name == "uk_cf_t_9")));
            assert!(index_changes.iter().any(|c| matches!(c, IndexChange::AddIndex(i) if i.name == "uk_cf_t_1")));
        } else {
            panic!("Expected AlterTable");
        }
    }

    /// INVALID 索引（valid=false，如 CREATE INDEX CONCURRENTLY 失败残留）即使与定义
    /// 内容完全相同，也应强制 DROP + CREATE 重建为有效索引——否则它占着名字且永远无效。
    #[test]
    fn diff_indexes_recreates_invalid_index() {
        let invalid_idx = IndexDefine {
            name: "idx_t_name".to_string(),
            columns: vec!["name".to_string()],
            kind: IndexKind::Normal,
            valid: false,
        };
        let design_idx = IndexDefine {
            name: "idx_t_1".to_string(),
            columns: vec!["name".to_string()], // 内容相同
            kind: IndexKind::Normal,
            valid: true,
        };
        let old = vec![make_simple_table("t", vec![], vec![invalid_idx])];
        let new = vec![make_simple_table("t", vec![], vec![design_idx])];
        let changes = DdlDiff::diff(&old, &new);
        assert_eq!(changes.len(), 1, "INVALID 索引应触发 AlterTable");
        if let TableChange::AlterTable { index_changes, .. } = &changes[0] {
            assert_eq!(index_changes.len(), 2, "应报 DropIndex + AddIndex（重建）");
            assert!(
                index_changes
                    .iter()
                    .any(|c| matches!(c, IndexChange::DropIndex(i) if i.name == "idx_t_name")),
                "应有 DropIndex(DB真实名，释放占名): {index_changes:?}"
            );
            assert!(
                index_changes
                    .iter()
                    .any(|c| matches!(c, IndexChange::AddIndex(i) if i.name == "idx_t_1")),
                "应有 AddIndex(重建为有效索引): {index_changes:?}"
            );
        } else {
            panic!("Expected AlterTable");
        }
    }

    /// INVALID 索引在定义中无对应内容时，仅产生 DropIndex（清理残留），不产生 AddIndex。
    #[test]
    fn diff_indexes_drops_invalid_index_absent_in_new() {
        let invalid_idx = IndexDefine {
            name: "idx_t_stale".to_string(),
            columns: vec!["gone".to_string()],
            kind: IndexKind::Normal,
            valid: false,
        };
        let old = vec![make_simple_table("t", vec![], vec![invalid_idx])];
        let new = vec![make_simple_table("t", vec![], vec![])];
        let changes = DdlDiff::diff(&old, &new);
        assert_eq!(changes.len(), 1);
        if let TableChange::AlterTable { index_changes, .. } = &changes[0] {
            assert_eq!(index_changes.len(), 1, "应仅报 DropIndex");
            assert!(matches!(&index_changes[0], IndexChange::DropIndex(i) if i.name == "idx_t_stale"));
        } else {
            panic!("Expected AlterTable");
        }
    }

    /// 手工索引保护：非系统命名前缀（uk_/idx_）且名字不在定义中的多余索引，
    /// 视为手工创建 → 保留不删（PreservedManualIndex，无 DDL），仅报告提示。
    #[test]
    fn diff_indexes_preserves_manual_index() {
        let manual = IndexDefine {
            name: "my_dba_index".to_string(), // 无系统前缀
            columns: vec!["name".to_string()],
            kind: IndexKind::Normal,
            valid: true,
        };
        let system = IndexDefine {
            name: "idx_t_old".to_string(), // 系统前缀 → 允许清理
            columns: vec!["code".to_string()],
            kind: IndexKind::Normal,
            valid: true,
        };
        let old = vec![make_simple_table("t", vec![], vec![manual.clone(), system])];
        let new = vec![make_simple_table("t", vec![], vec![])];
        let changes = DdlDiff::diff(&old, &new);
        if let TableChange::AlterTable { index_changes, .. } = &changes[0] {
            assert_eq!(index_changes.len(), 2, "{index_changes:?}");
            assert!(
                index_changes.iter().any(|c| matches!(c, IndexChange::PreservedManualIndex(i) if i.name == "my_dba_index")),
                "手工索引应保留: {index_changes:?}"
            );
            assert!(
                index_changes.iter().any(|c| matches!(c, IndexChange::DropIndex(i) if i.name == "idx_t_old")),
                "系统命名索引应正常清理: {index_changes:?}"
            );
            // 保留项不生成任何 DDL
            let dialect = PostgresDdlDialect::default();
            let ddl = DdlDiff::changes_to_ddl(&dialect, &changes).unwrap();
            assert!(
                !ddl.iter().any(|s| s.contains("my_dba_index")),
                "保留索引不得出现在 DDL: {ddl:?}"
            );
        } else {
            panic!("Expected AlterTable");
        }
    }

    /// 自定义名索引改列重建：名字仍在定义中（new 有同名条目）→ 不受手工保护拦截，
    /// 正常 Drop + Add（否则旧索引占名，CREATE 撞 already exists）。
    #[test]
    fn diff_indexes_custom_name_rebuild_not_blocked() {
        let old_idx = IndexDefine {
            name: "uk_my_custom".to_string(),
            columns: vec!["code".to_string()],
            kind: IndexKind::Unique,
            valid: true,
        };
        let new_idx = IndexDefine {
            name: "uk_my_custom".to_string(),
            columns: vec!["tax_no".to_string()], // 改列
            kind: IndexKind::Unique,
            valid: true,
        };
        let old = vec![make_simple_table("t", vec![], vec![old_idx])];
        let new = vec![make_simple_table("t", vec![], vec![new_idx])];
        let changes = DdlDiff::diff(&old, &new);
        if let TableChange::AlterTable { index_changes, .. } = &changes[0] {
            assert_eq!(index_changes.len(), 2, "同名自定义索引应 Drop+Add 重建: {index_changes:?}");
            assert!(index_changes.iter().any(|c| matches!(c, IndexChange::DropIndex(i) if i.name == "uk_my_custom")));
            assert!(!index_changes.iter().any(|c| matches!(c, IndexChange::PreservedManualIndex(_))));
        } else {
            panic!("Expected AlterTable");
        }
    }

    /// INVALID 且手工名的索引：自愈优先于手工保护（INVALID 占名必撞 CREATE），仍 DROP。
    #[test]
    fn diff_indexes_invalid_manual_index_still_dropped() {
        let invalid_manual = IndexDefine {
            name: "my_dba_broken".to_string(),
            columns: vec!["code".to_string()],
            kind: IndexKind::Normal,
            valid: false,
        };
        let old = vec![make_simple_table("t", vec![], vec![invalid_manual])];
        let new = vec![make_simple_table("t", vec![], vec![])];
        let changes = DdlDiff::diff(&old, &new);
        if let TableChange::AlterTable { index_changes, .. } = &changes[0] {
            assert_eq!(index_changes.len(), 1);
            assert!(matches!(&index_changes[0], IndexChange::DropIndex(i) if i.name == "my_dba_broken"));
        } else {
            panic!("Expected AlterTable");
        }
    }

    /// 默认值「编译形态 vs 内省形态」语义等价：不判变更（消除每次部署重复
    /// SET DEFAULT 的永久假阳性）；真差异仍应判变更。
    #[test]
    fn column_changed_default_form_equivalence() {
        // 同一列的两种形态：内省（去 cast 去引号/小写/jsonb 带空格）vs 编译（定界/大写/紧凑）
        let equiv_pairs = [
            ("active", "'active'"),                       // VARCHAR：内省已剥 cast+引号
            ("'active'", "'active'"),                     // 内省未剥引号形态（无 cast）
            ("true", "TRUE"),                             // bool 大小写
            ("1", "1"),                                   // 整数裸字面量
            ("{\"a\": 1}", "'{\"a\":1}'"),                // jsonb 空格差异（语义比较）
            ("now()", "now()"),                           // 表达式透传
        ];
        for (old_v, new_v) in equiv_pairs {
            let mut old_col = make_col("status", FieldType::String, true);
            old_col.default_value = Some(old_v.to_string());
            let mut new_col = make_col("status", FieldType::String, true);
            new_col.default_value = Some(new_v.to_string());
            assert!(
                !DdlDiff::column_changed(&old_col, &new_col),
                "等价形态不应判变更: {old_v} vs {new_v}"
            );
        }

        // 真差异：值不同必须判变更
        let mut old_col = make_col("status", FieldType::String, true);
        old_col.default_value = Some("a".to_string());
        let mut new_col = make_col("status", FieldType::String, true);
        new_col.default_value = Some("b".to_string());
        assert!(DdlDiff::column_changed(&old_col, &new_col), "值不同应判变更");

        // None vs 空串等价（内省 nextval/NULL 清洗为空串，编译侧无默认值为 None）
        let mut col_empty = make_col("id", FieldType::Int, false);
        col_empty.default_value = Some(String::new());
        let col_none = make_col("id", FieldType::Int, false);
        assert!(
            !DdlDiff::column_changed(&col_empty, &col_none),
            "None 与空串都表示无默认值"
        );

        // None vs 有值：真差异
        let col_some = {
            let mut c = make_col("id", FieldType::Int, false);
            c.default_value = Some("1".to_string());
            c
        };
        assert!(DdlDiff::column_changed(&col_none, &col_some), "无默认值 vs 有默认值应判变更");
    }

    /// 列注释（label）变更：结构相同但 label 不同，应产出 AlterTable 且
    /// column_comment_changes 非空、column_changes 为空（不触发结构变更）。
    #[test]
    fn diff_detects_column_comment_change() {
        // old：DB 还原，label=""（DB 缺注释）；new：设计期 label="字典项主键"。
        let mut old_col = make_col("id", FieldType::Int, false);
        old_col.label = String::new();
        let mut new_col = make_col("id", FieldType::Int, false);
        new_col.label = "字典项主键".to_string();
        let old = vec![make_simple_table("t", vec![old_col], vec![])];
        let new = vec![make_simple_table("t", vec![new_col], vec![])];
        let changes = DdlDiff::diff(&old, &new);
        assert_eq!(changes.len(), 1, "列注释差异应触发 AlterTable");
        if let TableChange::AlterTable {
            column_changes,
            column_comment_changes,
            ..
        } = &changes[0]
        {
            assert!(
                column_changes.is_empty(),
                "结构相同，column_changes 应为空: {column_changes:?}"
            );
            assert_eq!(column_comment_changes.len(), 1, "应有 1 条列注释变更");
            assert_eq!(column_comment_changes[0].column, "id");
            assert_eq!(column_comment_changes[0].old_label, "");
            assert_eq!(column_comment_changes[0].new_label, "字典项主键");
        } else {
            panic!("Expected AlterTable");
        }
        // changes_to_ddl 应生成 COMMENT ON COLUMN（非 ALTER COLUMN TYPE）
        let dialect = PostgresDdlDialect::default();
        let stmts = DdlDiff::changes_to_ddl(&dialect, &changes).unwrap();
        assert!(
            stmts
                .iter()
                .any(|s| s.contains("COMMENT ON COLUMN") && s.contains("字典项主键")),
            "应生成 COMMENT ON COLUMN 写入新注释: {stmts:?}"
        );
        assert!(
            stmts.iter().all(|s| !s.contains("ALTER COLUMN")),
            "结构无变化，不应生成 ALTER COLUMN: {stmts:?}"
        );
    }

    /// label 相同时不应报列注释变更（避免假阳性）。
    #[test]
    fn diff_no_column_comment_change_when_label_equal() {
        let col = make_col("id", FieldType::Int, false);
        let tables = vec![make_simple_table("t", vec![col], vec![])];
        let changes = DdlDiff::diff(&tables, &tables);
        assert!(changes.is_empty(), "label 相同应无任何变更");
    }

    /// DB 多出列 + 设计期新增列（真实变更）：应产出 AlterTable。
    /// 验证 DropColumn 不影响判定，但真实 AddColumn 仍能触发升级。
    #[test]
    fn diff_db_extra_column_plus_real_add_still_reports() {
        let old = vec![make_simple_table(
            "t",
            vec![
                make_col("id", FieldType::Int, false),
                make_col("extra_col", FieldType::String, true), // DB 多出
            ],
            vec![],
        )];
        let new = vec![make_simple_table(
            "t",
            vec![
                make_col("id", FieldType::Int, false),
                make_col("name", FieldType::String, true), // 设计期新增（真实变更）
            ],
            vec![],
        )];
        let changes = DdlDiff::diff(&old, &new);
        assert_eq!(changes.len(), 1, "有真实新增列应产出 AlterTable");
        if let TableChange::AlterTable { column_changes, .. } = &changes[0] {
            // 应含 AddColumn(name)（真实变更），也可含 DropColumn(extra_col)（不参与判定但保留）
            assert!(
                column_changes
                    .iter()
                    .any(|c| matches!(c, ColumnChange::AddColumn(c) if c.name == "name")),
                "应有 AddColumn(name): {column_changes:?}"
            );
        } else {
            panic!("Expected AlterTable");
        }
    }

    /// 索引 DDL 顺序：改列集合（或撞名）场景，DROP 必须先于 CREATE——
    /// 否则 CREATE 与 DB 侧同名索引（DBA 手工建 / CONCURRENTLY 失败的 INVALID 残留 /
    /// 改列前的旧索引）撞名报 already exists，排后面的 DROP 本可解冲突却没机会执行。
    #[test]
    fn index_ddl_drop_before_create() {
        let idx = |name: &str, col: &str| IndexDefine {
            name: name.to_string(),
            columns: vec![col.to_string()],
            kind: IndexKind::Normal,
            valid: true,
        };
        let cols = || {
            vec![
                make_col("a", FieldType::String, true),
                make_col("b", FieldType::String, true),
            ]
        };
        // DB 现状：idx_t_1 on (a)；设计期：同名 idx_t_1 on (b)——内容不匹配 → DropIndex + AddIndex
        let old = vec![make_simple_table("t", cols(), vec![idx("idx_t_1", "a")])];
        let new = vec![make_simple_table("t", cols(), vec![idx("idx_t_1", "b")])];
        let changes = DdlDiff::diff(&old, &new);
        let stmts = DdlDiff::changes_to_ddl(&PostgresDdlDialect::default(), &changes).unwrap();
        let drop_pos = stmts.iter().position(|s| s.starts_with("DROP INDEX"));
        let create_pos = stmts.iter().position(|s| s.starts_with("CREATE INDEX"));
        assert!(drop_pos.is_some(), "应有 DROP INDEX: {stmts:?}");
        assert!(create_pos.is_some(), "应有 CREATE INDEX: {stmts:?}");
        assert!(
            drop_pos.unwrap() < create_pos.unwrap(),
            "DROP 必须先于 CREATE（撞名防护）: {stmts:?}"
        );
        // DROP 用 DB 真实名（old 侧还原的 idx_t_1）
        assert!(stmts[drop_pos.unwrap()].contains("idx_t_1"));
    }

    /// 新索引引用本次新增列：CREATE INDEX 应出现在 ADD COLUMN 之后（列变更先行）。
    #[test]
    fn index_create_after_add_column() {
        let old = vec![make_simple_table(
            "t",
            vec![make_col("a", FieldType::String, true)],
            vec![],
        )];
        let new = vec![make_simple_table(
            "t",
            vec![
                make_col("a", FieldType::String, true),
                make_col("b", FieldType::String, true),
            ],
            vec![IndexDefine {
                name: "idx_t_b".to_string(),
                columns: vec!["b".to_string()],
                kind: IndexKind::Normal,
                valid: true,
            }],
        )];
        let changes = DdlDiff::diff(&old, &new);
        let stmts = DdlDiff::changes_to_ddl(&PostgresDdlDialect::default(), &changes).unwrap();
        let add_pos = stmts.iter().position(|s| s.contains("ADD COLUMN"));
        let create_pos = stmts.iter().position(|s| s.starts_with("CREATE INDEX"));
        assert!(add_pos.is_some(), "应有 ADD COLUMN: {stmts:?}");
        assert!(create_pos.is_some(), "应有 CREATE INDEX: {stmts:?}");
        assert!(
            add_pos.unwrap() < create_pos.unwrap(),
            "CREATE INDEX 必须在 ADD COLUMN 之后（新索引引用新列）: {stmts:?}"
        );
    }
}
