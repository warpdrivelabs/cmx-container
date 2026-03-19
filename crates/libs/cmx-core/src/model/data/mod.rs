pub mod response;
pub mod request;
pub mod context;
pub mod dataset;




use std::collections::HashMap;
use crate::model::cell::CellValue;
use serde::{Deserialize, Serialize};


#[derive(Debug, Serialize, Deserialize,Clone)]
pub struct KeyValue {
    // 使用动态类型存储属性
    attributes  : HashMap<String, CellValue>,
    // 使用动态类型存储定义(可以为空)
    definitions : Option<HashMap<String, CellValue>>,
}


impl KeyValue {
    // 创建一个新的 KeyValue 实例
    pub fn new() -> Self {
        Self {
            attributes: HashMap::new(),
            definitions: None,
        }
    }

    // 获取指定键的值
    pub fn get(&self, key: &str) -> Option<&CellValue> {
        self.attributes.get(key)
    }

    // 插入或更新指定键的值
    pub fn put(&mut self, key: String, value: CellValue) {
        self.attributes.insert(key, value);
    }

    // 获取当前存储的键值对数量
    pub fn get_size(&self) -> usize {
        self.attributes.len()
    }

    // 删除指定键的值
    pub fn delete(&mut self, key: &str) -> Option<CellValue> {
        self.attributes.remove(key)
    }

    // 检查指定键是否存在
    pub fn exists(&self, key: &str) -> bool {
        self.attributes.contains_key(key)
    }
}




