//! 字典数据批量导入导出 SQL 构造（DB-free 纯逻辑）。
//!
//! 复用 lib.rs 中已有的 `valid_col` / `to_dv_by_col` / `DictView`，
//! 与单行 `build_upsert_sql_dv` 的 backfill 策略保持一致，避免行为分叉。
//!
//! - 导出：`build_export_sql` 走 keyset 分页拉取全表（`WHERE pk > $last_pk ORDER BY pk LIMIT N`）
//! - 导入：`build_batch_insert_sql` 多行 INSERT，`BatchConflictMode` 控制冲突处理
//! - replace 模式：`build_truncate_sql` 前置 TRUNCATE

use serde_json::Value;

use cmx_core::model::cell::DataValue;

use crate::{DictView, to_dv_by_col, valid_col};

// ============================================================================
// 导出 SQL：keyset 分页拉取全表
// ============================================================================

/// 构造导出 SQL（keyset pagination）。
///
/// - `last_pk=None`：首批，`SELECT cols FROM tbl ORDER BY pk LIMIT N`
/// - `last_pk=Some(v)`：`SELECT cols FROM tbl WHERE pk > $1 ORDER BY pk LIMIT N`
///
/// 列白名单来自 `view.columns`；排序恒为 pk 升序（keyset 要求有序且唯一，pk 满足）。
/// 与 `build_search_sql` 不共用：导出无 WHERE/过滤/分页 OFFSET，语义清晰独立。
///
/// # Arguments
///
/// - `view`：字典表视图
/// - `last_pk`：上一批最后一行的 pk 值；首批传 `None`
/// - `limit`：每批行数（推荐 5000）
///
/// # Returns
///
/// 返回 `(sql, params)`。`params` 在首批为空，后续批为单元素 `Vec<DataValue>`。
pub fn build_export_sql(
    view: &DictView,
    last_pk: Option<&DataValue>,
    limit: i64,
) -> (String, Vec<DataValue>) {
    let col_list = view
        .columns
        .iter()
        .map(|c| format!("\"{}\"", c.name))
        .collect::<Vec<_>>()
        .join(", ");
    match last_pk {
        None => (
            format!(
                "SELECT {} FROM \"{}\" ORDER BY \"{}\" LIMIT {}",
                col_list, view.table_name, view.pk, limit
            ),
            Vec::new(),
        ),
        Some(_) => (
            format!(
                "SELECT {} FROM \"{}\" WHERE \"{}\" > $1 ORDER BY \"{}\" LIMIT {}",
                col_list, view.table_name, view.pk, view.pk, limit
            ),
            vec![last_pk.cloned().unwrap()],
        ),
    }
}

/// 从一行结果集 JSON 中提取 pk 的 DataValue（用于 keyset pagination 的 `last_pk`）。
///
/// 内部走 `to_dv_by_col`，保证类型与导出查询的 pk 类型一致（整型字符串 coerce）。
pub fn extract_pk(view: &DictView, row: &serde_json::Map<String, Value>) -> DataValue {
    to_dv_by_col(
        view,
        &view.pk,
        row.get(&view.pk).unwrap_or(&Value::Null),
    )
}

// ============================================================================
// 导入 SQL：TRUNCATE + 批量多行 INSERT
// ============================================================================

/// 构造 TRUNCATE SQL（仅 replace 模式用）。
///
/// 用 `RESTART IDENTITY` 重置序列；不加 `CASCADE`（字典表无外键引用，避免误伤）。
pub fn build_truncate_sql(view: &DictView) -> String {
    format!("TRUNCATE TABLE \"{}\" RESTART IDENTITY", view.table_name)
}

/// 批量写入冲突处理模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatchConflictMode {
    /// 合并：`ON CONFLICT(pk) DO UPDATE`（默认，等同现有 upsert 语义）
    Upsert,
    /// 仅新增：`ON CONFLICT(pk) DO NOTHING`（主键冲突跳过）
    InsertOnly,
    /// 替换：不加 `ON CONFLICT`（前置已 TRUNCATE 故实际不冲突）
    Replace,
}

