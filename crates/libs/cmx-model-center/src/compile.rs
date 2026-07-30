//! DCT/DOC/RPT 定义 JSON → cmx-core TableDefine 编译器。
//!
//! 从 lib.rs 拆出：原"一、编译器"段 + "二、内省辅助"段。

use cmx_core::model::cell::{ColumnDefine, FieldType, IndexDefine, IndexKind, TableDefine};
use serde_json::{Value, json};
use tracing::warn;

use cmx_api_types::{Error, Result};

use crate::{db_err, VARCHAR_DEFAULT_LENGTH};

// ════════════════════════════════════════════════════════════════════════
//  一、编译器：DCT/DOC 定义 JSON → cmx-core TableDefine
// ════════════════════════════════════════════════════════════════════════

/// DCT dataType 词元 → FieldType（大小写不敏感）。
///
/// 兼容前端/迁移脚本中常见的大小写、缩写、领域别名（`CHAR`/`STRING`/`TEXT`/`CLOB`/`NUMBER`
/// 等）。未识别的词元默认走 `String`，避免一处拼写错误炸掉整个模块编译（行为同原实现）。
pub(crate) fn map_field_type(data_type: &str) -> FieldType {
    match data_type.to_ascii_uppercase().as_str() {
        // 字符串家族：所有 VARCHAR 变体都映射为 String，长度在调用方按 VARCHAR_DEFAULT_LENGTH 兜底
        "VARCHAR" | "CHAR" | "STRING" => FieldType::String,
        // 长文本：TEXT/CLOB 通常不设 length
        "TEXT" | "CLOB" => FieldType::Text,
        // 整数家族：PG 内部按是否需 > 2^31 自动选 int/bigint，但 cmx-core 只暴露 Int
        "INT" | "INTEGER" | "BIGINT" | "TINYINT" | "SMALLINT" | "LONG" => FieldType::Int,
        // 精度数：精度/标度在调用方通过 (fieldLength, decimalDigits) 注入
        "DECIMAL" | "NUMERIC" | "NUMBER" => FieldType::Decimal,
        // 浮点（前端不常见，但兼容历史定义）
        "FLOAT" | "DOUBLE" | "REAL" => FieldType::Float,
        // 日期
        "DATE" => FieldType::Date,
        // 时间戳（带时区与否由 PG 端 DDL 决定，cmx-core 不区分）
        "DATETIME" | "TIMESTAMP" => FieldType::DateTime,
        // 布尔
        "BOOL" | "BOOLEAN" => FieldType::Bool,
        // JSON 家族：JSON/JSONB 在 PG 端行为有差异，但 cmx-core 不区分
        "JSON" | "JSONB" => FieldType::Json,
        // UUID
        "UUID" => FieldType::Uuid,
        // 二进制
        "BINARY" | "BLOB" | "BYTEA" => FieldType::Binary,
        // 兜底：未知词元走 String（与原行为一致，避免一处拼写错误炸掉整个模块）
        _ => FieldType::String,
    }
}

