//! WASM 实例包装
//!
//! 封装 wasmtime::Instance 和关联的 Store，提供实例级别的管理。

use chrono::{DateTime, Utc};

/// WASM 实例包装
///
/// 封装 wasmtime::Instance 和其关联的 Store，
/// 每个 WASM 插件对应一个独立的 WasmInstance。
pub struct WasmInstance {
    /// 插件ID
    pub plugin_id: String,

    /// wasmtime Instance 引用（通过 Store 管理）
    instance: wasmtime::Instance,

    /// 关联的 Store（持有运行时状态）
    store: wasmtime::Store<WasmStoreData>,

    /// 模块元信息
    pub module_info: WasmModuleInfo,
}

/// Store 的数据类型
///
/// 存储在 wasmtime::Store 中，宿主函数通过 Caller 访问。
pub struct WasmStoreData {
    /// 当前调用上下文
    pub caller_data: cmx_traits::CallerData,
}

impl WasmStoreData {
    /// 创建新的 Store 数据
    pub fn new(caller_data: cmx_traits::CallerData) -> Self {
        Self { caller_data }
    }
}

/// 模块元信息
#[derive(Debug, Clone)]
pub struct WasmModuleInfo {
    /// 导出函数列表
    pub exports: Vec<String>,

    /// 模块哈希（用于缓存标识，预留）
    pub hash: Option<String>,

    /// 加载时间
    pub loaded_at: DateTime<Utc>,
}

impl WasmInstance {
    /// 创建新的 WASM 实例
    pub fn new(
        plugin_id: String,
        instance: wasmtime::Instance,
        store: wasmtime::Store<WasmStoreData>,
        exports: Vec<String>,
    ) -> Self {
        let module_info = WasmModuleInfo {
            exports,
            hash: None,
            loaded_at: Utc::now(),
        };

        Self {
            plugin_id,
            instance,
            store,
            module_info,
        }
    }

    /// 获取导出函数
    ///
    /// 通过名称查找 WASM 导出的函数。
    pub fn get_export_func(
        &mut self,
        name: &str,
    ) -> Option<wasmtime::Func> {
        self.instance
            .get_func(&mut self.store, name)
    }

    /// 获取 Store 的可变引用
    pub fn store_mut(&mut self) -> &mut wasmtime::Store<WasmStoreData> {
        &mut self.store
    }

    /// 获取 Store 的不可变引用
    pub fn store(&self) -> &wasmtime::Store<WasmStoreData> {
        &self.store
    }

    /// 获取模块元信息
    pub fn module_info(&self) -> &WasmModuleInfo {
        &self.module_info
    }
}
