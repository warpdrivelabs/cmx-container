/*
 * @Describe: PgSubflowRouter —— 子流程组织路由的 PG 实现（M5.2）。
 *
 * 实现 cmx-flow-model::SubflowRouter：给定「逻辑子流程 key + 组织 id」解析出具体子流程定义 key。
 * 数据源 cmx_flow_subflow_binding（called_key + org_id → target_definition_key），三层解析：
 *   1. 精确：本组织 org_id 的启用绑定；
 *   2. 继承：沿 cmx_org.path 向上找最近祖先的绑定（path 最长 = 最近，优先）；
 *   3. 兜底：org_id IS NULL 的默认绑定。
 * 全无 → RouteError::NoBinding。
 *
 * 只读查询走 cmx-database-pg 的 query_sql。引擎经 SubflowRouter trait 依赖它，不直连表——中立。
 */

use async_trait::async_trait;
use cmx_core::model::cell::DataValue;
use cmx_database_pg::query_sql;
use cmx_flow_model::{RouteError, RouteResult, SubflowRouter};

/// 子流程组织路由器。持目标 db_id（绑定表 + cmx_org 所在库），所有查询走该库。
#[derive(Clone)]
pub struct PgSubflowRouter {
    db_id: String,
}

impl PgSubflowRouter {
    /// 用指定 db_id 构建（须已在 cmx-database-pg 注册数据源）。
    pub fn new(db_id: impl Into<String>) -> Self {
        Self { db_id: db_id.into() }
    }

    /// 执行一条只取首行 target_definition_key 的查询；无行 → None。
    async fn query_one_target(&self, sql: &str, tag: &str) -> RouteResult<Option<String>> {
        let ds = query_sql(&self.db_id, None, sql, tag)
            .await
            .map_err(|e| RouteError::Backend(format!("查询子流程绑定失败: {e}")))?;
        let schema = ds.schema.as_ref();
        for row in ds.iter() {
            match row.get_by_name(schema, "target_definition_key") {
                Some(DataValue::String(s)) => return Ok(Some(s.clone())),
                Some(DataValue::ShortStr(s)) | Some(DataValue::LongStr(s)) => {
                    return Ok(Some(s.to_string()));
                }
                _ => {}
            }
        }
        Ok(None)
    }
}

/// 单引号转义（值来自 BPMN 定义 / 实例组织，无强注入面，仍防御）。
fn esc(s: &str) -> String {
    s.replace('\'', "''")
}

#[async_trait]
impl SubflowRouter for PgSubflowRouter {
    async fn resolve(&self, called_key: &str, org_id: Option<&str>) -> RouteResult<String> {
        let k = esc(called_key);

        // 有组织：先精确，再沿 path 向上继承。
        if let Some(org) = org_id {
            let o = esc(org);
            // 1) 精确：本组织的启用绑定。
            let exact = format!(
                "SELECT target_definition_key FROM cmx_flow_subflow_binding \
                 WHERE called_key = '{k}' AND org_id = '{o}' AND enabled = TRUE LIMIT 1"
            );
            if let Some(t) = self.query_one_target(&exact, "subflow_exact").await? {
                return Ok(t);
            }
            // 2) 继承：本组织的所有祖先（含自身）里，谁绑了本 key，取 path 最长（最近）。
            //    cmx_org 的 path 为物化路径，祖先的 path 是本组织 path 的前缀。
            let inherited = format!(
                "SELECT b.target_definition_key \
                 FROM cmx_flow_subflow_binding b \
                 JOIN cmx_org anc ON anc.id = b.org_id \
                 JOIN cmx_org self_org ON self_org.id = '{o}' \
                 WHERE b.called_key = '{k}' AND b.enabled = TRUE \
                   AND self_org.path IS NOT NULL AND anc.path IS NOT NULL \
                   AND self_org.path LIKE anc.path || '%' \
                 ORDER BY length(anc.path) DESC LIMIT 1"
            );
            if let Some(t) = self.query_one_target(&inherited, "subflow_inherit").await? {
                return Ok(t);
            }
        }

        // 3) 兜底：org_id IS NULL 的默认绑定。
        let default = format!(
            "SELECT target_definition_key FROM cmx_flow_subflow_binding \
             WHERE called_key = '{k}' AND org_id IS NULL AND enabled = TRUE LIMIT 1"
        );
        if let Some(t) = self.query_one_target(&default, "subflow_default").await? {
            return Ok(t);
        }

        Err(RouteError::NoBinding {
            called_key: called_key.to_string(),
            org: org_id.map(|s| s.to_string()),
        })
    }
}
