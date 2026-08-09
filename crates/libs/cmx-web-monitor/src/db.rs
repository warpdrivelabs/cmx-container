//! 数据库连接池状态适配（薄封装 cmx-database-pg 的全局 PG 单例）。
//!
//! 复用各服务已注册进 `get_default_pg_db_manager()` 的数据源——flow-server 的 flow/iam、
//! 平台 web-server 的各库都会自动出现。零查询开销（读 deadpool 池计数）。

use serde_json::{Value, json};

/// 所有数据源的连接池状态 JSON 数组：`[{dbId,maxSize,size,available,waiting,inUse}]`。
pub async fn pool_snapshot() -> Value {
    let statuses = cmx_database_pg::get_default_pg_db_manager()
        .pool_statuses()
        .await;
    let arr: Vec<Value> = statuses
        .into_iter()
        .map(|s| {
            json!({
                "dbId": s.db_id,
                "maxSize": s.max_size,
                "size": s.size,
                "available": s.available,
                "waiting": s.waiting,
                // 派生：在用 = 已建 - 空闲（前端画使用率条）。
                "inUse": s.size.saturating_sub(s.available),
            })
        })
        .collect();
    json!(arr)
}
