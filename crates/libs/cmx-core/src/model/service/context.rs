//! 服务运行时上下文模块
//!
//! 包含服务调用上下文 SVRContext，用于在节点间传递数据。

use std::collections::HashMap;
use serde::{Deserialize, Serialize};

/// 服务调用上下文
///
/// 用于在服务编排的各节点间传递数据，包含：
/// - 初始输入
/// - 请求头
/// - 各步骤的输出缓存
/// - 事务ID（仅在事务框内执行时设置）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SVRContext {
    /// 初始输入数据
    pub initial_input: String,
    /// 请求头
    pub headers: HashMap<String, String>,
    /// 各步骤的输出缓存（key: 节点ID，value: 输出字符串）
    #[serde(default)]
    pub step_outputs: HashMap<String, String>,
    /// 事务ID（仅在事务框内执行时设置）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub txn_id: Option<String>,
}

impl SVRContext {
    /// 创建新的上下文
    ///
    /// # 参数
    /// - `initial_input`: 初始输入数据
    /// - `headers`: 请求头
    pub fn new(initial_input: String, headers: HashMap<String, String>) -> Self {
        Self {
            initial_input,
            headers,
            step_outputs: HashMap::new(),
            txn_id: None,
        }
    }

    /// 获取指定步骤的输出
    ///
    /// # 参数
    /// - `step_id`: 步骤ID（节点ID）
    ///
    /// # 返回值
    /// - `Option<String>`: 该步骤的输出字符串，不存在则返回 None
    pub fn get_step_output(&self, step_id: &str) -> Option<&String> {
        self.step_outputs.get(step_id)
    }

    /// 设置指定步骤的输出
    ///
    /// # 参数
    /// - `step_id`: 步骤ID（节点ID）
    /// - `output`: 该步骤的输出字符串
    pub fn set_step_output(&mut self, step_id: impl Into<String>, output: impl Into<String>) {
        self.step_outputs.insert(step_id.into(), output.into());
    }

    /// 添加指定步骤的输出（set_step_output 的别名）
    pub fn add_step_output(&mut self, step_id: impl Into<String>, output: impl Into<String>) {
        self.set_step_output(step_id, output);
    }

    /// 清除指定步骤的输出
    ///
    /// # 参数
    /// - `step_id`: 步骤ID（节点ID）
    pub fn remove_step_output(&mut self, step_id: &str) {
        self.step_outputs.remove(step_id);
    }

    /// 清除所有步骤输出
    pub fn clear_step_outputs(&mut self) {
        self.step_outputs.clear();
    }

    /// 设置事务ID
    ///
    /// # 参数
    /// - `txn_id`: 事务ID
    pub fn set_txn_id(&mut self, txn_id: String) {
        self.txn_id = Some(txn_id);
    }

    /// 清除事务ID
    pub fn clear_txn_id(&mut self) {
        self.txn_id = None;
    }
}
