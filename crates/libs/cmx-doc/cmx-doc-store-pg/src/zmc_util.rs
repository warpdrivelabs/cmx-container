//! zmc_util —— ZmcDocLoader 共享辅助函数与驱动抽象。
//!
//! 消除 zmc_loader / zmc_loader_sqlx 的重复:
//! - [`ZmcExecutor`] trait 抽象 cmx-database / cmx-database-pg 两个 DatabaseManager 的
//!   零拷贝查询能力,关联类型 `Row` 保留具体驱动的 Row 类型;
//! - 所有辅助函数(`rebind_schema` / `collect_ids` / `build_child_group` 等)泛型于
//!   `<R: ZmcRowSource>`,tokio-postgres 与 sqlx 两条驱动路径共用一份实现。

use std::sync::Arc;

use cmx_biz::{BizError, Result};
use cmx_core::model::cell::{DataValue, FieldType};
use cmx_doc_model::meta::LayerView;
use cmx_rowsource::{ZmcChildGroup, ZmcColType, ZmcDataSet, ZmcRowSource, ZmcSchema};

/// 抽象两个 DatabaseManager 的零拷贝查询能力。
///
/// 关联类型 [`Row`](Self::Row) 让 ZmcDocLoader 算法可以泛型化,
/// 同时保留具体驱动(tokio-postgres / sqlx)的 Row 类型。
///
/// 仅用于静态泛型(`<E: ZmcExecutor>`),不支持 dyn dispatch。
pub trait ZmcExecutor {
    /// 具体行类型(`TokioPgRowSource` 或 `SqlxPgRowSource`)。
    type Row: ZmcRowSource;

    /// 执行带 DataValue 参数的只读 SQL,返回零拷贝 [`ZmcDataSet`]。
    ///
    /// 抽象 cmx-database / cmx-database-pg 的 `query_sql_zmc_with_datavalues` 方法,
    /// 错误统一 map 为 [`BizError::internal`]。
    fn query_sql_zmc_with_datavalues(
        &self,
        db_id: &str,
        sql: &str,
        params: Vec<DataValue>,
        dataset_id: &str,
    ) -> impl std::future::Future<Output = Result<ZmcDataSet<Self::Row>>>;
}

// ─────────────────────── ZmcExecutor impl:两个驱动 ───────────────────────

impl ZmcExecutor for cmx_database::DatabaseManager {
    type Row = cmx_database::SqlxPgRowSource;

    async fn query_sql_zmc_with_datavalues(
        &self,
        db_id: &str,
        sql: &str,
        params: Vec<DataValue>,
        dataset_id: &str,
    ) -> Result<ZmcDataSet<Self::Row>> {
        cmx_database::DatabaseManager::query_sql_zmc_with_datavalues(
            self, db_id, sql, params, dataset_id,
        )
        .await
        .map_err(|e| BizError::internal(e.to_string()))
    }
}

impl ZmcExecutor for cmx_database_pg::DatabaseManager {
    type Row = cmx_database_pg::zmcdataset::TokioPgRowSource;

    async fn query_sql_zmc_with_datavalues(
        &self,
        db_id: &str,
        sql: &str,
        params: Vec<DataValue>,
        dataset_id: &str,
    ) -> Result<ZmcDataSet<Self::Row>> {
        cmx_database_pg::DatabaseManager::query_sql_zmc_with_datavalues(
            self, db_id, sql, params, dataset_id,
        )
        .await
        .map_err(|e| BizError::internal(e.to_string()))
    }
}

/// Blanket impl:让 `&Arc<DatabaseManager>` 也能直接当 ZmcExecutor 用
/// (handlers 普遍持有 `Arc<DatabaseManager>` 全局句柄)。
impl<T: ZmcExecutor + ?Sized> ZmcExecutor for std::sync::Arc<T> {
    type Row = T::Row;

    async fn query_sql_zmc_with_datavalues(
        &self,
        db_id: &str,
        sql: &str,
        params: Vec<DataValue>,
        dataset_id: &str,
    ) -> Result<ZmcDataSet<Self::Row>> {
        (**self).query_sql_zmc_with_datavalues(db_id, sql, params, dataset_id).await
    }
}

// ─────────────────────── 泛型辅助函数 ───────────────────────

/// 老 FieldType → 中立 [`ZmcColType`](空表兜底 schema 用;有行时不走此路)。
pub fn field_type_to_zmc(ft: &FieldType) -> ZmcColType {
    match ft {
        FieldType::Int => ZmcColType::Int8,
        FieldType::Float => ZmcColType::Float8,
        FieldType::Decimal => ZmcColType::Numeric,
        FieldType::Bool => ZmcColType::Bool,
        FieldType::DateTime => ZmcColType::Timestamptz,
        FieldType::Date => ZmcColType::Date,
        FieldType::Uuid => ZmcColType::Uuid,
        FieldType::Binary => ZmcColType::Bytea,
        FieldType::Json => ZmcColType::Jsonb,
        // String/Text/Array/Unknown 一律当文本(空表无实际值,仅占位表头)
        _ => ZmcColType::Text,
    }
}

