//! AST 模式 JSON 文档生成器。
//!
//! 使用 AST 解析结构体定义，生成带有完整嵌套结构的 API 文档。
//! 通过 `TypeRegistry` 解析类型的字段结构，将 Rust 类型信息映射为 JSON Schema 风格的文档。

use anyhow::Result;
use chrono::Utc;

use crate::ast_parser::{
    ResolvedField, TypeRegistry, extract_container_element, is_container_type, is_primitive_type,
};
use crate::models::{
    Example, FieldSpec, FunctionDoc, InputSpec, OutputSpec, PluginDocument, PluginInfo,
    SourceLocation,
};
use crate::parser::{ParsedDoc, ParsedFunction};

/// AST 扫描结果。
///
/// 包含从源文件中通过 AST 分析得到的全部信息，包括插件元数据、
/// 函数列表及其文档注释、类型注册表等。
#[derive(Debug, Clone)]
pub struct AstScanResult {
    /// 插件名称。
    pub plugin_name: String,
    /// 插件版本号。
    pub plugin_version: String,
    /// 插件的简要描述。
    pub plugin_description: Option<String>,
    /// 解析后的函数列表，每项为函数签名与文档注释的配对。
    pub functions: Vec<(ParsedFunction, ParsedDoc)>,
    /// 源文件相对于项目根目录的路径。
    pub file_path: String,
    /// 类型注册表，存储源码中定义的所有结构体、枚举及其字段信息。
    pub type_registry: TypeRegistry,
}

