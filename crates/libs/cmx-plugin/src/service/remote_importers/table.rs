//! `RemoteTableDefinitionImporter` — 表结构定义的远程(gRPC)实现。
//!
//! 注:表结构建表是本地 DDL 操作,远程模式下「建表」仍需本地执行,
//! 此实现仅远程登记元数据。完整的远程建表需额外的 DDL 同步机制(本期不做)。
//! 当前行为:调用方在 mode=grpc 时,建表部分应另行处理(或 fallback 到本地)。

use async_trait::async_trait;
use cmx_core::model::meta::table::TableDefine;
use cmx_traits::error::TraitError;
use cmx_traits::resource::TableDefinitionImporter;
use cmx_traits::resource::{ResourceDataCategory, ResourceDataImportRequest};
use serde_json::json;
use tracing::info;

use super::RemoteImporterContext;

/// 远程表结构定义导入器(经 gRPC 或 HTTP 调用元数据/表单中心)。
pub struct RemoteTableDefinitionImporter {
    ctx: RemoteImporterContext,
}

impl RemoteTableDefinitionImporter {
    /// 创建远程表结构定义导入器。
    pub fn new(ctx: RemoteImporterContext) -> Self {
        Self { ctx }
    }
}

#[async_trait]
impl TableDefinitionImporter for RemoteTableDefinitionImporter {
    async fn apply_table_definitions(
        &self,
        domain_code: &str,
        app_code: &str,
        module_code: &str,
        app_id: &str,
        definitions: &[TableDefine],
        _biz_db_id: &str,
    ) -> Result<usize, TraitError> {
        if definitions.is_empty() {
            return Ok(0);
        }
        // 表结构打包为 { "tables": [...] } 单文件 ZIP(对齐模块包格式)
        let payload = json!({ "tables": definitions, "app_id": app_id });
        let zip_data =
            crate::center_client::packer::pack_payload_to_zip(&payload, "module_tables.json")
                .map_err(|e| TraitError::Business(format!("打包表结构定义失败: {e}")))?;

        // 表结构归类到 Form 中心传输(元数据登记与表单共用基础设施);
        // 实际目标服务由配置 discovery.form_service / urls.form 决定,可独立配置为元数据中心。
        let req = ResourceDataImportRequest {
            category: ResourceDataCategory::Form,
            domain_code: domain_code.to_string(),
            application_code: app_code.to_string(),
            module_code: module_code.to_string(),
            plugin_id: String::new(),
            app_id: app_id.to_string(),
            version: String::new(),
            zip_data,
        };

        let result = self
            .ctx
            .send(crate::center_client::types::DataCategory::Form, req)
            .await
            .map_err(|e| TraitError::Business(e.to_string()))?;
        info!(
            created = result.created_count,
            updated = result.updated_count,
            "远程表结构登记完成"
        );
        Ok((result.created_count + result.updated_count) as usize)
    }

    async fn list_table_definitions(
        &self,
        app_code: &str,
        module_code: &str,
    ) -> Result<Vec<TableDefine>, TraitError> {
        let req = ResourceDataImportRequest {
            category: ResourceDataCategory::Form,
            domain_code: String::new(),
            application_code: app_code.to_string(),
            module_code: module_code.to_string(),
            plugin_id: String::new(),
            app_id: String::new(),
            version: String::new(),
            zip_data: Vec::new(),
        };
        let result = self
            .ctx
            .send_list(crate::center_client::types::DataCategory::Form, req)
            .await
            .map_err(|e| TraitError::Business(e.to_string()))?;
        // 远程返回的是 JSON 数组,但 TableDefineImporter.list 返回的是 Vec<TableDefine>
        // 接收端 list_data 对 Form 类别返回 FormDefinition 列表(不含 TableDefine)
        // 这里需要特殊处理:Table 走 Form 通道时,接收端应返回 TableDefine 列表
        // 当前接收端 list_data 的 Form 分支返回 FormDefinition,需扩展
        serde_json::from_slice(&result.json_data)
            .map_err(|e| TraitError::Business(format!("反序列化远程表结构定义失败: {e}")))
    }
}
