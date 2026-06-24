//! 公共工具函数。

use std::collections::HashMap;

use nacos_sdk::api::props::ClientProps;

/// 递归将 `toml::Value` 转换为 `config::Value`。
///
/// 支持所有 TOML 类型：String、Integer、Float、Boolean、Table（递归）、Array、Datetime（转为字符串）。
pub fn toml_to_config_value(toml_val: toml::Value) -> config::Value {
    match toml_val {
        toml::Value::String(s) => config::Value::new(None, s),
        toml::Value::Integer(i) => config::Value::new(None, i),
        toml::Value::Float(f) => config::Value::new(None, f),
        toml::Value::Boolean(b) => config::Value::new(None, b),
        toml::Value::Table(table) => {
            let mut map = HashMap::new();
            for (k, v) in table {
                map.insert(k, toml_to_config_value(v));
            }
            config::Value::new(None, map)
        }
        toml::Value::Array(arr) => {
            let vec: Vec<config::Value> = arr.into_iter().map(toml_to_config_value).collect();
            config::Value::new(None, vec)
        }
        toml::Value::Datetime(dt) => config::Value::new(None, dt.to_string()),
    }
}

/// 构建 Nacos `ClientProps`（Naming 和 ConfigCenter 共用）。
///
/// 设置服务器地址、命名空间、应用名称，并在同时提供用户名和密码时启用认证。
pub fn build_nacos_client_props(
    server_addr: &str,
    namespace: &str,
    app_name: &str,
    username: &Option<String>,
    password: &Option<String>,
) -> ClientProps {
    let mut props = ClientProps::new()
        .server_addr(server_addr)
        .namespace(namespace)
        .app_name(app_name);

    if let (Some(user), Some(pass)) = (username, password) {
        props = props.auth_username(user).auth_password(pass);
    }

    props
}
