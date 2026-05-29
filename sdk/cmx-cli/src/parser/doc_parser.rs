//! 文档注释解析器。
//!
//! 解析 `///` 文档注释，提取函数描述、输入输出字段、示例、错误说明等信息。
//! 支持标准 rustdoc 章节（Arguments、Returns、Examples 等）以及中文变体，
//! 支持表格和列表两种字段描述格式，并支持嵌套结构（列表+表格混合模式）。

use anyhow::Result;
use pulldown_cmark::{Event, Parser, Tag, TagEnd};

/// 解析后的文档注释。
///
/// 从 `#[plugin_fn]` 函数的文档注释中提取的结构化信息，
/// 包含函数摘要、输入输出字段定义、示例、错误说明等完整文档内容。
#[derive(Debug, Clone, Default)]
pub struct ParsedDoc {
    /// 函数的简短描述（文档注释的第一行）。
    pub summary: String,
    /// 函数的详细描述（摘要之后、第一个章节之前的段落）。
    pub description: Option<String>,
    /// 输入字段列表，从 `# Arguments` 章节解析。
    pub input_fields: Vec<FieldInfo>,
    /// 输出字段列表，从 `# Returns` 章节解析。
    pub output_fields: Vec<FieldInfo>,
    /// 输入数据的编码格式（如 `json`、`msgpack`）。
    pub input_encoding: Option<String>,
    /// 输出数据的编码格式。
    pub output_encoding: Option<String>,
    /// 函数使用示例列表。
    pub examples: Vec<ExampleInfo>,
    /// 函数可能返回的错误说明列表。
    pub errors: Vec<String>,
    /// 使用注意事项列表。
    pub notes: Vec<String>,
    /// 可能触发 panic 的场景说明列表。
    pub panics: Vec<String>,
    /// unsafe 函数的安全保证说明。
    pub safety: Option<String>,
}

/// 字段信息。
///
/// 描述函数输入/输出中的一个字段，支持嵌套结构和数组元素类型。
/// 可从 Markdown 表格行或列表项中解析得到。
#[derive(Debug, Clone)]
pub struct FieldInfo {
    /// 字段名称。
    pub name: String,
    /// 字段的 Rust 类型名称（如 `String`、`i32`、`Vec<T>`）。
    pub type_name: String,
    /// 字段的格式限定（如 `date-time`、`uri`）。
    pub format: Option<String>,
    /// 字段是否为必填项。
    pub required: Option<bool>,
    /// 字段的文字说明。
    pub description: String,
    /// 子属性列表，用于 `object` 类型字段的嵌套展开。
    pub sub_fields: Vec<FieldInfo>,
    /// 数组元素类型信息，用于 `array` 类型字段。
    pub items: Option<Box<FieldInfo>>,
}

/// 示例信息。
///
/// 包含一组输入/输出对，用于展示函数的典型用法。
#[derive(Debug, Clone)]
pub struct ExampleInfo {
    /// 示例的输入数据。
    pub input: String,
    /// 示例的输出数据。
    pub output: String,
}

/// 将章节名称标准化为统一的英文标识。
///
/// 支持英文和中文的章节名称变体，统一映射为标准 rustdoc 章节名。
/// 例如 `"参数"` 映射为 `"Arguments"`，`"返回值"` 映射为 `"Returns"`。
///
/// # Arguments
///
/// * `name` - 原始章节名称文本。
///
/// # Returns
///
/// 标准化后的章节名称字符串。
fn normalize_section_name(name: &str) -> String {
    match name.trim().to_lowercase().as_str() {
        "arguments" | "argument" | "参数" | "输入" | "输入处理" => "Arguments".to_string(),
        "returns" | "return" | "返回值" | "输出" => "Returns".to_string(),
        "errors" | "error" | "错误" | "错误说明" => "Errors".to_string(),
        "panics" | "panic" | "panic 场景" => "Panics".to_string(),
        "safety" => "Safety".to_string(),
        "examples" | "example" | "示例" => "Examples".to_string(),
        "notes" | "note" | "注意" | "注意事项" => "Notes".to_string(),
        "编码" | "encoding" => "Encoding".to_string(),
        other => other.to_string(),
    }
}

