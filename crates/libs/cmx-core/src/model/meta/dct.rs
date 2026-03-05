use std::sync::{Arc, RwLock};
use std::collections::HashMap;
// use chrono::NaiveDateTime;
// use rust_decimal::Decimal;
use serde::{Serialize, Deserialize};
use crate::model::cell::CellValue;
// use crate::data::cell::SimpleCellValue;
// use crate::data::dataset::col::CMX_SYS_OBJCOLS;
use crate::model::data::dataset::TableSchema;
use crate::model::data::KeyValue;
// use ospbase::data::dataset::ColumnDef;
use lazy_static::lazy_static;

// use super::tables::CMX_SYS_TABLE_NAMES::{CMX_SYS_DICTS, CMX_SYS_OBJCOLS};

// #[allow(non_camel_case_types)]
// // Add new enum for DCT fields
// #[derive(Debug, Clone, Hash, Eq, PartialEq, Serialize, Deserialize)]
// pub enum DCTField {
//     DCT_ID,              // 字典ID
//     OBJ_ID,             // 对象ID
//     DCT_MC,             // 字典名称
//     DCT_DES,            // 字典描述
//     DCT_QXOBJID,        // 权限对象ID
//     DCT_TYPE,           // 字典类型
//     DCT_BMCOLID,        // 编码列ID
//     DCT_BZCOLID,        // 标准列ID
//     DCT_MCCOLID,        // 名称列ID
//     DCT_JSCOLID,        // 简称列ID
//     DCT_MXCOLID,        // 明细列ID
//     DCT_BMSTRU,         // 编码结构
//     DCT_PTCOLID,        // 拼音列ID
//     DCT_KZCOLID,        // 扩展列ID
//     CMX_SYS_ID,             // 系统ID
//     DCT_FIXLEN,         // 固定长度
//     DCT_STEP,           // 步长
//     DCT_KPI,            // KPI标识
//     DCT_SELECT,         // 选择标识
//     DCT_MUNIT,          // 计量单位
//     DCT_CTLONE,         // 控制标识
//     DCT_EXTAUTO,        // 扩展自动
//     DCT_CREATE,         // 创建时间
//     DCT_CHANGE,         // 修改时间
//     DCT_NOTUSE,         // 不使用标识
//     DCT_SUBJECT,        // 主题
//     DCT_UNIT,           // 单位
//     DCT_CST,            // 自定义标识
//     DCT_FKEY1,          // 外键1
//     DCT_FKEYDCT1,       // 外键字典1
//     DCT_SYNCDATE,       // 同步日期
//     F_STAU,             // 状态
//     F_CRDATE,           // 创建日期
//     F_CHDATE,           // 修改日期
//     DCT_QXSTAT,         // 权限状态
//     DCT_AFFIXDCT,       // 附加字典
//     F_GUID,             // 全局唯一标识
//     F_CRUSER,           // 创建用户
//     F_CHUSER,           // 修改用户
//     F_ENABLE,           // 启用标识
//     F_DELETED,          // 删除标识
//     F_DICTSFKEY,        // 字典外键
// }

#[derive(Debug, Serialize, Deserialize,Clone)]
pub struct DCTMeta {
    /// 字典ID
    pub dct_id: String,

    data: HashMap<String, CellValue>,

    pub info: Option<KeyValue>,    
    pub table_schema: TableSchema,
    pub settings: Option<KeyValue>,
    #[serde(skip)]  // 这个字段不需要序列化,直接从DCTMetaManager中获取
    pub dct_metas: Option<HashMap<String, Arc<DCTMeta>>>,
}

impl DCTMeta {
    // pub fn new(
    //     id: String,
    //     name: String,
    //     table_schema: TableSchema,
    //     settings: Option<KeyValue>,
    // ) -> Self {
    //     Self {      
    //         id,
    //         name,
    //         info: None,
    //         table_schema,
    //         settings,
    //         dct_metas: None,
    //     }
    // }
    // Helper methods to get/set values
    pub fn get(&self, field: &str) -> Option<&CellValue> {
        self.data.get(field)
    }

    pub fn set(&mut self, field: &str, value: CellValue) {
        self.data.insert(field.to_string(), value);
    }
    // pub fn get_string(&self, field: &str) -> Option<String> {
    //     self.get(field).and_then(|v| v.to_string().map(|s| s.to_string()))
    // }
    // pub fn get_integer(&self, field: &str) -> Option<i64> {
    //     self.get(field).and_then(|v| v.to_i64())
    // }
    // // Required field getters
    // pub fn dct_id(&self) -> &str {
    //     self.get(&DCTField::DCT_ID)
    //         .and_then(|v| v.to_string().unwrap().as_str())
    // }

