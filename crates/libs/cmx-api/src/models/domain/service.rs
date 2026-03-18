//! Domain 实体的自定义 Service
//!
//! 展示如何扩展 GenericCrudService 实现自定义业务逻辑

use crate::crud::service::GenericCrudService;
use crate::error::{Error, Result};
use cmx_core::model::data::dataset::DataSet;
use cmx_database::DatabaseManager;
use modql::filter::{ListOptions, OpValString, OpValsString};
use serde_json::Value;
use tracing::{debug, info};

use super::{DomainBmc, DomainFilter};

/// Domain 自定义服务
///
/// 继承 GenericCrudService 并添加自定义业务方法
pub struct DomainService;

impl DomainService {
    /// 扩展方法：按名称查询
    ///
    /// # 参数
    /// * `mm` - 数据库管理器
    /// * `db_id` - 数据库 ID
    /// * `name` - 域名
    ///
    /// # 返回值
    /// 返回匹配的 Domain DataSet
    pub async fn get_by_name(
        mm: &DatabaseManager,
        db_id: &str,
        name: &str,
    ) -> Result<DataSet> {
        info!("{:<12} - DomainService::get_by_name - name: {}", "SERVICE", name);

        let filter = DomainFilter {
            code: None,
            name: Some(OpValsString(vec![OpValString::Eq(name.to_string())])),
            r#type: None,
            status: None,
            archived: None,
        };

        GenericCrudService::<DomainBmc, DomainFilter>::list(mm, db_id, Some(filter), None).await
    }

    /// 扩展方法：批量创建
    ///
    /// # 参数
    /// * `mm` - 数据库管理器
    /// * `db_id` - 数据库 ID
    /// * `items` - 要创建的数据列表
    ///
    /// # 返回值
    /// 返回创建结果列表
    pub async fn batch_create(
        mm: &DatabaseManager,
        db_id: &str,
        items: Vec<Value>,
    ) -> Result<Vec<DataSet>> {
        info!(
            "{:<12} - DomainService::batch_create - count: {}",
            "SERVICE",
            items.len()
        );

        let mut results = Vec::new();
        for item in items {
            let result = Self::create(mm, db_id, item).await?;
            results.push(result);
        }

        info!("{:<12} - 批量创建完成，成功 {} 条", "SERVICE", results.len());
        Ok(results)
    }

    /// 覆盖方法：自定义创建逻辑
    ///
    /// 添加额外的验证和业务逻辑
    pub async fn create(
        mm: &DatabaseManager,
        db_id: &str,
        data: Value,
    ) -> Result<DataSet> {
        info!("{:<12} - DomainService::create", "SERVICE");

        // 自定义验证：名称长度
        if let Some(name) = data.get("name").and_then(|v| v.as_str()) {
            if name.len() < 2 {
                return Err(Error::bad_request("域名长度不能小于2个字符"));
            }
            if name.len() > 100 {
                return Err(Error::bad_request("域名长度不能超过100个字符"));
            }
        }

        // 调用父类方法
        GenericCrudService::<DomainBmc>::create(mm, db_id, data).await
    }

    /// 扩展方法：按状态统计
    ///
    /// # 参数
    /// * `mm` - 数据库管理器
    /// * `db_id` - 数据库 ID
    ///
    /// # 返回值
    /// 返回各状态的统计信息
    pub async fn count_by_status(mm: &DatabaseManager, db_id: &str) -> Result<DataSet> {
        debug!("{:<12} - DomainService::count_by_status", "SERVICE");

        let sql = r#"
            SELECT status, COUNT(*) as count 
            FROM cmx_domain 
            WHERE archived = 0
            GROUP BY status
        "#;

        mm.query_sql(db_id, None, sql, "count_by_status")
            .await
            .map_err(|e| Error::internal_error(format!("统计查询失败: {}", e)))
    }

    /// 扩展方法：搜索域名
    ///
    /// 支持模糊搜索和分页
    pub async fn search(
        mm: &DatabaseManager,
        db_id: &str,
        keyword: &str,
        page: i64,
        page_size: i64,
    ) -> Result<(DataSet, i64)> {
        debug!(
            "{:<12} - DomainService::search - keyword: {}",
            "SERVICE", keyword
        );

        let filter = DomainFilter {
            code: None,
            name: Some(OpValsString(vec![OpValString::Contains(
                keyword.to_string(),
            )])),
            r#type: None,
            status: None,
            archived: None,
        };

        let list_options = ListOptions {
            limit: Some(page_size),
            offset: Some((page - 1) * page_size),
            order_bys: Some("name".into()),
        };

        GenericCrudService::<DomainBmc, DomainFilter>::page(mm, db_id, Some(filter), list_options)
            .await
    }
}
