//! cmx-mdm-store-pg —— 主数据（MDM）模块的 PostgreSQL 持久化/服务层。
//!
//! 模块结构：
//! - [`doc_accessor`]：读 CR 单据（cv_mdm_apply 头 + cv_mdm_apply_line 行）。
//! - [`activation_store`]：cmx_mdm_activation 激活映射配置读写（激活器 + UI 配置器）。
//! - [`dct_accessor`]：cm_* 主数据写入闸口（强制 lifecycle_status='published'，唯一入口）。
//! - [`md_accessor`]：md_audit / md_event_log 治理表写入 + CR 状态归档。
//! - [`activation_service`]：激活器主流程（七步单事务编排）。
//! - [`error`]：错误助手（api_err / api_err_db）。
//!
//! 惯例（对齐 cmx-dct-store-pg）：store 是模块级自由 async 函数，DB 连接走
//! `cmx_database_pg::get_default_pg_db_manager()` 全局单例，不经 HTTP / State 注入。

mod activation_service;
mod activation_store;
mod cr_service;
mod dct_accessor;
mod doc_accessor;
mod error;
mod match_store;
mod md_accessor;

pub use activation_store::{find_by_doc_type, list, upsert};
pub use cr_service::{abort_cr, check_status, clone_revise, create_cr, get_cr_detail, list_cr};
pub use error::{api_err, api_err_db};
// 激活器主流程对 api 层暴露（M1 activate + M3 merge/unmerge）
pub use activation_service::{activate, merge, unmerge};
// M3 匹配/合并 store 对 api 层暴露
pub use match_store::{
    get_match_group, insert_match_group, list_audit, list_events, list_match_groups,
    list_subscriptions, load_by_ids, load_published, transition_match_group, update_match_group,
    upsert_subscription,
};
// set_cr_status 供 api 层改 CR 状态(submit/reject,自动提交);激活器内部直接用 md_accessor
pub use md_accessor::set_cr_status as set_cr_status_pub;