    // pub fn obj_id(&self) -> &str {
    //     self.get(&DCTField::ObjId)
    //         .and_then(|v| v.as_str())
    //         .unwrap_or_default()
    // }
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        let meta: DCTMeta = serde_json::from_str(json)?;
        // meta.process_foreign_keys();
        Ok(meta)
    }
    // // 新增函数：处理外键关联
    // pub fn process_foreign_keys(&mut self) {
    //     let mut dct_metas = HashMap::new();
        
    //     // 遍历所有列定义
    //     for column in &self.table_schema.columns {
    //         // 检查是否是外键列
    //         if column.col_isfkey().unwrap() {
    //             // 如果有外键表名，获取对应的 DCTMeta
    //             if let Some(foreign_table) = column.get(&CMX_SYS_OBJCOLS::COL_FOBJ) {
    //                 if let Some(foreign_meta) = DCTMetaManager::get_meta(foreign_table.as_str().unwrap()) {
    //                     dct_metas.insert(foreign_table.as_str().unwrap().to_string(), foreign_meta);
    //                 }
    //             }
    //         }
    //     }
        
    //     // 如果找到了相关的外键 meta，设置到 dct_metas
    //     if !dct_metas.is_empty() {
    //         self.dct_metas = Some(dct_metas);
    //     }
    // }
}

lazy_static! {
    static ref DCT_METAS: RwLock<HashMap<String, Arc<DCTMeta>>> = RwLock::new(HashMap::new());
}

#[derive(Debug)]
pub struct DCTMetaManager;

impl DCTMetaManager {
    pub fn add_meta(meta: Arc<DCTMeta>) {
        let mut metas = DCT_METAS.write().unwrap();
        metas.insert(meta.dct_id.clone(), meta);
    }

    pub fn get_meta(id: &str) -> Option<Arc<DCTMeta>> {
        let metas = DCT_METAS.read().unwrap();
        metas.get(id).cloned()
    }

    // 新增：清理所有元数据（主要用于测试）
    pub fn clear() {
        let mut metas = DCT_METAS.write().unwrap();
        metas.clear();
    }

    // 新增：获取所有元数据
    pub fn get_all() -> Vec<Arc<DCTMeta>> {
        let metas = DCT_METAS.read().unwrap();
        metas.values().cloned().collect()
    }
}

// #[cfg(test)]
// mod tests {
//     use crate::data::dataset::col::{ColumnDef, ColumnType};

//     use super::*;
//     use std::collections::HashMap;

//     #[test]
//     fn test_dct_meta_serialization() {
//         // 创建一个简单的 TableSchema
//         let columns = vec![
//             ColumnDef {
//                 name: "id".to_string(),
//                 column_type: ColumnType::Integer,
//                 nullable: false,
//                 is_foreign_key: false,  
//                 foreign_table_name: None,
//                 foreign_column_name: None,
//                 multilingual: false,
//                 default_value: None,
//                 comment: None,
//             },
//             ColumnDef {
//                 name: "name".to_string(),
//                 column_type: ColumnType::Text,
//                 nullable: false,
//                 is_foreign_key: false,
//                 foreign_table_name: None,
//                 foreign_column_name: None,
//                 multilingual: false,
//                 default_value: None,
//                 comment: None,
//             },
//             ColumnDef {
//                 name: "created_at".to_string(),
//                 column_type: ColumnType::Timestamp,
//                 nullable: false,
//                 is_foreign_key: false,
//                 foreign_table_name: None,
//                 foreign_column_name: None,
//                 multilingual: false,
//                 default_value: None,
//                 comment: None,
//             },
//         ];
//         let schema = TableSchema::new("test_table".to_string(), columns);

//         // 创建测试用的 DCTMeta
//         let meta = DCTMeta::new(
//             "test_id".to_string(),
//             "test_name".to_string(),
//             // "test_encoding".to_string(),
//             // "test_permission".to_string(),
//             // "test_hierarchy".to_string(),
//             schema,
//             None,
//         );

//         // 测试序列化
//         let json = meta.to_json().expect("Failed to serialize");
        
//         // 测试反序列化
//         let deserialized_meta = DCTMeta::from_json(&json).expect("Failed to deserialize");
        
