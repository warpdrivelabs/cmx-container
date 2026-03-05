use std::{collections::HashMap, sync::Arc, sync::RwLock};
use lazy_static::lazy_static;

use crate::model::data::{dataset::TableSchema, KeyValue};

use super::dct::DCTMeta;

use serde::{Deserialize, Serialize};

lazy_static! {
    static ref FCT_METAS: RwLock<HashMap<String, Arc<FCTMeta>>> = RwLock::new(HashMap::new());
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FCTMeta {
    pub id: String,                // ID
    pub name: String,              // Name
    pub info: Option<KeyValue>, 
    pub table_schema: TableSchema,  // Changed from Arc<TableSchema>
    #[serde(skip)]
    pub dct_metas: Option<HashMap<String, Arc<DCTMeta>>>,     // 所有用到的DCTMeta，String为FCT的某一列，此列引用了DCTMeta
    pub settings: Option<KeyValue>, // 可选的 KeyValue 成员
}

#[derive(Debug)]
pub struct FCTMetaManager;

impl FCTMetaManager {

    pub fn add_meta(meta: Arc<FCTMeta>) {
        let mut metas = FCT_METAS.write().unwrap();
        metas.insert(meta.id.clone(), meta);
    }

    pub fn get_meta(id: &str) -> Option<Arc<FCTMeta>> {
        let metas = FCT_METAS.read().unwrap();
        metas.get(id).cloned()
    }

    // Optional: Add a clear method for testing
    #[cfg(test)]
    pub fn clear() {
        let mut metas = FCT_METAS.write().unwrap();
        metas.clear();
    }
}