/// 空表兜底:查询推断的 schema 若列数不足(空结果集 → 0 列),用定义 schema 的列名 + 推断
/// PG 类型覆盖,保证前端拿到正确表头。
///
/// 非空结果集直接返回(schema 已由首行的真实列类型推导,精度更好)。
pub fn rebind_schema<R: ZmcRowSource>(ds: ZmcDataSet<R>, layer: &LayerView) -> ZmcDataSet<R> {
    if ds.schema.col_count() == layer.schema.fields.len() && !ds.is_empty() {
        return ds; // 列数吻合且有行,用真实类型
    }
    // 空表或列数不符:用定义 schema 的列名 + FieldType→ZmcColType 覆盖
    let columns: Vec<String> = layer.schema.fields.iter().map(|f| f.name.clone()).collect();
    let types: Vec<ZmcColType> = layer
        .schema
        .fields
        .iter()
        .map(|f| field_type_to_zmc(&f.field_type))
        .collect();
    let schema = Arc::new(ZmcSchema::from_parts(columns, types));
    ZmcDataSet::with_schema(ds.id.clone(), schema, ds.rows)
}

/// 把父 id JSON 值按 childKey 列类型化成 [`DataValue`](懒下钻 parent_ids 用)。
pub fn typecast_ids(
    layer: &LayerView,
    child_key: &str,
    ids: &[serde_json::Value],
) -> Result<Vec<DataValue>> {
    let col = layer.column(child_key).ok_or_else(|| {
        BizError::business(format!("子键 {child_key} 不在层 {}", layer.table_name))
    })?;
    ids.iter()
        .map(|v| cmx_doc_model::query::json_to_datavalue(col, v))
        .collect()
}

/// 从根/父 [`ZmcDataSet`] 收集 "id" 列值(作为子层 ANY 参数)。
pub fn collect_ids<R: ZmcRowSource>(ds: &ZmcDataSet<R>) -> Vec<DataValue> {
    let Some(id_idx) = ds.schema.col_index("id") else {
        return Vec::new();
    };
    let ty = ds.schema.types[id_idx];
    let mut out = Vec::with_capacity(ds.row_count());
    for row in 0..ds.row_count() {
        if let Some(dv) = id_datavalue(ds, row, id_idx, ty) {
            out.push(dv);
        }
    }
    out
}

/// 取 id 列的 [`DataValue`](用于 ANY 参数绑定;仅取常见 id 类型 int/text/uuid)。
fn id_datavalue<R: ZmcRowSource>(
    ds: &ZmcDataSet<R>,
    row: usize,
    col: usize,
    ty: ZmcColType,
) -> Option<DataValue> {
    let r = ds.rows.get(row)?;
    match ty {
        ZmcColType::Int2 => r.get_i16(col).map(|v| DataValue::Int(v as i64)),
        ZmcColType::Int4 => r.get_i32(col).map(|v| DataValue::Int(v as i64)),
        ZmcColType::Int8 => r.get_i64(col).map(DataValue::Int),
        ZmcColType::Uuid => r.get_uuid(col).map(DataValue::Uuid),
        _ => r.get_str(col).map(|s| DataValue::String(s.to_string())),
    }
}

/// 把子 [`ZmcDataSet`] 组装成 [`ZmcChildGroup`]:逐子行取 childKey 列的父 id 字符串。
pub fn build_child_group<R: ZmcRowSource>(
    child: ZmcDataSet<R>,
    child_key: &str,
    child_id: &str,
) -> Result<ZmcChildGroup<R>> {
    let key_idx = child.schema.col_index(child_key);
    let mut parent_ids = Vec::with_capacity(child.row_count());
    for row in 0..child.row_count() {
        let pid = match key_idx {
            Some(ci) => child.row_key_string(row, ci).unwrap_or_default(),
            None => String::new(),
        };
        parent_ids.push(pid);
    }
    Ok(ZmcChildGroup {
        child_key: child_id.to_string(),
        child,
        parent_ids,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_type_mapping_covers_common_variants() {
        assert_eq!(field_type_to_zmc(&FieldType::Int), ZmcColType::Int8);
        assert_eq!(field_type_to_zmc(&FieldType::Decimal), ZmcColType::Numeric);
        assert_eq!(field_type_to_zmc(&FieldType::Bool), ZmcColType::Bool);
        assert_eq!(field_type_to_zmc(&FieldType::String), ZmcColType::Text);
        assert_eq!(field_type_to_zmc(&FieldType::Text), ZmcColType::Text);
    }
}