/// 取字段中文标题（caption.zh_CN / caption 字符串 / 空）。
///
/// 优先级：`caption.zh_CN` > `caption.en` > `caption` 字符串。caption 缺失或为非字符串/非对象
/// 形态（如 `null`/数字）一律回退空串（与原行为一致），不报错。
fn field_caption(f: &Value) -> String {
    match f.get("caption") {
        // 形态 1：caption 是对象（i18n），优先 zh_CN，其次 en
        Some(Value::Object(o)) => o
            .get("zh_CN")
            .or_else(|| o.get("en"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        // 形态 2：caption 是字符串（简写）
        Some(Value::String(s)) => s.clone(),
        // 形态 3：缺失 / null / 数字等 → 空串
        _ => String::new(),
    }
}

/// 表/汇总表标题（caption.zh_CN / caption 字符串 / tableAlias / name）。
///
/// 优先级：`tableAlias` > `caption`（同 field_caption）> `name`。三者全缺时回退到 `fallback`
/// （通常是 `tableName` 短 id，保证总返回非空）。
fn table_caption(t: &Value, fallback: &str) -> String {
    // 1) tableAlias 优先（部分表用它表示"友好名"）
    t.get("tableAlias")
        .and_then(|v| v.as_str())
        // 2) caption：i18n 对象或字符串都接受
        .or_else(|| match t.get("caption") {
            Some(Value::Object(o)) => o
                .get("zh_CN")
                .or_else(|| o.get("en"))
                .and_then(|v| v.as_str()),
            Some(Value::String(s)) => Some(s.as_str()),
            _ => None,
        })
        // 3) name（部分定义把它当显示名用）
        .or_else(|| t.get("name").and_then(|v| v.as_str()))
        // 4) 兜底
        .unwrap_or(fallback)
        .to_string()
}

/// 单个字段对象 → ColumnDefine。id_field 命中则标记主键。
///
/// 字段缺失/类型不匹配返回 `None`（由调用方按需跳过），不做失败中断：
/// - `name` 缺失或空串 → `None`（必须）
/// - `dataType` 缺失 → 默认 `VARCHAR`（与原行为一致，宽松容错）
/// - `nullable` 缺失 → 默认 `true`
/// - `fieldLength` / `decimalDigits` 缺失 → 走 VARCHAR 兜底 255 / Decimal 标度 0
///
/// 主键判定三路满足其一即视为 PK：
/// 1. `id_field` 非空且与字段名相等（约定式 PK）
/// 2. `isPrimaryKey` 是非 0 整数（部分老定义形态）
/// 3. `isPrimaryKey` 是 `true` 布尔
fn field_to_column(f: &Value, id_field: &str, ordinal: u32) -> Option<ColumnDefine> {
    // name 是必须项：缺失 / 空串 / 非字符串都视为"非法字段"，由调用方决定是否继续
    let name = f.get("name").and_then(|v| v.as_str())?.to_string();
    if name.is_empty() {
        return None;
    }
    // dataType 默认 VARCHAR（宽松容错：缺省视为短文本，不阻断编译）
    let data_type = f
        .get("dataType")
        .and_then(|v| v.as_str())
        .unwrap_or("VARCHAR");
    let ft = map_field_type(data_type);
    // nullable 默认 true（与 PG DDL 默认一致，简化前端定义）
    let nullable = f.get("nullable").and_then(|v| v.as_bool()).unwrap_or(true);
    // fieldLength：VARCHAR 用 length；DECIMAL 用 precision
    let field_len = f
        .get("fieldLength")
        .and_then(|v| v.as_u64())
        .map(|n| n as u32);
    // decimalDigits：DECIMAL 用 scale
    let dec = f
        .get("decimalDigits")
        .and_then(|v| v.as_u64())
        .map(|n| n as u32);

    // 主键判定（三路满足任一即视为 PK）：
    // 1) 显式 id_field 约定
    // 2) isPrimaryKey 是非 0 整数（兼容老形态）
    // 3) isPrimaryKey 是 true 布尔（现代形态）
    let is_pk = (!id_field.is_empty() && name == id_field)
        || f.get("isPrimaryKey")
            .and_then(|v| v.as_i64())
            .map(|n| n != 0)
            .unwrap_or(false)
        || f.get("isPrimaryKey")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

    // 长度 / 精度：VARCHAR 用 length；DECIMAL 用 precision(=fieldLength)+scale(=decimalDigits)。
    // VARCHAR 未指定 fieldLength 时默认 255（避免被建表逻辑当成 TEXT，导致与期望不一致时无法 ALTER 修正）。
    let (length, precision, scale) = match ft {
        FieldType::String => (
            Some(field_len.unwrap_or(VARCHAR_DEFAULT_LENGTH)),
            None,
            None,
        ),
        FieldType::Decimal => (None, field_len, dec.or(Some(0))),
        _ => (None, None, None),
    };

    Some(ColumnDefine {
        name,
        label: field_caption(f),
        field_type: ft,
        is_primary_key: is_pk,
        // PK 列强制 NOT NULL（业务约束：PK 不能为 NULL）
        is_nullable: if is_pk { false } else { nullable },
        default_value: None,
        i18n: false,
        length,
        precision,
        scale,
        db_type: None,
        ordinal: Some(ordinal),
        create_time: None,
        update_time: None,
        is_foreign_key: false,
        foreign_key_table: None,
        foreign_key_column: None,
        extensions: Default::default(),
    })
}

/// uniqueKeys = [[col,...], ...] → 唯一索引定义。
///
/// 定义文件中的 `uniqueKeys` 是数组的数组（每个子数组 = 一组联合唯一约束），本函数
/// 将其扁平化为 `IndexDefine` 列表。索引名按 `uk_<table>_<i+1>` 规则生成（`i` 是子数组下标），
/// 这样多次编译同一文件可得到稳定的索引名（PG 端便于识别 + 升级时对齐）。
///
/// 空数组 / 非数组元素会被跳过（容错）。
fn unique_indexes(t: &Value, table_name: &str) -> Vec<IndexDefine> {
    let mut indexes = Vec::new();
    if let Some(uks) = t.get("uniqueKeys").and_then(|v| v.as_array()) {
        for (i, uk) in uks.iter().enumerate() {
            if let Some(cols) = uk.as_array() {
                // 提取列名（非字符串元素自动跳过）
                let cnames: Vec<String> = cols
                    .iter()
                    .filter_map(|c| c.as_str().map(|s| s.to_string()))
                    .collect();
                // 列名全空时跳过（避免建空唯一约束）
                if !cnames.is_empty() {
                    indexes.push(IndexDefine {
                        name: format!("uk_{}_{}", table_name, i + 1),
                        columns: cnames,
                        kind: IndexKind::Unique,
                    });
                }
            }
        }
    }
    indexes
}

/// 从 base fieldSets 对象里取某字段集的 fields 数组。
///
/// 返回 `Some(&Vec<Value>)` 表示找到，`None` 表示 key 不存在或结构不匹配（容错）：
/// - `fieldSets` 缺失 / 非对象 → `None`
/// - `fieldSets[set_name]` 缺失 / 非对象 → `None`
/// - `fields` 缺失 / 非数组 → `None`
///
/// 借用返回，零拷贝。
fn base_fieldset<'a>(base: &'a Value, set_name: &str) -> Option<&'a Vec<Value>> {
    base.get("fieldSets")?
        .get(set_name)?
        .get("fields")?
        .as_array()
}

