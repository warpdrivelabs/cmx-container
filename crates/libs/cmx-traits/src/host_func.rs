//! WASM 宿主函数注册 trait 定义
//!
//! 定义宿主函数提供者接口和 WASM Linker 抽象。
//! 各模块（cmx-database, cmx-buffer, cmx-plugin 等）通过实现
//! HostFunctionProvider trait 注册自身提供的宿主函数，
//! cmx-runtime 通过 WasmLinker trait 消费这些注册。
//!
//! # 设计目标
//!
//! - cmx-runtime 不依赖任何宿主函数提供模块
//! - 各模块不依赖 cmx-runtime（仅依赖 cmx-traits）
//! - 所有组装逻辑集中在 web-server 初始化阶段
//!
//! # 数据传递协议
//!
//! WASM 端调用宿主函数时传递 (input_ptr, input_len) 指向 WASM 线性内存中的输入数据。
//! cmx-runtime 的 linker_adapter 负责从 WASM 内存读取输入数据，
//! 然后将解析后的字节传递给宿主函数闭包。宿主函数无需关心内存布局。

use crate::caller_data::CallerData;
use crate::error::HostFuncError;

/// WASM 调用者访问接口
///
/// 提供宿主函数访问 WASM 线性内存和调用上下文的能力。
/// cmx-runtime 提供具体实现，cmx-traits 仅定义接口。
pub trait WasmCallerAccess {
    /// 从 WASM 线性内存读取字节
    ///
    /// # 参数
    ///
    /// * `offset` - 内存偏移量
    /// * `len` - 读取长度
    ///
    /// # 错误
    ///
    /// 越界访问时返回错误。
    fn read_memory(&mut self, offset: u32, len: u32) -> Result<Vec<u8>, HostFuncError>;

    /// 向 WASM 线性内存写入字节
    ///
    /// # 参数
    ///
    /// * `offset` - 内存偏移量
    /// * `data` - 要写入的数据
    ///
    /// # 错误
    ///
    /// 越界访问时返回错误。
    fn write_memory(&mut self, offset: u32, data: &[u8]) -> Result<(), HostFuncError>;

    /// 分配 WASM 内存并写入数据
    ///
    /// 通过 WASM 的内存分配函数分配空间，写入数据后返回指针和长度。
    ///
    /// # 参数
    ///
    /// * `data` - 要写入的数据
    ///
    /// # 返回值
    ///
    /// 返回 (指针偏移量, 数据长度) 元组。
    fn alloc_and_write(&mut self, data: &[u8]) -> Result<(u32, u32), HostFuncError>;

    /// 获取当前调用上下文
    fn caller_data(&self) -> &CallerData;
}

/// WASM Linker 抽象接口
///
/// cmx-runtime 提供具体实现（RuntimeLinkerAdapter），
/// 将 cmx-traits 的调用适配到 wasmtime::Linker。
pub trait WasmLinker: Send + Sync {
    /// 注册一个带返回值的宿主函数
    ///
    /// # 参数
    ///
    /// * `module` - 模块命名空间（如 "cmx:database"）
    /// * `name` - 函数名（如 "execute_sql"）
    /// * `func` - 宿主函数闭包，接收 (WasmCallerAccess, 输入数据) 返回输出字节
    fn define(
        &mut self,
        module: &str,
        name: &str,
        func: HostFuncWrapper,
    ) -> Result<(), HostFuncError>;

    /// 注册一个无返回值的宿主函数（仅副作用）
    ///
    /// # 参数
    ///
    /// * `module` - 模块命名空间
    /// * `name` - 函数名
    /// * `func` - 宿主函数闭包，接收 (WasmCallerAccess, 输入数据)
    fn define_void(
        &mut self,
        module: &str,
        name: &str,
        func: HostVoidFuncWrapper,
    ) -> Result<(), HostFuncError>;
}

/// 带返回值的宿主函数包装器
///
/// 使用 `Box<dyn Fn>` 进行类型擦除，避免暴露 wasmtime 的泛型参数。
/// 宿主函数接收 WASM 调用者访问接口和已读取的输入数据，返回字节数据。
///
/// # 参数
///
/// * `&mut dyn WasmCallerAccess` - WASM 调用者访问接口（用于写回输出、获取上下文）
/// * `&[u8]` - 输入数据（由 linker_adapter 从 WASM 内存预读取）
pub type HostFuncWrapper = Box<
    dyn Fn(&mut dyn WasmCallerAccess, &[u8]) -> Result<Vec<u8>, HostFuncError> + Send + Sync,
>;

/// 无返回值的宿主函数包装器
///
/// 仅执行副作用（如日志记录），不返回数据。
///
/// # 参数
///
/// * `&mut dyn WasmCallerAccess` - WASM 调用者访问接口
/// * `&[u8]` - 输入数据（由 linker_adapter 从 WASM 内存预读取）
pub type HostVoidFuncWrapper =
    Box<dyn Fn(&mut dyn WasmCallerAccess, &[u8]) -> Result<(), HostFuncError> + Send + Sync>;

/// 宿主函数注册器 trait
///
/// 各模块通过实现此 trait，将自身提供的宿主函数注册到 WASM Linker。
/// cmx-runtime 在创建 WASM 实例时，遍历所有注册器完成 Linker 配置。
///
/// # 实现示例
///
/// ```rust,ignore
/// struct DatabaseHostFunctions {
///     db_manager: Arc<DatabaseManager>,
/// }
///
/// impl HostFunctionProvider for DatabaseHostFunctions {
///     fn namespace(&self) -> &str { "cmx:database" }
///
///     fn register_functions(&self, linker: &mut dyn WasmLinker) -> Result<(), HostFuncError> {
///         let db = self.db_manager.clone();
///         linker.define("cmx:database", "execute_sql", Box::new(move |caller, input| {
///             // input 已由 linker_adapter 从 WASM 内存预读取
///             let request: DbRequest = serde_json::from_slice(input)?;
///             // 执行数据库操作...
///             Ok(serde_json::to_vec(&response)?)
///         }))?;
///         Ok(())
///     }
///
///     fn provided_functions(&self) -> Vec<&str> {
///         vec!["cmx:database/execute_sql"]
///     }
/// }
/// ```
pub trait HostFunctionProvider: Send + Sync {
    /// 命名空间标识
    ///
    /// 用于日志、调试和函数名前缀。
    /// 建议使用 `cmx:模块名` 格式，如 "cmx:database", "cmx:buffer"。
    fn namespace(&self) -> &str;

    /// 向 Linker 注册所有宿主函数
    ///
    /// 实现方在此方法中调用 linker.define() 或 linker.define_void()
    /// 注册本模块提供的所有宿主函数。
    ///
    /// # 参数
    ///
    /// * `linker` - WASM Linker 抽象接口
    fn register_functions(&self, linker: &mut dyn WasmLinker) -> Result<(), HostFuncError>;

    /// 列出该提供者注册的所有函数名
    ///
    /// 用于调试、元数据查询和文档生成。
    /// 默认返回空列表。
    fn provided_functions(&self) -> Vec<&str> {
        Vec::new()
    }
}
