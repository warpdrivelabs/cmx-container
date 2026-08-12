//! 业务模型模块
//!
//! 存放业务相关的模型定义。
//!
//! 每个实体模块按标准结构组织：
//! - `bmc.rs` - DbBmc 实现（表元信息）
//! - `filter.rs` - Filter 定义（查询过滤）
//! - `entity.rs` - Entity 定义（实体结构）
//! - `service.rs` - Service 实现（业务逻辑，可选）
//! - `handler.rs` - Handler 实现（HTTP 处理，可选）

pub mod debug;
pub mod dev;
pub mod module;
pub mod portal;
pub mod service;
