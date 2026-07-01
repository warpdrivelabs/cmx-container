//! 字典 schema 与注册表（`dict/registry.json`，schema 数组）。

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::config::data_path;
use crate::error::{PortalError, PortalResult};
use crate::fsutil::{read_json_opt, write_json_atomic};
use crate::util::write_lock;

/// 字典 schema（仅声明引擎用到的字段；其余原样透传）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DictSchema {
    #[serde(rename = "dictId")]
    pub dict_id: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub hierarchical: bool,
    #[serde(default, rename = "idField")]
    pub id_field: Option<String>,
    #[serde(default, rename = "parentField")]
    pub parent_field: Option<String>,
    #[serde(default, rename = "labelField")]
    pub label_field: Option<String>,
    #[serde(default, rename = "fullTextFields")]
    pub full_text_fields: Option<Vec<String>>,
    #[serde(default, rename = "filterFields")]
    pub filter_fields: Option<Vec<String>>,
    /// DAM 命名空间（决定 entries 分层路径）。
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub app: Option<String>,
    #[serde(default)]
    pub module: Option<String>,
    /// 透传其余字段（保存时不丢）。
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl DictSchema {
    pub fn id_field(&self) -> &str {
        self.id_field.as_deref().unwrap_or("id")
    }
    pub fn parent_field(&self) -> &str {
        self.parent_field.as_deref().unwrap_or("parentId")
    }
    pub fn label_field(&self) -> &str {
        self.label_field.as_deref().unwrap_or("name")
    }
}

fn registry_path() -> std::path::PathBuf {
    data_path(["dict", "registry.json"])
}

/// 读取全部 schema。
pub async fn load_schemas() -> PortalResult<Vec<DictSchema>> {
    match read_json_opt(&registry_path()).await? {
        Some(serde_json::Value::Array(arr)) => {
            let mut out = Vec::with_capacity(arr.len());
            for v in arr {
                if let Ok(s) = serde_json::from_value::<DictSchema>(v) {
                    out.push(s);
                }
            }
            Ok(out)
        }
        _ => Ok(Vec::new()),
    }
}

/// 取单个 schema（未注册返回 404）。
pub async fn get_schema(dict_id: &str) -> PortalResult<DictSchema> {
    load_schemas()
        .await?
        .into_iter()
        .find(|s| s.dict_id == dict_id)
        .ok_or_else(|| PortalError::not_found(format!("字典 \"{dict_id}\" 未注册")))
}

/// 取单个 schema（不存在返回 None）。
pub async fn try_get_schema(dict_id: &str) -> PortalResult<Option<DictSchema>> {
    Ok(load_schemas().await?.into_iter().find(|s| s.dict_id == dict_id))
}

/// 注册 / 更新 schema。
pub async fn register_schema(body: &serde_json::Value) -> PortalResult<serde_json::Value> {
    let dict_id = body.get("dictId").and_then(|v| v.as_str()).filter(|s| !s.is_empty());
    let Some(dict_id) = dict_id else {
        return Err(PortalError::bad_request("schema.dictId 不能为空"));
    };
    let schema: DictSchema = serde_json::from_value(body.clone())?;
    let _guard = write_lock().lock().await;
    let mut schemas = load_schemas().await?;
    if let Some(existing) = schemas.iter_mut().find(|s| s.dict_id == dict_id) {
        *existing = schema;
    } else {
        schemas.push(schema);
    }
    write_json_atomic(&registry_path(), &schemas, true).await?;
    Ok(json!({ "ok": true, "dictId": dict_id }))
}

/// schema 列表（原样 JSON，保留所有字段）。
pub async fn list_schemas_json() -> PortalResult<serde_json::Value> {
    match read_json_opt(&registry_path()).await? {
        Some(v @ serde_json::Value::Array(_)) => Ok(v),
        _ => Ok(json!([])),
    }
}