/// 将一组字段 JSON 数组追加到 columns（去重 + 自增 ordinal）。
///
/// 内部循环：每个字段尝试 `field_to_column` 转换，转换成功且列名未在 `seen` 中才追加。
/// 序号 `ord` 每次成功转换后自增（保证 ColumnDefine.ordinal 与"列在表中的位置"对齐）。
///
/// 设计要点：
/// - 去重：保证同名列不重复加入（覆盖逻辑 = 第一个出现的胜出，与原行为一致）
/// - 序号连续：哪怕中途字段被跳过，ord 仍会按"成功转换"次数自增，与"列在表中的视觉位置"对齐
fn push_field_set(
    fields: &[Value],
    id_field: &str,
    columns: &mut Vec<ColumnDefine>,
    seen: &mut std::collections::HashSet<String>,
    ord: &mut u32,
) {
    for f in fields {
        // 序号在尝试转换前 +1（保证即使跳过非法字段也不会让后续列的 ordinal 回退）
        *ord += 1;
        if let Some(c) = field_to_column(f, id_field, *ord)
            // seen.insert 返回 true 表示新插入（未重复），false 表示已存在（跳过）
            && seen.insert(c.name.clone())
        {
            columns.push(c);
        }
    }
}

/// 从已收集的 columns 构造 TableDefine（统一 15 字段初始化）。
///
/// 收敛"构造 TableDefine 的 15+ 个字段都必须显式写"的负担，所有调用方（`compile_dct` /
/// `compile_doc` / `compile_rpt`）共用同一构造入口，缺省字段在此统一填默认值，避免散落
/// 写多份带来的字段集漂移。
///
/// 参数：
/// - `table_name`：物理表名（PG 端 identifier）
/// - `display_name`：显示名（前端友好）
/// - `comment`：表注释（对应 PG `COMMENT ON TABLE`）
/// - `primary_keys`：主键列名列表（多列联合 PK 也支持）
/// - `indexes`：唯一索引列表
/// - `columns`：列定义列表
fn finish_table(
    table_name: String,
    display_name: String,
    comment: Option<String>,
    primary_keys: Vec<String>,
    indexes: Vec<IndexDefine>,
    columns: Vec<ColumnDefine>,
) -> TableDefine {
    TableDefine {
        table_name,
        display_name,
        columns,
        primary_keys,
        indexes,
        version: 1,
        create_time: None,
        update_time: None,
        i18n: false,
        comment,
        schema: None,
        tablespace: None,
        is_partitioned: false,
        partition_type: None,
        partition_columns: vec![],
        extensions: Default::default(),
    }
}