/// 构造批量多行 INSERT。
///
/// - 列序固定为 `view.columns` 中所有列 + backfill 列（首行未提供时纳入）
/// - 服务端托管列（`create_time` / `update_time` / `sort_no` / `status` / ...）：
///   用户提供了值则尊重（迁移场景保留历史时间戳）；首行未提供时走 backfill 默认值
/// - `full_path` 缺失时用本行 `code_field` 值兜底（与单行 upsert 一致）
/// - null 用占位符 + NullTyped（按列类型推断 NULL 类型），整型列字符串数字 coerce
///
/// # Arguments
///
/// - `view`：字典表视图
/// - `rows`：待写入的行集合（每行一个 JSON 对象，键为列名）
/// - `mode`：冲突处理模式
///
/// # Returns
///
/// - `rows` 为空或无有效用户列时返回 `None`
/// - 否则返回 `(sql, params)`，配合 `execute_sql_with_datavalues` 绑定
pub fn build_batch_insert_sql(
    view: &DictView,
    rows: &[serde_json::Map<String, Value>],
    mode: BatchConflictMode,
) -> Option<(String, Vec<DataValue>)> {
    if rows.is_empty() {
        return None;
    }

    // 用户列白名单：view.columns 中所有列，按 view 顺序。
    //
    // 注意：单行 upsert 用 `is_server_managed_col` 过滤掉 create_time/update_time
    // （服务端 backfill 用 now() 强填），但批量导入是数据迁移/同步场景，用户提供的
    // create_time/update_time 应被尊重（保留历史时间戳）；未提供时由 backfill 兜底。
    // 故批量导入路径不过滤 is_server_managed_col。
    let user_cols: Vec<&str> = view
        .columns
        .iter()
        .map(|c| c.name.as_str())
        .collect();
    if user_cols.is_empty() {
        return None;
    }

    // 服务端 backfill 列（与 build_upsert_sql_dv 一致）
    //
    // **不含** `effective_from`：元数据 `required: true` 必填，必须由客户端显式提供。
    // 若列入 backfill，客户端传 null 会走参数绑定而非 CURRENT_DATE 兜底，触发数据库 NOT NULL 违反。
    let backfill: &[(&str, &str, bool)] = &[
        ("create_time", "now()", false),
        ("update_time", "now()", true),
        ("sort_no", "0", false),
        ("status", "1", false),
        ("is_system", "0", false),
        ("is_leaf", "1", false),
        ("level_no", "1", false),
    ];

    // 列来源决策（避免 SQL 列重复 + Null 兜底）：
    //
    // `user_cols` = view 所有列（含 server_managed）。批量导入场景尊重用户提供的值
    // （迁移保留历史时间戳）；用户未提供（Null）时按 backfill 表用 SQL 字面量兜底。
    //
    // 因此：
    //   - `user_cols` 已覆盖所有 view 列 → 独立的 `backfill_cols` 追加仅对 view 中
    //     不存在但 backfill 表有的列生效（实际不会有，保留作未来扩展兜底）
    //   - 每行处理 `user_cols` 时，Null + 在 backfill 表中 → 用字面量（如 now()）
    //   - ON CONFLICT 的 `on_update=true` 列（如 update_time）从 user_cols 的
    //     `EXCLUDED.x` 中排除，改由 `= now()` 单独处理（避免 SET 同列两次）
    let first = &rows[0];
    let provided: std::collections::HashSet<&str> =
        first.keys().map(|s| s.as_str()).collect();
    let backfill_cols: Vec<(&str, &str, bool)> = backfill
        .iter()
        .filter(|(name, _, _)| {
            valid_col(view, name) && !provided.contains(name) && !user_cols.contains(name)
        })
        .copied()
        .collect();

    // on_update=true 的列：Upsert 模式下 ON CONFLICT 子句用字面量单独 SET
    let on_update_cols: Vec<&str> = backfill
        .iter()
        .filter(|(_, _, on_upd)| *on_upd)
        .map(|(name, _, _)| *name)
        .collect();

    // full_path 缺失时用 code 值兜底
    let full_path_backfill =
        valid_col(view, "full_path") && !provided.contains("full_path");

    // 构造列名列表（user_cols + backfill_cols + full_path 兜底）
    let mut col_names: Vec<String> =
        user_cols.iter().map(|c| format!("\"{}\"", c)).collect();
    for (name, _, _) in &backfill_cols {
        col_names.push(format!("\"{}\"", name));
    }
    if full_path_backfill {
        col_names.push("\"full_path\"".to_string());
    }

    // 构造每行的 placeholders + 累积参数
    let mut placeholders_per_row: Vec<String> = Vec::with_capacity(rows.len());
    let mut params: Vec<DataValue> = Vec::new();
    let mut i = 0usize;
    for row in rows {
        let mut ph: Vec<String> = Vec::with_capacity(col_names.len());
        // 用户列：每行逐列决定参数绑定 or backfill 字面量
        for c in &user_cols {
            let v = row.get(*c).cloned().unwrap_or(Value::Null);
            // Null + 在 backfill 表中 → 用 SQL 字面量（不占参数位）
            if v.is_null()
                && let Some((_, lit, _)) =
                    backfill.iter().find(|(name, _, _)| name == c)
                {
                    ph.push(lit.to_string());
                    continue;
                }
            // 正常参数绑定
            i += 1;
            ph.push(format!("${}", i));
            params.push(to_dv_by_col(view, c, &v));
        }
        // backfill 列用 SQL 字面量（user_cols 之外的列，通常为空）
        for (_, lit, _) in &backfill_cols {
            ph.push(lit.to_string());
        }
        // full_path 兜底：用本行 code 的值
        if full_path_backfill {
            i += 1;
            ph.push(format!("${}", i));
            let code_v = row.get(&view.code_field).cloned().unwrap_or(Value::Null);
            params.push(to_dv_by_col(view, &view.code_field, &code_v));
        }
        placeholders_per_row.push(format!("({})", ph.join(", ")));
    }

    // ON CONFLICT 子句按 mode 分支
    let conflict_clause = match mode {
        BatchConflictMode::Upsert => {
            // user_cols 的 EXCLUDED.x：排除 pk 和 on_update_cols（后者由字面量单独 SET）
            let mut updates: Vec<String> = user_cols
                .iter()
                .filter(|c| **c != view.pk && !on_update_cols.contains(c))
                .map(|c| format!("\"{}\" = EXCLUDED.\"{}\"", c, c))
                .collect();
            // backfill_cols（user_cols 之外）的 EXCLUDED.x
            for (name, _, _) in &backfill_cols {
                if name != &view.pk {
                    updates.push(format!("\"{}\" = EXCLUDED.\"{}\"", name, name));
                }
            }
            // on_update=true 的列总是 = now()（Upsert 刷新更新时间）
            for name in &on_update_cols {
                if valid_col(view, name)
                    && let Some((_, lit, _)) =
                        backfill.iter().find(|(n, _, _)| n == name)
                    {
                        updates.push(format!("\"{}\" = {}", name, lit));
                    }
            }
            if updates.is_empty() {
                format!("ON CONFLICT (\"{}\") DO NOTHING", view.pk)
            } else {
                format!("ON CONFLICT (\"{}\") DO UPDATE SET {}", view.pk, updates.join(", "))
            }
        }
        BatchConflictMode::InsertOnly => {
            format!("ON CONFLICT (\"{}\") DO NOTHING", view.pk)
        }
        BatchConflictMode::Replace => String::new(),
    };

    let sql = format!(
        "INSERT INTO \"{}\" ({}) VALUES {}{}",
        view.table_name,
        col_names.join(", "),
        placeholders_per_row.join(", "),
        if conflict_clause.is_empty() {
            String::new()
        } else {
            format!(" {}", conflict_clause)
        }
    );
    Some((sql, params))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DictColumn, DictView};

    /// 构造测试用 DictView：cf_currency 表，pk=id，含 id/code/name 三列。
    fn mock_view(table: &str, pk: &str) -> DictView {
        let columns = vec![
            DictColumn {
                name: "id".to_string(),
                caption: "ID".to_string(),
                data_type: "VARCHAR".to_string(),
                is_pk: true,
                nullable: false,
                dim_type: String::new(),
                ref_dict: String::new(),
                display_field: String::new(),
                ref_field: String::new(),
                physical_field: String::new(),
                edit: None,
                edit_settings: None,
                display: None,
                extra: None,
            },
            DictColumn {
                name: "code".to_string(),
                caption: "编码".to_string(),
                data_type: "VARCHAR".to_string(),
                is_pk: false,
                nullable: false,
                dim_type: String::new(),
                ref_dict: String::new(),
                display_field: String::new(),
                ref_field: String::new(),
                physical_field: String::new(),
                edit: None,
                edit_settings: None,
                display: None,
                extra: None,
            },
            DictColumn {
                name: "name".to_string(),
                caption: "名称".to_string(),
                data_type: "VARCHAR".to_string(),
                is_pk: false,
                nullable: true,
                dim_type: String::new(),
                ref_dict: String::new(),
                display_field: String::new(),
                ref_field: String::new(),
                physical_field: String::new(),
                edit: None,
                edit_settings: None,
                display: None,
                extra: None,
            },
        ];
        DictView {
            dict_code: "test".to_string(),
            dict_name: "测试".to_string(),
            table_name: table.to_string(),
            id_field: "id".to_string(),
            code_field: "code".to_string(),
            label_field: "name".to_string(),
            parent_field: None,
            self_hierarchy: false,
            columns,
            pk: pk.to_string(),
            spec: std::sync::Arc::new(cmx_biz::validation::TableSpec {
                table: table.to_string(),
                columns: std::collections::HashMap::new(),
                order: Vec::new(),
            }),
        }
    }

    fn mock_row(id: &str, code: &str) -> serde_json::Map<String, Value> {
        let mut m = serde_json::Map::new();
        m.insert("id".to_string(), Value::String(id.to_string()));
        m.insert("code".to_string(), Value::String(code.to_string()));
        m
    }

    #[test]
    fn build_export_sql_first_batch() {
        let view = mock_view("cf_currency", "id");
        let (sql, params) = build_export_sql(&view, None, 5000);
        assert!(sql.contains("ORDER BY \"id\""));
        assert!(sql.contains("LIMIT 5000"));
        assert!(sql.contains("\"id\", \"code\", \"name\""));
        assert!(params.is_empty());
    }

    #[test]
    fn build_export_sql_keyset() {
        let view = mock_view("cf_currency", "id");
        let (sql, params) =
            build_export_sql(&view, Some(&DataValue::String("100".to_string())), 5000);
        assert!(sql.contains("WHERE \"id\" > $1"));
        assert!(sql.contains("ORDER BY \"id\""));
        assert_eq!(params.len(), 1);
    }

    #[test]
    fn build_truncate_sql_basic() {
        let view = mock_view("cf_currency", "id");
        let sql = build_truncate_sql(&view);
        assert!(sql.contains("TRUNCATE TABLE \"cf_currency\""));
        assert!(sql.contains("RESTART IDENTITY"));
    }

    #[test]
    fn build_batch_insert_empty_returns_none() {
        let view = mock_view("cf_currency", "id");
        let rows: Vec<serde_json::Map<String, Value>> = vec![];
        assert!(build_batch_insert_sql(&view, &rows, BatchConflictMode::Upsert).is_none());
    }

    #[test]
    fn build_batch_insert_upsert_has_on_conflict_update() {
        let view = mock_view("cf_currency", "id");
        let rows = vec![mock_row("CNY", "CNY")];
        let (sql, params) =
            build_batch_insert_sql(&view, &rows, BatchConflictMode::Upsert).unwrap();
        assert!(sql.contains("ON CONFLICT (\"id\") DO UPDATE SET"));
        assert!(sql.contains("\"code\" = EXCLUDED.\"code\""));
        assert!(sql.contains("\"name\" = EXCLUDED.\"name\""));
        // 用户列绑参数（id + code + name 各 1 个 = 3 个）
        assert_eq!(params.len(), 3);
    }

    #[test]
    fn build_batch_insert_insert_only_does_nothing() {
        let view = mock_view("cf_currency", "id");
        let rows = vec![mock_row("CNY", "CNY")];
        let (sql, _) =
            build_batch_insert_sql(&view, &rows, BatchConflictMode::InsertOnly).unwrap();
        assert!(sql.contains("ON CONFLICT (\"id\") DO NOTHING"));
    }

    #[test]
    fn build_batch_insert_replace_no_on_conflict() {
        let view = mock_view("cf_currency", "id");
        let rows = vec![mock_row("CNY", "CNY")];
        let (sql, _) =
            build_batch_insert_sql(&view, &rows, BatchConflictMode::Replace).unwrap();
        assert!(!sql.contains("ON CONFLICT"));
    }

    #[test]
    fn build_batch_insert_multi_rows() {
        let view = mock_view("cf_currency", "id");
        let rows = vec![
            mock_row("CNY", "CNY"),
            mock_row("USD", "USD"),
            mock_row("EUR", "EUR"),
        ];
        let (sql, params) =
            build_batch_insert_sql(&view, &rows, BatchConflictMode::Upsert).unwrap();
        // 3 行 VALUES 子句
        assert!(sql.matches('(').count() >= 4); // (col_list) + 3 行 (...)
        // 每行 3 个用户列参数 = 9 个参数
        assert_eq!(params.len(), 9);
    }

    /// 回归测试：用户提供的 create_time（is_server_managed_col）在批量导入时必须被尊重，
    /// 不能因为单行 upsert 路径的过滤策略而被丢弃，否则 DB NOT NULL 约束失败。
    ///
    /// 场景：cf_client 表含 create_time NOT NULL 列，用户 JSON 提供 create_time 值，
    /// 期望 SQL 包含 "create_time" 列且绑参数。
    #[test]
    fn build_batch_insert_user_provided_create_time_is_kept() {
        let mut view = mock_view("cf_client", "id");
        // 加 create_time 列（NOT NULL，TIMESTAMP 类型）
        view.columns.push(crate::DictColumn {
            name: "create_time".to_string(),
            caption: "创建时间".to_string(),
            data_type: "TIMESTAMP".to_string(),
            is_pk: false,
            nullable: false,
            dim_type: String::new(),
            ref_dict: String::new(),
            display_field: String::new(),
            ref_field: String::new(),
            physical_field: String::new(),
            edit: None,
            edit_settings: None,
            display: None,
            extra: None,
        });
        let mut row = mock_row("CMX", "CMX");
        row.insert(
            "create_time".to_string(),
            Value::String("2026-01-01T00:00:00+00:00".to_string()),
        );
        let (sql, params) =
            build_batch_insert_sql(&view, std::slice::from_ref(&row), BatchConflictMode::Upsert)
                .unwrap();
        // SQL 必须包含 create_time 列（不能被 is_server_managed_col 过滤掉）
        assert!(
            sql.contains("\"create_time\""),
            "SQL 应包含 create_time 列, 实际: {sql}"
        );
        // 用户提供的 create_time 走参数绑定（而非 backfill 的 now() 字面量）
        // 总参数：id + code + name + create_time = 4 个
        assert_eq!(params.len(), 4);
        // SQL 不应混入 now() 字面量（用户提供了值，不走 backfill）
        assert!(
            !sql.contains("now()"),
            "用户提供了 create_time 时不应走 now() backfill, 实际: {sql}"
        );
    }

    #[test]
    fn extract_pk_from_row() {
        let view = mock_view("cf_currency", "id");
        let mut row = serde_json::Map::new();
        row.insert("id".to_string(), Value::String("abc".to_string()));
        let pk = extract_pk(&view, &row);
        match pk {
            DataValue::String(s) => assert_eq!(s, "abc"),
            other => panic!("expected String, got {:?}", other),
        }
    }

    /// 回归测试：CSV 无引号空字段 → Null → backfill 兜底 + ON CONFLICT 不双写。
    ///
    /// 场景：cf_client 含 create_time/update_time（NOT NULL，server_managed）。
    /// 用户 CSV：create_time 有值，update_time 空字段（Null）。
    /// 期望：
    ///   1. SQL 中 update_time 列只出现一次（不能 "update_time" 出现两次）
    ///   2. update_time 走 now() 字面量（Null 兜底）
    ///   3. create_time 走参数绑定（用户提供了值）
    ///   4. ON CONFLICT 子句 update_time 只 SET 一次（= now()，不出现 EXCLUDED."update_time"）
    #[test]
    fn csv_null_field_falls_back_and_no_duplicate_col() {
        let mut view = mock_view("cf_client", "code");
        view.columns.push(crate::DictColumn {
            name: "create_time".to_string(),
            caption: "创建时间".to_string(),
            data_type: "TIMESTAMP".to_string(),
            is_pk: false,
            nullable: false,
            dim_type: String::new(),
            ref_dict: String::new(),
            display_field: String::new(),
            ref_field: String::new(),
            physical_field: String::new(),
            edit: None,
            edit_settings: None,
            display: None,
            extra: None,
        });
        view.columns.push(crate::DictColumn {
            name: "update_time".to_string(),
            caption: "更新时间".to_string(),
            data_type: "TIMESTAMP".to_string(),
            is_pk: false,
            nullable: false,
            dim_type: String::new(),
            ref_dict: String::new(),
            display_field: String::new(),
            ref_field: String::new(),
            physical_field: String::new(),
            edit: None,
            edit_settings: None,
            display: None,
            extra: None,
        });
        // 模拟 CSV 解析结果：create_time 有值，update_time 是无引号空字段 → 不在 row 中
        let mut row = serde_json::Map::new();
        row.insert("code".to_string(), Value::String("CMX".to_string()));
        row.insert(
            "create_time".to_string(),
            Value::String("2026-01-01T00:00:00+00:00".to_string()),
        );
        // 注意：不插入 update_time（CSV 空字段 → Null → 跳过 key）

        let (sql, params) =
            build_batch_insert_sql(&view, std::slice::from_ref(&row), BatchConflictMode::Upsert)
                .unwrap();

        // 拆分 INSERT 列表 / VALUES / ON CONFLICT 三段分别校验
        let on_conflict_pos = sql.find("ON CONFLICT").unwrap();
        let insert_part = &sql[..on_conflict_pos];
        let on_conflict_part = &sql[on_conflict_pos..];

        // 1. INSERT 列表中 update_time 只出现 1 次（不能重复列）
        let insert_ut = insert_part.matches("\"update_time\"").count();
        assert_eq!(
            insert_ut, 1,
            "INSERT 列表中 update_time 应只 1 次, 实际 {insert_ut}: {sql}"
        );

        // 2. VALUES 中 update_time 走 now() 字面量（Null 兜底）
        assert!(
            insert_part.contains("now()"),
            "update_time 为 Null 时应走 now() 字面量, 实际: {sql}"
        );

        // 3. create_time 走参数绑定（用户提供了值）
        // 参数：id + code + name + create_time = 4 个（update_time 走字面量不占参数位）
        assert_eq!(
            params.len(),
            4,
            "应 4 个参数（id+code+name+create_time），update_time 走字面量"
        );

        // 4. ON CONFLICT 子句中 update_time 只出现 1 次（= now()，不出现 EXCLUDED）
        let on_conflict_ut = on_conflict_part.matches("\"update_time\"").count();
        assert_eq!(
            on_conflict_ut, 1,
            "ON CONFLICT 中 update_time 只应 1 次（= now()）, 实际: {sql}"
        );
        assert!(
            !on_conflict_part.contains("EXCLUDED.\"update_time\""),
            "ON CONFLICT 不应有 EXCLUDED.update_time（应 = now()）, 实际: {sql}"
        );
    }
}
