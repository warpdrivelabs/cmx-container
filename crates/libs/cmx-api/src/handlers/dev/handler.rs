//! 开发工具 HTTP Handler
//!
//! 提供模板管理等 API

use axum::Json;
use percent_encoding::percent_encode;
use percent_encoding::NON_ALPHANUMERIC;
use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;
use tracing::info;
use zip::ZipArchive;

use crate::api_response::ApiResp;
use crate::error::Result;
use cmx_utils::ConfigManager;

use super::request::CreateProjectRequest;
use super::response::{CreateProjectResponse, TemplateInfo};

/// 获取模板列表 Handler
///
/// 从配置文件中读取模板目录路径，返回该目录下的所有模板信息
#[utoipa::path(
    get,
    path = "/api/dev/templates",
    responses(
        (status = 200, description = "获取成功", body = ApiResp<Vec<TemplateInfo>>),
        (status = 500, description = "服务器内部错误")
    ),
    tag = "Dev"
)]
pub async fn list_templates() -> Result<Json<ApiResp<Vec<TemplateInfo>>>> {
    info!("[api] list_templates called");

    let config = ConfigManager::global();
    
    let templates_path = config
        .get_string("templates.path")
        .map_err(|e| crate::error::Error::InternalError(format!("读取模板路径配置失败: {}", e)))?;

    let templates_dir = PathBuf::from(&templates_path);
    
    if !templates_dir.exists() {
        return Err(crate::error::Error::InternalError(format!(
            "模板目录不存在: {}",
            templates_path
        )));
    }

    let mut templates = Vec::new();

    let entries = fs::read_dir(&templates_dir)
        .map_err(|e| crate::error::Error::InternalError(format!("读取模板目录失败: {}", e)))?;

    for entry in entries {
        let entry = entry.map_err(|e| {
            crate::error::Error::InternalError(format!("读取目录项失败: {}", e))
        })?;
        
        let path = entry.path();
        
        if path.extension().map_or(false, |ext| ext == "zip") {
            let template_name = path
                .file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or("unknown")
                .to_string();

            let metadata = fs::metadata(&path).ok();
            let modified_time = metadata
                .as_ref()
                .and_then(|m| m.modified().ok())
                .map(|time| chrono::DateTime::from(time));
            let file_size = metadata.as_ref().map(|m| m.len()).unwrap_or(0);

            templates.push(TemplateInfo {
                name: template_name,
                path: path.to_string_lossy().to_string(),
                modified_time,
                file_size: Some(file_size),
            });
        }
    }

    info!("[api] list_templates success: found {} templates", templates.len());
    Ok(Json(ApiResp::ok(templates)))
}

