//! WASM 宿主函数注册 trait 定义。
//!
//! 定义宿主函数提供者接口，实现依赖反转。
//! 各模块（cmx-database、cmx-buffer、cmx-plugin 等）通过实现
//! HostFunctionProvider trait 提供业务逻辑函数，
//! cmx-runtime 负责将这些函数包装成 Extism 宿主函数。
//!
//! # 设计目标
//!
//! - 各模块不依赖 extism，只依赖 cmx-traits。
//! - cmx-runtime 是唯一依赖 extism 的模块。
//! - 所有组装逻辑集中在 cmx-runtime。

use crate::error::HostFuncError;

/// 参数类型枚举。
///
/// 描述宿主函数的参数类型，与 Extism 的 `ValType` 对应。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ValType {
    /// 32 位整数。
    I32,
    /// 64 位整数。
    I64,
    /// 32 位浮点数。
    F32,
    /// 64 位浮点数。
    F64,
    /// 指针（通常是 I64）。
    #[default]
    Ptr,
}

impl ValType {
    /// 转换为 Extism 的 `ValType`。
    ///
    /// 仅在 cmx-runtime 中使用。
    ///
    /// # Returns
    ///
    /// 返回对应的 `extism::ValType`。
    pub fn to_extism(self) -> extism::ValType {
        match self {
            ValType::I32 => extism::ValType::I32,
            ValType::I64 => extism::ValType::I64,
            ValType::F32 => extism::ValType::F32,
            ValType::F64 => extism::ValType::F64,
            ValType::Ptr => extism::ValType::I64,
        }
    }
}

/// 宿主函数定义。
///
/// 描述一个宿主函数的元信息，包括函数名、参数类型和命名空间。
#[derive(Debug, Clone)]
pub struct HostFunctionDef {
    /// 函数名。
    pub name: &'static str,
    /// 输入参数类型列表。
    pub input_types: &'static [ValType],
    /// 输出参数类型列表。
    pub output_types: &'static [ValType],
    /// 命名空间。
    pub namespace: &'static str,
}

impl HostFunctionDef {
    /// 创建新的宿主函数定义。
    ///
    /// # Arguments
    ///
    /// * `name` - 函数名。
    /// * `namespace` - 命名空间。
    /// * `input_types` - 输入参数类型列表。
    /// * `output_types` - 输出参数类型列表。
    ///
    /// # Returns
    ///
    /// 返回新的 [`HostFunctionDef`]。
    pub fn new(
        name: &'static str,
        namespace: &'static str,
        input_types: &'static [ValType],
        output_types: &'static [ValType],
    ) -> Self {
        Self {
            name,
            input_types,
            output_types,
            namespace,
        }
    }

    /// 创建无输入参数的函数定义。
    ///
    /// # Arguments
    ///
    /// * `name` - 函数名。
    /// * `namespace` - 命名空间。
    /// * `output_types` - 输出参数类型列表。
    ///
    /// # Returns
    ///
    /// 返回新的 [`HostFunctionDef`]，`input_types` 为空切片。
    pub fn no_input(
        name: &'static str,
        namespace: &'static str,
        output_types: &'static [ValType],
    ) -> Self {
        Self {
            name,
            input_types: &[],
            output_types,
            namespace,
        }
    }

    /// 创建无输出参数的函数定义。
    ///
    /// # Arguments
    ///
    /// * `name` - 函数名。
    /// * `namespace` - 命名空间。
    /// * `input_types` - 输入参数类型列表。
    ///
    /// # Returns
    ///
    /// 返回新的 [`HostFunctionDef`]，`output_types` 为空切片。
    pub fn no_output(
        name: &'static str,
        namespace: &'static str,
        input_types: &'static [ValType],
    ) -> Self {
        Self {
            name,
            input_types,
            output_types: &[],
            namespace,
        }
    }

