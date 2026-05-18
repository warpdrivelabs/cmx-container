//! serde_json 操作测试模块
//!
//! 测试 cmx-core 中常用的 serde_json 功能，包括：
//! - JSON 序列化/反序列化
//! - Value 类型操作
//! - 错误处理

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// 表示一个简单的用户结构
#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct User {
    name: String,
    age: u32,
    email: Option<String>,
}

/// 表示一个嵌套的配置结构
#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct Config {
    app_name: String,
    version: String,
    features: Vec<String>,
    settings: Settings,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
struct Settings {
    debug: bool,
    max_connections: u32,
    timeout_seconds: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialize_simple_struct() {
        let user = User {
            name: "Alice".to_string(),
            age: 30,
            email: Some("alice@example.com".to_string()),
        };

        let json_str = serde_json::to_string(&user).unwrap();
        println!("[test_serialize_simple_struct] JSON: {}", json_str);
        assert!(json_str.contains("\"name\":\"Alice\""));
        assert!(json_str.contains("\"age\":30"));
        assert!(json_str.contains("\"email\":\"alice@example.com\""));
    }

    #[test]
    fn test_deserialize_simple_struct() {
        let json_str = r#"{"name":"Bob","age":25,"email":null}"#;

        let user: User = serde_json::from_str(json_str).unwrap();
        assert_eq!(user.name, "Bob");
        assert_eq!(user.age, 25);
        assert_eq!(user.email, None);
    }

    #[test]
    fn test_roundtrip_serialization() {
        let user = User {
            name: "Charlie".to_string(),
            age: 35,
            email: Some("charlie@example.com".to_string()),
        };
        // let config = Config {
        //     api_key: "12345".to_string(),
        //     password: "secret".to_string(),
        // };
        //
        // let value = serde_json::to_value(&config).unwrap();


        // // 获取 "profile" 字段，如果不存在则返回 null
        // let profile_value = &value["profile"];
        //
        // // 将 Value 转换回结构体 Profile
        // // from_value 返回 Result，需要处理 unwrap
        // let profile: Profile = serde_json::from_value(profile_value.clone()).unwrap();

        let json_str = serde_json::to_string(&user).unwrap();
        println!("[test_roundtrip_serialization] JSON: {}", json_str);
        let deserialized: User = serde_json::from_str(&json_str).unwrap();
        println!("[test_roundtrip_serialization] Deserialized: {:?}", deserialized);

        assert_eq!(user, deserialized);

        let result = serde_json::json!({
        "final": true,
        "merge_output": 1,
        "tx_insert_output": 1,
        "tx_update_output": 1,
        "tx_query_output": 1,
        "tx_delete_output": 1,
        "txn_id": 1,
        "message": "服务编排执行完成",
    });

        println!("{:?}", result)
    }

    #[test]
    fn test_nested_struct_serialization() {
        let config = Config {
            app_name: "MyApp".to_string(),
            version: "1.0.0".to_string(),
            features: vec!["feature_a".to_string(), "feature_b".to_string()],
            settings: Settings {
                debug: true,
                max_connections: 100,
                timeout_seconds: 30,
            },
        };

        let json_str = serde_json::to_string_pretty(&config).unwrap();
        println!("[test_nested_struct_serialization] JSON:\n{}", json_str);
        assert!(json_str.contains("\"app_name\":\"MyApp\""));
        assert!(json_str.contains("\"features\""));
        assert!(json_str.contains("\"settings\""));
    }

    #[test]
    fn test_json_value_object() {
        let json_str = r#"{
            "name": "David",
            "age": 28,
            "active": true,
            "tags": ["rust", "testing"],
            "metadata": {
                "created": "2024-01-01",
                "score": 95.5
            }
        }"#;

        let value: Value = serde_json::from_str(json_str).unwrap();
        println!("[test_json_value_object] Value: {:?}", value);
        println!("[test_json_value_object] JSON: {}", value);

