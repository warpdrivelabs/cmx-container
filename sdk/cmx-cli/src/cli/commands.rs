//! CLI 命令实现
//!
//! CMX CLI 是一个多功能的命令行工具，提供文档生成、插件管理等功能。

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use walkdir::WalkDir;

use crate::generator::{generate_document, ScanResult};
use crate::parser::{parse_doc_comments, parse_rust_file};

/// CMX CLI - CMX 插件开发工具集
#[derive(Parser, Debug)]
#[command(name = "cmx-cli")]
#[command(author, version, about = "CMX 插件开发工具集", long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    /// 子命令
    #[command(subcommand)]
    command: Commands,
}

/// 子命令定义
#[derive(Subcommand, Debug)]
enum Commands {
    /// 文档相关命令
    #[command(name = "doc")]
    Doc(DocCommand),

    /// 插件相关命令
    #[command(name = "plugin")]
    Plugin(PluginCommand),
}

/// 文档相关命令
#[derive(Parser, Debug)]
struct DocCommand {
    /// 文档子命令
    #[command(subcommand)]
    command: DocSubcommand,
}

/// 文档子命令
#[derive(Subcommand, Debug)]
enum DocSubcommand {
    /// 扫描 Rust 代码，生成 WASM 函数文档
    #[command(name = "scan")]
    Scan(ScanArgs),

    /// 验证文档格式（预留）
    #[command(name = "validate")]
    Validate(ValidateArgs),
}

/// 扫描参数
#[derive(Parser, Debug)]
struct ScanArgs {
    /// 要扫描的目录路径
    #[arg(required = true)]
    paths: Vec<String>,

    /// 输出文件路径
    #[arg(short, long)]
    output: Option<String>,

    /// 美化 JSON 输出
    #[arg(long)]
    pretty: bool,

    /// 排除的文件模式
    #[arg(long)]
    exclude: Option<String>,

    /// 插件名称（默认从 Cargo.toml 读取）
    #[arg(long)]
    plugin_name: Option<String>,
}

/// 验证参数（预留）
#[derive(Parser, Debug)]
struct ValidateArgs {
    /// 要验证的文档文件路径
    #[arg(required = true)]
    file: String,
}

/// 插件相关命令
#[derive(Parser, Debug)]
struct PluginCommand {
    /// 插件子命令
    #[command(subcommand)]
    command: PluginSubcommand,
}

/// 插件子命令
#[derive(Subcommand, Debug)]
enum PluginSubcommand {
    /// 构建 WASM 插件（预留）
    #[command(name = "build")]
    Build(BuildArgs),

    /// 显示插件信息（预留）
    #[command(name = "info")]
    Info(InfoArgs),

    /// 初始化新插件项目
    #[command(name = "new")]
    New(NewPluginArgs),
}

/// 构建参数（预留）
#[derive(Parser, Debug)]
struct BuildArgs {
    /// 项目路径
    #[arg(default_value = ".")]
    path: String,

    /// 发布模式
    #[arg(long)]
    release: bool,
}

/// 信息参数（预留）
#[derive(Parser, Debug)]
struct InfoArgs {
    /// WASM 文件路径
    #[arg(required = true)]
    wasm_file: String,
}

/// 新建插件参数
#[derive(Parser, Debug)]
struct NewPluginArgs {
    /// 插件名称
    #[arg(required = true)]
    name: String,

    /// 目标路径
    #[arg(short, long, default_value = ".")]
    path: String,
}

/// 运行 CLI
pub fn run() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Doc(doc_cmd) => handle_doc_command(doc_cmd),
        Commands::Plugin(plugin_cmd) => handle_plugin_command(plugin_cmd),
    }
}

/// 处理文档命令
fn handle_doc_command(cmd: DocCommand) -> Result<()> {
    match cmd.command {
        DocSubcommand::Scan(args) => handle_scan_command(args),
        DocSubcommand::Validate(args) => handle_validate_command(args),
    }
}

