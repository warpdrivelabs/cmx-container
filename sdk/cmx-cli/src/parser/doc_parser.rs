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
}

/// 字段信息
#[derive(Debug, Clone)]
pub struct FieldInfo {
    /// 字段名
    pub name: String,
    /// 字段类型
    pub type_name: String,
    /// 是否必填
    pub required: Option<bool>,
    /// 字段说明
    pub description: String,
}

/// 示例信息
#[derive(Debug, Clone)]
pub struct ExampleInfo {
    /// 输入
    pub input: String,
    /// 输出
    pub output: String,
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

    // 解析各节
    let sections = parse_sections(&full_doc);

    // 提取简短描述和详细描述
    if let Some(main) = sections.get("") {
        let lines: Vec<&str> = main.lines().collect();
        if !lines.is_empty() {
            result.summary = lines[0].to_string();
        }
        if lines.len() > 1 {
            result.description = Some(lines[1..].join("\n").trim().to_string());
            if result.description.as_ref().is_none_or(|s| s.is_empty()) {
                result.description = None;
            }
        }
    }

    // 解析输入节
    if let Some(input) = sections.get("输入") {
        result.input_fields = parse_table_or_list(input);
    }
    // 兼容旧的 "# 输入处理" 格式
    if let Some(input) = sections.get("输入处理") {
        result.input_fields = parse_table_or_list(input);
    }

    // 解析输出节
    if let Some(output) = sections.get("输出") {
        result.output_fields = parse_table_or_list(output);
    }

    // 解析编码节
    if let Some(encoding) = sections.get("编码") {
        for line in encoding.lines() {
            let line = line.trim();
            if line.starts_with("- 输入编码:") || line.starts_with("* 输入编码:") {
                result.input_encoding = extract_code_value(line);
            } else if line.starts_with("- 输出编码:") || line.starts_with("* 输出编码:") {
                result.output_encoding = extract_code_value(line);
            }
        }
    }

    // 解析示例节
    if let Some(example) = sections.get("示例") {
        result.examples = parse_examples(example);
    }

    // 解析错误节
    if let Some(errors) = sections.get("错误") {
        result.errors = parse_list(errors);
    }

    // 解析注意事项节
    if let Some(notes) = sections.get("注意") {
        result.notes = parse_list(notes);
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

            // 解析新节标题
            let title = trimmed.trim_start_matches('#').trim().to_string();
            current_section = title;
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
                required,
                description,
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
                    required: None,
                    description,
                });
            }
        }
    }

    fields
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
        let doc = "简短描述\n\n详细描述\n\n# 输入\n\n| 字段 | 类型 |\n|------|------|\n| name | string |";
        let sections = parse_sections(doc);
        assert!(sections.contains_key(""));
        assert!(sections.contains_key("输入"));
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