/// DCT 定义 doc + 其 base fieldset doc → Vec<TableDefine>（每个 dictionaryTable 一张表）。
///
/// # 编译流程
///
/// 遍历 `dictionaryTables[]`，对每张表：
/// 1. 抽 `dictMeta.{tableName, idField, dictName, remark}` → 物理表名 / 主键约定 / 显示名 / 表注释
/// 2. 合并字段来源（本表 fields + 7 个内建字段集 + 任意 *FieldSet 兜底），按列名去重
/// 3. 收集 `is_primary_key=true` 的列 → `primary_keys`
/// 4. 抽 `uniqueKeys` → 唯一索引
/// 5. 走 `finish_table` 统一构造 TableDefine
///
/// # 字段集合并顺序（保证列序稳定）
///
/// 本表 fields → baseFieldSet → hierarchyFieldSet → scopeFieldSet → effectiveFieldSet →
/// disableFieldSet → auditFieldSet → systemFieldSet → 任意其它 *FieldSet 兜底。
/// `hierarchyFieldSet` 提供自分级字典的 `parent_id` / `full_path` / `level_no` / `is_leaf`。
///
/// # 容错
///
/// - `tableName` 缺失 / 空串 → 跳过该表
/// - `fields` 缺失 / 非数组 → 视为空数组
/// - 任意 `*FieldSet` 引用值缺失 / 非字符串 / 在 base 中查不到 → 静默跳过
/// - 单字段 `field_to_column` 返回 `None` → 跳过该字段
pub(crate) fn compile_dct(doc: &Value, base: &Value) -> Vec<TableDefine> {
    let tables = match doc.get("dictionaryTables").and_then(|v| v.as_array()) {
        Some(t) => t,
        None => return vec![],
    };
    let mut out = Vec::new();
    for t in tables {
        // 抽 dictMeta；缺失时用空对象兜底（dictMeta 是必填但前端偶有省略）
        let dm = t.get("dictMeta").cloned().unwrap_or(json!({}));
        let table_name = dm
            .get("tableName")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        // tableName 缺失直接跳过该表（无 tableName 无法建表）
        if table_name.is_empty() {
            continue;
        }
        // idField 约定主键（与 isPrimaryKey 显式标注并行生效）
        let id_field = dm.get("idField").and_then(|v| v.as_str()).unwrap_or("id");
        // 显示名：dictName 缺省回退到 table_name（保证总非空）
        let display = dm
            .get("dictName")
            .and_then(|v| v.as_str())
            .unwrap_or(&table_name)
            .to_string();
        // 表注释：对应 PG COMMENT ON TABLE
        let comment = dm
            .get("remark")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // 合并字段来源：本表 + 三个引用字段集（跳过 null / 缺失）。
        let mut columns: Vec<ColumnDefine> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut ord: u32 = 0;
        // 1) 本表自有字段（最优先：定义中"我这张表有什么"）
        if let Some(own) = t.get("fields").and_then(|v| v.as_array()) {
            push_field_set(own, id_field, &mut columns, &mut seen, &mut ord);
        }
        // 2) 合并全部 *FieldSet 引用（base/hierarchy/audit/effective/disable/scope/system…）。
        //    固定顺序保证列序稳定；hierarchyFieldSet 提供自分级字典的 parent_id/full_path/level_no/is_leaf。
        for set_key in [
            "baseFieldSet",
            "hierarchyFieldSet",
            "scopeFieldSet",
            "effectiveFieldSet",
            "disableFieldSet",
            "auditFieldSet",
            "systemFieldSet",
        ] {
            if let Some(set_name) = t.get(set_key).and_then(|v| v.as_str())
                && let Some(fields) = base_fieldset(base, set_name)
            {
                push_field_set(fields, id_field, &mut columns, &mut seen, &mut ord);
            }
        }
        // 3) 兜底：捕获上面未列出的任何 `*FieldSet` 键（前向兼容新增字段集）。
        if let Some(obj) = t.as_object() {
            for (k, v) in obj {
                if k.ends_with("FieldSet")
                    && !matches!(
                        k.as_str(),
                        "baseFieldSet"
                            | "hierarchyFieldSet"
                            | "scopeFieldSet"
                            | "effectiveFieldSet"
                            | "disableFieldSet"
                            | "auditFieldSet"
                            | "systemFieldSet"
                    )
                    && let Some(set_name) = v.as_str()
                    && let Some(fields) = base_fieldset(base, set_name)
                {
                    push_field_set(fields, id_field, &mut columns, &mut seen, &mut ord);
                }
            }
        }

        // 收集主键：扫描所有列，挑出 is_primary_key=true 的（兼容多列联合 PK）
        let primary_keys: Vec<String> = columns
            .iter()
            .filter(|c| c.is_primary_key)
            .map(|c| c.name.clone())
            .collect();

        let indexes = unique_indexes(t, &table_name);
        out.push(finish_table(
            table_name,
            display,
            comment,
            primary_keys,
            indexes,
            columns,
        ));
    }
    out
}

