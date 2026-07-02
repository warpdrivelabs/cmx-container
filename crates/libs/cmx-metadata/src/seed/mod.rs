//! 种子数据（Seed Data）模块
//!
//! 职责：在插件安装时，根据配置加载 CSV/JSON 数据文件，
//! 生成并执行 DML 语句，为业务表预置初始化数据。
//!
//! # 功能特性
//! - 支持 JSON 和 CSV 两种数据文件格式
//! - 使用 PostgreSQL ON CONFLICT (UPSERT) 语义，避免重复数据
//! - 批量执行提高性能，错误不阻断安装流程
//! - 执行后校验数据条数一致性

mod config;
mod dml;
mod executor;
mod loader;

pub use config::{SeedDataConfig, SeedDataFailure, SeedDataSummary, SeedDataTableResult};
pub use executor::PgSeedDataExecutor;
