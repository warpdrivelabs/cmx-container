//! 类型解析器
//!
//! 维护结构体定义注册表，解析和展开嵌套类型。

use crate::ast_parser::struct_parser::StructDefinition;
use std::collections::HashMap;

/// 类型注册表
#[derive(Debug, Clone, Default)]
pub struct TypeRegistry {
    /// 结构体定义映射
    structs: HashMap<String, StructDefinition>,
}

impl TypeRegistry {
    /// 创建新的类型注册表
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册结构体定义
    pub fn register(&mut self, struct_def: StructDefinition) {
        self.structs.insert(struct_def.name.clone(), struct_def);
    }

    /// 注册多个结构体定义
    pub fn register_all(&mut self, structs: impl IntoIterator<Item = StructDefinition>) {
        for s in structs {
            self.register(s);
        }
    }

    /// 解析类型并展开为嵌套结构
    ///
    /// # Arguments
    ///
    /// * `type_name` - 类型名称（如 `InsertData`）
    /// * `field_name` - 字段名称
    /// * `description` - 字段描述
    /// * `max_depth` - 最大展开深度
    ///
    /// # Returns
    ///
    /// 展开后的嵌套字段信息
    pub fn resolve_type(
        &self,
        type_name: &str,
        field_name: &str,
        description: &str,
        max_depth: usize,
    ) -> ResolvedField {
        self.resolve_field_internal(type_name, field_name, description, true, max_depth, 0)
    }

    fn resolve_field_internal(
        &self,
        type_name: &str,
        field_name: &str,
        description: &str,
        required: bool,
        max_depth: usize,
        current_depth: usize,
    ) -> ResolvedField {
        let clean_type = clean_type_name(type_name);

        if let Some(struct_def) = self.structs.get(&clean_type) {
            if current_depth >= max_depth {
                return ResolvedField {
                    name: field_name.to_string(),
                    type_name: clean_type,
                    description: description.to_string(),
                    required,
                    sub_fields: Vec::new(),
                    is_expanded: false,
                };
            }

            let sub_fields: Vec<ResolvedField> = struct_def
                .fields
                .iter()
                .map(|f| {
                    self.resolve_field_internal(
                        &f.type_name,
                        &f.name,
                        &f.description,
                        f.required,
                        max_depth,
                        current_depth + 1,
                    )
                })
                .collect();

            ResolvedField {
                name: field_name.to_string(),
                type_name: clean_type,
                description: description.to_string(),
                required,
                sub_fields,
                is_expanded: true,
            }
        } else {
            let is_option = clean_type.starts_with("Option<");
            ResolvedField {
                name: field_name.to_string(),
                type_name: clean_type,
                description: description.to_string(),
                required: required && !is_option,
                sub_fields: Vec::new(),
                is_expanded: false,
            }
        }
    }

    /// 检查类型是否已注册
    pub fn is_registered(&self, type_name: &str) -> bool {
        self.structs.contains_key(&clean_type_name(type_name))
    }

    /// 获取结构体定义
    pub fn get_struct(&self, type_name: &str) -> Option<&StructDefinition> {
        self.structs.get(&clean_type_name(type_name))
    }
}

/// 已解析的字段
#[derive(Debug, Clone)]
pub struct ResolvedField {
    /// 字段名
    pub name: String,
    /// 类型名
    pub type_name: String,
    /// 描述
    pub description: String,
    /// 是否必需
    pub required: bool,
    /// 子字段（嵌套结构）
    pub sub_fields: Vec<ResolvedField>,
    /// 是否已展开
    pub is_expanded: bool,
}

impl ResolvedField {
    /// 检查是否为基本类型
    pub fn is_primitive(&self) -> bool {
        is_primitive_type(&self.type_name)
    }

    /// 检查是否为容器类型（Vec, Option, etc.）
    pub fn is_container(&self) -> bool {
        is_container_type(&self.type_name)
    }

    /// 获取容器元素类型
    pub fn element_type(&self) -> Option<String> {
        extract_container_element(&self.type_name)
    }
}

/// 清理类型名（移除引用、指针等）
fn clean_type_name(type_name: &str) -> String {
    let cleaned = type_name
        .trim()
        .trim_start_matches('&')
        .trim_start_matches("mut ")
        .trim();

    // 处理 `crate::` 前缀，简化类型名
    if let Some(after_crate) = cleaned.strip_prefix("crate::") {
        // 取最后一部分作为类型名
        after_crate
            .split("::")
            .last()
            .unwrap_or(after_crate)
            .to_string()
    } else if cleaned.contains("::") {
        // 只取最后的类型名
        cleaned.split("::").last().unwrap_or(cleaned).to_string()
    } else {
        cleaned.to_string()
    }
}

/// 判断是否为基本类型
pub fn is_primitive_type(type_name: &str) -> bool {
    matches!(
        clean_type_name(type_name).as_str(),
        "i8" | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "isize"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "usize"
            | "f32"
            | "f64"
            | "bool"
            | "char"
            | "String"
            | "str"
            | "()"
    )
}

/// 判断是否为容器类型
pub fn is_container_type(type_name: &str) -> bool {
    let cleaned = clean_type_name(type_name);
    cleaned.starts_with("Vec<")
        || cleaned.starts_with("Option<")
        || cleaned.starts_with("Box<")
        || cleaned.starts_with("Result<")
        || cleaned.starts_with("HashMap<")
        || cleaned.starts_with("Map<")
        || cleaned.starts_with("Set<")
}

/// 提取容器元素类型
pub fn extract_container_element(type_name: &str) -> Option<String> {
    let cleaned = clean_type_name(type_name);

    // HashMap<K, V> - 返回 V (需要先检查，避免被通用逻辑匹配)
    if cleaned.starts_with("HashMap<")
        && let Some(start) = cleaned.find('<')
    {
        let after_bracket = &cleaned[start + 1..];
        if let Some(comma_pos) = after_bracket.find(',') {
            let value_part = after_bracket[comma_pos + 1..].trim_start();
            if let Some(end) = value_part.find('>') {
                return Some(value_part[..end].trim().to_string());
            }
        }
    }

    // Vec<T>, Option<T>, Box<T>, Result<T> 等
    if let Some(start) = cleaned.find('<')
        && let Some(end) = cleaned.rfind('>')
        && end > start
    {
        return Some(cleaned[start + 1..end].to_string());
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_type_name() {
        assert_eq!(clean_type_name("String"), "String");
        assert_eq!(clean_type_name("&String"), "String");
        assert_eq!(clean_type_name("&mut String"), "String");
        assert_eq!(clean_type_name("crate::types::User"), "User");
        assert_eq!(clean_type_name("Option<String>"), "Option<String>");
    }

    #[test]
    fn test_is_primitive() {
        assert!(is_primitive_type("String"));
        assert!(is_primitive_type("i32"));
        assert!(is_primitive_type("bool"));
        assert!(!is_primitive_type("Vec<String>"));
        assert!(!is_primitive_type("InsertData"));
    }

    #[test]
    fn test_extract_container_element() {
        assert_eq!(
            extract_container_element("Vec<String>"),
            Some("String".to_string())
        );
        assert_eq!(
            extract_container_element("Option<i32>"),
            Some("i32".to_string())
        );
        assert_eq!(
            extract_container_element("HashMap<String, i32>"),
            Some("i32".to_string())
        );
    }
}
