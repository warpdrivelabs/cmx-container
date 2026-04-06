//! WASM Linker 适配器
//!
//! 实现 cmx_traits::WasmLinker trait，
//! 将 cmx-traits 的抽象接口适配到 wasmtime::Linker。
//!
//! # 宿主函数签名约定
//!
//! 所有通过此适配器注册的宿主函数使用统一的 WASM 签名：
//! - 带返回值：`(i32, i32) -> i32`
//!   - 参数1：输入数据指针（i32）
//!   - 参数2：输入数据长度（i32）
//!   - 返回值：输出数据长度（i32），负值表示错误
//! - 无返回值：`(i32, i32) -> ()`
//!   - 参数1：输入数据指针（i32）
//!   - 参数2：输入数据长度（i32）
//!
//! # 实现说明
//!
//! 由于 wasmtime::Caller 的生命周期与 func_wrap 闭包绑定，
//! 无法创建独立的适配器结构体持有 Caller 引用。
//! 因此使用闭包捕获方式创建 WasmCallerAccess 的 trait 对象，
//! 闭包通过可变引用捕获 Caller，避免自引用借用问题。

use std::cell::RefCell;

use cmx_traits::{HostFuncError, HostFuncWrapper, HostVoidFuncWrapper, WasmCallerAccess, WasmLinker};

use crate::instance::WasmStoreData;

thread_local! {
    /// 线程局部输出缓冲区
    ///
    /// 宿主函数的返回数据通过此缓冲区传递。
    static OUTPUT_BUFFER: RefCell<Vec<u8>> = RefCell::new(Vec::new());
}

/// 运行时 Linker 适配器
///
/// 实现 cmx_traits::WasmLinker trait，
/// 将类型擦除的宿主函数闭包适配为 wasmtime::Linker 的强类型函数签名。
pub struct RuntimeLinkerAdapter<'a> {
    /// wasmtime Linker 可变引用
    inner: &'a mut wasmtime::Linker<WasmStoreData>,
}

impl<'a> RuntimeLinkerAdapter<'a> {
    /// 创建新的 Linker 适配器
    pub fn new(linker: &'a mut wasmtime::Linker<WasmStoreData>) -> Self {
        Self { inner: linker }
    }
}

/// 从 wasmtime::Caller 获取 memory 导出
///
/// # 参数
///
/// * `caller` - wasmtime 调用者引用
///
/// # 返回值
///
/// 返回 memory 的 wasmtime::Memory 引用。
fn get_memory(caller: &mut wasmtime::Caller<'_, WasmStoreData>) -> Result<wasmtime::Memory, HostFuncError> {
    caller
        .get_export("memory")
        .and_then(|ext| ext.into_memory())
        .ok_or_else(|| HostFuncError::InvalidParam("WASM 模块未导出 memory".to_string()))
}

