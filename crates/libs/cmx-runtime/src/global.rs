//! 全局 Extism 引擎管理器
//!
//! 提供全局单例模式，与现有架构兼容。

use crate::{ExtismEngine, ExtismError};
use cmx_traits::runtime::RuntimeInvoker;
use std::sync::{Arc, OnceLock};

/// 全局 Extism 引擎管理器
pub struct GlobalExtismEngine {
    /// 引擎实例
    engine: Arc<ExtismEngine>,
}

/// 全局引擎实例
static GLOBAL_ENGINE: OnceLock<GlobalExtismEngine> = OnceLock::new();

impl GlobalExtismEngine {
    /// 初始化全局引擎
    ///
    /// # 参数
    /// - `engine`: Extism 引擎实例
    ///
    /// # 返回值
    /// 返回初始化结果
    pub fn initialize(engine: Arc<ExtismEngine>) -> Result<(), ExtismError> {
        GLOBAL_ENGINE
            .set(GlobalExtismEngine { engine })
            .map_err(|_| ExtismError::InternalError("全局引擎已初始化".to_string()))?;
        Ok(())
    }

    /// 获取全局引擎引用
    ///
    /// # Panics
    /// 如果全局引擎未初始化，将 panic
    ///
    /// # 返回值
    /// 返回全局引擎引用
    pub fn get() -> &'static GlobalExtismEngine {
        GLOBAL_ENGINE
            .get()
            .expect("全局引擎未初始化，请先调用 initialize()")
    }

    /// 获取运行时调用器
    ///
    /// # 返回值
    /// 返回 RuntimeInvoker trait 对象
    pub fn get_as_invoker() -> Arc<dyn RuntimeInvoker> {
        Self::get().engine.clone()
    }

    /// 获取引擎实例
    ///
    /// # 返回值
    /// 返回引擎实例的 Arc 引用
    pub fn engine(&self) -> Arc<ExtismEngine> {
        self.engine.clone()
    }

    /// 检查全局引擎是否已初始化
    ///
    /// # 返回值
    /// 返回是否已初始化
    pub fn is_initialized() -> bool {
        GLOBAL_ENGINE.get().is_some()
    }
}
