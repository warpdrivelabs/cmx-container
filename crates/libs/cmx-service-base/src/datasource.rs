//! 数据源注册原语。
//!
//! [`register_pg_datasources`]（default）：把一组 pg 形态 `DbConfig` 注册到 tokio-postgres
//! 全局管理器。flow + portal 共享。失败只 warn 不阻断启动（对齐 web-server 既有语义）。
//!
//! sqlx 数据源注册（portal 的老链路）逻辑重（持久化 cmx_sys_datasource / 迁移 / 从库回读 /
//! 按部署模式过滤），**留在 portal 侧**（web-server config/datasource.rs），本 base 库不承载。

use cmx_database_pg::{DbConfig, DbType, get_default_pg_db_manager};
use tracing::{info, warn};

use crate::Result;

/// 注册一组 pg 数据源（tokio-postgres 链路）。非 Postgres 项跳过；单项失败只 warn 不阻断。
pub async fn register_pg_datasources(configs: &[DbConfig]) -> Result<()> {
    let pg_mm = get_default_pg_db_manager();
    for c in configs {
        if !matches!(c.db_type, DbType::Postgres) {
            continue;
        }
        match pg_mm.register_data_source(c.clone()).await {
            Ok(_) => info!("成功注册 PG 数据源(新链路): {}", c.db_id),
            Err(e) => warn!("注册 PG 数据源(新链路) {} 失败(不阻断启动): {}", c.db_id, e),
        }
    }
    Ok(())
}
