use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::model::cell::CellValue;


pub mod  svrkey{

    pub const KEY_TIME_IN: &'static str = "cmx_time_in";
    pub const KEY_REQUEST_ID: &'static str = "cmx_request_id";
}


/// Context结构体用于管理键值对形式的上下文数据
#[derive(Clone, Debug)]
pub struct SVRContext {

    data: Arc<RwLock<HashMap<String, CellValue>>>,
}

impl SVRContext {
    /// 创建一个新的Context实例
    pub fn new() -> Self {
        SVRContext {
            data: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 设置键值对
    pub fn set<K: Into<String>>(&self, key: K, value: CellValue) {
        let mut data = self.data.write().unwrap();
        data.insert(key.into(), value);
    }

    /// 获取指定键的值
    pub fn get<K: AsRef<str>>(&self, key: K) -> Option<CellValue> {
        let data = self.data.read().unwrap();
        data.get(key.as_ref()).cloned()
    }

    /// 删除指定键的值
    pub fn remove<K: AsRef<str>>(&self, key: K) -> Option<CellValue> {
        let mut data = self.data.write().unwrap();
        data.remove(key.as_ref())
    }

    /// 检查是否包含指定键
    pub fn contains_key<K: AsRef<str>>(&self, key: K) -> bool {
        let data = self.data.read().unwrap();
        data.contains_key(key.as_ref())
    }

    /// 清空所有数据
    pub fn clear(&self) {
        let mut data = self.data.write().unwrap();
        data.clear();
    }
}

impl Default for SVRContext {
    fn default() -> Self {
        Self::new()
    }
}