/// 处理扫描命令
fn handle_scan_command(args: ScanArgs) -> Result<()> {
    // 收集所有 Rust 文件
    let rust_files = collect_rust_files(&args.paths, args.exclude.as_deref())?;

    if rust_files.is_empty() {
        eprintln!("未找到任何 Rust 源文件");
        return Ok(());
    }

    // 解析所有文件
    let mut all_functions = Vec::new();
    let mut first_file_path = String::new();

    for file_path in &rust_files {
        let content = fs::read_to_string(file_path)
            .with_context(|| format!("无法读取文件: {}", file_path))?;

        let functions = parse_rust_file(&content)?;

        if !functions.is_empty() && first_file_path.is_empty() {
            first_file_path = file_path.clone();
        }

        for func in functions {
            let doc = parse_doc_comments(&func.doc_comments)?;
            all_functions.push((func, doc));
        }
    }

    if all_functions.is_empty() {
        eprintln!("未找到任何 #[plugin_fn] 函数");
        return Ok(());
    }

    // 获取插件信息
    let (plugin_name, plugin_version, plugin_description) =
        get_plugin_info(&args.paths, args.plugin_name.as_deref())?;

    // 生成文档
    let result = ScanResult {
        plugin_name,
        plugin_version,
        plugin_description,
        functions: all_functions,
        file_path: first_file_path,
    };

    let json = generate_document(&result, args.pretty)?;

    // 输出结果
    if let Some(output_path) = args.output {
        fs::write(&output_path, &json)
            .with_context(|| format!("无法写入文件: {}", output_path))?;
        println!("文档已生成: {}", output_path);
    } else {
        println!("{}", json);
    }

    Ok(())
}

/// 处理验证命令（预留）
fn handle_validate_command(args: ValidateArgs) -> Result<()> {
    let content = fs::read_to_string(&args.file)
        .with_context(|| format!("无法读取文件: {}", args.file))?;

    // 尝试解析 JSON
    let _: serde_json::Value = serde_json::from_str(&content)
        .with_context(|| "JSON 格式无效")?;

    println!("✓ 文档格式验证通过: {}", args.file);
    Ok(())
}

/// 处理插件命令
fn handle_plugin_command(cmd: PluginCommand) -> Result<()> {
    match cmd.command {
        PluginSubcommand::Build(args) => handle_build_command(args),
        PluginSubcommand::Info(args) => handle_info_command(args),
        PluginSubcommand::New(args) => handle_new_plugin_command(args),
    }
}

/// 处理构建命令（预留）
fn handle_build_command(args: BuildArgs) -> Result<()> {
    let mode = if args.release { "release" } else { "debug" };
    println!("构建 WASM 插件: {} ({})", args.path, mode);
    println!("提示: 此功能尚未实现，请使用 cargo build --target wasm32-unknown-unknown");
    Ok(())
}

/// 处理信息命令（预留）
fn handle_info_command(args: InfoArgs) -> Result<()> {
    println!("插件信息: {}", args.wasm_file);
    println!("提示: 此功能尚未实现");
    Ok(())
}

/// 处理新建插件命令
fn handle_new_plugin_command(args: NewPluginArgs) -> Result<()> {
    let target_path = Path::new(&args.path).join(&args.name);
    
    if target_path.exists() {
        anyhow::bail!("目录已存在: {}", target_path.display());
    }

    // 创建目录结构
    fs::create_dir_all(&target_path)?;
    fs::create_dir_all(target_path.join("src"))?;

    // 生成 Cargo.toml
    let cargo_toml = format!(
        r#"[package]
name = "{}"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
extism-pdk = "1.4.1"
serde = {{ version = "1.0", features = ["derive"] }}
serde_json = "1.0"
"#,
        args.name
    );
    fs::write(target_path.join("Cargo.toml"), cargo_toml)?;

    // 生成 lib.rs 模板
    let lib_rs = r#"//! 插件描述

use extism_pdk::*;
use serde::{Deserialize, Serialize};

/// 示例函数
///
/// # 输入
///
/// | 字段 | 类型 | 必填 | 说明 |
/// |------|------|------|------|
/// | `input.input` | string | 是 | 输入数据 |
///
/// # 输出
///
/// | 字段 | 类型 | 说明 |
/// |------|------|------|
/// | `result` | string | 输出结果 |
///
/// # 示例
///
/// **输入:**
/// ```json
/// {"input": "hello", "context": {}}
/// ```
///
/// **输出:**
/// ```json
/// {"result": "processed: hello"}
/// ```
#[plugin_fn]
pub fn hello(Json(input): Json<serde_json::Value>) -> FnResult<Json<serde_json::Value>> {
    Ok(Json(serde_json::json!({
        "result": format!("processed: {}", input)
    })))
}
"#;
    fs::write(target_path.join("src/lib.rs"), lib_rs)?;

    println!("✓ 插件项目已创建: {}", target_path.display());
    println!("\n下一步:");
    println!("  cd {}", args.name);
    println!("  cargo build --target wasm32-unknown-unknown");

    Ok(())
}

