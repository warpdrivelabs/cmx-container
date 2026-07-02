//! 工作区节点存储：所有节点汇总在 `node/nodes.json`（单一文件）。
//!
//! 文件结构：`{ "version": 1, "nodes": { [id]: WorkspaceNodeRecord } }`。
//! 复刻 Node `lib/workspaceNodesStore.js`：list（摘要，按 updatedAt 倒序）/ get（完整）/
//! save（upsert，updatedAt 由服务端维护）/ delete。写操作走全局写锁 + 原子写。

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::config::data_path;
use crate::error::{PortalError, PortalResult};
use crate::fsutil::{read_json_opt, write_json_atomic};
use crate::util::{validate_id, write_lock};

/// 节点完整记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceNodeRecord {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub details: String,
    #[serde(default)]
    pub workspace: serde_json::Value,
    #[serde(default, rename = "updatedAt")]
    pub updated_at: String,
}

/// 列表摘要项（不含 workspace 内容）。
#[derive(Debug, Clone, Serialize)]
pub struct WorkspaceNodeSummary {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub details: String,
    #[serde(rename = "updatedAt")]
    pub updated_at: String,
}

/// 保存入参（来自 HTTP body）。
#[derive(Debug, Clone, Deserialize)]
pub struct WorkspaceNodeInput {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub details: Option<String>,
    #[serde(default)]
    pub workspace: Option<serde_json::Value>,
}

fn nodes_path() -> std::path::PathBuf {
    data_path(["node", "nodes.json"])
}

/// 读取节点文档（容错：缺失 / 结构非法时返回空文档）。
async fn read_doc() -> PortalResult<serde_json::Value> {
    match read_json_opt(&nodes_path()).await? {
        Some(v) if v.get("nodes").map(|n| n.is_object()).unwrap_or(false) => Ok(v),
        _ => Ok(json!({ "version": 1, "nodes": {} })),
    }
}

/// 列出节点摘要，按 updatedAt 倒序。
pub async fn list_workspace_nodes() -> PortalResult<serde_json::Value> {
    let doc = read_doc().await?;
    let mut items: Vec<WorkspaceNodeSummary> = doc["nodes"]
        .as_object()
        .map(|m| {
            m.values()
                .map(|row| WorkspaceNodeSummary {
                    id: row.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    name: row.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    icon: row.get("icon").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    details: row.get("details").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                    updated_at: row.get("updatedAt").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                })
                .collect()
        })
        .unwrap_or_default();
    // updatedAt 倒序（与 Node 的 localeCompare 倒序等价：字典序逆序）
    items.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    let total = items.len();
    Ok(json!({ "items": items, "total": total }))
}

/// 按 id 读取完整节点定义。
pub async fn get_workspace_node_by_id(id: &str) -> PortalResult<WorkspaceNodeRecord> {
    let key = validate_id(id, "id")?;
    let doc = read_doc().await?;
    let row = doc["nodes"].get(&key).cloned().ok_or_else(|| {
        PortalError::not_found(format!("workspace-node 不存在：{key}"))
    })?;
    let record: WorkspaceNodeRecord = serde_json::from_value(row)?;
    Ok(record)
}

/// upsert 保存节点（updatedAt 由服务端写当前时间）。
pub async fn save_workspace_node(input: WorkspaceNodeInput) -> PortalResult<WorkspaceNodeRecord> {
    let id = validate_id(&input.id, "id")?;
    let workspace = match input.workspace {
        Some(w) if w.is_object() => w,
        _ => return Err(PortalError::bad_request("workspace 必须为对象")),
    };
    let record = WorkspaceNodeRecord {
        id: id.clone(),
        name: input.name.unwrap_or_default(),
        icon: input.icon.map(|s| s.trim().to_string()).unwrap_or_default(),
        details: input.details.unwrap_or_default(),
        workspace,
        updated_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
    };

    let _guard = write_lock().lock().await;
    let mut doc = read_doc().await?;
    doc["nodes"][&id] = serde_json::to_value(&record)?;
    write_json_atomic(&nodes_path(), &doc, true).await?;
    Ok(record)
}

/// 删除节点。返回 `{ id, removed }`。
pub async fn delete_workspace_node(id: &str) -> PortalResult<serde_json::Value> {
    let key = validate_id(id, "id")?;
    let _guard = write_lock().lock().await;
    let mut doc = read_doc().await?;
    let had = doc["nodes"].get(&key).is_some();
    if !had {
        return Ok(json!({ "id": key, "removed": false }));
    }
    if let Some(map) = doc["nodes"].as_object_mut() {
        map.remove(&key);
    }
    write_json_atomic(&nodes_path(), &doc, true).await?;
    Ok(json!({ "id": key, "removed": true }))
}
