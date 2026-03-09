//! 配置管理集成测试
//!
//! 测试完整的配置加载流程和优先级机制

use cmx_utils::{Config, DefaultConfigLoader, ConfigValue};
use std::fs;
use std::io::Write;
use tempfile::tempdir;

/// 测试完整的配置加载流程
#[test]
fn test_full_config_loading_workflow() {
    // 创建临时目录
    let dir = tempdir().unwrap();
    let config_dir = dir.path();

    // 创建 default.toml
    let default_toml = config_dir.join("default.toml");
    let mut file = fs::File::create(&default_toml).unwrap();
    file.write_all(
        br#"[database]
host = "localhost"
port = 5432
pool_size = 10

[server]
host = "0.0.0.0"
port = 8080
"#,
    )
    .unwrap();

    // 创建 production.toml（覆盖部分配置）
    let production_toml = config_dir.join("production.toml");
    let mut file = fs::File::create(&production_toml).unwrap();
    file.write_all(
        br#"[database]
host = "prod-db.example.com"
port = 5433

[server]
port = 443
"#,
    )
    .unwrap();

    // 创建 .env 文件
    let env_file = config_dir.join(".env");
    let mut file = fs::File::create(&env_file).unwrap();
    file.write_all(b"database.pool_size=20\nserver.host=127.0.0.1\n")
        .unwrap();

    // 设置环境变量CONFIG_FILE指向production.toml
    unsafe {
        std::env::set_var("CONFIG_FILE", &production_toml);
    }

    // 使用默认配置加载器
    let config = DefaultConfigLoader::new(config_dir)
        .with_env_prefix("APP_")
        .with_system_env(false) // 禁用系统环境变量以避免干扰
        .with_command_line(false) // 禁用命令行参数
        .load()
        .unwrap();

    // 清理环境变量
    unsafe {
        std::env::remove_var("CONFIG_FILE");
    }

    // 验证配置合并结果
    // production.toml 覆盖 default.toml
    assert_eq!(
        config.get_string("database.host").unwrap(),
        "prod-db.example.com"
    );
    assert_eq!(config.get_int("database.port").unwrap(), 5433);

    // .env 文件覆盖 production.toml
    assert_eq!(config.get_int("database.pool_size").unwrap(), 20);
    assert_eq!(config.get_string("server.host").unwrap(), "127.0.0.1");

    // production.toml 覆盖 default.toml
    assert_eq!(config.get_int("server.port").unwrap(), 443);
}

/// 测试配置反序列化为结构体
#[test]
#[allow(dead_code)]
fn test_config_deserialize() {
    use serde::Deserialize;

    #[derive(Deserialize, Debug)]
    struct DatabaseConfig {
        host: String,
        port: u16,
        pool_size: u32,
    }

    #[derive(Deserialize, Debug)]
    struct ServerConfig {
        host: String,
        port: u16,
    }

    #[derive(Deserialize, Debug)]
    struct AppConfig {
        database: DatabaseConfig,
        server: ServerConfig,
    }

    // 创建临时目录
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("config.toml");

    // 创建配置文件
    let mut file = fs::File::create(&config_path).unwrap();
    file.write_all(
        br#"[database]
host = "localhost"
port = 5432
pool_size = 10

[server]
host = "0.0.0.0"
port = 8080
"#,
    )
    .unwrap();

    // 加载配置
    let config = Config::from_file(&config_path).unwrap();

    // 反序列化为结构体
    let app_config: AppConfig = config.deserialize().unwrap();

    assert_eq!(app_config.database.host, "localhost");
    assert_eq!(app_config.database.port, 5432);
    assert_eq!(app_config.database.pool_size, 10);
    assert_eq!(app_config.server.host, "0.0.0.0");
    assert_eq!(app_config.server.port, 8080);
}

/// 测试子配置视图
#[test]
fn test_sub_config() {
    // 创建临时目录
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("config.toml");

    // 创建配置文件
    let mut file = fs::File::create(&config_path).unwrap();
    file.write_all(
        br#"[database]
host = "localhost"
port = 5432

[cache]
host = "redis.example.com"
port = 6379
"#,
    )
    .unwrap();

    // 加载配置
    let config = Config::from_file(&config_path).unwrap();

    // 获取数据库子配置
    let db_config = config.sub_config("database").unwrap();
    assert_eq!(db_config.get_string("host").unwrap(), "localhost");
    assert_eq!(db_config.get_int("port").unwrap(), 5432);

    // 获取缓存子配置
    let cache_config = config.sub_config("cache").unwrap();
    assert_eq!(cache_config.get_string("host").unwrap(), "redis.example.com");
    assert_eq!(cache_config.get_int("port").unwrap(), 6379);
}

/// 测试配置构建器的灵活使用
#[test]
fn test_config_builder_flexibility() {
    use cmx_utils::MemorySource;

    // 创建多个配置源
    let default_source = MemorySource::new()
        .with("app.name", ConfigValue::new_string("MyApp"))
        .with("app.version", ConfigValue::new_string("1.0.0"))
        .with("app.debug", ConfigValue::new_boolean(false));

    let override_source = MemorySource::new()
        .with("app.debug", ConfigValue::new_boolean(true))
        .with("app.port", ConfigValue::new_integer(8080));

    // 使用构建器组合配置源
    let config = Config::builder()
        .add_source(default_source)
        .add_source(override_source)
        .build()
        .unwrap();

    // 验证配置合并
    assert_eq!(config.get_string("app.name").unwrap(), "MyApp");
    assert_eq!(config.get_string("app.version").unwrap(), "1.0.0");
    assert_eq!(config.get_bool("app.debug").unwrap(), true); // 被覆盖
    assert_eq!(config.get_int("app.port").unwrap(), 8080); // 新增
}

/// 测试类型转换
#[test]
fn test_type_conversions() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("config.toml");

    let mut file = fs::File::create(&config_path).unwrap();
    file.write_all(
        br#"
string_value = "hello"
int_value = 42
float_value = 3.14
bool_value = true
large_int = 9223372036854775807
small_int = 255
"#,
    )
    .unwrap();

    let config = Config::from_file(&config_path).unwrap();

    // 测试各种类型转换
    assert_eq!(config.get_string("string_value").unwrap(), "hello");
    assert_eq!(config.get_int("int_value").unwrap(), 42);
    assert!((config.get_float("float_value").unwrap() - 3.14).abs() < 0.001);
    assert_eq!(config.get_bool("bool_value").unwrap(), true);

    // 测试不同整数类型
    let large: i64 = config.get_as("large_int").unwrap();
    assert_eq!(large, 9223372036854775807);

    let small_u8: u8 = config.get_as("small_int").unwrap();
    assert_eq!(small_u8, 255);

    let int_u16: u16 = config.get_as("int_value").unwrap();
    assert_eq!(int_u16, 42);
}
