//! 文档注释解析器
//!
//! 解析 `///` 文档注释，提取函数描述、输入输出、示例等信息。

use anyhow::Result;
use pulldown_cmark::{Event, Parser, Tag, TagEnd};

/// 解析后的文档注释
#[derive(Debug, Clone, Default)]
pub struct ParsedDoc {
    /// 简短描述
    pub summary: String,
    /// 详细描述
    pub description: Option<String>,
    /// 输入字段
    pub input_fields: Vec<FieldInfo>,
    /// 输出字段
    pub output_fields: Vec<FieldInfo>,
    /// 输入编码
    pub input_encoding: Option<String>,
    /// 输出编码
    pub output_encoding: Option<String>,
    /// 示例
    pub examples: Vec<ExampleInfo>,
    /// 错误说明
    pub errors: Vec<String>,
    /// 注意事项
    pub notes: Vec<String>,
    /// Panic 场景说明
    pub panics: Vec<String>,
    /// Safety 说明
    pub safety: Option<String>,
}

/// 字段信息
#[derive(Debug, Clone)]
pub struct FieldInfo {
    /// 字段名
    pub name: String,
    /// 字段类型
    pub type_name: String,
    /// 格式
    pub format: Option<String>,
    /// 是否必填
    pub required: Option<bool>,
    /// 字段说明
    pub description: String,
    /// 子属性（用于 object 类型）
    pub sub_fields: Vec<FieldInfo>,
    /// 数组元素类型（用于 array 类型）
    pub items: Option<Box<FieldInfo>>,
}

/// 示例信息
#[derive(Debug, Clone)]
pub struct ExampleInfo {
    /// 输入
    pub input: String,
    /// 输出
    pub output: String,
}

/// 标准化章节名称
fn normalize_section_name(name: &str) -> String {
    match name.trim().to_lowercase().as_str() {
        // Arguments 变体
        "arguments" | "argument" | "参数" | "输入" | "输入处理" => "Arguments".to_string(),

        // Returns 变体
        "returns" | "return" | "返回值" | "输出" => "Returns".to_string(),

        // Errors 变体
        "errors" | "error" | "错误" | "错误说明" => "Errors".to_string(),

        // Panics
        "panics" | "panic" | "panic 场景" => "Panics".to_string(),

        // Safety
        "safety" => "Safety".to_string(),

        // Examples
        "examples" | "example" | "示例" => "Examples".to_string(),

        // Notes
        "notes" | "note" | "注意" | "注意事项" => "Notes".to_string(),

        // 编码
        "编码" | "encoding" => "Encoding".to_string(),

        // 其他保持原样
        other => other.to_string(),
    }
}

/// 解析文档注释
pub fn parse_doc_comments(doc_comments: &[String]) -> Result<ParsedDoc> {
    let mut result = ParsedDoc::default();

    // 合并所有注释行
    let full_doc = doc_comments
        .iter()
        .map(|s| s.trim_start())
        .collect::<Vec<_>>()
        .join("\n");

    // 解析各节（使用标准化名称）
    let sections = parse_sections(&full_doc);

    // 提取简短描述和详细描述
    if let Some(main) = sections.get("") {
        let lines: Vec<&str> = main.lines().collect();
        if !lines.is_empty() {
            // 摘要取第一行，移除末尾句号
            result.summary = lines[0].trim_end_matches('.').trim_end_matches('。').to_string();
        }
        if lines.len() > 1 {
            let desc_lines: Vec<&str> = lines[1..]
                .iter()
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect();
            if !desc_lines.is_empty() {
                result.description = Some(desc_lines.join("\n"));
            }
        }
    }

    // 解析输入节 (Arguments / 输入)
    if let Some(input) = sections.get("Arguments") {
        result.input_fields = parse_nested_fields(input);
    } else if let Some(input) = sections.get("输入") {
        result.input_fields = parse_nested_fields(input);
    } else if let Some(input) = sections.get("输入处理") {
        result.input_fields = parse_nested_fields(input);
    }

    // 解析输出节 (Returns / 输出)
    if let Some(output) = sections.get("Returns") {
        result.output_fields = parse_nested_fields(output);
    } else if let Some(output) = sections.get("输出") {
        result.output_fields = parse_nested_fields(output);
    }

    // 解析编码节
    if let Some(encoding) = sections.get("Encoding") {
        for line in encoding.lines() {
            let line = line.trim();
            if line.starts_with("- 输入编码:") || line.starts_with("* 输入编码:") {
                result.input_encoding = extract_code_value(line);
            } else if line.starts_with("- 输出编码:") || line.starts_with("* 输出编码:") {
                result.output_encoding = extract_code_value(line);
            }
        }
    }

    // 解析示例节 (Examples / 示例)
    if let Some(example) = sections.get("Examples") {
        result.examples = parse_examples(example);
    } else if let Some(example) = sections.get("示例") {
        result.examples = parse_examples(example);
    }

    // 解析错误节 (Errors / 错误)
    if let Some(errors) = sections.get("Errors") {
        result.errors = parse_list(errors);
    } else if let Some(errors) = sections.get("错误") {
        result.errors = parse_list(errors);
    }

    // 解析注意事项节 (Notes / 注意)
    if let Some(notes) = sections.get("Notes") {
        result.notes = parse_list(notes);
    } else if let Some(notes) = sections.get("注意") {
        result.notes = parse_list(notes);
    }

    // 解析 Panic 场景节
    if let Some(panics) = sections.get("Panics") {
        result.panics = parse_list(panics);
    }

    // 解析 Safety 节
    if let Some(safety) = sections.get("Safety") {
        let safety_content = safety.trim().to_string();
        if !safety_content.is_empty() {
            result.safety = Some(safety_content);
        }
    }

    Ok(result)
}