/// 收集所有 Rust 源文件
fn collect_rust_files(paths: &[String], exclude: Option<&str>) -> Result<Vec<String>> {
    let mut files = Vec::new();

    for path_str in paths {
        let path = Path::new(path_str);

        if path.is_file() && path.extension().map_or(false, |ext| ext == "rs") {
            files.push(path_str.clone());
        } else if path.is_dir() {
            for entry in WalkDir::new(path)
                .follow_links(true)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let entry_path = entry.path();
                if entry_path.extension().map_or(false, |ext| ext == "rs") {
                    let file_path = entry_path.to_string_lossy().to_string();

                    // 检查排除模式
                    if let Some(exclude_pattern) = exclude {
                        if file_path.contains(exclude_pattern) {
                            continue;
                        }
                    }

                    files.push(file_path);
                }
            }
        }
    }

    Ok(files)
}

/// 从 Cargo.toml 获取插件信息
fn get_plugin_info(
    paths: &[String],
    override_name: Option<&str>,
) -> Result<(String, String, Option<String>)> {
    // 尝试从第一个路径查找 Cargo.toml
    for path_str in paths {
        let path = Path::new(path_str);
        let cargo_toml_path = if path.is_file() {
            path.parent().unwrap_or(path).join("Cargo.toml")
        } else {
            path.join("Cargo.toml")
        };

        if cargo_toml_path.exists() {
            if let Ok(content) = fs::read_to_string(&cargo_toml_path) {
                if let Ok(value) = content.parse::<toml::Value>() {
                    let name = override_name.map(|s| s.to_string()).unwrap_or_else(|| {
                        value
                            .get("package")
                            .and_then(|p| p.get("name"))
                            .and_then(|n| n.as_str())
                            .unwrap_or("unknown")
                            .to_string()
                    });

                    // 检查是否使用 workspace 版本
                    let version = if let Some(version) = value
                        .get("package")
                        .and_then(|p| p.get("version"))
                        .and_then(|v| v.as_str())
                    {
                        version.to_string()
                    } else {
                        // 尝试从 workspace Cargo.toml 读取版本
                        get_workspace_version(&cargo_toml_path).unwrap_or_else(|| "0.0.0".to_string())
                    };

                    let description = value
                        .get("package")
                        .and_then(|p| p.get("description"))
                        .and_then(|d| d.as_str())
                        .map(|s| s.to_string());

                    return Ok((name, version, description));
                }
            }
        }
    }

    // 默认值
    Ok((
        override_name.unwrap_or("unknown").to_string(),
        "0.0.0".to_string(),
        None,
    ))
}

/// 从 workspace Cargo.toml 获取版本号
fn get_workspace_version(cargo_toml_path: &Path) -> Option<String> {
    // 向上查找 workspace Cargo.toml
    let mut current = cargo_toml_path.parent()?;
    
    loop {
        let workspace_cargo = current.join("Cargo.toml");
        if workspace_cargo.exists() {
            if let Ok(content) = fs::read_to_string(&workspace_cargo) {
                if let Ok(value) = content.parse::<toml::Value>() {
                    // 检查是否是 workspace
                    if value.get("workspace").is_some() {
                        if let Some(version) = value
                            .get("workspace")
                            .and_then(|w| w.get("package"))
                            .and_then(|p| p.get("version"))
                            .and_then(|v| v.as_str())
                        {
                            return Some(version.to_string());
                        }
                    }
                }
            }
        }
        
        current = current.parent()?;
    }
}
