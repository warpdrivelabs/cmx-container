// use std::sync::Arc;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
// use ospbase::data::dataset::row::RowSet;
// use crate::model::data::dataset::DataSet;
use serde::{Serialize, Deserialize};

use crate::model::cell::CellValue;
use crate::model::data::KeyValue;
use crate::model::data::dataset::rds::RowDataSet;

use super::fct::FCTMeta;
use super::dct::DCTMeta;

#[derive(Debug, Serialize, Deserialize)]
pub struct DMEMeta {
    pub id: String,                          
    pub name: String,
    pub info: Option<KeyValue>,
    pub settings: Option<KeyValue>, 
    pub fct_list: Option<RowDataSet>,
    
    #[serde(skip)]                        
    pub fct_meta_map: Option<HashMap<String, FCTMeta>>,
    #[serde(skip)] 
    pub key_metrics: Option<Vec<DCTMeta>>,
    #[serde(skip)]       
    pub unit_dct: Option<DCTMeta>,   
}

impl DMEMeta {
    /// 创建一个新的 `FDMeta` 实例。
    ///
    /// # 参数
    /// - `id`: 实例的唯一标识符。
    /// - `name`: 实例的名称。
    ///
    /// # 返回
    /// 返回一个新的 `FDMeta` 实例。
    pub fn new(id: String, name: String) -> Self {
        Self {
            id,
            name,
            info: None,
            fct_meta_map: None,
            key_metrics: None, // 初始化关键指标数组
            unit_dct: None, // 初始化单位DCT
            settings: None, // 初始化KeyValue
            fct_list: None,
        }
    }
    /// 向 `fct_meta_map` 中添加一个 FCTMeta 实例。
    ///
    /// # 参数
    /// - `key`: FCTMeta 实例的键。
    /// - `fct_meta`: 要添加的 FCTMeta 实例。
    pub fn add_fct_meta(&mut self, key: String, fct_meta: FCTMeta) {
        if let Some(map) = &mut self.fct_meta_map {
            map.insert(key, fct_meta);
        } else {
            let mut map = HashMap::new();
            map.insert(key, fct_meta);
            self.fct_meta_map = Some(map);
        }
    }

    /// 根据键获取 FCTMeta 实例。
    ///
    /// # 参数
    /// - `key`: 要查找的 FCTMeta 实例的键。
    ///
    /// # 返回
    /// 如果找到，返回对应的 FCTMeta 实例；否则返回 `None`。
    pub fn get_fct_meta(&self, key: &str) -> Option<&FCTMeta> {
        self.fct_meta_map.as_ref().and_then(|map| map.get(key))
    }

    /// 向 `key_metrics` 中添加一个 DCTMeta 实例。
    ///
    /// # 参数
    /// - `key_metric`: 要添加的 DCTMeta 实例。
    pub fn add_key_metric(&mut self, key_metric: DCTMeta) {
        if let Some(ref mut key_metrics) = self.key_metrics {
            key_metrics.push(key_metric);
        } else {
            self.key_metrics = Some(vec![key_metric]);
        }
    }
    
    /// 根据索引获取 DCTMeta 实例。
    ///
    /// # 参数
    /// - `index`: 要查找的 DCTMeta 实例的索引。
    ///
    /// # 返回
    /// 如果找到，返回对应的 DCTMeta 实例；否则返回 `None`。
    pub fn get_key_metric(&self, index: usize) -> Option<&DCTMeta> {
        self.key_metrics.as_ref().and_then(|metrics| metrics.get(index))
    }

    /// 设置单位 DCT 实例。
    ///
    /// # 参数
    /// - `unit_dct`: 要设置的单位 DCT 实例。
    pub fn set_unit_dct(&mut self, unit_dct: DCTMeta) {
        self.unit_dct = Some(unit_dct);
    }

    /// 获取单位 DCT 实例。
    ///
    /// # 返回
    /// 返回当前设置的单位 DCT 实例，如果没有设置则返回 `None`。
    pub fn get_unit_dct(&self) -> Option<&DCTMeta> {
        self.unit_dct.as_ref()
    }