/// 编译一张 DOC 表（或一张 sum/summary 表）→ TableDefine。
///
/// DOC 与 DCT 编译路径共用同套 `field_to_column` / `finish_table` 工具，但字段合并规则不同：
/// - 字段来源：仅本表 `fields` + `documentFieldSets[]` 引用的 base 字段集
/// - 不抽 `dictMeta`（DOC 用顶层 `tableName` / `name` / `id`）
/// - DOC 主键约定为 `id`（与 DCT 不同），sum/summaries 表也沿用
///
/// # 三段 fallback
///
/// DOC 表名可能写在 `tableName` / `name` / `id` 三个字段任一处（不同模块风格不同）：
/// 优先 `tableName` > `name` > `id`。三者全缺 / 全空 → `None`（调用方跳过该表）。
fn compile_doc_table(t: &Value, base: &Value) -> Option<TableDefine> {
    // DOC 表名 fallback 链：tableName → name → id（部分老定义用 id 作表名）
    let table_name = t
        .get("tableName")
        .or_else(|| t.get("name"))
        .or_else(|| t.get("id"))
        .and_then(|v| v.as_str())?
        .to_string();
    if table_name.is_empty() {
        return None;
    }
    // 显示名：tableAlias > caption > name > table_name（fallback）
    let display = table_caption(t, &table_name);
    // 表注释：对应 PG COMMENT ON TABLE
    let comment = t
        .get("remark")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let mut columns: Vec<ColumnDefine> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut ord: u32 = 0;
    // DOC 主键约定：id（若存在）。sum/summaries 表也沿用该约定。
    let id_field = "id";
    // 本表 fields
    if let Some(own) = t.get("fields").and_then(|v| v.as_array()) {
        push_field_set(own, id_field, &mut columns, &mut seen, &mut ord);
    }
    // documentFieldSets: [ "voucherCommonFields", ... ] 引用 base。汇总表通常不配，
    // 但保留同样展开能力，便于后续把通用审计字段抽到 base。
    if let Some(sets) = t.get("documentFieldSets").and_then(|v| v.as_array()) {
        for s in sets {
            if let Some(set_name) = s.as_str()
                && let Some(fields) = base_fieldset(base, set_name)
            {
                push_field_set(fields, id_field, &mut columns, &mut seen, &mut ord);
            }
        }
    }

    // 收集主键（同 DCT：扫描所有列）
    let primary_keys: Vec<String> = columns
        .iter()
        .filter(|c| c.is_primary_key)
        .map(|c| c.name.clone())
        .collect();

    Some(finish_table(
        table_name.clone(),
        display,
        comment,
        primary_keys,
        unique_indexes(t, &table_name),
        columns,
    ))
}

/// DOC 定义 doc → Vec<TableDefine>（每个 voucherTable 一张表）。
///
/// DOC 列 = 本表 fields + documentFieldSets 引用的 base 字段集；每层表下的
/// summaries/sum 汇总表也按同一 TableDefine 链路编译，以复用创建与升级执行器。
///
/// # 编译顺序
///
/// 1. 遍历 `voucherTables[]`，对每张主表走 `compile_doc_table`
/// 2. 对每张主表的 `summaries[]` 与 `sum[]` 走 `compile_doc_table`
/// 3. 用 `seen_tables` 去重（防御性：极少数定义会把主表/汇总表命名重复）
pub(crate) fn compile_doc(doc: &Value, base: &Value) -> Vec<TableDefine> {
    let tables = match doc.get("voucherTables").and_then(|v| v.as_array()) {
        Some(t) => t,
        None => return vec![],
    };
    let mut out = Vec::new();
    // seen_tables：防御性去重（极少数定义可能在主表/汇总表间出现同名）
    let mut seen_tables: std::collections::HashSet<String> = std::collections::HashSet::new();
    for t in tables {
        // 1) 主表自身
        if let Some(def) = compile_doc_table(t, base)
            && seen_tables.insert(def.table_name.clone())
        {
            out.push(def);
        }
        // 2) 该主表下的汇总表（summaries + sum，两种命名都支持）
        for key in ["summaries", "sum"] {
            if let Some(summaries) = t.get(key).and_then(|v| v.as_array()) {
                for summary in summaries {
                    if let Some(def) = compile_doc_table(summary, base)
                        && seen_tables.insert(def.table_name.clone())
                    {
                        out.push(def);
                    }
                }
            }
        }
    }
    out
}