impl WasmLinker for RuntimeLinkerAdapter<'_> {
    /// 注册一个带返回值的宿主函数
    ///
    /// 闭包签名：(i32, i32) -> i32
    /// 返回值为输出数据长度，负值表示错误。
    fn define(
        &mut self,
        module: &str,
        name: &str,
        func: HostFuncWrapper,
    ) -> Result<(), HostFuncError> {
        let func_label = format!("{}/{}", module, name);
        let func_label_for_closure = func_label.clone();

        self.inner
            .func_wrap(
                module,
                name,
                move |mut caller: wasmtime::Caller<'_, WasmStoreData>,
                      input_ptr: i32,
                      input_len: i32|
                      -> i32 {
                    let mut caller_access: Box<dyn WasmCallerAccess> = Box::new(CallerAccessProxy {
                        caller: &mut caller,
                    });

                    let input = match caller_access.read_memory(input_ptr as u32, input_len as u32) {
                        Ok(data) => data,
                        Err(e) => {
                            tracing::error!("宿主函数 {} 读取输入失败: {}", func_label_for_closure, e);
                            return -1;
                        }
                    };

                    let result = match func(caller_access.as_mut(), &input) {
                        Ok(data) => data,
                        Err(e) => {
                            tracing::error!("宿主函数 {} 执行失败: {}", func_label_for_closure, e);
                            return -1;
                        }
                    };

                    // 通过线程局部变量传递返回数据
                    let output_len = result.len() as i32;
                    OUTPUT_BUFFER.with(|buf| {
                        *buf.borrow_mut() = result;
                    });

                    output_len
                },
            )
            .map_err(|e| HostFuncError::registration_failed(module, name, e.to_string()))?;

        tracing::debug!("注册宿主函数: {}", func_label);
        Ok(())
    }

    /// 注册一个无返回值的宿主函数
    ///
    /// 闭包签名：(i32, i32) -> ()
    fn define_void(
        &mut self,
        module: &str,
        name: &str,
        func: HostVoidFuncWrapper,
    ) -> Result<(), HostFuncError> {
        let func_label = format!("{}/{}", module, name);
        let func_label_for_closure = func_label.clone();

        self.inner
            .func_wrap(
                module,
                name,
                move |mut caller: wasmtime::Caller<'_, WasmStoreData>,
                      input_ptr: i32,
                      input_len: i32| {
                    let mut caller_access: Box<dyn WasmCallerAccess> = Box::new(CallerAccessProxy {
                        caller: &mut caller,
                    });

                    let input = match caller_access.read_memory(input_ptr as u32, input_len as u32) {
                        Ok(data) => data,
                        Err(e) => {
                            tracing::error!("宿主函数 {} 读取输入失败: {}", func_label_for_closure, e);
                            return;
                        }
                    };

                    if let Err(e) = func(caller_access.as_mut(), &input) {
                        tracing::error!("宿主函数 {} 执行失败: {}", func_label_for_closure, e);
                    }
                },
            )
            .map_err(|e| HostFuncError::registration_failed(module, name, e.to_string()))?;

        tracing::debug!("注册宿主函数(无返回值): {}", func_label);
        Ok(())
    }
}

/// Caller 访问代理
///
/// 使用独立的生命周期参数，避免自引用借用问题。
/// 通过 `&'b mut Caller<'a, T>` 形式持有引用，
/// 其中 `'a` 是 Caller 内部生命周期，`'b` 是引用的有效期。
struct CallerAccessProxy<'a, 'b> {
    /// wasmtime Caller 的可变引用
    /// `'a` 为 Caller 的内部数据生命周期
    /// `'b` 为此引用的有效期
    caller: &'b mut wasmtime::Caller<'a, WasmStoreData>,
}

impl WasmCallerAccess for CallerAccessProxy<'_, '_> {
    /// 从 WASM 线性内存读取字节
    fn read_memory(&mut self, offset: u32, len: u32) -> Result<Vec<u8>, HostFuncError> {
        let memory = get_memory(&mut self.caller)?;

        let data = memory
            .data(&self.caller)
            .get(offset as usize..)
            .and_then(|slice| slice.get(..len as usize))
            .ok_or(HostFuncError::MemoryOutOfBounds { offset, len })?;
        Ok(data.to_vec())
    }

    /// 向 WASM 线性内存写入字节
    fn write_memory(&mut self, offset: u32, data: &[u8]) -> Result<(), HostFuncError> {
        let memory = get_memory(&mut self.caller)?;

        let end = offset as usize + data.len();
        let mem_data = memory.data_mut(&mut self.caller);
        if end > mem_data.len() {
            return Err(HostFuncError::MemoryOutOfBounds {
                offset,
                len: data.len() as u32,
            });
        }
        mem_data[offset as usize..end].copy_from_slice(data);
        Ok(())
    }

    /// 动态内存分配（暂未实现）
    fn alloc_and_write(&mut self, _data: &[u8]) -> Result<(u32, u32), HostFuncError> {
        Err(HostFuncError::InvalidParam(
            "动态内存分配暂未实现，请使用 write_memory 指定偏移量".to_string(),
        ))
    }

    /// 获取当前调用上下文
    fn caller_data(&self) -> &cmx_traits::CallerData {
        &self.caller.data().caller_data
    }
}