    /// 根据键获取 KeyValue 中的值。
    ///
    /// # 参数
    /// - `key`: 要查找的键。
    ///
    /// # 返回
    /// 如果找到，返回对应的值；否则返回 `None`。
    pub fn get_key_value(&self, key: &str) -> Option<&CellValue> {
        self.settings.as_ref().and_then(|kv| kv.get(key))
    }

    /// 向 KeyValue 中添加一个键值对。
    ///
    /// # 参数
    /// - `key`: 要添加的键。
    /// - `value`: 要添加的值。
    pub fn put_key_value(&mut self, key: String, value: CellValue) {
        if let Some(ref mut kv) = self.settings {
            kv.put(key, value);
        } else {
            let mut kv = KeyValue::new();
            kv.put(key, value);
            self.settings = Some(kv);
        }
    }

    /// 序列化为 JSON 字符串
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// 从 JSON 字符串反序列化
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}


use lazy_static::lazy_static;

lazy_static! {
    static ref FD_METAS: RwLock<HashMap<String, Arc<DMEMeta>>> = RwLock::new(HashMap::new());
}

#[derive(Debug)]
pub struct DMEMetaManager;

impl DMEMetaManager {
    /// Add a FDMeta to the cache
    pub fn add_meta(meta: Arc<DMEMeta>) {
        let mut metas = FD_METAS.write().unwrap();
        metas.insert(meta.id.clone(), meta);
    }

    /// Get a FDMeta from the cache by id
    pub fn get_meta(id: &str) -> Option<Arc<DMEMeta>> {
        let metas = FD_METAS.read().unwrap();
        metas.get(id).cloned()
    }

    /// Remove a FDMeta from the cache
    pub fn remove_meta(id: &str) -> Option<Arc<DMEMeta>> {
        let mut metas = FD_METAS.write().unwrap();
        metas.remove(id)
    }

    /// Get all cached FDMetas
    pub fn get_all_metas() -> Vec<Arc<DMEMeta>> {
        let metas = FD_METAS.read().unwrap();
        metas.values().cloned().collect()
    }