        assert_eq!(value["name"], "David");
        assert_eq!(value["age"], 28);
        assert_eq!(value["active"], true);
        assert!(value["tags"].is_array());
        assert!(value["metadata"].is_object());
    }

    #[test]
    fn test_json_value_to_string() {
        let value = serde_json::json!({
            "message": "Hello",
            "count": 42
        });

        let str1 = value.to_string();
        let str2 = serde_json::to_string(&value).unwrap();

        assert_eq!(str1, str2);
        assert!(str1.contains("Hello"));
    }

    #[test]
    fn test_json_value_from_str() {
        let value: Value = serde_json::from_str(r#"{"key": "value"}"#).unwrap();
        assert_eq!(value["key"], "value");
    }

    #[test]
    fn test_json_array操作() {
        let json_str = r#"{"items": [1, 2, 3, 4, 5]}"#;
        let value: Value = serde_json::from_str(json_str).unwrap();

        let items = &value["items"];
        assert!(items.is_array());
        assert_eq!(items.as_array().unwrap().len(), 5);

        for (i, item) in items.as_array().unwrap().iter().enumerate() {
            assert_eq!(item.as_i64().unwrap() as u32, (i + 1) as u32);
        }
    }

    #[test]
    fn test_json_null值处理() {
        let json_str = r#"{"name": "Eve", "email": null}"#;
        let value: Value = serde_json::from_str(json_str).unwrap();

        assert!(value["name"].is_string());
        assert!(value["email"].is_null());
    }

    #[test]
    fn test_json_number操作() {
        let value: Value = serde_json::from_str(r#"{"count": 100, "price": 99.99}"#).unwrap();

        let count = &value["count"];
        let price = &value["price"];

        assert!(count.is_i64());
        assert!(price.is_f64());

        assert_eq!(count.as_i64().unwrap(), 100);
        assert!((price.as_f64().unwrap() - 99.99).abs() < f64::EPSILON);
    }

    #[test]
    fn test_json_bool操作() {
        let value: Value = serde_json::from_str(r#"{"enabled": true, "disabled": false}"#).unwrap();

        assert_eq!(value["enabled"].as_bool().unwrap(), true);
        assert_eq!(value["disabled"].as_bool().unwrap(), false);
    }

    #[test]
    fn test_json_object操作() {
        let json_str = r#"{"a": 1, "b": 2}"#;
        let mut value: Value = serde_json::from_str(json_str).unwrap();
        println!("[test_json_object操作] Initial: {}", value);

        assert!(value.is_object());
        assert_eq!(value.as_object().unwrap().len(), 2);

        value["c"] = serde_json::json!(3);
        println!("[test_json_object操作] After add c: {}", value);
        assert_eq!(value.as_object().unwrap().len(), 3);
        assert_eq!(value["c"], 3);

        value.as_object_mut().unwrap().remove("a");
        println!("[test_json_object操作] After remove a: {}", value);
        assert_eq!(value.as_object().unwrap().len(), 2);
        assert!(value.get("a").is_none());
    }

    #[test]
    fn test_json_merge() {
        let mut base = serde_json::json!({
            "name": "Test",
            "version": "1.0"
        });

        let overlay = serde_json::json!({
            "version": "2.0",
            "new_field": "added"
        });

        for (key, value) in overlay.as_object().unwrap() {
            base[key] = value.clone();
        }

        assert_eq!(base["name"], "Test");
        assert_eq!(base["version"], "2.0");
        assert_eq!(base["new_field"], "added");
    }

    #[test]
    fn test_json_macro_usage() {
        let value = serde_json::json!({
            "string": "hello",
            "number": 42,
            "float": 3.14,
            "boolean": true,
            "null": null,
            "array": [1, 2, 3],
            "object": {
                "nested": "value"
            }
        });

        println!("[test_json_macro_usage] JSON Macro result: {}", value);

        assert_eq!(value["string"], "hello");
        assert_eq!(value["number"], 42);
        assert!((value["float"].as_f64().unwrap() - 3.14).abs() < f64::EPSILON);
        assert_eq!(value["boolean"], true);
        assert!(value["null"].is_null());
        assert!(value["array"].is_array());
        assert!(value["object"].is_object());
    }

    #[test]
    fn test_deserialize_error_handling() {
        let invalid_json = r#"{"name": "test", "age": "not a number"}"#;

        let result: Result<User, _> = serde_json::from_str(invalid_json);
        assert!(result.is_err());
    }

    #[test]
    fn test_serialize_error_handling() {
        let value: Value = serde_json::from_str(r#"{"key": "value"}"#).unwrap();

        let result = serde_json::to_string(&value);
        assert!(result.is_ok());
    }

    #[test]
    fn test_pretty_print() {
        let user = User {
            name: "Frank".to_string(),
            age: 40,
            email: None,
        };

        let pretty = serde_json::to_string_pretty(&user).unwrap();
        println!("[test_pretty_print] Pretty JSON:\n{}", pretty);

        let compact = serde_json::to_string(&user).unwrap();
        println!("[test_pretty_print] Compact JSON: {}", compact);

        assert!(pretty.contains("\n"));
        assert!(pretty.len() > compact.len());
    }

    #[test]
    fn test_vec序列化() {
        let users = vec![
            User {
                name: "Alice".to_string(),
                age: 30,
                email: None,
            },
            User {
                name: "Bob".to_string(),
                age: 25,
                email: Some("bob@example.com".to_string()),
            },
        ];

        let json_str = serde_json::to_string(&users).unwrap();
        assert!(json_str.starts_with("["));
        assert!(json_str.ends_with("]"));

        let deserialized: Vec<User> = serde_json::from_str(&json_str).unwrap();
        assert_eq!(deserialized.len(), 2);
        assert_eq!(deserialized[0].name, "Alice");
        assert_eq!(deserialized[1].name, "Bob");
    }

    #[test]
    fn test_map序列化() {
        use std::collections::HashMap;

        let mut map = HashMap::new();
        map.insert("key1".to_string(), "value1".to_string());
        map.insert("key2".to_string(), "value2".to_string());

        let json_str = serde_json::to_string(&map).unwrap();
        let deserialized: HashMap<String, String> = serde_json::from_str(&json_str).unwrap();

        assert_eq!(deserialized.get("key1").unwrap(), "value1");
        assert_eq!(deserialized.get("key2").unwrap(), "value2");
    }

    #[test]
    fn test_option序列化() {
        let user_with_email = User {
            name: "Grace".to_string(),
            age: 28,
            email: Some("grace@example.com".to_string()),
        };

        let user_without_email = User {
            name: "Henry".to_string(),
            age: 35,
            email: None,
        };

        let json_with = serde_json::to_string(&user_with_email).unwrap();
        let json_without = serde_json::to_string(&user_without_email).unwrap();

        assert!(json_with.contains("email"));
        assert!(json_without.contains("email"));
    }

    #[test]
    fn test_number类型转换() {
        let value: Value = serde_json::from_str(r#"{"int": 42, "float": 3.14}"#).unwrap();

        let int_val = &value["int"];
        let float_val = &value["float"];

        assert!(int_val.is_i64());
        assert!(float_val.is_f64());

        assert_eq!(int_val.as_i64().unwrap(), 42);
        assert!((float_val.as_f64().unwrap() - 3.14).abs() < f64::EPSILON);
    }

    #[test]
    fn test_slice操作() {
        let value: Value = serde_json::from_str(r#"{"items": [1, 2, 3, 4, 5]}"#).unwrap();
        let items = value["items"].as_array().unwrap();

        assert_eq!(items.first().unwrap().as_i64().unwrap(), 1);
        assert_eq!(items.last().unwrap().as_i64().unwrap(), 5);

        let slice = &items[1..4];
        assert_eq!(slice.len(), 3);
    }
}
