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
        db_name: None,
        db_schema: Some("public".to_string()),
        pool_config,
        health_check_interval: 60,
        health_check_timeout: 5,
        domain_code: None,
        application_code: None,
        module_code: None,
        default: true,
        source_type: None,
    };
    let manager = DatabaseManager::new(DatabaseManagerConfig::default());
    manager
        .register_data_source(db_config)
        .await
        .expect("测试数据库连接失败，请确认 PG 可达");
    manager
}

/// 确保测试所需的表已创建(幂等)。
///
/// 在没有 psql 的环境下，通过 DatabaseManager::execute_sql 执行 DDL。
/// 需在测试开始前调用。
///
/// # Panics
/// DDL 执行失败时 panic
pub async fn ensure_tables(manager: &DatabaseManager) {
    // 逐条执行(prepared statement 不支持多条 SQL 合并)
    let stmts: &[&str] = &[
        // 先清理旧表(确保字段结构最新,开发期表结构变更后重建)
        "DROP TABLE IF EXISTS cmx_menu",
        "DROP TABLE IF EXISTS cmx_form",
        // cmx_form
        r#"CREATE TABLE IF NOT EXISTS cmx_form (
            id VARCHAR(64) NOT NULL,
            code VARCHAR(128) NOT NULL,
            name VARCHAR(256) NOT NULL,
            description TEXT,
            definition JSONB,
            version VARCHAR(64) DEFAULT '1.0.0',
            domain_code VARCHAR(64) NOT NULL,
            application_code VARCHAR(64) NOT NULL,
            module_code VARCHAR(64) NOT NULL,
            status INT4 DEFAULT 1,
            archived INT4 DEFAULT 0,
            create_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            update_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            create_by VARCHAR(100),
            create_name VARCHAR(100),
            update_by VARCHAR(100),
            update_name VARCHAR(100),
            CONSTRAINT pk_cmx_form PRIMARY KEY (id),
            CONSTRAINT uk_cmx_form_code UNIQUE (code)
        )"#,
        r#"CREATE INDEX IF NOT EXISTS idx_cmx_form_module ON cmx_form (domain_code, application_code, module_code)"#,
        // cmx_menu(标准分级字段)
        r#"CREATE TABLE IF NOT EXISTS cmx_menu (
            id VARCHAR(64) NOT NULL,
            code VARCHAR(128) NOT NULL,
            name VARCHAR(256) NOT NULL,
            description VARCHAR(500),
            path VARCHAR(512),
            icon VARCHAR(128),
            component VARCHAR(512),
            sort_order INT4 DEFAULT 0,
            visible INT4 DEFAULT 1,
            domain_code VARCHAR(64) NOT NULL,
            application_code VARCHAR(64) NOT NULL,
            module_code VARCHAR(64) NOT NULL,
            definition JSONB,
            status INT4 DEFAULT 1,
            leaf INT4 DEFAULT 1,
            depth INT4 DEFAULT 1,
            parent_id VARCHAR(64),
            parent_code VARCHAR(128),
            id_path VARCHAR(1000),
            code_path VARCHAR(1000),
            archived INT4 DEFAULT 0,
            create_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            update_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            create_by VARCHAR(100),
            create_name VARCHAR(100),
            update_by VARCHAR(100),
            update_name VARCHAR(100),
            ext_attributes TEXT,
            CONSTRAINT pk_cmx_menu PRIMARY KEY (id),
            CONSTRAINT uk_cmx_menu_code UNIQUE (code)
        )"#,
        r#"CREATE INDEX IF NOT EXISTS idx_cmx_menu_module ON cmx_menu (domain_code, application_code, module_code)"#,
        r#"CREATE INDEX IF NOT EXISTS idx_cmx_menu_parent_id ON cmx_menu (parent_id)"#,
    ];
    for sql in stmts {
        manager
            .execute_sql(TEST_DB_KEY, None, sql)
            .await
            .expect("执行建表/索引 DDL 失败");
    }
}

