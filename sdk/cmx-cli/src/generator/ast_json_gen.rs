//! AST 模式 JSON 文档生成器
//!
//! 使用 AST 解析结构体定义，生成带有完整嵌套结构的 API 文档。

use anyhow::Result;
use chrono::Utc;

use crate::ast_parser::{TypeRegistry, ResolvedField, is_primitive_type, is_container_type, extract_container_element};
use crate::models::{
    Example, FieldSpec, FunctionDoc, InputSpec, OutputSpec, PluginDocument, PluginInfo,
    SourceLocation,
};
use crate::parser::{ParsedFunction, ParsedDoc};

/// 扫描结果
#[derive(Debug, Clone)]
pub struct AstScanResult {
    /// 插件名称
    pub plugin_name: String,
    /// 插件版本
    pub plugin_version: String,
    /// 插件描述
    pub plugin_description: Option<String>,
    /// 解析后的函数列表
    pub functions: Vec<(ParsedFunction, ParsedDoc)>,
    /// 相对文件路径
    pub file_path: String,
    /// 类型注册表
    pub type_registry: TypeRegistry,
}

/// 生成插件文档（AST 模式）
pub fn generate_ast_document(result: &AstScanResult, pretty: bool, expand_depth: usize) -> Result<String> {
    let functions: Vec<FunctionDoc> = result
        .functions
        .iter()
        .map(|(func, doc)| build_ast_function_doc(func, doc, &result.file_path, &result.type_registry, expand_depth))
        .collect();

    let document = PluginDocument {
        plugin: PluginInfo {
            name: result.plugin_name.clone(),
            version: result.plugin_version.clone(),
            description: result.plugin_description.clone(),
            generated_at: Utc::now().to_rfc3339(),
        },
        functions,
        types: None,
    };

    if pretty {
        Ok(serde_json::to_string_pretty(&document)?)
    } else {
        Ok(serde_json::to_string(&document)?)
    }
}

/// 构建函数文档（AST 模式）
fn build_ast_function_doc(
    func: &ParsedFunction,
    doc: &ParsedDoc,
    file_path: &str,
    registry: &TypeRegistry,
    expand_depth: usize,
) -> FunctionDoc {
    // 使用 AST 模式构建字段
    let input_fields = build_ast_fields(&doc.input_fields, registry, expand_depth);
    let output_fields = build_ast_fields(&doc.output_fields, registry, expand_depth);

    // 构建示例
    let examples: Vec<Example> = doc
        .examples
        .iter()
        .map(|e| Example {
            input: e.input.clone(),
            output: e.output.clone(),
        })
        .collect();

    // 使用文档中的编码信息，如果没有则使用从签名提取的
    let input_encoding = doc
        .input_encoding
        .clone()
        .unwrap_or_else(|| func.input_encoding.clone());
    let output_encoding = doc
        .output_encoding
        .clone()
        .unwrap_or_else(|| func.output_encoding.clone());

    FunctionDoc {
        name: func.name.clone(),
        doc_type: func.doc_type.clone(),
        summary: if doc.summary.is_empty() {
            func.name.clone()
        } else {
            doc.summary.clone()
        },
        description: doc.description.clone(),
        input: InputSpec {
            encoding: input_encoding,
            type_name: func.input_type.clone(),
            fields: input_fields,
        },
        output: OutputSpec {
            encoding: output_encoding,
            type_name: func.output_type.clone(),
            fields: output_fields,
        },
        examples,
        errors: doc.errors.clone(),
        notes: doc.notes.clone(),
        panics: doc.panics.clone(),
        safety: doc.safety.clone(),
        location: SourceLocation {
            file: file_path.to_string(),
            line: func.line,
        },
    }
}

/// 使用 AST 解析构建字段
fn build_ast_fields(
    fields: &[crate::parser::FieldInfo],
    registry: &TypeRegistry,
    expand_depth: usize,
) -> Vec<FieldSpec> {
    fields
        .iter()
        .map(|f| resolve_field_to_spec(f, registry, expand_depth))
        .collect()
}