/// 创建项目 Handler
///
/// 根据模板创建新项目
#[utoipa::path(
    post,
    path = "/api/dev/projects",
    request_body = CreateProjectRequest,
    responses(
        (status = 200, description = "创建成功", body = CreateProjectResponse),
        (status = 400, description = "参数错误"),
        (status = 500, description = "服务器内部错误")
    ),
    tag = "Dev"
)]
pub async fn create_project(
    Json(req): Json<CreateProjectRequest>,
) -> Result<Json<CreateProjectResponse>> {
    info!("[api] create_project called: {:?}", req);

    // 步骤 1: 参数校验
    if req.id.is_empty() {
        return Ok(Json(CreateProjectResponse {
            code: -1,
            message: Some("插件编码不能为空".to_string()),
            project_url: None,
        }));
    }
    if req.name.is_empty() {
        return Ok(Json(CreateProjectResponse {
            code: -1,
            message: Some("插件名称不能为空".to_string()),
            project_url: None,
        }));
    }
    if req.path.is_empty() {
        return Ok(Json(CreateProjectResponse {
            code: -1,
            message: Some("保存路径不能为空".to_string()),
            project_url: None,
        }));
    }

    // 步骤 2: 获取模板
    let config = ConfigManager::global();
    let templates_path = config
        .get_string("templates.path")
        .map_err(|e| crate::error::Error::InternalError(format!("读取模板路径配置失败: {}", e)))?;

    let template_zip_path = PathBuf::from(&templates_path).join(format!("{}.zip", req.template));
    
    if !template_zip_path.exists() {
        return Ok(Json(CreateProjectResponse {
            code: -1,
            message: Some(format!("模板不存在: {}", req.template)),
            project_url: None,
        }));
    }

    // 步骤 3: 创建临时目录
    let temp_dir = std::env::temp_dir().join(format!("cmx_template_{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&temp_dir)
        .map_err(|e| crate::error::Error::InternalError(format!("创建临时目录失败: {}", e)))?;

    // 步骤 4: 解压模板
    let template_zip_data = fs::read(&template_zip_path)
        .map_err(|e| crate::error::Error::InternalError(format!("读取模板文件失败: {}", e)))?;
    
    let reader = std::io::Cursor::new(template_zip_data);
    let mut archive = ZipArchive::new(reader)
        .map_err(|e| crate::error::Error::InternalError(format!("解析ZIP文件失败: {}", e)))?;

    let extract_dir = temp_dir.join("template");
    fs::create_dir_all(&extract_dir)
        .map_err(|e| crate::error::Error::InternalError(format!("创建解压目录失败: {}", e)))?;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|e| {
            crate::error::Error::InternalError(format!("读取ZIP条目失败: {}", e))
        })?;
        
        let outpath = match file.enclosed_name() {
            Some(path) => extract_dir.join(path),
            None => continue,
        };

        if file.name().ends_with('/') {
            fs::create_dir_all(&outpath).map_err(|e| {
                crate::error::Error::InternalError(format!("创建目录失败: {}", e))
            })?;
        } else {
            if let Some(p) = outpath.parent() {
                if !p.exists() {
                    fs::create_dir_all(p).map_err(|e| {
                        crate::error::Error::InternalError(format!("创建父目录失败: {}", e))
                    })?;
                }
            }
            let mut outfile = fs::File::create(&outpath).map_err(|e| {
                crate::error::Error::InternalError(format!("创建文件失败: {}", e))
            })?;
            std::io::copy(&mut file, &mut outfile).map_err(|e| {
                crate::error::Error::InternalError(format!("写入文件失败: {}", e))
            })?;
        }
    }

    // 步骤 5: 创建项目目录
    let target_dir = PathBuf::from(&req.path).join(&req.id);
    fs::create_dir_all(&target_dir)
        .map_err(|e| crate::error::Error::InternalError(format!("创建项目目录失败: {}", e)))?;

    // 步骤 6: 渲染模板文件
    fn process_template_dir(
        src_dir: &PathBuf,
        target_dir: &PathBuf,
        req: &CreateProjectRequest,
    ) -> Result<()> {
        for entry in fs::read_dir(src_dir)
            .map_err(|e| crate::error::Error::InternalError(format!("读取目录失败: {}", e)))?
        {
            let entry = entry.map_err(|e| {
                crate::error::Error::InternalError(format!("读取目录项失败: {}", e))
            })?;
            let path = entry.path();
            
            if path.is_dir() {
                let dir_name = path.file_name().unwrap().to_str().unwrap();
                let new_target_dir = target_dir.join(dir_name);
                fs::create_dir_all(&new_target_dir).map_err(|e| {
                    crate::error::Error::InternalError(format!("创建目录失败: {}", e))
                })?;
                process_template_dir(&path, &new_target_dir, req)?;
            } else {
                let file_name = path.file_name().unwrap().to_str().unwrap();
                let target_file = if file_name.ends_with(".hbs") {
                    target_dir.join(&file_name[..file_name.len() - 4])
                } else {
                    target_dir.join(file_name)
                };

                if file_name.ends_with(".hbs") {
                    let mut content = String::new();
                    fs::File::open(&path)
                        .map_err(|e| {
                            crate::error::Error::InternalError(format!("打开模板文件失败: {}", e))
                        })?
                        .read_to_string(&mut content)
                        .map_err(|e| {
                            crate::error::Error::InternalError(format!("读取模板文件失败: {}", e))
                        })?;

                    let content = content
                        .replace("{{plugin_id}}", &req.id)
                        .replace("{{plugin_name}}", &req.name)
                        .replace("{{project_name}}", &req.id)
                        .replace("{{project_path}}", &target_dir.to_string_lossy())
                        .replace("{{description}}", req.description.as_deref().unwrap_or(""))
                        .replace("{{domain_code}}", req.domain_code.as_deref().unwrap_or(""))
                        .replace("{{application_code}}", req.application_code.as_deref().unwrap_or(""))
                        .replace("{{module_code}}", req.module_code.as_deref().unwrap_or(""));

                    let mut file = fs::File::create(&target_file).map_err(|e| {
                        crate::error::Error::InternalError(format!("创建文件失败: {}", e))
                    })?;
                    file.write_all(content.as_bytes()).map_err(|e| {
                        crate::error::Error::InternalError(format!("写入文件失败: {}", e))
                    })?;
                } else {
                    fs::copy(&path, &target_file).map_err(|e| {
                        crate::error::Error::InternalError(format!("复制文件失败: {}", e))
                    })?;
                }
            }
        }
        Ok(())
    }

    process_template_dir(&extract_dir, &target_dir, &req)?;

    // 清理临时目录
    let _ = fs::remove_dir_all(&temp_dir);

    // 步骤 7: 生成项目 URL
    let code_server_url = config
        .get_string("code_server.url")
        .unwrap_or_else(|_| "http://localhost:8080".to_string());
    
    let target_path_str = target_dir.to_string_lossy().to_string();
    let encoded_path = percent_encode(target_path_str.as_bytes(), NON_ALPHANUMERIC);
    let project_url = format!("{}?folder={}", code_server_url, encoded_path);

    info!("[api] create_project success: project_url={}", project_url);
    
    Ok(Json(CreateProjectResponse {
        code: 0,
        message: Some("项目创建成功".to_string()),
        project_url: Some(project_url),
    }))
}
