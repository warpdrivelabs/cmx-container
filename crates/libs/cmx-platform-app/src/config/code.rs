//! 编码引擎全局注入
//!
//! 供 DCT/DOC 钩子调用；未注入时钩子跳过（等价现状零影响）。

use tracing::warn;

/// 初始化编码引擎全局注入。
///
/// 把 `cmx_code_api::engine::CodeEngine` 注册为全局编码铸号器（`GlobalCodeMinter`）。
/// 幂等失败（重复初始化）只 `warn`，不致命——与抽取前 `main` 内联逻辑完全一致。
pub fn init_code_engine() {
    let engine = std::sync::Arc::new(cmx_code_api::engine::CodeEngine);
    if let Err(e) = cmx_traits::code::GlobalCodeMinter::set(engine) {
        warn!("编码引擎全局注入失败（可能重复初始化）：{e}");
    }
}