    #[cfg(test)]
    pub fn clear() {
        let mut metas = FD_METAS.write().unwrap();
        metas.clear();
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use crate::data::dataset::{TableSchema, col::{ColumnDef, ColumnType}};

//     fn create_test_schema() -> TableSchema {
//         let columns = vec![
//             ColumnDef::new("id".to_string(), ColumnType::Integer)
//                 .with_nullable(false),
//             ColumnDef::new("name".to_string(), ColumnType::Text)
//                 .with_nullable(false),
//             ColumnDef::new("value".to_string(), ColumnType::Double)
//                 .with_nullable(true),
//         ];
//         TableSchema::new("test_table".to_string(), columns)
//     }

//     #[test]
//     fn test_fd_meta_basic_serialization() {
//         // 创建基本的 FDMeta
//         let fd_meta = DEMeta::new(
//             "fd1".to_string(),
//             "Test FD".to_string(),
//         );

//         // 序列化为 JSON
//         let json = fd_meta.to_json().expect("Failed to serialize");
        
//         // 反序列化
//         let deserialized = DEMeta::from_json(&json).expect("Failed to deserialize");
        
//         // 验证基本字段
//         assert_eq!(deserialized.id, "fd1");
//         assert_eq!(deserialized.name, "Test FD");
//         assert!(deserialized.fct_meta_map.is_none());
//         assert!(deserialized.key_metrics.is_none());
//         assert!(deserialized.unit_dct.is_none());
//         assert!(deserialized.settings.is_none());
//     }

//     #[test]
//     fn test_fd_meta_with_all_fields() {
//         let schema = create_test_schema();

//         // 创建 FCTMeta
//         let fct_meta = FCTMeta {
//             id: "fct1".to_string(), 
//             name: "Test FCT".to_string(),
//             info: None,
//             table_schema: schema.clone(),
//             dct_metas: None,
//             settings: None,
//         };

//         // 创建 DCTMeta
//         let dct_meta = DCTMeta::new(
//             "dct1".to_string(),
//             "Test DCT".to_string(),
//             // "default".to_string(),
//             // "perm1".to_string(),
//             // "standard".to_string(),
//             schema.clone(),
//             None,
//         );

//         // 创建完整的 FDMeta
//         let mut fd_meta = DEMeta::new(
//             "fd1".to_string(),
//             "Test FD".to_string(),
//         );

//         // 添加 FCTMeta
//         fd_meta.add_fct_meta("fact1".to_string(), fct_meta);

//         // 添加关键指标
//         fd_meta.add_key_metric(dct_meta.clone());

//         // 设置单位 DCT
//         fd_meta.set_unit_dct(dct_meta);

//         // 添加 KeyValue
//         fd_meta.put_key_value("type".to_string(), CellValue::Text(Option::Some("dimension".to_string())));
//         fd_meta.put_key_value("version".to_string(), CellValue::Integer(1));

//         // 序列化为 JSON
//         let json = fd_meta.to_json().expect("Failed to serialize");
        
//         // 反序列化
//         let deserialized = DEMeta::from_json(&json).expect("Failed to deserialize");
        
//         // 验证所有字段
//         assert_eq!(deserialized.id, "fd1");
//         assert_eq!(deserialized.name, "Test FD");
        
//         // 验证 FCTMeta
//         let fct_meta_map = deserialized.fct_meta_map.as_ref().expect("FCTMeta map should exist");
//         let fct = fct_meta_map.get("fact1").expect("FCTMeta 'fact1' should exist");
//         assert_eq!(fct.id, "fct1");
//         assert_eq!(fct.name, "Test FCT");
//         // assert_eq!(fct.table_schema, schema);
//         assert!(fct.dct_metas.is_none());
//         assert!(fct.settings.is_none());
        
//         // 验证关键指标
//         let key_metrics = deserialized.key_metrics.as_ref().expect("Key metrics should exist");
//         assert_eq!(key_metrics.len(), 1);
//         let metric = key_metrics.get(0).expect("Key metric should exist");
//         assert_eq!(metric.id, "dct1");
//         assert_eq!(metric.name, "Test DCT");
//         // assert_eq!(metric.default_value, "default");
//         // assert_eq!(metric.permission, "perm1");
//         // assert_eq!(metric.standard, "standard");
//         // assert_eq!(metric.table_schema, schema);
//         assert!(metric.settings.is_none());
        
//         // 验证单位 DCT
//         let unit_dct = deserialized.unit_dct.as_ref().expect("Unit DCT should exist");
//         assert_eq!(unit_dct.id, "dct1");
//         assert_eq!(unit_dct.name, "Test DCT");
//         // assert_eq!(unit_dct.default_value, "default");
//         // assert_eq!(unit_dct.permission, "perm1");
//         // assert_eq!(unit_dct.standard, "standard");
//         // assert_eq!(unit_dct.table_schema, schema);
//         assert!(unit_dct.settings.is_none());
        
//         // 验证 KeyValue
//         // let key_value = deserialized.key_value.as_ref().expect("KeyValue should exist");
//         // assert_eq!(key_value.get("type").expect("'type' key should exist"), CellValue::Text("dimension".to_string()));
//         // assert_eq!(key_value.get("version").expect("'version' key should exist"), CellValue::Integer(1));
//     }

//     #[test]
//     fn test_fd_meta_manager() {
//         // Create test FDMeta
//         let fd_meta = Arc::new(DEMeta::new(
//             "fd1".to_string(),
//             "Test FD".to_string(),
//         ));

//         // Test add and get
//         FDMetaManager::add_meta(fd_meta.clone());
//         let retrieved = FDMetaManager::get_meta("fd1").unwrap();
//         assert_eq!(retrieved.id, "fd1");
//         assert_eq!(retrieved.name, "Test FD");

//         // Test get_all_metas
//         let all_metas = FDMetaManager::get_all_metas();
//         assert_eq!(all_metas.len(), 1);

//         // Test remove
//         let removed = FDMetaManager::remove_meta("fd1").unwrap();
//         assert_eq!(removed.id, "fd1");
//         assert!(FDMetaManager::get_meta("fd1").is_none());

//         // Clean up
//         FDMetaManager::clear();
//     }

// }