    /// 创建标准 MsgPack 函数定义（一个输入指针，一个输出指针）。
    ///
    /// # Arguments
    ///
    /// * `name` - 函数名。
    /// * `namespace` - 命名空间。
    ///
    /// # Returns
    ///
    /// 返回新的 [`HostFunctionDef`]，输入输出均为单个 `ValType::Ptr`。
    pub fn msgpack_fn(name: &'static str, namespace: &'static str) -> Self {
        Self {
            name,
            input_types: &[ValType::Ptr],
            output_types: &[ValType::Ptr],
            namespace,
        }
    }

    /// 创建无返回值的函数定义。
    ///
    /// # Arguments
    ///
    /// * `name` - 函数名。
    /// * `namespace` - 命名空间。
    /// * `input_types` - 输入参数类型列表。
    ///
    /// # Returns
    ///
    /// 返回新的 [`HostFunctionDef`]，`output_types` 为空切片。
    pub fn void_fn(
        name: &'static str,
        namespace: &'static str,
        input_types: &'static [ValType],
    ) -> Self {
        Self {
            name,
            input_types,
            output_types: &[],
            namespace,
        }
    }
}

/// 宿主函数提供者 trait。
///
/// 各模块通过实现此 trait 提供业务逻辑函数。
/// cmx-runtime 负责将这些函数包装成 Extism 宿主函数。
///
/// # 设计优势
///
/// - **解耦**：实现方不需要依赖 extism 库。
/// - **简洁**：只需要实现 `namespace`、`functions` 和 `call` 三个方法。
/// - **类型安全**：使用 [`ValType`] 枚举描述参数类型。
///
/// # Examples
///
/// ```rust,ignore
/// use cmx_traits::runtime::{HostFunctionProvider, HostFunctionDef, ValType};
/// use cmx_traits::error::HostFuncError;
///
/// struct DatabaseHostFunctions;
///
/// impl HostFunctionProvider for DatabaseHostFunctions {
///     fn namespace(&self) -> &str { "cmx:database" }
///
///     fn functions(&self) -> Vec<HostFunctionDef> {
///         vec![
///             HostFunctionDef::msgpack_fn("db_query", "cmx:database"),
///             HostFunctionDef::msgpack_fn("db_execute", "cmx:database"),
///         ]
///     }
///
///     fn call(&self, name: &str, input: Vec<u8>) -> Result<Vec<u8>, HostFuncError> {
///         match name {
///             "db_query" => self.do_query(input),
///             "db_execute" => self.do_execute(input),
///             _ => Err(HostFuncError::invalid_function(name)),
///         }
///     }
/// }
/// ```
pub trait HostFunctionProvider: Send + Sync {
    /// 返回命名空间标识。
    ///
    /// 用于日志、调试和函数名前缀。
    /// 建议使用 `cmx:模块名` 格式，如 `cmx:database`、`cmx:buffer`。
    fn namespace(&self) -> &str;

    /// 返回提供的宿主函数列表。
    ///
    /// 每个函数定义包含函数名、参数类型等信息。
    fn functions(&self) -> Vec<HostFunctionDef>;

    /// 调用宿主函数。
    ///
    /// cmx-runtime 在收到 WASM 调用时，通过此方法调用实际的业务逻辑。
    ///
    /// # Arguments
    ///
    /// * `name` - 函数名。
    /// * `input` - 输入数据（MsgPack 编码的字节）。
    ///
    /// # Returns
    ///
    /// 返回输出数据（MsgPack 编码的字节），或错误。
    ///
    /// # Errors
    ///
    /// * [`HostFuncError::ExecutionFailed`] - 函数执行失败。
    /// * [`HostFuncError::InvalidParam`] - 输入参数无效。
    fn call(&self, name: &str, input: Vec<u8>) -> Result<Vec<u8>, HostFuncError>;

    /// 列出该提供者注册的所有函数名。
    ///
    /// 用于调试、元数据查询和文档生成。
    ///
    /// # Returns
    ///
    /// 返回函数名切片列表。
    fn provided_functions(&self) -> Vec<&str> {
        self.functions().iter().map(|f| f.name).collect()
    }
}
