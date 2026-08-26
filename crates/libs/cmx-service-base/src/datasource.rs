//! 数据源注册原语。
//!
//! [`register_pg_datasources`]（default）：把一组 pg 形态 `DbConfig` 注册到 tokio-postgres
//! 全局管理器。flow + portal 共享。注册时**建池即首连验证**（cmx-database-pg 的 `new_db_pool`
//! 在建池后立即拨号一条连接，sqlx `connect()` 等效语义）——数据库不可达 / 库不存在 / 认证
//! 失败在注册阶段即返回 [`Err`]，启动钩子据此终止启动（fail-fast），带病启动的服务 DB 端点
//! 只会全部返错。非 Postgres 项跳过。
//!
//! sqlx 数据源注册（portal 的老链路）逻辑重（持久化 cmx_sys_datasource / 迁移 / 从库回读 /
//! 按部署模式过滤），**留在 portal 侧**（web-server config/datasource.rs），本 base 库不承载。

use cmx_database_pg::{DbConfig, DbType, get_default_pg_db_manager};
use tracing::info;

use crate::{BaseError, Result};

/// 注册一组 pg 数据源（tokio-postgres 链路）。非 Postgres 项跳过；单项注册失败（含首连
/// 验证失败）返回 [`Err`]——调用方（引擎启动钩子）应终止启动。
pub async fn register_pg_datasources(configs: &[DbConfig]) -> Result<()> {
    let pg_mm = get_default_pg_db_manager();
    for c in configs {
        if !matches!(c.db_type, DbType::Postgres) {
            continue;
        }
        pg_mm
            .register_data_source(c.clone())
            .await
            .map(|_| info!("成功注册 PG 数据源(新链路，首连已验证): {}", c.db_id))
            .map_err(|e| {
                BaseError::Setup(format!(
                    "注册 PG 数据源(新链路) {} 失败: {e}",
                    c.db_id
                ))
            })?;
    }
    Ok(())
}
