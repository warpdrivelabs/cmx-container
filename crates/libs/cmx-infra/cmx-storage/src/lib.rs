//! cmx-storage 对象存储抽象层
//!
//! 提供统一的、可扩展的对象存储服务抽象，支持 S3、本地文件系统等多种存储后端。
//! 基于 OpenDAL 库构建，屏蔽不同存储服务的差异，为上层应用提供一致的文件操作接口。
//!
//! ## 核心架构
//!
//! 采用三层架构设计：
//! - **后端层** ([`backend`]): 封装 OpenDAL，提供纯 I/O 操作的存储后端抽象 ([`StorageBackend`])
//! - **服务层** ([`service`]): 组合后端 I/O 与数据库操作，提供面向业务的高级文件服务 ([`StorageService`])
//! - **接口层** ([`handler`]): 基于 axum 框架提供 REST API 接口
//!
//! ## 存储后端
//!
//! 支持的存储后端类型：
//! - [`backend::local::LocalBackend`][]: 本地文件系统存储
//! - [`backend::s3::S3Backend`]: Amazon S3 及 S3 兼容存储（如 MinIO、腾讯云 COS）
//!
//! ## 错误处理
//!
//! 所有存储操作返回 [`Error`] 类型，详见 [`error`] 模块。

pub mod backend;
pub mod bmc;
pub mod config;
pub mod error;
pub mod global;
pub mod handler;
pub mod manager;
pub mod mime_detect;
pub mod path_gen;
pub mod service;
pub mod types;

pub use cmx_api_types::ApiResp;
pub use error::{Error, Result};
