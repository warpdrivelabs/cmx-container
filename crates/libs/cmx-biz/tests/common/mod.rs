//! 测试公共辅助:数据库连接 setup
use cmx_database::{DatabaseManager, DatabaseManagerConfig, DbConfig, DbType, PoolConfig};

/// 测试数据库连接 URL
pub const TEST_DB_URL: &str = "postgresql://postgres:postgres@192.168.137.80:5432/postgres";
/// 测试数据库标识
pub const TEST_DB_KEY: &str = "test_db";

/// 初始化测试数据库管理器(注册一个 Postgres 数据源)
///
/// # Panics
/// 数据库连接失败时 panic(测试环境应保证 PG 可达)
pub async fn setup_db_manager() -> DatabaseManager {
    let pool_config = PoolConfig {
        max_connections: 5,
        min_connections: 1,
        connect_timeout: 30,
        acquire_timeout: 30,
        idle_timeout: 600,
        max_lifetime: 1800,
    };
    let db_config = DbConfig {
        db_type: DbType::Postgres,
        db_url: TEST_DB_URL.to_string(),
        db_id: TEST_DB_KEY.to_string(),
        db_schema: Some("public".to_string()),
        pool_config,
        health_check_interval: 60,
        health_check_timeout: 5,
        default: true,
    };
    let manager = DatabaseManager::new(DatabaseManagerConfig::default());
    manager
        .register_data_source(db_config)
        .await
        .expect("测试数据库连接失败，请确认 PG 可达");
    manager
}
