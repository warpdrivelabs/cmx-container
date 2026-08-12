//! cmx-dct-store-pg 对外类型——字典元数据文档 + 数据查询参数 + 查询结果。
//!
//! 这些类型是场景函数 [`crate::dict_meta`] / [`crate::dict_search`] 的入参/出参，
//! 取代旧路径里裸 `serde_json::Value` 与内部结构 `DictView` 的直接暴露：
//! - [`DictMeta`]：投影后的元数据文档（可直接 `to_value` 下发前端）。
//! - [`SearchQuery`] / [`Sort`]：强类型查询参数（内部 `to_raw` 喂 `build_search_sql`）。
//! - [`SearchResult`]：分页查询结果（强类型，不再裸 Value）。

use serde::Serialize;
use serde_json::{Map, Value};

// ============================================================================
// 字典元数据文档（投影已下沉，可直接下发）
// ============================================================================

/// 字典元数据文档——[`crate::dict_meta`] 的返回值。
///
/// 列已是 camelCase 下发形态（由 `project_meta_column` 投影）。derive `Serialize` +
/// `rename_all = "camelCase"`，handler 可直接 `serde_json::to_value(&meta)` 包进 `ApiResp`，
/// 无需手动拼 `json!({...})`。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DictMeta {
    pub dict_code: String,
    pub dict_name: String,
    pub table_name: String,
    pub pk: String,
    pub id_field: String,
    pub code_field: String,
    pub label_field: String,
    pub parent_field: Option<String>,
    pub self_hierarchy: bool,
    pub code_rule: Option<Value>,
    /// 已投影的列对象数组（每个元素含 name/caption/dataType/isPrimaryKey/...）。
    pub columns: Vec<Value>,
    /// 业务唯一键清单（投影自 DictView.unique_keys，如 `[["supplier_id","account_no"]]`）。
    /// 合并明细去重时用：去掉外键列后剩余字段即去重键。
    pub unique_keys: Vec<Vec<String>>,
}

impl DictMeta {
    /// 返回所有列名（按 columns 顺序）。供只需列清单的调用方（如查重/合并取头表列）。
    pub fn column_names(&self) -> Vec<&str> {
        self.columns
            .iter()
            .filter_map(|c| c.get("name").and_then(|v| v.as_str()))
            .collect()
    }
}

// ============================================================================
// 数据查询参数（强类型）
// ============================================================================

/// 排序参数。
pub struct Sort {
    pub field: String,
    pub desc: bool,
}

/// 字典数据查询参数——[`crate::dict_search`] / [`crate::dict_search_zmc`] 的入参。
///
/// 取代旧路径裸 `Value` body：调用方从字段清单即可知支持哪些查询能力。
/// `filters` 值支持标量（`col = $n`）与数组（`col IN (...)`）两种形态，与 `build_search_sql`
/// 约定一致；内部 [`to_raw`] 转回 `build_search_sql` 期望的 raw JSON。
pub struct SearchQuery {
    /// 列过滤：`{col: scalar | array | null}`（列白名单校验在 build_search_sql 内做）。
    pub filters: Map<String, Value>,
    /// code/label 模糊匹配。
    pub q: Option<String>,
    /// 排序；None 时回退 sort_no → pk。
    pub sort: Option<Sort>,
    /// 页码（1-based，最小 1）。
    pub page: u64,
    /// 每页行数（1..=5000）。
    pub page_size: u64,
    /// 自分级 children 查询的父 id；None 表示不按 parent 过滤。
    pub parent_id: Option<Value>,
}

impl SearchQuery {
    /// 默认查询：空 filters、无 q/sort、page=1、page_size=500。
    pub fn default_query() -> Self {
        Self {
            filters: Map::new(),
            q: None,
            sort: None,
            page: 1,
            page_size: 500,
            parent_id: None,
        }
    }