/// 生成插件文档（AST 模式）。
///
/// 将 AST 扫描结果转换为完整的 JSON 文档，包括插件元信息和所有函数的 API 文档。
/// 类型信息根据 `expand_depth` 参数递归展开嵌套结构。
///
/// # Arguments
///
/// * `result` - AST 扫描结果，包含函数列表和类型注册表。
/// * `pretty` - 是否以格式化的 JSON 输出（`true` 为美化格式，`false` 为紧凑格式）。
/// * `expand_depth` - 类型展开的最大递归深度，控制嵌套结构的展开层数。
///
/// # Returns
///
/// 序列化后的 JSON 文档字符串。
///
/// # Errors
///
/// 当 JSON 序列化失败时返回错误。
pub fn generate_ast_document(
    result: &AstScanResult,
    pretty: bool,
    expand_depth: usize,
) -> Result<String> {
    let functions: Vec<FunctionDoc> = result
        .functions
        .iter()
        .map(|(func, doc)| {
            build_ast_function_doc(
                func,
                doc,
                &result.file_path,
                &result.type_registry,
                expand_depth,
            )
        })
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

/// 构建单个函数的 API 文档（AST 模式）。
///
/// 综合函数签名信息和文档注释信息，生成完整的 `FunctionDoc`。
/// 编码信息优先使用文档注释中的值，若未指定则回退到从函数签名提取的值。
///
/// # Arguments
///
/// * `func` - 解析后的函数签名信息。
/// * `doc` - 解析后的文档注释内容。
/// * `file_path` - 源文件路径。
/// * `registry` - 类型注册表，用于解析嵌套类型。
/// * `expand_depth` - 类型展开的最大递归深度。
///
/// # Returns
///
/// 构建完成的函数文档结构体。
fn build_ast_function_doc(
    func: &ParsedFunction,
    doc: &ParsedDoc,
    file_path: &str,
    registry: &TypeRegistry,
    expand_depth: usize,
) -> FunctionDoc {
    let input_fields = build_ast_fields(&doc.input_fields, registry, expand_depth);
    let output_fields = build_ast_fields(&doc.output_fields, registry, expand_depth);

    let examples: Vec<Example> = doc
        .examples
        .iter()
        .map(|e| Example {
            input: e.input.clone(),
            output: e.output.clone(),
        })
        .collect();

    // 优先使用文档注释中的编码信息，若未指定则使用从签名提取的值
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

/// 将解析后的字段列表转换为 `FieldSpec` 列表（AST 模式）。
///
/// 遍历文档注释中解析出的字段信息，利用类型注册表解析每个字段的类型结构。
///
/// # Arguments
///
/// * `fields` - 从文档注释中解析出的字段信息列表。
/// * `registry` - 类型注册表。
/// * `expand_depth` - 类型展开的最大递归深度。
///
/// # Returns
///
/// 转换后的 `FieldSpec` 列表。
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

/// 将文档注释中的字段信息解析为 `FieldSpec`。
///
/// 处理逻辑如下：
/// 1. 若字段已有子字段（来自文档注释中的表格），直接转换为 `object` 类型。
/// 2. 否则，利用类型注册表解析字段类型，递归展开嵌套结构。
/// 3. 当字段类型为 `unknown` 或 `serde_json::Value` 时，尝试从描述文本中提取类型名。
///
/// # Arguments
///
/// * `field` - 待解析的字段信息。
/// * `registry` - 类型注册表。
/// * `max_depth` - 类型展开的最大递归深度。
///
/// # Returns
///
/// 解析后的 `FieldSpec`。
fn resolve_field_to_spec(
    field: &crate::parser::FieldInfo,
    registry: &TypeRegistry,
    max_depth: usize,
) -> FieldSpec {
    if !field.sub_fields.is_empty() {
        // 字段已有来自文档注释的子字段，直接作为 object 类型处理
        let sub_fields: Vec<FieldSpec> = field.sub_fields.iter().map(table_field_to_spec).collect();
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

    // 确定类型名称：优先使用字段自身类型，否则尝试从描述中提取
    let type_name = if field.type_name != "unknown" && field.type_name != "serde_json::Value" {
        field.type_name.clone()
    } else {
        extract_type_from_description(&field.description, registry)
            .unwrap_or_else(|| "serde_json::Value".to_string())
    };

    if JSON_SCHEMA_TYPES.contains(&type_name.as_str()) {
        let clean_desc = trim_description(
            field
                .description
                .replace(&format!("`{}`", type_name), "")
                .trim(),
        );
        return FieldSpec {
            name: field.name.clone(),
            type_name: type_name.clone(),
            format: None,
            required: field.required,
            description: clean_desc,
            sub_fields: Vec::new(),
            items: None,
        };
    }

    let resolved = registry.resolve_type(&type_name, &field.name, &field.description, max_depth);

    resolved_field_to_spec(&resolved)
}

/// 从字段描述文本中提取类型名称。
///
/// 扫描描述文本中被反引号包裹且以大写字母开头的标识符，
/// 优先返回在类型注册表中已注册的候选类型名。
///
/// # Arguments
///
/// * `desc` - 字段描述文本。
/// * `registry` - 类型注册表，用于验证候选类型名是否已注册。
///
/// # Returns
///
/// 找到已注册类型时返回 `Some(type_name)`，否则返回第一个候选或 `None`。
const JSON_SCHEMA_TYPES: &[&str] = &["string", "integer", "number", "boolean", "array", "object"];

fn extract_type_from_description(desc: &str, registry: &TypeRegistry) -> Option<String> {
    let desc = desc.trim();
    let mut candidates = Vec::new();
    let mut search_start = 0;

    while let Some(start) = desc[search_start..].find('`') {
        let abs_start = search_start + start;
        if let Some(end) = desc[abs_start + 1..].find('`') {
            let type_candidate = &desc[abs_start + 1..abs_start + 1 + end];
            if !type_candidate.is_empty()
                && (type_candidate.chars().next().unwrap().is_uppercase()
                    || JSON_SCHEMA_TYPES.contains(&type_candidate))
            {
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

/// 将文档注释表格中的字段信息转换为 `FieldSpec`。
///
/// 对表格中的类型名进行简化映射，将 Rust 基本类型名转换为 JSON Schema 类型名
/// （如 `String` → `string`、`i32` → `integer`），其余类型统一为 `object`。
///
/// # Arguments
///
/// * `field` - 表格行解析得到的字段信息。
///
/// # Returns
///
/// 转换后的 `FieldSpec`，不包含子字段。
fn table_field_to_spec(field: &crate::parser::FieldInfo) -> FieldSpec {
    let type_name = match field.type_name.as_str() {
        "string" | "str" | "String" => "string".to_string(),
        "integer" | "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64" => {
            "integer".to_string()
        }
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

/// 去除描述文本末尾的句号（支持中英文句号）。
///
/// # Arguments
///
/// * `desc` - 原始描述文本。
///
/// # Returns
///
/// 去除末尾句号后的描述文本。
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

/// 将 AST 解析后的 `ResolvedField` 转换为 `FieldSpec`。
///
/// 根据字段的类型名称判断其 JSON Schema 类型：
/// - 容器类型（`Vec<T>` 等）映射为 `array`，`format` 记录元素类型。
/// - Rust 基本类型映射为对应的 JSON Schema 类型。
/// - 其他类型映射为 `object`，并递归展开子字段。
///
/// # Arguments
///
/// * `resolved` - AST 解析后的已解析字段。
///
/// # Returns
///
/// 转换后的 `FieldSpec`，包含递归展开的子字段。
fn resolved_field_to_spec(resolved: &ResolvedField) -> FieldSpec {
    let sub_fields: Vec<FieldSpec> = resolved
        .sub_fields
        .iter()
        .map(resolved_field_to_spec)
        .collect();

    let (type_name, format) = if is_container_type(&resolved.type_name) {
        if let Some(elem_type) = extract_container_element(&resolved.type_name) {
            if is_primitive_type(&elem_type) {
                ("array".to_string(), None)
            } else {
                // 容器元素为非基本类型，在 format 中记录元素类型名
                ("array".to_string(), Some(elem_type))
            }
        } else {
            ("array".to_string(), None)
        }
    } else if is_primitive_type(&resolved.type_name) {
        (map_primitive_to_json_type(&resolved.type_name), None)
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
        items: None,
    }
}

/// 将 Rust 基本类型名映射为 JSON Schema 类型名。
///
/// # Arguments
///
/// * `rust_type` - Rust 类型名称字符串。
///
/// # Returns
///
/// 对应的 JSON Schema 类型名（`integer`、`number`、`boolean`、`string`、`null`、`object`）。
fn map_primitive_to_json_type(rust_type: &str) -> String {
    match rust_type {
        "i8" | "i16" | "i32" | "i64" | "i128" | "isize" | "u8" | "u16" | "u32" | "u64" | "u128"
        | "usize" => "integer".to_string(),

        "f32" | "f64" => "number".to_string(),

        "bool" => "boolean".to_string(),

        "char" | "String" | "str" => "string".to_string(),

        "()" => "null".to_string(),

        _ => "object".to_string(),
    }
}
