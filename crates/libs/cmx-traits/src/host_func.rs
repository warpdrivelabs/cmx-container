//! WASM 宿主函数注册 trait 定义
//!
//! 定义宿主函数提供者接口。
//! 各模块（cmx-database, cmx-buffer, cmx-plugin 等）通过实现
//! ExtismFunctionProvider trait 注册自身提供的宿主函数，
//! cmx-runtime 通过此 trait 消费这些注册。
//!
//! # 设计目标
//!
//! - cmx-runtime 不依赖任何宿主函数提供模块
//! - 各模块不依赖 cmx-runtime（仅依赖 cmx-traits）
//! - 所有组装逻辑集中在 web-server 初始化阶段
//!
//! # Extism 适配
//!
//! 使用 Extism 的 PluginBuilder 作为宿主函数注册的目标。
//! 各模块通过 PluginBuilder 注册自己的宿主函数。

use crate::error::HostFuncError;

/// 宿主函数注册器 trait（Extism 版本）
///
/// 各模块通过实现此 trait，将自身提供的宿主函数注册到 Extism 引擎。
/// cmx-runtime 在创建 WASM 插件时，遍历所有注册器完成函数注册。
///
/// # 实现示例
///
/// ```rust,ignore
/// use cmx_traits::ExtismFunctionProvider;
/// use extism::PluginBuilder;
///
/// struct DatabaseHostFunctions {
///     db_manager: Arc<DatabaseManager>,
/// }
///
/// impl ExtismFunctionProvider for DatabaseHostFunctions {
///     fn namespace(&self) -> &str { "cmx:database" }
///
///     fn register_functions(&self, builder: &mut PluginBuilder) -> Result<(), HostFuncError> {
///         // 使用 extism::host_fn! 宏定义宿主函数
///         extism::host_fn!(db_query(_user_data: (); request: String) -> String {
///             // 实现数据库查询逻辑
///             Ok("result".to_string())
///         });
///         
///         builder.with_function(
///             "db_query",
///             [extism::ValType::I64],
///             [extism::ValType::I64],
///             extism::UserData::new(()),
///             db_query,
///         );
///         
///         Ok(())
///     }
///
///     fn provided_functions(&self) -> Vec<&str> {
///         vec!["db_query", "db_execute"]
///     }
/// }
/// ```
pub trait ExtismFunctionProvider: Send + Sync {
    /// 命名空间标识
    ///
    /// 用于日志、调试和函数名前缀。
    /// 建议使用 `cmx:模块名` 格式，如 "cmx:database", "cmx:buffer"。
    fn namespace(&self) -> &str;

    /// 注册所有宿主函数
    ///
    /// 实现方在此方法中通过 PluginBuilder 注册宿主函数。
    /// cmx-runtime 将在插件加载时调用此方法。
    ///
    /// # 参数
    ///
    /// - `builder`: PluginBuilder 实例，用于注册宿主函数
    ///
    /// # 返回值
    ///
    /// 返回注册结果，或错误。
    fn register_functions(&self, builder: &mut extism::PluginBuilder) -> Result<(), HostFuncError>;

    /// 列出该提供者注册的所有函数名
    ///
    /// 用于调试、元数据查询和文档生成。
    /// 默认返回空列表。
    fn provided_functions(&self) -> Vec<&str> {
        Vec::new()
    }
}
