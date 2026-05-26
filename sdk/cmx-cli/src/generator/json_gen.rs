//! JSON 文档生成器
//!
//! 将解析后的函数信息组装成 JSON 文档。

use anyhow::Result;
use chrono::Utc;

use crate::models::{
    Example, FieldSpec, FunctionDoc, InputSpec, OutputSpec, PluginDocument, PluginInfo,
    SourceLocation,
};
use crate::parser::{ParsedDoc, ParsedFunction};

/// 扫描结果
#[derive(Debug, Clone)]
pub struct ScanResult {
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
}

/// 生成插件文档
pub fn generate_document(result: &ScanResult, pretty: bool) -> Result<String> {
    let functions: Vec<FunctionDoc> = result
        .functions
        .iter()
        .map(|(func, doc)| build_function_doc(func, doc, &result.file_path))
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

/// 构建函数文档
fn build_function_doc(func: &ParsedFunction, doc: &ParsedDoc, file_path: &str) -> FunctionDoc {
    // 构建输入字段
    let input_fields: Vec<FieldSpec> = doc
        .input_fields
        .iter()
        .map(convert_field_info_to_spec)
        .collect();

    // 构建输出字段
    let output_fields: Vec<FieldSpec> = doc
        .output_fields
        .iter()
        .map(convert_field_info_to_spec)
        .collect();

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

/// 将 FieldInfo 递归转换为 FieldSpec
fn convert_field_info_to_spec(f: &crate::parser::FieldInfo) -> FieldSpec {
    FieldSpec {
        name: f.name.clone(),
        type_name: f.type_name.clone(),
        format: f.format.clone(),
        required: f.required,
        description: trim_description(&f.description),
        sub_fields: f.sub_fields.iter().map(convert_field_info_to_spec).collect(),
        items: f.items.as_ref().map(|i| Box::new(convert_field_info_to_spec(i))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_document() {
        let result = ScanResult {
            plugin_name: "test-plugin".to_string(),
            plugin_version: "0.1.0".to_string(),
            plugin_description: Some("Test plugin".to_string()),
            functions: vec![],
            file_path: "src/lib.rs".to_string(),
        };

        let json = generate_document(&result, true).unwrap();
        assert!(json.contains("test-plugin"));
        assert!(json.contains("0.1.0"));
    }
}
