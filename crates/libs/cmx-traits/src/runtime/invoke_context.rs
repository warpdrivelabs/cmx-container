//! WASM 调用选项与调用上下文。
//!
//! 提供超时控制、调用深度限制和循环检测的配置与跟踪机制。
//!
//! # 多层防护设计
//!
//! ```text
//! ┌─────────────────────────────────────┐
//! │ 第1层: Extism 原生超时               │  Manifest::with_timeout
//! │   → 单次 plugin.call() 超时中断      │
//! ├─────────────────────────────────────┤
//! │ 第2层: 调用深度限制                  │  InvokeOptions::max_depth
//! │   → 防止无限递归调用                 │
//! ├─────────────────────────────────────┤
//! │ 第3层: 循环检测                      │  InvokeContext::call_chain
//! │   → 检测 A→B→A 循环调用链           │
//! └─────────────────────────────────────┘
//! ```

use std::cell::RefCell;
use std::collections::HashSet;
use std::time::Duration;

/// 默认超时时间（30 秒）。
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// 默认最大调用深度。
pub const DEFAULT_MAX_DEPTH: u32 = 8;

/// WASM 调用选项。
///
/// 每次调用 WASM 函数时可以传入此选项，控制超时时间和调用深度限制。
#[derive(Debug, Clone)]
pub struct InvokeOptions {
    /// 单次调用的超时时间。
    ///
    /// 默认 30 秒。Extism 原生支持，超时后自动中断 WASM 执行。
    pub timeout: Duration,

    /// 最大调用深度（插件间调用嵌套层数）。
    ///
    /// 默认 8 层。当 WASM-A 调用 WASM-B，WASM-B 又调用 WASM-C 时，
    /// 深度为 2。超过限制时立即返回错误。
    pub max_depth: u32,

    /// 是否调试模式。
    pub debug: bool,
}

impl Default for InvokeOptions {
    fn default() -> Self {
        Self {
            timeout: DEFAULT_TIMEOUT,
            max_depth: DEFAULT_MAX_DEPTH,
            debug: false,
        }
    }
}

impl InvokeOptions {
    /// 创建默认调用选项。
    ///
    /// # Returns
    ///
    /// 返回使用默认值的 [`InvokeOptions`]。
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置超时时间。
    ///
    /// # Arguments
    ///
    /// * `timeout` - 超时时间。
    ///
    /// # Returns
    ///
    /// 返回更新后的 `self`（Builder 模式）。
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// 设置最大调用深度。
    ///
    /// # Arguments
    ///
    /// * `max_depth` - 最大调用深度。
    ///
    /// # Returns
    ///
    /// 返回更新后的 `self`（Builder 模式）。
    pub fn with_max_depth(mut self, max_depth: u32) -> Self {
        self.max_depth = max_depth;
        self
    }
}

// 线程局部调用深度计数器和调用链追踪
// 调用链使用 "plugin_id/function_name" 作为 key，用于检测函数级别的循环调用
// 例如 A.a → B.b → A.a 这种递归调用
thread_local! {
    static CALL_DEPTH: RefCell<u32> = const { RefCell::new(0) };
    static CALL_CHAIN: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
}

/// 调用上下文管理器。
///
/// 提供线程安全的调用深度跟踪和循环检测。
/// 每次进入 `invoke` 时创建一个 [`InvokeGuard`]，
/// 退出时自动恢复状态（RAII 模式）。
pub struct InvokeContext;

impl InvokeContext {
    /// 获取当前调用深度。
    ///
    /// # Returns
    ///
    /// 返回当前线程的调用深度。
    pub fn current_depth() -> u32 {
        CALL_DEPTH.with(|d| *d.borrow())
    }

    /// 检查调用链中是否已包含指定插件函数的调用。
    ///
    /// 用于检测函数级别的递归调用（如 `A.a → B.b → A.a`）。
    /// 使用 `plugin_id/function_name` 组合作为调用链的 key。
    ///
    /// # Arguments
    ///
    /// * `plugin_id` - 插件 ID。
    /// * `function_name` - 函数名。
    ///
    /// # Returns
    ///
    /// 检测到循环返回 `true`，否则返回 `false`。
    pub fn is_cycle(plugin_id: &str, function_name: &str) -> bool {
        let key = format!("{}/{}", plugin_id, function_name);
        CALL_CHAIN.with(|c| {
            let chain = c.borrow();
            chain.contains(&key)
        })
    }