/// RPT 报表定义 → Vec<TableDefine>。报表落地的三张 cr_* 物理表
/// （cr_report_instance / cr_cell_value / cr_report_snapshot）是全部报表模板共享的
/// 基础设施，其表结构声明在 base_rpt_meta 的 storageTables 中，一次建出、幂等升级。
/// 报表模板本身（grid/cells/datasets）是运行期概念，不产生 DDL。
/// 列 = storageTables[i].fields + 其 auditFieldSet 引用的 base 字段集，复用 field_to_column。
///
/// # 与 DCT/DOC 的差异
///
/// - 入口是 `base`（不是 `doc`）：所有报表都共用同一组 storageTables，与模板正交
/// - 字段合并：本表 fields + 任意 `*FieldSet`（前向兼容，目前主要 auditFieldSet）
/// - 报表模板本身（`_doc`）不参与编译，参数名加下划线表示"故意不用"
pub(crate) fn compile_rpt(_doc: &Value, base: &Value) -> Vec<TableDefine> {
    let tables = match base.get("storageTables").and_then(|v| v.as_array()) {
        Some(t) => t,
        None => return vec![],
    };
    let mut out = Vec::new();
    // 防御性去重：base 自身的 storageTables 理论上无重复，但保证一次建表清单唯一
    let mut seen_tables: std::collections::HashSet<String> = std::collections::HashSet::new();
    for t in tables {
        let table_name = t
            .get("tableName")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        // 跳过空名 / 重复名
        if table_name.is_empty() || !seen_tables.insert(table_name.clone()) {
            continue;
        }
        // idField 默认 "id"（与 DCT 一致）
        let id_field = t.get("idField").and_then(|v| v.as_str()).unwrap_or("id");
        // 显示名：displayName 缺省回退到 table_name
        let display = t
            .get("displayName")
            .and_then(|v| v.as_str())
            .unwrap_or(&table_name)
            .to_string();
        // 表注释
        let comment = t
            .get("remark")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let mut columns: Vec<ColumnDefine> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut ord: u32 = 0;
        // 本表 fields
        if let Some(own) = t.get("fields").and_then(|v| v.as_array()) {
            push_field_set(own, id_field, &mut columns, &mut seen, &mut ord);
        }
        // 合并全部 *FieldSet 引用（当前仅 auditFieldSet；前向兼容任意 *FieldSet 键）。
        if let Some(obj) = t.as_object() {
            for (k, v) in obj {
                if k.ends_with("FieldSet")
                    && let Some(set_name) = v.as_str()
                    && let Some(fields) = base_fieldset(base, set_name)
                {
                    push_field_set(fields, id_field, &mut columns, &mut seen, &mut ord);
                }
            }
        }

        // 收集主键
        let primary_keys: Vec<String> = columns
            .iter()
            .filter(|c| c.is_primary_key)
            .map(|c| c.name.clone())
            .collect();

        let indexes = unique_indexes(t, &table_name);
        out.push(finish_table(
            table_name,
            display,
            comment,
            primary_keys,
            indexes,
            columns,
        ));
    }
    out
}

// ════════════════════════════════════════════════════════════════════════
//  二、内省辅助：读定义文件（复用 definitions store）
// ════════════════════════════════════════════════════════════════════════

