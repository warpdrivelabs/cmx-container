//! 开发工具 HTTP Handler
//!
//! 提供模板管理等 API

use axum::Json;
use cmx_database::get_default_db_manager;
use percent_encoding::percent_encode;
use percent_encoding::NON_ALPHANUMERIC;
use serde_json::json;
use std::convert::TryFrom;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use tracing::{info, warn};
use zip::ZipArchive;

use crate::api_response::ApiResp;
use crate::error::Result;
use cmx_utils::ConfigManager;
use crate::handlers::sys_datasource::SysDatasourceService;
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

        if path.extension().is_some_and(|ext| ext == "zip") {
            let template_name = path
                .file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or("unknown")
                .to_string();

            let metadata = fs::metadata(&path).ok();
            let modified_time = metadata
                .as_ref()
                .and_then(|m| m.modified().ok())
                .map(chrono::DateTime::from);
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
            if let Some(p) = outpath.parent()
                && !p.exists() {
                    fs::create_dir_all(p).map_err(|e| {
                        crate::error::Error::InternalError(format!("创建父目录失败: {}", e))
                    })?;
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
        src_dir: &Path,
        target_dir: &Path,
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
                let target_file = if let Some(stripped) = file_name.strip_suffix(".hbs") {
                    target_dir.join(stripped)
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
                        .replace("{{module_code}}", req.module_code.as_deref().unwrap_or(""))
                        .replace("{{datasource_id}}", req.datasource_id.as_deref().unwrap_or(""));

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

    if let Some(datasource_id) = &req.datasource_id {
        if !datasource_id.is_empty() {
            create_vscode_settings(&target_dir, datasource_id).await?;
        }
    }

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

async fn create_vscode_settings(target_dir: &Path, datasource_id: &str) -> Result<()> {
    info!("[api] create_vscode_settings called for datasource_id: {}", datasource_id);

    let db_manager = get_default_db_manager();
    let default_db_id =db_manager.get_default_db_id().await;

    // let sql = "SELECT db_url, db_type FROM cmx_sys_datasource WHERE db_id = $1";
    // let params = json!([datasource_id]);

    let dataset = SysDatasourceService::get_by_db_id(db_manager, &default_db_id, &datasource_id).await?;
    // let dataset = db_manager
    //     .query_sql_with_json(default_db_id.as_str(), None, sql, params, "datasource_query")
    //     .await
    //     .map_err(|e| crate::error::Error::InternalError(
    //         format!("查询数据源失败: {}", e)
    //     ))?;

    if let Some(row) = dataset.iter().next() {
        let db_url = row.get_by_name(&dataset.schema, "db_url")
            .and_then(|v| String::try_from(v.clone()).ok())
            .ok_or_else(|| crate::error::Error::InternalError(
                "无法获取 db_url".to_string()
            ))?;

        let db_type = row.get_by_name(&dataset.schema, "db_type")
            .and_then(|v| String::try_from(v.clone()).ok())
            .ok_or_else(|| crate::error::Error::InternalError(
                "无法获取 db_type".to_string()
            ))?;

        let driver = match db_type.to_lowercase().as_str() {
            "postgres" | "postgresql" => "PostgreSQL",
            "mysql" => "MySQL",
            "sqlite" | "sqlite3" => "SQLite",
            _ => &db_type,
        };

        let new_connection = json!({
            "driver": driver,
            "name": datasource_id,
            "connectionString": db_url,
            "previewLimit": 50
        });

        let vscode_dir = target_dir.join(".vscode");
        fs::create_dir_all(&vscode_dir)
            .map_err(|e| crate::error::Error::InternalError(
                format!("创建 .vscode 目录失败: {}", e)
            ))?;

        let settings_path = vscode_dir.join("settings.json");

        let mut settings_content = if settings_path.exists() {
            let existing_content = fs::read_to_string(&settings_path)
                .map_err(|e| crate::error::Error::InternalError(
                    format!("读取现有 settings.json 失败: {}", e)
                ))?;

            serde_json::from_str::<serde_json::Value>(&existing_content)
                .unwrap_or_else(|_| json!({}))
        } else {
            json!({
                "sqltools": {
                    "connections": []
                }
            })
        };

        let connections = if let Some(sqltools) = settings_content.get_mut("sqltools") {
            if let Some(conns) = sqltools.get_mut("connections") {
                conns.take()
            } else {
                json!([])
            }
        } else {
            json!([])
        };

        let mut connections_arr = if let Some(arr) = connections.as_array() {
            arr.clone()
        } else {
            vec![]
        };

        let connection_exists = connections_arr.iter().any(|conn| {
            conn.get("name") == Some(&serde_json::Value::String(datasource_id.to_string()))
        });

        if !connection_exists {
            connections_arr.push(new_connection);
            settings_content["sqltools"]["connections"] = json!(connections_arr);
        } else {
            info!("[api] 数据源 {} 已存在于 settings.json 中，跳过", datasource_id);
        }

        let final_content = serde_json::to_string_pretty(&settings_content)
            .map_err(|e| crate::error::Error::InternalError(
                format!("序列化 settings.json 失败: {}", e)
            ))?;

        fs::write(&settings_path, final_content)
            .map_err(|e| crate::error::Error::InternalError(
                format!("写入 settings.json 失败: {}", e)
            ))?;

        info!("[api] 创建/更新 .vscode/settings.json 成功");
    } else {
        warn!("[api] 未找到数据源: {}", datasource_id);
    }

    Ok(())
}