//         // 验证字段
//         assert_eq!(meta.id, deserialized_meta.id);
//         assert_eq!(meta.name, deserialized_meta.name);
//         // assert_eq!(meta.encoding_structure, deserialized_meta.encoding_structure);
//         // assert_eq!(meta.data_permission_id, deserialized_meta.data_permission_id);
//         // assert_eq!(meta.hierarchy_type, deserialized_meta.hierarchy_type);
//         // assert_eq!(meta.key_value.as_ref(), deserialized_meta.key_value.unwrap());
//         assert_eq!(meta.dct_metas.is_none(), deserialized_meta.dct_metas.is_none());
//     }

//     #[test]
//     fn test_nested_dct_meta() {
//         // 创建一个简单的 TableSchema
//         let columns = vec![
//             ColumnDef {
//                 name: "id".to_string(),
//                 column_type: ColumnType::Integer,
//                 nullable: false,
//                 is_foreign_key: false,
//                 foreign_table_name: None,
//                 foreign_column_name: None,
//                 multilingual: false,
//                 default_value: None,
//                 comment: None,
//             },
//             ColumnDef {
//                 name: "name".to_string(),
//                 column_type: ColumnType::Text,
//                 nullable: false,
//                 is_foreign_key: false,
//                 foreign_table_name: None,
//                 foreign_column_name: None,
//                 multilingual: false,
//                 default_value: None,
//                 comment: None,
//             },
//             ColumnDef {
//                 name: "created_at".to_string(),
//                 column_type: ColumnType::Timestamp,
//                 nullable: false,
//                 is_foreign_key: false,
//                 foreign_table_name: None,
//                 foreign_column_name: None,
//                 multilingual: false,
//                 default_value: None,
//                 comment: None,
//             },
//         ];
//         let schema = TableSchema::new("test_table".to_string(), columns);

//         // 创建父 DCTMeta
//         let mut meta = DCTMeta::new(
//             "parent_id".to_string(),
//             "parent_name".to_string(),
//             // "parent_encoding".to_string(),
//             // "parent_permission".to_string(),
//             // "parent_hierarchy".to_string(),
//             schema.clone(),
//             None,
//         );

//         // 创建子 DCTMeta
//         let mut nested_metas = HashMap::new();
//         let child_meta = DCTMeta::new(
//             "child_id".to_string(),
//             "child_name".to_string(),
//             // "child_encoding".to_string(),
//             // "child_permission".to_string(),
//             // "child_hierarchy".to_string(),
//             schema,
//             None,
//         );
//         nested_metas.insert("child1".to_string(), child_meta.into());
//         meta.dct_metas = Some(nested_metas);

//         // 测试序列化和反序列化
//         let json = meta.to_json().expect("Failed to serialize nested structure");
//         let deserialized_meta = DCTMeta::from_json(&json).expect("Failed to deserialize nested structure");

//         // 验证嵌套结构
//         assert!(deserialized_meta.dct_metas.is_some());
//         let child = deserialized_meta.dct_metas.as_ref().unwrap().get("child1").expect("Child not found");
//         assert_eq!(child.id, "child_id");
//         assert_eq!(child.name, "child_name");
//         // assert_eq!(child.encoding_structure, "child_encoding");
//         // assert_eq!(child.data_permission_id, "child_permission");
//         // assert_eq!(child.hierarchy_type, "child_hierarchy");
//     }

//     #[test]
//     fn test_dct_meta_manager() {
//         // let manager = DCTMetaManager::new();
        
//         // 清理之前的数据
//         DCTMetaManager::clear();

//         // 创建测试用的 DCTMeta
//         let meta = DCTMeta::new(
//             "test_id".to_string(),
//             "test_name".to_string(),
//             // "test_encoding".to_string(),
//             // "test_permission".to_string(),
//             // "test_hierarchy".to_string(),
//             TableSchema::new("test_table".to_string(), vec![]),
//             None,
//         );

//         // 添加元数据到管理器
//         DCTMetaManager::add_meta(Arc::new(meta));

//         // 获取元数据
//         let retrieved_meta = DCTMetaManager::get_meta("test_id").expect("Meta not found");

//         // 验证元数据
//         assert_eq!(retrieved_meta.id, "test_id");
//         assert_eq!(retrieved_meta.name, "test_name");
//         // assert_eq!(retrieved_meta.encoding_structure, "test_encoding");
//         // assert_eq!(retrieved_meta.data_permission_id, "test_permission");
//         // assert_eq!(retrieved_meta.hierarchy_type, "test_hierarchy");
//     }
// }

