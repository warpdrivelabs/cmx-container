//! AST 结构解析器
//!
//! 使用 `syn` 解析 Rust 结构体定义，提取字段信息。

use syn::{Item, ItemStruct, Type, TypePath};
use anyhow::{Result, Context};

/// 结构体定义
#[derive(Debug, Clone)]
pub struct StructDefinition {
    /// 结构体名称
    pub name: String,
    /// 结构体字段
    pub fields: Vec<FieldDefinition>,
    /// 是否为元组结构体
    pub is_tuple: bool,
}

/// 字段定义
#[derive(Debug, Clone)]
pub struct FieldDefinition {
    /// 字段名
    pub name: String,
    /// 字段类型（字符串形式）
    pub type_name: String,
    /// 是否为必需
    pub required: bool,
    /// 字段描述
    pub description: String,
}

/// 解析 Rust 文件中的所有结构体定义
pub fn parse_structs(content: &str) -> Result<Vec<StructDefinition>> {
    let file = syn::parse_file(content)
        .context("Failed to parse Rust file")?;

    let mut structs = Vec::new();

    for item in file.items {
        if let Item::Struct(item_struct) = item {
            structs.push(parse_struct(&item_struct)?);
        }
    }

    Ok(structs)
}

/// 解析单个结构体
fn parse_struct(item: &ItemStruct) -> Result<StructDefinition> {
    let name = item.ident.to_string();

    let fields = if let syn::Fields::Named(named) = &item.fields {
        named
            .named
            .iter()
            .map(|f| {
                let field_name = f.ident.as_ref()
                    .map(|i| i.to_string())
                    .unwrap_or_default();
                let type_name = type_to_string(&f.ty);
                let description = extract_doc_comments(&f.attrs);
                let required = !type_name.starts_with("Option<");

                FieldDefinition {
                    name: field_name,
                    type_name,
                    required,
                    description,
                }
            })
            .collect()
    } else if let syn::Fields::Unnamed(unnamed) = &item.fields {
        // 元组结构体
        unnamed
            .unnamed
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let field_name = format!("_{}", i);
                let type_name = type_to_string(&f.ty);
                let description = extract_doc_comments(&f.attrs);

                FieldDefinition {
                    name: field_name,
                    type_name,
                    required: true,
                    description,
                }
            })
            .collect()
    } else {
        // 单元结构体
        Vec::new()
    };

    Ok(StructDefinition {
        name,
        fields,
        is_tuple: matches!(&item.fields, syn::Fields::Unnamed(_)),
    })
}

/// 从字段属性中提取文档注释
fn extract_doc_comments(attrs: &[syn::Attribute]) -> String {
    let mut doc_comments = Vec::new();

    for attr in attrs {
        if attr.path().is_ident("doc")
            && let syn::Meta::NameValue(nv) = &attr.meta
        {
            let nv_value = &nv.value;
            // 检查是否是 Lit 表达式
            if let syn::Expr::Lit(lit_expr) = nv_value {
                let lit = &lit_expr.lit;
                // 检查是否是字符串字面量
                if let syn::Lit::Str(s) = lit {
                    let comment = s.value().trim().to_string();
                    if !comment.is_empty() {
                        doc_comments.push(comment);
                    }
                }
            }
        }
    }

    doc_comments.join(" ")
}

/// 将类型转换为字符串
fn type_to_string(ty: &Type) -> String {
    match ty {
        Type::Path(TypePath { path, .. }) => {
            let segments: Vec<String> = path
                .segments
                .iter()
                .map(|seg| {
                    let ident = seg.ident.to_string();
                    if let syn::PathArguments::AngleBracketed(args) = &seg.arguments {
                        let args_str: Vec<String> = args
                            .args
                            .iter()
                            .filter_map(|arg| {
                                if let syn::GenericArgument::Type(t) = arg {
                                    Some(type_to_string(t))
                                } else {
                                    None
                                }
                            })
                            .collect();
                        if args_str.is_empty() {
                            ident
                        } else {
                            format!("{}<{}>", ident, args_str.join(", "))
                        }
                    } else {
                        ident
                    }
                })
                .collect();
            segments.join("::")
        }
        Type::Reference(ref_ty) => {
            let inner = type_to_string(&ref_ty.elem);
            if ref_ty.mutability.is_some() {
                format!("&mut {}", inner)
            } else {
                format!("&{}", inner)
            }
        }
        Type::Slice(slice) => {
            format!("[{}]", type_to_string(&slice.elem))
        }
        Type::Array(arr) => {
            format!("[{}; N]", type_to_string(&arr.elem))
        }
        Type::Tuple(tuple) => {
            if tuple.elems.is_empty() {
                "()".to_string()
            } else {
                let elems: Vec<String> = tuple
                    .elems
                    .iter()
                    .map(type_to_string)
                    .collect();
                format!("({})", elems.join(", "))
            }
        }
        _ => "unknown".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_struct() {
        let content = r#"
            pub struct InsertData {
                pub table: String,
                pub name: String,
                pub value: i32,
            }
        "#;

        let structs = parse_structs(content).unwrap();
        assert_eq!(structs.len(), 1);
        assert_eq!(structs[0].name, "InsertData");
        assert_eq!(structs[0].fields.len(), 3);
        assert_eq!(structs[0].fields[0].name, "table");
        assert_eq!(structs[0].fields[0].type_name, "String");
    }

    #[test]
    fn test_parse_nested_struct() {
        let content = r#"
            pub struct Outer {
                pub inner: Inner,
                pub value: i32,
            }

            pub struct Inner {
                pub field: String,
            }
        "#;

        let structs = parse_structs(content).unwrap();
        assert_eq!(structs.len(), 2);
    }

    #[test]
    fn test_parse_struct_with_doc_comments() {
        let content = r#"
            /// 用于事务删除函数的输入参数。
            pub struct DeleteData {
                /// 表名。
                pub table: String,
                /// 名称字段值。
                pub name: String,
            }
        "#;

        let structs = parse_structs(content).unwrap();
        assert_eq!(structs.len(), 1);
        assert_eq!(structs[0].name, "DeleteData");
        assert_eq!(structs[0].fields.len(), 2);
        assert_eq!(structs[0].fields[0].name, "table");
        assert_eq!(structs[0].fields[0].description, "表名。");
        assert_eq!(structs[0].fields[1].name, "name");
        assert_eq!(structs[0].fields[1].description, "名称字段值。");
    }
}
