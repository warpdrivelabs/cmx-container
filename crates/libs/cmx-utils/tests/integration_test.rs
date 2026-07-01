//! 配置管理集成测试
//!
//! 测试完整的配置加载流程和优先级机制

use cmx_utils::{Config, DefaultConfigLoader};
use std::fs;
use std::io::Write;
use tempfile::tempdir;

/// 测试完整的配置加载流程
#[test]
fn test_full_config_loading_workflow() {
    let dir = tempdir().unwrap();
    let config_dir = dir.path();

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

    // SAFETY: `std::env::set_var` 在多线程环境下可能引发数据竞争。此处安全的前提是：
    // 测试运行期间没有其他线程并发读写 `CONFIG_FILE` 环境变量。该变量名专用于本测试，
    // 设置后立即加载配置并在加载完成后移除，假设 cargo 默认并行测试未触及同名变量。
    unsafe {
        std::env::set_var("CONFIG_FILE", &production_toml);
    }

    let config = DefaultConfigLoader::new(config_dir)
        .with_command_line(false)
        .load()
        .unwrap();

    // SAFETY: `std::env::remove_var` 在多线程环境下可能引发数据竞争，此处安全的前提是：
    // 与上方 `set_var` 配对，且没有其他线程并发读写 `CONFIG_FILE`。
    // 配置加载已完成，移除该变量以避免污染后续测试。
    unsafe {
        std::env::remove_var("CONFIG_FILE");
    }

    assert_eq!(
        config.get_string("database.host").unwrap(),
        "prod-db.example.com"
    );
    assert_eq!(config.get_int("database.port").unwrap(), 5433);
    assert_eq!(config.get_int("database.pool_size").unwrap(), 10);
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

    let dir = tempdir().unwrap();
    let config_path = dir.path().join("config.toml");

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

    let config = Config::from_file(&config_path).unwrap();

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
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("config.toml");

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

    let config = Config::from_file(&config_path).unwrap();

    let db_config = config.sub_config("database").unwrap();
    assert_eq!(db_config.get_string("host").unwrap(), "localhost");
    assert_eq!(db_config.get_int("port").unwrap(), 5432);

    let cache_config = config.sub_config("cache").unwrap();
    assert_eq!(cache_config.get_string("host").unwrap(), "redis.example.com");
    assert_eq!(cache_config.get_int("port").unwrap(), 6379);
}

/// 测试配置构建器的灵活使用（使用 set_default / set_override 替代 MemorySource）
#[test]
fn test_config_builder_flexibility() {
    let config = Config::builder()
        .add_source(
            config::Config::builder()
                .set_default("app.name", "MyApp")
                .unwrap()
                .set_default("app.version", "1.0.0")
                .unwrap()
                .set_default("app.debug", false)
                .unwrap()
                .build()
                .unwrap(),
        )
        .add_source(
            config::Config::builder()
                .set_override("app.debug", true)
                .unwrap()
                .set_override("app.port", 8080)
                .unwrap()
                .build()
                .unwrap(),
        )
        .build()
        .unwrap();

    assert_eq!(config.get_string("app.name").unwrap(), "MyApp");
    assert_eq!(config.get_string("app.version").unwrap(), "1.0.0");
    assert!(config.get_bool("app.debug").unwrap());
    assert_eq!(config.get_int("app.port").unwrap(), 8080);
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
float_value = 3.15
bool_value = true
large_int = 9223372036854775807
small_int = 255
"#,
    )
    .unwrap();

    let config = Config::from_file(&config_path).unwrap();

    assert_eq!(config.get_string("string_value").unwrap(), "hello");
    assert_eq!(config.get_int("int_value").unwrap(), 42);
    assert!((config.get_float("float_value").unwrap() - 3.15).abs() < 0.001);
    assert!(config.get_bool("bool_value").unwrap());

    let large: i64 = config.get_as("large_int").unwrap();
    assert_eq!(large, 9223372036854775807);

    let small_u8: u8 = config.get_as("small_int").unwrap();
    assert_eq!(small_u8, 255);

    let int_u16: u16 = config.get_as("int_value").unwrap();
    assert_eq!(int_u16, 42);
}

/// 测试 CommandLineSource
#[test]
fn test_command_line_source() {
    use cmx_utils::CommandLineSource;
    use config::Source;

    let args = vec![
        "--host".to_string(),
        "localhost".to_string(),
        "--port=8080".to_string(),
        "--debug".to_string(),
        "true".to_string(),
    ];

    let source = CommandLineSource::from_args(args.into_iter());
    let map = source.collect().unwrap();

    assert_eq!(map.get("host").unwrap().clone().into_string().unwrap(), "localhost");
    assert_eq!(map.get("port").unwrap().clone().into_string().unwrap(), "8080");
    assert_eq!(map.get("debug").unwrap().clone().into_string().unwrap(), "true");
}

/// 测试 get_optional 和 get_as_or
#[test]
fn test_optional_and_default() {
    let dir = tempdir().unwrap();
    let config_path = dir.path().join("config.toml");

    let mut file = fs::File::create(&config_path).unwrap();
    file.write_all(b"existing_key = \"hello\"\n").unwrap();

    let config = Config::from_file(&config_path).unwrap();

    assert_eq!(
        config.get_as_or("existing_key", "default".to_string()),
        "hello"
    );
    assert_eq!(
        config.get_as_or("missing_key", "default".to_string()),
        "default"
    );

    let optional_val: Option<String> = config.get_optional("missing_key");
    assert!(optional_val.is_none());

    let existing_val: Option<String> = config.get_optional("existing_key");
    assert_eq!(existing_val.unwrap(), "hello");
}