/// 读某定义文件全文（domain/application/module/file）。
///
/// 走 `cmx_model::definitions::store::get_definition`：先在内存缓存查，未命中再走文件系统
/// （`data/meta/definitions/<domain>/<app>/<module>/<file>`），最终反序列化为 `Value`。
///
/// 错误以 `Error::BadRequest` 抛出（含 file 名便于排查）。
async fn read_def(domain: &str, app: &str, module: &str, file: &str) -> Result<Value> {
    let r = cmx_model::definitions::store::DefRef {
        domain: Some(domain.to_string()),
        application: Some(app.to_string()),
        // app 别名（与 application 同义；store 同时支持两种 key）
        app: Some(app.to_string()),
        module: Some(module.to_string()),
        file: Some(file.to_string()),
        // id / kind 不参与本次查找
        id: None,
        kind: None,
    };
    cmx_model::definitions::store::get_definition(&r)
        .await
        .map_err(|e| Error::BadRequest(format!("读取定义失败 {file}: {e}")))
}

/// 读 base 字段集文件（domain=base）。
///
/// base 是与业务域并列的"基础字段集"域，所有 DCT/DOC/RPT 定义都引用它来获得通用列
/// （如 code/name/status/审计字段等）。路径恒为 `data/meta/definitions/base/<file>`。
async fn read_base(file: &str) -> Result<Value> {
    let r = cmx_model::definitions::store::DefRef {
        domain: Some("base".to_string()),
        application: None,
        app: None,
        module: None,
        file: Some(file.to_string()),
        id: None,
        kind: None,
    };
    cmx_model::definitions::store::get_definition(&r)
        .await
        .map_err(db_err("读取 base 字段集失败"))
}

/// 编译一个定义（kind=DCT/DOC/RPT）→ (TableDefine 列表, 源 JSON)。
///
/// 统一入口：根据 `kind` 选择 base 引用 key（`baseDctMetaRef` / `baseDocMetaRef` /
/// `baseRptMetaRef`）与默认 base 文件名，然后调对应编译器。
///
/// # base 引用规则
///
/// - `kind == "DOC"` → key=`baseDocMetaRef`，default=`base_doc_meta_v1.json`
/// - `kind == "RPT"` → key=`baseRptMetaRef`，default=`base_rpt_meta_v1.json`
/// - 其他（含 "DCT"） → key=`baseDctMetaRef`，default=`base_dct_meta_v1.json`
///
/// base 文件缺失时降级为空字段集（`{"fieldSets": {}}`），避免单个 base 故障阻断整个模块编译。
/// 但会打 `warn!` 日志，便于排查"定义依赖了未提供的 base"的情况。
///
/// # 返回值
///
/// - `Vec<TableDefine>`：编译出的表定义（每张表 1 个 TableDefine）
/// - `Value`：原始定义 JSON（部署路径会用它做 source_json 留档 + 取 moduleMeta.version）
pub(crate) async fn compile_definition(
    kind: &str,
    domain: &str,
    app: &str,
    module: &str,
    file: &str,
) -> Result<(Vec<TableDefine>, Value)> {
    // 1) 读定义文件全文
    let doc = read_def(domain, app, module, file).await?;
    // 2) 按 kind 选 base 引用 key + 默认 base 文件名
    let base_ref_key = match kind {
        "DOC" => "baseDocMetaRef",
        "RPT" => "baseRptMetaRef",
        _ => "baseDctMetaRef",
    };
    let default_base = match kind {
        "DOC" => "base_doc_meta_v1.json",
        "RPT" => "base_rpt_meta_v1.json",
        _ => "base_dct_meta_v1.json",
    };
    // 3) 读 base（定义里可显式覆盖默认；缺失回退默认）
    let base_file = doc
        .get(base_ref_key)
        .and_then(|r| r.get("file"))
        .and_then(|v| v.as_str())
        .unwrap_or(default_base)
        .to_string();
    // 4) base 失败时降级为空字段集（保留 warn 日志，便于排查"定义依赖了未提供的 base"）
    let base = match read_base(&base_file).await {
        Ok(v) => v,
        Err(e) => {
            // base 文件缺失时降级为空字段集（某些定义不依赖 base），但必须留日志；
            // 传输/解析错误同样降级，避免单个 base 文件故障阻断整个模块编译。
            warn!(base_file = %base_file, error = %e, "读取 base 字段集失败，降级为空字段集");
            json!({ "fieldSets": {} })
        }
    };
    // 5) 调对应编译器
    let defs = match kind {
        "DOC" => compile_doc(&doc, &base),
        "RPT" => compile_rpt(&doc, &base),
        _ => compile_dct(&doc, &base),
    };
    Ok((defs, doc))
}