/// 解析文档注释，提取结构化文档信息。
///
/// 接收 `///` 文档注释的原始文本行，按章节拆分后分别解析摘要、输入输出字段、
/// 示例、错误说明等内容。章节名称支持中英文变体，会自动标准化处理。
///
/// # Arguments
///
/// * `doc_comments` - 文档注释行列表（每行已去除 `/// ` 前缀）。
///
/// # Returns
///
/// 解析后的 `ParsedDoc` 结构体，包含完整的文档信息。
///
/// # Errors
///
/// 当文档格式严重异常时返回解析错误。
pub fn parse_doc_comments(doc_comments: &[String]) -> Result<ParsedDoc> {
    let mut result = ParsedDoc::default();

    let full_doc = doc_comments
        .iter()
        .map(|s| s.trim_start())
        .collect::<Vec<_>>()
        .join("\n");

    let sections = parse_sections(&full_doc);

    if let Some(main) = sections.get("") {
        let lines: Vec<&str> = main.lines().collect();
        if !lines.is_empty() {
            // 摘要取第一行，移除末尾的中文或英文句号
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

    // 依次尝试英文和中文的章节名称变体
    if let Some(input) = sections.get("Arguments") {
        result.input_fields = parse_nested_fields(input);
    } else if let Some(input) = sections.get("输入") {
        result.input_fields = parse_nested_fields(input);
    } else if let Some(input) = sections.get("输入处理") {
        result.input_fields = parse_nested_fields(input);
    }

    if let Some(output) = sections.get("Returns") {
        result.output_fields = parse_nested_fields(output);
    } else if let Some(output) = sections.get("输出") {
        result.output_fields = parse_nested_fields(output);
    }

    // 从 Encoding 章节提取输入/输出编码格式
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

    if let Some(example) = sections.get("Examples") {
        result.examples = parse_examples(example);
    } else if let Some(example) = sections.get("示例") {
        result.examples = parse_examples(example);
    }

    if let Some(errors) = sections.get("Errors") {
        result.errors = parse_list(errors);
    } else if let Some(errors) = sections.get("错误") {
        result.errors = parse_list(errors);
    }

    if let Some(notes) = sections.get("Notes") {
        result.notes = parse_list(notes);
    } else if let Some(notes) = sections.get("注意") {
        result.notes = parse_list(notes);
    }

    if let Some(panics) = sections.get("Panics") {
        result.panics = parse_list(panics);
    }

    if let Some(safety) = sections.get("Safety") {
        let safety_content = safety.trim().to_string();
        if !safety_content.is_empty() {
            result.safety = Some(safety_content);
        }
    }

    Ok(result)
}

/// 按二级标题将文档拆分为多个章节。
///
/// 以 `#` 开头的行视为章节标题，标题到下一个标题之间的内容作为该章节的正文。
/// 无标题的前导内容归入空字符串键（`""`）对应的章节。
///
/// # Arguments
///
/// * `doc` - 合并后的完整文档注释文本。
///
/// # Returns
///
/// 以标准化章节名称为键、章节正文为值的哈希表。
fn parse_sections(doc: &str) -> std::collections::HashMap<String, String> {
    let mut sections = std::collections::HashMap::new();
    let mut current_section = String::new();
    let mut current_content = String::new();

    for line in doc.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with('#') {
            // 保存前一节的内容
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
#[allow(dead_code)]
/// 解析表格或列表格式的字段定义。
///
/// 自动检测内容格式：包含 `|` 和 `---` 时按 Markdown 表格解析，否则按列表格式解析。
///
/// # Arguments
///
/// * `content` - 章节的原始文本内容。
///
/// # Returns
///
/// 解析得到的字段信息列表。
fn parse_table_or_list(content: &str) -> Vec<FieldInfo> {
    if content.contains('|') && content.contains("---") {
        return parse_table(content);
    }

    parse_field_list(content)
}

/// 解析 Markdown 表格格式的字段定义。
///
/// 表格应至少包含字段名和类型两列，可选包含必填和说明列。
/// 表头分隔行（`---`）之前的内容视为表头并跳过。
///
/// # Arguments
///
/// * `content` - Markdown 表格的原始文本。
///
/// # Returns
///
/// 从表格行解析得到的字段信息列表。
fn parse_table(content: &str) -> Vec<FieldInfo> {
    let mut fields = Vec::new();
    let mut in_header = true;

    for line in content.lines() {
        let line = line.trim();

        if line.is_empty() {
            continue;
        }

        // 跳过表头分隔行（如 |------|------|）
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

        // 按列顺序解析：字段名、类型、必填（可选）、说明（可选）
        if cells.len() >= 2 {
            let name = clean_field_name(&cells[0]);
            let raw_type = if cells.len() > 1 { cells[1].trim().to_string() } else { "unknown".to_string() };
            let type_name = clean_field_name(&raw_type);
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

#[allow(dead_code)]
/// 解析列表格式的字段定义。
///
/// 解析 ``- `field`: description`` 或 ``* `field`: description`` 格式的列表项。
/// 列表格式无法提取类型信息，字段类型默认为 `"unknown"`。
///
/// # Arguments
///
/// * `content` - 列表格式的原始文本。
///
/// # Returns
///
/// 解析得到的字段信息列表。
fn parse_field_list(content: &str) -> Vec<FieldInfo> {
    let mut fields = Vec::new();

    for line in content.lines() {
        let line = line.trim();

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

/// 解析嵌套结构字段，支持列表+表格混合模式。
///
/// 先按列表行解析顶层字段，遇到紧跟的 Markdown 表格时将其作为该字段的子属性展开。
/// 如果内容以表格开头（无列表前缀），则整个内容按纯表格解析。
///
/// # Arguments
///
/// * `content` - 章节的原始文本内容。
///
/// # Returns
///
/// 解析得到的顶层字段列表，嵌套字段的 `sub_fields` 中包含子属性。
fn parse_nested_fields(content: &str) -> Vec<FieldInfo> {
    let mut root_fields = Vec::new();
    let lines: Vec<&str> = content.lines().collect();

    let mut idx = 0;
    while idx < lines.len() {
        let line = lines[idx];
        let trimmed = line.trim();

        if trimmed.is_empty() {
            idx += 1;
            continue;
        }

        if let Some(field) = parse_field_line(trimmed) {
            root_fields.push(field);
            idx += 1;

            idx = skip_empty_lines(&lines, idx);

            // 检查字段后面是否紧跟表格，若是则作为子属性展开
            if idx < lines.len() {
                let check_line = lines[idx].trim();
                if is_table_start(check_line) {
                    let table_lines = collect_table_lines(&lines[idx..]);
                    let table_fields = parse_table(&table_lines);
                    if let Some(last) = root_fields.last_mut() {
                        last.type_name = "object".to_string();
                        last.sub_fields = table_fields;
                    }
                    idx += table_lines.lines().count();
                    continue;
                }
            }
            continue;
        }

        // 顶层直接以表格开头的情况
        if is_table_start(trimmed) || trimmed.starts_with('|') {
            if root_fields.is_empty() {
                let table_content: String = lines[idx..].iter().cloned().collect::<Vec<_>>().join("\n");
                return parse_table(&table_content);
            }
            let search_idx = skip_empty_lines(&lines, idx + 1);
            if search_idx < lines.len() && is_table_start(lines[search_idx].trim()) {
                let table_lines = collect_table_lines(&lines[search_idx..]);
                let table_fields = parse_table(&table_lines);
                if let Some(last) = root_fields.last_mut() {
                    last.type_name = "object".to_string();
                    last.sub_fields = table_fields;
                }
                idx = search_idx + table_lines.lines().count();
                continue;
            }
            break;
        }

        idx += 1;
    }

    root_fields
}

/// 判断一行是否为 Markdown 表格的表头分隔行。
///
/// 表头分隔行以 `|` 开头且包含 `---` 模式。
fn is_table_start(line: &str) -> bool {
    line.starts_with('|') && line.contains("---")
}

/// 跳过连续的空行，返回第一个非空行的索引。
///
/// # Arguments
///
/// * `lines` - 文本行切片。
/// * `idx` - 开始跳过的起始索引。
///
/// # Returns
///
/// 第一个非空行的索引，若所有行均为空则返回 `lines.len()`。
fn skip_empty_lines(lines: &[&str], mut idx: usize) -> usize {
    while idx < lines.len() && lines[idx].trim().is_empty() {
        idx += 1;
    }
    idx
}

/// 从行切片中收集连续的表格行。
///
/// 从起始位置依次收集以 `|` 开头的行，遇到空行或非表格行时停止。
///
/// # Arguments
///
/// * `lines` - 文本行切片。
///
/// # Returns
///
/// 拼接后的表格文本。
fn collect_table_lines(lines: &[&str]) -> String {
    let mut result = String::new();
    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() || !trimmed.starts_with('|') {
            break;
        }
        result.push_str(trimmed);
        result.push('\n');
    }
    result
}

/// 解析单行列表格式的字段定义。
///
/// 支持以下格式：
/// - `- \`name\` - description`
/// - `- \`name\`: description`
/// - `- TypeName description`（从描述中提取类型名）
///
/// # Arguments
///
/// * `line` - 待解析的列表行文本。
///
/// # Returns
///
/// 解析成功时返回 `Some(FieldInfo)`，非列表行或格式不匹配时返回 `None`。
fn parse_field_line(line: &str) -> Option<FieldInfo> {
    let trimmed = line.trim();

    // 跳过非列表行
    if !trimmed.starts_with("- ") && !trimmed.starts_with("* ") {
        return None;
    }

    let rest = trimmed[2..].trim();

    // 解析 "* `name` - description" 格式
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

    // 解析 "* `name`: description" 格式
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

/// 从字段描述文本中提取类型名称。
///
/// 尝试以下策略提取类型信息：
/// 1. 匹配 `TypeName - description` 格式中的类型名
/// 2. 匹配以大写字母开头的连续标识符作为类型名
///
/// 类型名须以大写字母开头，仅包含字母、数字、下划线或泛型符号。
///
/// # Arguments
///
/// * `desc` - 字段描述文本。
///
/// # Returns
///
/// 元组 `(type_name, format)`，无法识别类型时 `type_name` 为 `"unknown"`。
fn extract_type_from_description(desc: &str) -> (String, Option<String>) {
    let desc = desc.trim();

    // 匹配 "TypeName - description" 格式
    if let Some(dash_pos) = desc.find(" - ") {
        let type_part = desc[..dash_pos].trim();
        if is_type_name(type_part) {
            return (type_part.to_string(), None);
        }
    }

    // 匹配以大写字母开头的连续标识符
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

/// 判断字符串是否符合 Rust 类型名的命名规则。
///
/// 类型名必须以大写字母开头，其余字符为字母、数字或下划线（即 PascalCase 标识符）。
///
/// # Arguments
///
/// * `s` - 待判断的字符串。
///
/// # Returns
///
/// 符合类型名规则时返回 `true`。
fn is_type_name(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let mut chars = s.chars();
    if !chars.next().unwrap().is_ascii_uppercase() {
        return false;
    }
    chars.all(|c| c.is_alphanumeric() || c == '_')
}

/// 清理字段名，移除包裹的反引号。
///
/// Markdown 文档中字段名通常以反引号包裹（如 `` `field_name` ``），
/// 此函数去除两端反引号并 trim 空白。
///
/// # Arguments
///
/// * `name` - 原始字段名文本。
///
/// # Returns
///
/// 清理后的字段名字符串。
fn clean_field_name(name: &str) -> String {
    name.trim()
        .trim_start_matches('`')
        .trim_end_matches('`')
        .to_string()
}

/// 解析无序列表，提取每个列表项的文本。
///
/// 支持 `- item` 和 `* item` 两种列表格式。
///
/// # Arguments
///
/// * `content` - 列表格式的原始文本。
///
/// # Returns
///
/// 列表项文本的向量。
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

/// 从文本行中提取反引号包裹的代码值。
///
/// 查找第一个反引号对（`` ` ``）中的内容，用于提取编码格式等配置值。
///
/// # Arguments
///
/// * `line` - 待提取的文本行。
///
/// # Returns
///
/// 找到反引号内容时返回 `Some(value)`，否则返回 `None`。
fn extract_code_value(line: &str) -> Option<String> {
    if let Some(start) = line.find('`')
        && let Some(end) = line[start + 1..].find('`') {
            return Some(line[start + 1..start + 1 + end].to_string());
        }
    None
}

/// 使用 pulldown-cmark 解析示例章节。
///
/// 从 Examples 章节中提取结构化的输入/输出示例对。
/// 支持带有"输入"/"输出"标记的代码块格式，也支持简单的 `输入: xxx` 行内格式。
///
/// # Arguments
///
/// * `content` - Examples 章节的原始文本。
///
/// # Returns
///
/// 解析得到的示例信息列表。
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

    if !current_input.is_empty() || !current_output.is_empty() {
        examples.push(ExampleInfo {
            input: current_input,
            output: current_output,
        });
    }

    // 回退到简单格式解析
    if examples.is_empty()
        && let Some(example) = parse_simple_example(content) {
            examples.push(example);
        }

    examples
}

/// 解析简单行内格式的示例。
///
/// 当结构化代码块解析失败时，尝试从 `输入: xxx` / `输出: xxx` 格式的行中提取示例。
/// 支持中文冒号（`：`）和英文冒号（`:`）。
///
/// # Arguments
///
/// * `content` - Examples 章节的原始文本。
///
/// # Returns
///
/// 解析成功时返回 `Some(ExampleInfo)`，否则返回 `None`。
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