/// 解析各节内容
fn parse_sections(doc: &str) -> std::collections::HashMap<String, String> {
    let mut sections = std::collections::HashMap::new();
    let mut current_section = String::new();
    let mut current_content = String::new();

    for line in doc.lines() {
        let trimmed = line.trim();

        // 检查是否是节标题 (以 # 开头)
        if trimmed.starts_with('#') {
            // 保存前一节
            if !current_content.is_empty() {
                sections.insert(current_section.clone(), current_content.trim().to_string());
            }

            // 解析新节标题，并标准化名称
            let title = trimmed.trim_start_matches('#').trim().to_string();
            current_section = normalize_section_name(&title).to_string();
            current_content = String::new();
        } else {
            if !current_content.is_empty() || !trimmed.is_empty() {
                current_content.push_str(line);
                current_content.push('\n');
            }
        }
    }

    // 保存最后一节
    if !current_content.is_empty() {
        sections.insert(current_section, current_content.trim().to_string());
    }

    sections
}

/// 解析表格或列表
fn parse_table_or_list(content: &str) -> Vec<FieldInfo> {
    // 尝试解析 Markdown 表格
    if content.contains('|') && content.contains("---") {
        return parse_table(content);
    }

    // 否则解析列表格式
    parse_field_list(content)
}

/// 解析 Markdown 表格
fn parse_table(content: &str) -> Vec<FieldInfo> {
    let mut fields = Vec::new();
    let mut in_header = true;

    for line in content.lines() {
        let line = line.trim();

        // 跳过空行
        if line.is_empty() {
            continue;
        }

        // 解析表头分隔符
        if in_header && line.contains("---") {
            in_header = false;
            continue;
        }

        let cells: Vec<String> = line
            .split('|')
            .filter(|s| !s.is_empty())
            .map(|s| s.trim().to_string())
            .collect();

        if in_header {
            continue;
        }

        // 解析数据行
        if cells.len() >= 2 {
            let name = clean_field_name(&cells[0]);
            let type_name = if cells.len() > 1 { cells[1].clone() } else { "unknown".to_string() };
            let required = if cells.len() > 2 {
                Some(cells[2].contains("是") || cells[2].to_lowercase().contains("true"))
            } else {
                None
            };
            let description = if cells.len() > 3 {
                cells[3].clone()
            } else if cells.len() > 2 {
                cells[2].clone()
            } else {
                String::new()
            };

            fields.push(FieldInfo {
                name,
                type_name,
                format: None,
                required,
                description,
                sub_fields: Vec::new(),
                items: None,
            });
        }
    }

    fields
}

