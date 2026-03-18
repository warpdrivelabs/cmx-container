//! REST Handler
//!
//! 提供通用 CRUD 的 REST Handler 函数。
//!
//! 注意：DatabaseManager 通过 get_default_db_manager() 全局获取，不需要通过 state 传递

use axum::extract::Query;
use axum::Json;
use sea_query::SqlWriter;
use cmx_core::model::data::dataset::DataSet;
use cmx_database::get_default_db_manager;
use serde::de::DeserializeOwned;
use serde_json::Value;
use tracing::debug;

use crate::crud::traits::DbBmc;
use crate::crud::service::GenericCrudService;
use crate::error::{Error, Result};
use crate::response::ApiResp;
use crate::rest::params::{DeleteParams, GetParams, ListParams, PageParams};

/// 创建实体的 Handler
///
/// # 参数
/// * `data` - 要创建的实体数据（从 JSON body 提取，可包含 db_id 字段）
///
/// # 返回值
/// 返回包含创建结果的 ApiResp
pub async fn create<MC>(
    Json(mut data): Json<Value>,
) -> Result<Json<ApiResp<DataSet>>>
where
    MC: DbBmc,
{
    debug!("{:<12} - handler::create", "HANDLER");

    let db_id = data.get("db_id")
        .and_then(|v| v.as_str())
        .unwrap_or(get_default_db_manager().get_default_db_id().await.as_str())
        .to_string();

    data.as_object_mut().map(|obj| obj.remove("db_id"));

    let mm = get_default_db_manager();
    let dataset = GenericCrudService::<MC>::create(&mm, &db_id, data).await?;

    Ok(Json(ApiResp::ok(dataset)))
}

/// 根据主键获取单条实体的 Handler
///
/// # 参数
/// * `params` - 查询参数（从 URL 查询参数提取，?id=xxx&db_id=xxx）
///
/// # 返回值
/// 返回包含查询结果的 ApiResp
pub async fn get_by_id<MC>(
    Query(params): Query<GetParams>,
) -> Result<Json<ApiResp<DataSet>>>
where
    MC: DbBmc,
{
    debug!("{:<12} - handler::get_by_id", "HANDLER");

    let mm = get_default_db_manager();
    let db_id = params.get_db_id().await;
    let id = params.id.clone();
    let dataset = GenericCrudService::<MC>::get(&mm, &db_id, id.into()).await?;

    Ok(Json(ApiResp::ok(dataset)))
}

/// 更新实体的 Handler
///
/// # 参数
/// * `data` - 要更新的数据（从 JSON body 提取，包含 id 和可选的 db_id 字段）
///
/// # 返回值
/// 返回包含更新后结果的 ApiResp
pub async fn update<MC>(
    Json(mut data): Json<Value>,
) -> Result<Json<ApiResp<DataSet>>>
where
    MC: DbBmc,
{
    debug!("{:<12} - handler::update", "HANDLER");

    let db_id = data.get("db_id")
        .and_then(|v| v.as_str())
        .unwrap_or(get_default_db_manager().get_default_db_id().await.as_str())
        .to_string();

    let id = data.get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::BadRequest("缺少 id 字段".to_string()))?
        .to_string();

    data.as_object_mut().map(|obj| {
        obj.remove("id");
        obj.remove("db_id");
    });

    let mm = get_default_db_manager();
    let dataset = GenericCrudService::<MC>::update(&mm, &db_id, id.into(), data).await?;

    Ok(Json(ApiResp::ok(dataset)))
}

/// 删除实体的 Handler
///
/// # 参数
/// * `params` - 查询参数（从 URL 查询参数提取，?id=xxx&db_id=xxx）
///
/// # 返回值
/// 返回包含删除信息的 ApiResp
pub async fn delete_by_id<MC>(
    Query(params): Query<DeleteParams>,
) -> Result<Json<ApiResp<DataSet>>>
where
    MC: DbBmc,
{
    debug!("{:<12} - handler::delete_by_id", "HANDLER");

    let mm = get_default_db_manager();
    let db_id = params.get_db_id().await;
    let id = params.id.clone();
    let dataset = GenericCrudService::<MC>::delete(&mm, &db_id, id.into()).await?;

    Ok(Json(ApiResp::ok(dataset)))
}

/// 列表查询的 Handler
///
/// # 参数
/// * `params` - 查询参数（从 JSON body 提取）
///
/// # 返回值
/// 返回包含查询结果的 ApiResp
pub async fn list<MC, F>(
    Json(params): Json<ListParams<F>>,
) -> Result<Json<ApiResp<DataSet>>>
where
    MC: DbBmc,
    F: DeserializeOwned + Into<modql::filter::FilterGroups> + Clone,
{
    debug!("{:<12} - handler::list", "HANDLER");

    let mm = get_default_db_manager();
    let db_id = params.get_db_id().await;
    let list_options = params.to_list_options();
    let filter = params.filter.clone();
    let dataset = GenericCrudService::<MC, F>::list(&mm, &db_id, filter, Some(list_options)).await?;

    Ok(Json(ApiResp::ok(dataset)))
}

/// 分页查询的 Handler
///
/// # 参数
/// * `params` - 查询参数（从 JSON body 提取）
///
/// # 返回值
/// 返回包含查询结果和分页信息的 ApiResp
pub async fn page<MC, F>(
    Json(params): Json<PageParams<F>>,
) -> Result<Json<ApiResp<DataSet>>>
where
    MC: DbBmc,
    F: DeserializeOwned + Into<modql::filter::FilterGroups> + Clone,
{
    debug!("{:<12} - handler::page", "HANDLER");

    let mm = get_default_db_manager();
    let db_id = params.get_db_id().await;
    let page_size = params.get_limit() as u64;
    let page_number = (params.offset.unwrap_or(0) / params.get_limit()) as u64 + 1;

    let list_options = params.to_list_options();
    let filter = params.filter.clone();
    let (dataset, total) = GenericCrudService::<MC, F>::page(&mm, &db_id, filter, list_options).await?;

    Ok(Json(ApiResp::ok_with_pagination(
        dataset,
        page_number,
        page_size,
        total as u64,
    )))
}
