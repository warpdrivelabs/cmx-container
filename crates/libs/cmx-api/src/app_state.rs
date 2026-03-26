//! CMX State 模块
//!
//! 定义应用程序的共享状态，支持运行时动态修改

use std::sync::Arc;
use tokio::sync::RwLock;

/// CMX 应用程序状态
///
/// 包含应用程序运行时的共享状态
/// DatabaseManager 通过 get_default_db_manager() 全局获取，不需要通过 state 传递
///
/// # 使用示例
/// ```rust
/// use cmx_api::CmxAppState;
/// use std::sync::Arc;
/// use tokio::sync::RwLock;
///
/// let state = CmxAppState::new("default".to_string());
///
/// // 运行时修改 default_db_id
/// {
///     let mut app_state = state.app_state.write().await;
///     app_state.default_db_id = "new_db".to_string();
/// }
///
/// // 读取 default_db_id
/// {
///     let app_state = state.app_state.read().await;
///     println!("{}", app_state.default_db_id);
/// }
/// ```
#[derive(Debug, Clone)]
pub struct CmxAppState {
    /// 内部可修改的状态
    pub app_state: Arc<RwLock<AppStateInner>>,
}

/// 内部状态结构
#[derive(Debug, Clone)]
pub struct AppStateInner {
    // /// 默认数据库 ID
    // pub default_db_id: String,
}

impl CmxAppState {


    pub fn new() -> Self {
        Self {
            app_state: Arc::new(RwLock::new(AppStateInner {})),
        }
    }

    // /// 创建新的 CmxAppState
    // ///
    // /// # 参数
    // /// * `default_db_id` - 默认数据库 ID
    // pub fn new(default_db_id: String) -> Self {
    //     Self {
    //         app_state: Arc::new(RwLock::new(AppStateInner { default_db_id })),
    //     }
    // }
    //
    // /// 获取默认数据库 ID
    // pub async fn get_default_db_id(&self) -> String {
    //     let app_state = self.app_state.read().await;
    //     app_state.default_db_id.clone()
    // }
    //
    // /// 设置默认数据库 ID
    // pub async fn set_default_db_id(&self, db_id: String) {
    //     let mut app_state = self.app_state.write().await;
    //     app_state.default_db_id = db_id;
    // }
}


