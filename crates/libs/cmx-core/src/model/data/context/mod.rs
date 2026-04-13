use crate::model::cell::CellValue;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// #[cfg(feature = "extism")]
// use extism_pdk::*;

pub mod svrkey {

    pub const KEY_TIME_IN: &'static str = "cmx_time_in";
    pub const KEY_REQUEST_ID: &'static str = "cmx_request_id";
}

/// Context结构体用于管理键值对形式的上下文数据
#[derive(Clone, Debug, Serialize, Deserialize)]
// #[cfg_attr(feature = "extism", derive(FromBytes, ToBytes))]
// #[cfg_attr(feature = "extism", encoding(Json))]
pub struct SVRContext {
    data: HashMap<String, CellValue>,

    request: HashMap<String, serde_json::Value>,
    response: HashMap<String, serde_json::Value>,
}

impl SVRContext {
    /// 创建一个新的Context实例
    pub fn new() -> Self {
        SVRContext {
            data: HashMap::new(),
            request: HashMap::new(),
            response: HashMap::new(),
        }
    }

    /// 设置键值对
    pub fn set<K: Into<String>>(&mut self, key: K, value: CellValue) {
        self.data.insert(key.into(), value);
    }

    /// 获取指定键的值
    pub fn get<K: AsRef<str>>(&self, key: K) -> Option<CellValue> {
        self.data.get(key.as_ref()).cloned()
    }

    /// 删除指定键的值
    pub fn remove<K: AsRef<str>>(&mut self, key: K) -> Option<CellValue> {
        self.data.remove(key.as_ref())
    }

    /// 检查是否包含指定键
    pub fn contains_key<K: AsRef<str>>(&self, key: K) -> bool {
        self.data.contains_key(key.as_ref())
    }

    /// 清空所有数据
    pub fn clear(&mut self) {
        self.data.clear();
    }
}

impl Default for SVRContext {
    fn default() -> Self {
        Self::new()
    }
}
