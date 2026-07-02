//! AST 解析器
//!
//! 使用 `syn` 解析 Rust 源文件，识别 `#[plugin_fn]` 属性函数。

use anyhow::{Context, Result};
use syn::{
    Attribute, File as SynFile, FnArg, Item, ItemFn, PathArguments, ReturnType, Type, TypePath,
    parse_file,
};

/// 解析后的函数信息
#[derive(Debug, Clone)]
pub struct ParsedFunction {
    /// 函数名
    pub name: String,
    /// 文档注释
    pub doc_comments: Vec<String>,
    /// 文档类型：默认 "func"，如果有 #[doc_type = "branch_fn"] 则为 "branch_fn"
    pub doc_type: String,
    /// 输入类型
    pub input_type: String,
    /// 输出类型
    pub output_type: String,
    /// 输入编码方式
    pub input_encoding: String,
    /// 输出编码方式
    pub output_encoding: String,
    /// 起始行号
    pub line: usize,
}

/// 解析 Rust 源文件
pub fn parse_rust_file(content: &str) -> Result<Vec<ParsedFunction>> {
    let file: SynFile = parse_file(content).context("Failed to parse Rust file")?;

    let mut functions = Vec::new();

    for item in &file.items {
        if let Item::Fn(fn_item) = item
            && has_plugin_fn_attribute(&fn_item.attrs)
            && let Some(parsed) = parse_plugin_function(fn_item)?
        {
            functions.push(parsed);
        }
    }

    Ok(functions)
}

/// 检查是否有 `#[plugin_fn]` 属性
fn has_plugin_fn_attribute(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        let path = &attr.path();
        path.segments.len() == 1 && path.segments[0].ident == "plugin_fn"
    })
}

/// 解析插件函数
fn parse_plugin_function(fn_item: &ItemFn) -> Result<Option<ParsedFunction>> {
    let name = fn_item.sig.ident.to_string();

    // 提取文档注释
    let doc_comments = extract_doc_comments(&fn_item.attrs);

    // 提取文档类型
    let doc_type = extract_doc_type(&fn_item.attrs);

    // 解析输入类型
    let (input_type, input_encoding) = parse_input_type(&fn_item.sig.inputs)?;

    // 解析输出类型
    let (output_type, output_encoding) = parse_output_type(&fn_item.sig.output)?;

    // 获取行号
    let line = fn_item.sig.fn_token.span.start().line;

    Ok(Some(ParsedFunction {
        name,
        doc_comments,
        doc_type,
        input_type,
        output_type,
        input_encoding,
        output_encoding,
        line,
    }))
}

/// 提取文档注释
fn extract_doc_comments(attrs: &[Attribute]) -> Vec<String> {
    attrs
        .iter()
        .filter_map(|attr| {
            if attr.path().is_ident("doc") {
                let meta = &attr.meta;
                if let syn::Meta::NameValue(nv) = meta
                    && let syn::Expr::Lit(lit) = &nv.value
                    && let syn::Lit::Str(lit_str) = &lit.lit
                {
                    return Some(lit_str.value());
                }
            }
            None
        })
        .collect()
}

/// 提取 #[doc_type = "..."] 属性值
fn extract_doc_type(attrs: &[Attribute]) -> String {
    for attr in attrs {
        if attr.path().is_ident("doc_type")
            && let syn::Meta::NameValue(nv) = &attr.meta
            && let syn::Expr::Lit(lit) = &nv.value
            && let syn::Lit::Str(lit_str) = &lit.lit
        {
            return lit_str.value();
        }
    }
    // 默认类型为 "func"
    "func".to_string()
}

/// 解析输入类型
fn parse_input_type(
    inputs: &syn::punctuated::Punctuated<FnArg, syn::Token![,]>,
) -> Result<(String, String)> {
    for input in inputs {
        if let FnArg::Typed(pat_type) = input {
            let type_str = type_to_string(&pat_type.ty);
            let (inner_type, encoding) = extract_encoding(&type_str);
            return Ok((inner_type, encoding));
        }
    }
    Ok(("unknown".to_string(), "raw".to_string()))
}

/// 解析输出类型
fn parse_output_type(output: &ReturnType) -> Result<(String, String)> {
    match output {
        ReturnType::Type(_, ty) => {
            let type_str = type_to_string(ty);
            // FnResult<Json<T>> 或 FnResult<Msgpack<T>>
            let inner = extract_fn_result_inner(&type_str);
            let (inner_type, encoding) = extract_encoding(&inner);
            Ok((inner_type, encoding))
        }
        ReturnType::Default => Ok(("unknown".to_string(), "raw".to_string())),
    }
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
                    if let PathArguments::AngleBracketed(args) = &seg.arguments {
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
        _ => "unknown".to_string(),
    }
}

/// 从 FnResult<T> 中提取内部类型
fn extract_fn_result_inner(type_str: &str) -> String {
    if let Some(start) = type_str.find("FnResult<") {
        let start = start + "FnResult<".len();
        if let Some(end) = type_str.rfind('>') {
            return type_str[start..end].to_string();
        }
    }
    type_str.to_string()
}

/// 从 Json<T> 或 Msgpack<T> 中提取编码方式和内部类型
fn extract_encoding(type_str: &str) -> (String, String) {
    if type_str.starts_with("Json<") && type_str.ends_with('>') {
        let inner = &type_str["Json<".len()..type_str.len() - 1];
        (inner.to_string(), "json".to_string())
    } else if type_str.starts_with("Msgpack<") && type_str.ends_with('>') {
        let inner = &type_str["Msgpack<".len()..type_str.len() - 1];
        (inner.to_string(), "msgpack".to_string())
    } else {
        (type_str.to_string(), "raw".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_encoding() {
        assert_eq!(
            extract_encoding("Json<FunctionInput>"),
            ("FunctionInput".to_string(), "json".to_string())
        );
        assert_eq!(
            extract_encoding("Msgpack<FunctionInput>"),
            ("FunctionInput".to_string(), "msgpack".to_string())
        );
        assert_eq!(
            extract_encoding("String"),
            ("String".to_string(), "raw".to_string())
        );
    }

    #[test]
    fn test_extract_fn_result_inner() {
        assert_eq!(
            extract_fn_result_inner("FnResult<Json<FunctionOutput>>"),
            "Json<FunctionOutput>".to_string()
        );
    }
}
