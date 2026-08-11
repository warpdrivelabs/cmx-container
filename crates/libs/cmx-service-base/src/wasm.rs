//! WASM 运行时初始化（feature `wasm`）。
//!
//! 从 web-server `config/runtime.rs` 提取的**通用部分**：建 Extism 引擎 + 注册 logging/db/buffer
//! 三个通用 host-fn provider + `GlobalRuntime::set`/`GlobalExtismEngine::initialize`。
//!
//! ★ 关键（trait/hook 拆分）：portal 专属的 `cmx:iam`（cmx-iam）/ plugin（cmx-plugin）两 provider
//! **不在本库**——由调用方经 `extra_providers` 注入。`HostFunctionProvider` 是 cmx-traits trait，
//! 故本库只依赖 cmx-runtime + cmx-traits + cmx-database，**不碰 cmx-iam/cmx-plugin**（flow 若开 wasm
//! 也不被拖重）。

use std::sync::Arc;

use cmx_database::get_default_db_manager;
use cmx_runtime::{ExtismEngine, ExtismEngineConfig, GlobalExtismEngine, LoggingHostFunctions};
use cmx_traits::runtime::{GlobalRuntime, HostFunctionProvider};
use tracing::info;

use crate::{BaseError, Result};

/// 初始化 WASM 运行时。注册 3 个通用 provider（logging/db/buffer）+ 调用方注入的 `extra_providers`
/// （portal 传 iam+plugin），设全局引擎。
///
/// - `extra_providers`：调用方构造的额外 host-fn provider（如 IamHostFunctions/PluginHostFunctions），
///   在通用 3 个之后依次注册。空 vec = 只装通用 3 个。
pub async fn init_wasm(extra_providers: Vec<Arc<dyn HostFunctionProvider>>) -> Result<()> {
    info!("初始化 WASM 运行时...");

    let engine = Arc::new(
        ExtismEngine::new(ExtismEngineConfig::default())
            .map_err(|e| BaseError::Setup(format!("Extism 引擎初始化失败: {e}")))?,
    );

    // 通用 provider ①日志
    let logging_provider: Arc<dyn HostFunctionProvider> = Arc::new(LoggingHostFunctions::new());
    engine
        .register_provider(logging_provider)
        .map_err(|e| BaseError::Setup(format!("注册日志宿主函数失败: {e}")))?;

    // 通用 provider ②数据库
    let db_manager = get_default_db_manager();
    let db_provider: Arc<dyn HostFunctionProvider> =
        Arc::new(cmx_database::DatabaseHostFunctions::new(db_manager.clone()));
    engine
        .register_provider(db_provider)
        .map_err(|e| BaseError::Setup(format!("注册数据库宿主函数失败: {e}")))?;

    // 通用 provider ③缓存（buffer host-fn，wasm feature 拉 cmx-buffer，与 redis 独立）
    let buffer_provider: Arc<dyn HostFunctionProvider> =
        Arc::new(cmx_buffer::BufferHostFunctions::new());
    engine
        .register_provider(buffer_provider)
        .map_err(|e| BaseError::Setup(format!("注册缓存宿主函数失败: {e}")))?;

    // 调用方注入的额外 provider（portal 传 iam/plugin）。
    let extra_count = extra_providers.len();
    for provider in extra_providers {
        engine
            .register_provider(provider)
            .map_err(|e| BaseError::Setup(format!("注册注入宿主函数失败: {e}")))?;
    }

    GlobalRuntime::set(engine.clone())
        .map_err(|e| BaseError::Setup(format!("设置全局运行时失败: {e:?}")))?;
    GlobalExtismEngine::initialize(engine)
        .map_err(|e| BaseError::Setup(format!("全局引擎初始化失败: {e}")))?;

    info!(
        "WASM 运行时初始化完成，已注册 {} 个宿主函数提供者",
        3 + extra_count // logging/db/buffer + 注入
    );

    Ok(())
}
