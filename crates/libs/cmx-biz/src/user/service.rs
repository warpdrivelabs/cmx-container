//! User Service

use cmx_core::model::data::dataset::DataSet;
use cmx_database::crud::GenericCrudService;
use cmx_database::DatabaseManager;
use modql::filter::ListOptions;
use serde_json::Value;

use super::{UserBmc, UserFilter, UserForCreate, UserForUpdate};
use crate::{BizError, Result};

/// 用户服务
pub struct UserService;

impl UserService {
    /// 创建用户
    pub async fn create(
        mm: &DatabaseManager,
        db_id: &str,
        data: UserForCreate,
    ) -> Result<DataSet> {
        GenericCrudService::<UserBmc>::create(mm, db_id, None, data)
            .await
            .map_err(BizError::from)
    }

    /// 查询单个用户
    pub async fn get(mm: &DatabaseManager, db_id: &str, id: &str) -> Result<DataSet> {
        GenericCrudService::<UserBmc>::get(mm, db_id, None, id.into())
            .await
            .map_err(BizError::from)
    }

    /// 更新用户
    pub async fn update(
        mm: &DatabaseManager,
        db_id: &str,
        id: Value,
        data: UserForUpdate,
    ) -> Result<DataSet> {
        GenericCrudService::<UserBmc>::update(mm, db_id, None, id, data)
            .await
            .map_err(BizError::from)
    }

    /// 分页查询用户
    pub async fn page(
        mm: &DatabaseManager,
        db_id: &str,
        filters: Option<Vec<UserFilter>>,
        list_options: ListOptions,
    ) -> Result<(DataSet, i64)> {
        GenericCrudService::<UserBmc, UserFilter>::page(mm, db_id, None, filters, list_options)
            .await
            .map_err(BizError::from)
    }
}