    /// 进入一个新的调用层级。
    ///
    /// 返回一个 [`InvokeGuard`]，drop 时自动恢复深度和调用链。
    /// 如果深度超限或检测到循环，返回 `Err`。
    ///
    /// # Arguments
    ///
    /// * `plugin_id` - 插件 ID。
    /// * `function_name` - 函数名。
    /// * `max_depth` - 最大调用深度。
    ///
    /// # Returns
    ///
    /// 成功时返回 [`InvokeGuard`]。
    ///
    /// # Errors
    ///
    /// * [`InvokeGuardError::DepthExceeded`] - 调用深度超限。
    /// * [`InvokeGuardError::CycleDetected`] - 检测到循环调用。
    pub fn enter(
        plugin_id: &str,
        function_name: &str,
        max_depth: u32,
    ) -> Result<InvokeGuard, InvokeGuardError> {
        // 检查深度限制
        let current_depth = CALL_DEPTH.with(|d| {
            let depth = *d.borrow();
            if depth >= max_depth {
                return Err(max_depth);
            }
            Ok(depth)
        });

        if let Err(limit) = current_depth {
            return Err(InvokeGuardError::DepthExceeded {
                current: limit,
                max: limit,
                plugin_id: plugin_id.to_string(),
                function_name: function_name.to_string(),
            });
        }

        // 检查循环调用（基于 plugin_id/function_name 组合，检测函数级别递归）
        // 例如 A.a → B.b → A.a 会触发循环检测
        let call_key = format!("{}/{}", plugin_id, function_name);
        let is_cycle = CALL_CHAIN.with(|c| {
            let mut chain = c.borrow_mut();
            if chain.contains(&call_key) {
                true
            } else {
                chain.insert(call_key);
                false
            }
        });

        if is_cycle {
            return Err(InvokeGuardError::CycleDetected {
                plugin_id: plugin_id.to_string(),
                function_name: function_name.to_string(),
            });
        }

        // 递增深度
        CALL_DEPTH.with(|d| {
            *d.borrow_mut() += 1;
        });

        Ok(InvokeGuard {
            plugin_id: plugin_id.to_string(),
            function_name: function_name.to_string(),
        })
    }
}

/// 调用守卫（RAII）。
///
/// 创建时递增深度并记录调用链，drop 时自动恢复。
/// 确保即使发生 panic 也能正确恢复状态。
pub struct InvokeGuard {
    /// 插件 ID（用于从调用链中移除）。
    plugin_id: String,
    /// 函数名（仅用于日志）。
    function_name: String,
}

impl InvokeGuard {
    /// 获取当前调用深度（进入后的深度）。
    ///
    /// # Returns
    ///
    /// 返回进入此守卫后的调用深度。
    pub fn depth(&self) -> u32 {
        InvokeContext::current_depth()
    }

    /// 获取插件 ID。
    ///
    /// # Returns
    ///
    /// 返回当前调用链节点的插件 ID。
    pub fn plugin_id(&self) -> &str {
        &self.plugin_id
    }

    /// 获取函数名。
    ///
    /// # Returns
    ///
    /// 返回当前调用链节点的函数名。
    pub fn function_name(&self) -> &str {
        &self.function_name
    }
}

impl Drop for InvokeGuard {
    fn drop(&mut self) {
        // 递减深度
        CALL_DEPTH.with(|d| {
            let mut depth = d.borrow_mut();
            *depth = depth.saturating_sub(1);
        });

        // 从调用链中移除 plugin_id/function_name
        let call_key = format!("{}/{}", self.plugin_id, self.function_name);
        CALL_CHAIN.with(|c| {
            c.borrow_mut().remove(&call_key);
        });
    }
}

/// 调用守卫错误。
#[derive(Debug)]
pub enum InvokeGuardError {
    /// 调用深度超限。
    DepthExceeded {
        /// 当前深度。
        current: u32,
        /// 最大深度。
        max: u32,
        /// 插件 ID。
        plugin_id: String,
        /// 函数名。
        function_name: String,
    },
    /// 检测到循环调用。
    CycleDetected {
        /// 插件 ID。
        plugin_id: String,
        /// 函数名。
        function_name: String,
    },
}

impl std::fmt::Display for InvokeGuardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DepthExceeded {
                current,
                max,
                plugin_id,
                function_name,
            } => {
                write!(
                    f,
                    "调用深度超限: 当前深度 {} >= 最大深度 {}，插件 {} 函数 {}",
                    current, max, plugin_id, function_name
                )
            }
            Self::CycleDetected {
                plugin_id,
                function_name,
            } => {
                write!(
                    f,
                    "检测到循环调用: 插件 {} 函数 {}",
                    plugin_id, function_name
                )
            }
        }
    }
}

impl std::error::Error for InvokeGuardError {}
