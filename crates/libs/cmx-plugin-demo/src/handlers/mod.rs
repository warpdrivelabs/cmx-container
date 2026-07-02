//! 业务处理逻辑。
//!
//! 按功能分类组织，每个文件对应一类宿主能力的使用示例。

use crate::host::HostFunctions;

/// 插件核心实现。
///
/// 通过泛型 `H: HostFunctions` 与具体宿主环境解耦，
/// 支持在测试中使用 MockHostFunctions。
pub struct PluginCore<H: HostFunctions> {
    host: H,
}

impl<H: HostFunctions> PluginCore<H> {
    /// 创建新的插件核心实例。
    pub fn new(host: H) -> Self {
        Self { host }
    }

    /// 获取宿主功能的引用。
    pub fn host(&self) -> &H {
        &self.host
    }
}

pub mod basic;
pub mod cache;
pub mod database;
pub mod iam;
pub mod orchestration;
pub mod plugin_call;