/// 解析字段列表格式
fn parse_field_list(content: &str) -> Vec<FieldInfo> {
    let mut fields = Vec::new();

    for line in content.lines() {
        let line = line.trim();

        // 解析 "- `field`: description" 格式
        if line.starts_with("- ") || line.starts_with("* ") {
            let rest = line[2..].trim();
            if let Some(colon_pos) = rest.find(':') {
                let name = clean_field_name(&rest[..colon_pos]);
                let description = rest[colon_pos + 1..].trim().to_string();

                fields.push(FieldInfo {
                    name,
                    type_name: "unknown".to_string(),
                    format: None,
                    required: None,
                    description,
                    sub_fields: Vec::new(),
                    items: None,
                });
            }
        }
    }

    fields
}

/// 解析嵌套结构字段（支持通过缩进表示层级）
fn parse_nested_fields(content: &str) -> Vec<FieldInfo> {
    // 首先尝试解析 Markdown 表格
    if content.contains('|') && content.contains("---") {
        return parse_table(content);
    }

    let mut root_fields = Vec::new();
    let mut stack: Vec<(usize, usize)> = Vec::new(); // (indent_level, field_index_in_parent)

    let lines: Vec<&str> = content.lines().collect();

    for line in &lines {
        let trimmed_line = line.trim();

        // 跳过空行
        if trimmed_line.is_empty() {
            continue;
        }

        // 计算缩进级别（每2空格为一级）
        let indent = (line.len() - line.trim_start().len()) / 2;

        // 解析字段行
        if let Some(field) = parse_field_line(trimmed_line) {
            // 弹出比当前缩进更深的所有字段
            while let Some((prev_indent, _)) = stack.last() {
                if *prev_indent >= indent {
                    stack.pop();
                } else {
                    break;
                }
            }

            // 将字段添加到正确的父级
            if stack.is_empty() {
                root_fields.push(field);
            } else {
                let (_, parent_idx) = stack.last().unwrap();
                root_fields[*parent_idx].sub_fields.push(field);
            }

            // 将当前字段压入栈（用于接收后续的子字段）
            let field_idx = if stack.is_empty() {
                root_fields.len() - 1
            } else {
                root_fields[stack.last().unwrap().1].sub_fields.len() - 1
            };
            stack.push((indent, field_idx));
        }
    }

    root_fields
}

/// 解析单行字段
fn parse_field_line(line: &str) -> Option<FieldInfo> {
    let trimmed = line.trim();

    // 跳过非列表行
    if !trimmed.starts_with("- ") && !trimmed.starts_with("* ") {
        return None;
    }

    let rest = trimmed[2..].trim();

    // 解析 "* `name` - description" 或 "* `name`: description" 格式
    if let Some(name_end) = rest.find(" - ") {
        let name = clean_field_name(&rest[..name_end]);
        let description = rest[name_end + 3..].trim().to_string();
        let (type_name, format) = extract_type_from_description(&description);

        return Some(FieldInfo {
            name,
            type_name,
            format,
            required: None,
            description,
            sub_fields: Vec::new(),
            items: None,
        });
    }

    if let Some(colon_pos) = rest.find(':') {
        let name = clean_field_name(&rest[..colon_pos]);
        let description = rest[colon_pos + 1..].trim().to_string();
        let (type_name, format) = extract_type_from_description(&description);

        return Some(FieldInfo {
            name,
            type_name,
            format,
            required: None,
            description,
            sub_fields: Vec::new(),
            items: None,
        });
    }

    // 如果没有分隔符，整个内容作为描述
    if !rest.is_empty() {
        let (type_name, format) = extract_type_from_description(rest);
        return Some(FieldInfo {
            name: String::new(),
            type_name,
            format,
            required: None,
            description: rest.to_string(),
            sub_fields: Vec::new(),
            items: None,
        });
    }

    None
}

/// 从描述中提取类型信息
fn extract_type_from_description(desc: &str) -> (String, Option<String>) {
    // 尝试匹配 "TypeName description" 或 "TypeName - description"
    // 类型名通常以大写字母开头，包含字母、数字、下划线

    let desc = desc.trim();

    // 匹配类型名前的空白或短横线
    if let Some(dash_pos) = desc.find(" - ") {
        let type_part = desc[..dash_pos].trim();
        if is_type_name(type_part) {
            return (type_part.to_string(), None);
        }
    }

    // 尝试匹配开头的类型名（以大写字母开头）
    let mut type_end = 0;
    for (i, c) in desc.char_indices() {
        if i == 0 && !c.is_ascii_uppercase() {
            break;
        }
        if !c.is_alphanumeric() && c != '_' && c != '<' && c != '>' && c != '&' && c != '(' && c != ')' {
            break;
        }
        type_end = i + c.len_utf8();
    }

    if type_end > 0 && type_end < desc.len() {
        let type_name = desc[..type_end].trim().to_string();
        if !type_name.is_empty() && (is_type_name(&type_name) || type_name.contains('<')) {
            return (type_name, None);
        }
    }

    ("unknown".to_string(), None)
}

