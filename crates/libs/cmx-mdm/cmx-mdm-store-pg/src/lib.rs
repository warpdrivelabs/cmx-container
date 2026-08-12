//! cmx-mdm-store-pg —— 主数据（MDM）模块的 PostgreSQL 持久化/服务层。
//!
//! 模块结构：
//! - [`doc_accessor`]：读 CR 单据（cv_mdm_apply 头 + cv_mdm_apply_line 行）。
//! - [`activation_store`]：cmx_mdm_activation 激活映射配置读写（激活器 + UI 配置器）。
//! - [`dct_accessor`]：cm_* 主数据写入闸口（强制 lifecycle_status='published'，唯一入口）。
//! - [`sql_builder`]：cm_* 写入的 SQL 构造与列值转换工具（dct_accessor 内部用）。
//! - [`md_accessor`]：md_audit / md_event_log 治理表写入 + CR 状态归档。
//! - [`activation_service`]：激活器 / 合并 / 还原三套主流程的单事务编排。
//! - [`cr_service`]：CR 变更请求服务（状态校验 / 列表 / 详情 / 克隆复活 / 作废）。
//! - [`match_store`]：匹配组 / 交叉引用 / 治理查询 store。
//! - [`match_config_store`]：查重规则配置 store。
//! - [`scan_store`]：查重发现项 store（md_match_scan，全库扫描结果载体）。
//! - [`error`]：错误助手（api_err / api_err_db / parse_jsonb_field）。
//!
//! 惯例（对齐 cmx-dct-store-pg）：store 是模块级自由 async 函数，DB 连接走
//! `cmx_database_pg::get_default_pg_db_manager()` 全局单例，不经 HTTP / State 注入。

/// 激活器 / 合并 / 还原三套主流程的单事务编排。
mod activation_service;
/// cmx_mdm_activation 激活映射配置读写（激活器 + UI 配置器）。
mod activation_store;
/// CR 变更请求服务（状态校验 / 列表 / 详情 / 克隆复活 / 作废）。
mod cr_service;
/// cm_* 主数据写入闸口（强制 lifecycle_status='published'，唯一入口）。
mod dct_accessor;
/// 读 CR 单据（cv_mdm_apply 头 + cv_mdm_apply_line 行）。
mod doc_accessor;
/// 错误助手（api_err / api_err_db / parse_jsonb_field）。
mod error;
/// md_match_config 查重规则配置读写。
mod match_config_store;
/// 匹配组 / 交叉引用 / 治理查询 store。
mod match_store;
/// md_match_scan 查重发现项 store（全库扫描结果载体，管家评审）。
mod scan_store;
/// md_audit / md_event_log 治理表写入 + CR 状态归档。
mod md_accessor;
/// cm_* 写入的 SQL 构造与列值转换工具（dct_accessor 内部用）。
mod sql_builder;

pub use activation_store::{find_by_doc_type, line_tables_for_dict, list, upsert, delete_by_code};
pub use cr_service::{abort_cr, check_status, clone_revise, get_cr_detail, list_cr};
pub use error::{api_err, api_err_db};
// 激活器主流程对 api 层暴露（M1 activate + M3 merge/unmerge）
pub use activation_service::{activate, merge, unmerge};
// M3 匹配/合并 store 对 api 层暴露
pub use match_store::{
    get_match_group, insert_match_group, list_audit, list_events, list_match_groups,
    list_subscriptions, load_by_ids, load_published, load_suspects, transition_match_group,
    update_match_group, upsert_subscription,
};
// M3.5 查重发现项 store（全库扫描 / 评审队列，cluster_hash 去重）
pub use scan_store::{
    get_scan, insert_findings, list_scans, transition_scan_status, InsertStats, PreparedCluster,
};
// 查重规则配置 store 对 api 层暴露（查重界面内维护）
pub use match_config_store::{
    delete_match_config, get_match_config, list_match_config, upsert_match_config,
};
// set_cr_status 供 api 层改 CR 状态(submit/reject,自动提交);激活器内部直接用 md_accessor
pub use md_accessor::set_cr_status as set_cr_status_pub;