    /// 从可选的请求 body 构造（POST 走 body；GET 传 None 走默认）。
    ///
    /// 支持的 body 键（与旧 raw 形态对齐，保证 build_search_sql 行为不变）：
    /// - `filters`：对象
    /// - `q`：字符串
    /// - `sort`：`{field, order}`（order 仅认 `"desc"`，其余按升序）
    /// - `page` / `pageSize`：数字（缺省 1 / 500，clamp 到 [1,1] / [1,5000]）
    /// - `parentId`：任意值（null 视为不过滤）
    pub fn from_body(body: Option<Value>) -> Self {
        let Some(b) = body else {
            return Self::default_query();
        };
        let obj = b.as_object();
        let filters = obj
            .and_then(|o| o.get("filters"))
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();
        let q = obj
            .and_then(|o| o.get("q"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let sort = obj.and_then(|o| o.get("sort")).and_then(|s| {
            let field = s.get("field")?.as_str()?.to_string();
            let desc = s.get("order").and_then(|v| v.as_str()) == Some("desc");
            Some(Sort { field, desc })
        });
        let page = obj
            .and_then(|o| o.get("page"))
            .and_then(|v| v.as_u64())
            .unwrap_or(1)
            .max(1);
        let page_size = obj
            .and_then(|o| o.get("pageSize"))
            .and_then(|v| v.as_u64())
            .unwrap_or(500)
            .clamp(1, 5000);
        let parent_id = obj
            .and_then(|o| o.get("parentId"))
            .filter(|v| !v.is_null())
            .cloned();
        Self {
            filters,
            q,
            sort,
            page,
            page_size,
            parent_id,
        }
    }

    /// 转成 `build_search_sql` 期望的 raw JSON（内部桥接，调用方不可见）。
    pub(super) fn to_raw(&self) -> Value {
        let mut m = Map::new();
        if !self.filters.is_empty() {
            m.insert("filters".into(), Value::Object(self.filters.clone()));
        }
        if let Some(q) = &self.q {
            m.insert("q".into(), Value::String(q.clone()));
        }
        if let Some(s) = &self.sort {
            m.insert(
                "sort".into(),
                serde_json::json!({
                    "field": s.field,
                    "order": if s.desc { "desc" } else { "asc" },
                }),
            );
        }
        m.insert("page".into(), Value::Number(self.page.into()));
        m.insert("pageSize".into(), Value::Number(self.page_size.into()));
        if let Some(pid) = &self.parent_id {
            m.insert("parentId".into(), pid.clone());
        }
        Value::Object(m)
    }
}

// ============================================================================
// 数据查询结果
// ============================================================================

/// 分页查询结果——[`crate::dict_search`] 的返回值。
pub struct SearchResult {
    pub rows: Vec<Value>,
    pub total: i64,
    pub page: u64,
    pub page_size: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_body_none_uses_defaults() {
        let q = SearchQuery::from_body(None);
        assert_eq!(q.page, 1);
        assert_eq!(q.page_size, 500);
        assert!(q.filters.is_empty());
        assert!(q.q.is_none());
        assert!(q.sort.is_none());
        assert!(q.parent_id.is_none());
    }

    #[test]
    fn from_body_parses_fields() {
        let body = serde_json::json!({
            "filters": {"code": "CNY"},
            "q": "hello",
            "sort": {"field": "name", "order": "desc"},
            "page": 3,
            "pageSize": 50,
            "parentId": "root"
        });
        let q = SearchQuery::from_body(Some(body));
        assert_eq!(q.filters.len(), 1);
        assert_eq!(q.q.as_deref(), Some("hello"));
        assert_eq!(q.sort.as_ref().unwrap().field, "name");
        assert!(q.sort.as_ref().unwrap().desc);
        assert_eq!(q.page, 3);
        assert_eq!(q.page_size, 50);
        assert_eq!(q.parent_id.as_ref().unwrap().as_str().unwrap(), "root");
    }

    #[test]
    fn from_body_clamps_page_and_size() {
        let body = serde_json::json!({"page": 0, "pageSize": 99999});
        let q = SearchQuery::from_body(Some(body));
        assert_eq!(q.page, 1);
        assert_eq!(q.page_size, 5000);
    }

    #[test]
    fn from_body_sort_order_asc_when_not_desc() {
        let body = serde_json::json!({"sort": {"field": "id", "order": "asc"}});
        let q = SearchQuery::from_body(Some(body));
        assert!(!q.sort.as_ref().unwrap().desc);
    }

    #[test]
    fn from_body_parent_id_null_is_none() {
        let body = serde_json::json!({"parentId": null});
        let q = SearchQuery::from_body(Some(body));
        assert!(q.parent_id.is_none());
    }

    #[test]
    fn to_raw_round_trips_key_fields() {
        let q = SearchQuery {
            filters: {
                let mut m = Map::new();
                m.insert("code".into(), Value::String("CNY".into()));
                m
            },
            q: Some("x".into()),
            sort: Some(Sort {
                field: "name".into(),
                desc: true,
            }),
            page: 2,
            page_size: 20,
            parent_id: Some(Value::String("p".into())),
        };
        let raw = q.to_raw();
        assert_eq!(raw.get("filters").unwrap().get("code").unwrap().as_str(), Some("CNY"));
        assert_eq!(raw.get("q").unwrap().as_str(), Some("x"));
        assert_eq!(
            raw.get("sort").unwrap().get("field").unwrap().as_str(),
            Some("name")
        );
        assert_eq!(
            raw.get("sort").unwrap().get("order").unwrap().as_str(),
            Some("desc")
        );
        assert_eq!(raw.get("page").unwrap().as_u64(), Some(2));
        assert_eq!(raw.get("pageSize").unwrap().as_u64(), Some(20));
        assert_eq!(raw.get("parentId").unwrap().as_str(), Some("p"));
    }

    #[test]
    fn to_raw_omits_empty_optionals() {
        let q = SearchQuery::default_query();
        let raw = q.to_raw();
        // page/pageSize 恒输出；filters/q/sort/parentId 空时不输出
        assert!(raw.get("filters").is_none());
        assert!(raw.get("q").is_none());
        assert!(raw.get("sort").is_none());
        assert!(raw.get("parentId").is_none());
        assert_eq!(raw.get("page").unwrap().as_u64(), Some(1));
    }

    #[test]
    fn dict_meta_column_names_extracts_names() {
        let meta = DictMeta {
            dict_code: "x".into(),
            dict_name: "x".into(),
            table_name: "t".into(),
            pk: "id".into(),
            id_field: "id".into(),
            code_field: "code".into(),
            label_field: "name".into(),
            parent_field: None,
            self_hierarchy: false,
            code_rule: None,
            columns: vec![
                serde_json::json!({"name": "id", "dataType": "BIGINT"}),
                serde_json::json!({"name": "code", "dataType": "VARCHAR"}),
            ],
            unique_keys: vec![],
        };
        assert_eq!(meta.column_names(), vec!["id", "code"]);
    }

    #[test]
    fn dict_meta_serializes_camel_case() {
        let meta = DictMeta {
            dict_code: "x".into(),
            dict_name: "x".into(),
            table_name: "t".into(),
            pk: "id".into(),
            id_field: "id".into(),
            code_field: "code".into(),
            label_field: "name".into(),
            parent_field: Some("pid".into()),
            self_hierarchy: true,
            code_rule: None,
            columns: vec![],
            unique_keys: vec![],
        };
        let v = serde_json::to_value(&meta).unwrap();
        assert_eq!(v.get("dictCode").unwrap().as_str(), Some("x"));
        assert_eq!(v.get("tableName").unwrap().as_str(), Some("t"));
        assert_eq!(v.get("idField").unwrap().as_str(), Some("id"));
        assert_eq!(v.get("selfHierarchy").unwrap().as_bool(), Some(true));
        assert_eq!(v.get("parentField").unwrap().as_str(), Some("pid"));
    }
}