/// 判断是否是类型名
fn is_type_name(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let mut chars = s.chars();
    // 必须以大写字母开头
    if !chars.next().unwrap().is_ascii_uppercase() {
        return false;
    }
    // 其余字符必须是字母、数字或下划线
    chars.all(|c| c.is_alphanumeric() || c == '_')
}

/// 清理字段名（移除反引号）
fn clean_field_name(name: &str) -> String {
    name.trim()
        .trim_start_matches('`')
        .trim_end_matches('`')
        .to_string()
}

/// 解析列表
fn parse_list(content: &str) -> Vec<String> {
    let mut items = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        if line.starts_with("- ") || line.starts_with("* ") {
            items.push(line[2..].trim().to_string());
        }
    }

    items
}

/// 提取代码值
fn extract_code_value(line: &str) -> Option<String> {
    // 查找反引号中的内容
    if let Some(start) = line.find('`')
        && let Some(end) = line[start + 1..].find('`') {
            return Some(line[start + 1..start + 1 + end].to_string());
        }
    None
}

/// 解析示例
fn parse_examples(content: &str) -> Vec<ExampleInfo> {
    let mut examples = Vec::new();
    let mut current_input = String::new();
    let mut current_output = String::new();
    let mut in_input = false;
    let mut in_output = false;
    let mut in_code_block = false;
    let mut code_content = String::new();

    let parser = Parser::new(content);

    for event in parser {
        match event {
            Event::Start(Tag::CodeBlock(_)) => {
                in_code_block = true;
                code_content = String::new();
            }
            Event::End(TagEnd::CodeBlock) => {
                in_code_block = false;
                if in_input {
                    current_input = code_content.trim().to_string();
                } else if in_output {
                    current_output = code_content.trim().to_string();
                }
            }
            Event::Text(text) => {
                if in_code_block {
                    code_content.push_str(&text);
                } else {
                    let text = text.to_string();
                    if text.contains("输入") {
                        in_input = true;
                        in_output = false;
                    } else if text.contains("输出") {
                        in_input = false;
                        in_output = true;
                    }
                }
            }
            _ => {}
        }
    }

    // 如果有输入和输出，添加示例
    if !current_input.is_empty() || !current_output.is_empty() {
        examples.push(ExampleInfo {
            input: current_input,
            output: current_output,
        });
    }

    // 如果没有解析到结构化示例，尝试解析简单的输入输出格式
    if examples.is_empty()
        && let Some(example) = parse_simple_example(content) {
            examples.push(example);
        }

    examples
}

/// 解析简单格式的示例
fn parse_simple_example(content: &str) -> Option<ExampleInfo> {
    let mut input = String::new();
    let mut output = String::new();

    for line in content.lines() {
        let line = line.trim();
        if line.starts_with("输入:") || line.starts_with("输入：") {
            input = line.split(':').nth(1).unwrap_or("").trim().to_string();
        } else if line.starts_with("输出:") || line.starts_with("输出：") {
            output = line.split(':').nth(1).unwrap_or("").trim().to_string();
        }
    }

    if !input.is_empty() || !output.is_empty() {
        Some(ExampleInfo { input, output })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_sections() {
        let doc = "简短描述\n\n详细描述\n\n# Arguments\n\n| 字段 | 类型 |\n|------|------|\n| name | string |";
        let sections = parse_sections(doc);
        assert!(sections.contains_key(""));
        assert!(sections.contains_key("Arguments"));
    }

    #[test]
    fn test_parse_table() {
        let content = "| 字段 | 类型 | 必填 | 说明 |\n|------|------|------|------|\n| `input.input` | string | 是 | 输入数据 |";
        let fields = parse_table(content);
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].name, "input.input");
        assert_eq!(fields[0].type_name, "string");
        assert_eq!(fields[0].required, Some(true));
    }
}