fn resolve_field_to_spec(
    field: &crate::parser::FieldInfo,
    registry: &TypeRegistry,
    max_depth: usize,
) -> FieldSpec {
    if !field.sub_fields.is_empty() {
        let sub_fields: Vec<FieldSpec> = field
            .sub_fields
            .iter()
            .map(|sf| table_field_to_spec(sf))
            .collect();
        return FieldSpec {
            name: field.name.clone(),
            type_name: "object".to_string(),
            format: None,
            required: field.required,
            description: trim_description(&field.description),
            sub_fields,
            items: None,
        };
    }

    let type_name = if field.type_name != "unknown" && field.type_name != "serde_json::Value" {
        field.type_name.clone()
    } else {
        extract_type_from_description(&field.description, registry)
            .unwrap_or_else(|| "serde_json::Value".to_string())
    };

    let resolved = registry.resolve_type(&type_name, &field.name, &field.description, max_depth);

    resolved_field_to_spec(&resolved)
}

fn extract_type_from_description(desc: &str, registry: &TypeRegistry) -> Option<String> {
    let desc = desc.trim();
    let mut candidates = Vec::new();
    let mut search_start = 0;

    while let Some(start) = desc[search_start..].find('`') {
        let abs_start = search_start + start;
        if let Some(end) = desc[abs_start + 1..].find('`') {
            let type_candidate = &desc[abs_start + 1..abs_start + 1 + end];
            if !type_candidate.is_empty() && type_candidate.chars().next().unwrap().is_uppercase() {
                candidates.push(type_candidate.to_string());
            }
            search_start = abs_start + 1 + end + 1;
        } else {
            break;
        }
    }

    for candidate in &candidates {
        if registry.is_registered(candidate) {
            return Some(candidate.clone());
        }
    }

    candidates.into_iter().next()
}

fn table_field_to_spec(field: &crate::parser::FieldInfo) -> FieldSpec {
    let type_name = match field.type_name.as_str() {
        "string" | "str" | "String" => "string".to_string(),
        "integer" | "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64" => "integer".to_string(),
        "number" | "f32" | "f64" => "number".to_string(),
        "boolean" | "bool" => "boolean".to_string(),
        "array" => "array".to_string(),
        _ => "object".to_string(),
    };
    FieldSpec {
        name: field.name.clone(),
        type_name,
        format: None,
        required: field.required,
        description: trim_description(&field.description),
        sub_fields: Vec::new(),
        items: None,
    }
}

/// 去掉描述末尾的句号
fn trim_description(desc: &str) -> String {
    let desc = desc.trim();
    if let Some(stripped) = desc.strip_suffix('。') {
        stripped.trim_end().to_string()
    } else if let Some(stripped) = desc.strip_suffix('.') {
        stripped.trim_end().to_string()
    } else {
        desc.to_string()
    }
}

/// 将解析后的字段转换为 FieldSpec
fn resolved_field_to_spec(resolved: &ResolvedField) -> FieldSpec {
    let sub_fields: Vec<FieldSpec> = resolved
        .sub_fields
        .iter()
        .map(resolved_field_to_spec)
        .collect();

    // 确定类型
    let (type_name, format) = if is_container_type(&resolved.type_name) {
        if let Some(elem_type) = extract_container_element(&resolved.type_name) {
            if is_primitive_type(&elem_type) {
                ("array".to_string(), None)
            } else {
                // 容器元素是非基本类型
                ("array".to_string(), Some(elem_type))
            }
        } else {
            ("array".to_string(), None)
        }
    } else if is_primitive_type(&resolved.type_name) {
        (map_primitive_to_json_type(&resolved.type_name), None)
    } else if resolved.sub_fields.is_empty() {
        // 没有子字段的非基本类型
        ("object".to_string(), None)
    } else {
        ("object".to_string(), None)
    };

    FieldSpec {
        name: resolved.name.clone(),
        type_name,
        format,
        required: Some(resolved.required),
        description: trim_description(&resolved.description),
        sub_fields,
        items: None, // items 通过 sub_fields 处理
    }
}

/// 将 Rust 基本类型映射为 JSON Schema 类型
fn map_primitive_to_json_type(rust_type: &str) -> String {
    match rust_type {
        "i8" | "i16" | "i32" | "i64" | "i128" | "isize"
        | "u8" | "u16" | "u32" | "u64" | "u128" | "usize" => "integer".to_string(),

        "f32" | "f64" => "number".to_string(),

        "bool" => "boolean".to_string(),

        "char" | "String" | "str" => "string".to_string(),

        "()" => "null".to_string(),

        _ => "object".to_string(),
    }
}
