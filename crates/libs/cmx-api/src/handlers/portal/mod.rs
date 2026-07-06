//! 门户/设计器业务 API 模块（迁移自 CMXPortalManager / CMXHTMLDesigner 的 Node 后端）。
//!
//! 路由路径与 Node 后端保持一致（挂在 `/api` 下），响应统一用 [`crate::ApiResp`] 信封。
//! 阶段 1：域/菜单/活动、工作区节点、表单页、原生页面、事实数据等简单文件 CRUD。

pub mod doc;
pub mod handler;
pub mod model_center;

use axum::Router;
use axum::routing::{get, post};

use crate::app_state::CmxAppState;
use crate::routes::traits::ModuleRoutes;

/// 门户业务模块路由聚合。
pub struct PortalModule;

impl ModuleRoutes for PortalModule {
    fn routes(self) -> Router<CmxAppState> {
        Router::new()
            // AI 对话中继
            .route("/ai/chat", post(handler::ai_chat))
            // AI 本地编辑代理
            .route("/agent/capabilities", get(handler::agent_capabilities))
            .route("/agent/message", post(handler::agent_message))
            .route("/agent/message/stream", post(handler::agent_message_stream))
            .route("/agent/approvals/{id}", post(handler::agent_approval))
            // 域 / 菜单 / 活动
            .route("/domains", get(handler::get_domains))
            .route("/menu-pages", get(handler::get_menu_pages))
            .route("/activities", get(handler::get_activities))
            // 工作区节点
            .route(
                "/workspace-nodes",
                get(handler::list_workspace_nodes).post(handler::save_workspace_node),
            )
            .route(
                "/workspace-nodes/{id}",
                get(handler::get_workspace_node).delete(handler::delete_workspace_node),
            )
            // 表单页
            .route(
                "/form-pages",
                get(handler::list_form_pages).post(handler::save_form_page),
            )
            .route("/form-pages/{id}", get(handler::get_form_page))
            // 原生页面
            .route(
                "/native-pages",
                get(handler::list_native_pages).post(handler::save_native_page),
            )
            .route("/native-pages/batch", post(handler::batch_native_pages))
            .route("/native-pages/{id}", get(handler::get_native_page))
            // HTML 页面（v2 分片 + v1 兼容）
            .route(
                "/html-pages",
                get(handler::list_html_pages).post(handler::save_html_page),
            )
            .route("/html-pages/batch", post(handler::batch_html_pages))
            .route("/html-pages/{id}", get(handler::get_html_page))
            // 事实数据
            .route("/fact/list", get(handler::list_facts))
            .route("/fact/get", post(handler::get_fact_post))
            .route(
                "/fact/{domain}/{app}/{module}/{file}",
                get(handler::get_fact_path),
            )
            // 帮助中心（按 DAM 组织的帮助文档：目录 + 文档 CRUD）
            .route("/help/catalog", get(handler::help_catalog))
            .route("/help/get", post(handler::help_get_post))
            .route("/help/doc", post(handler::help_save_doc))
            .route(
                "/help/doc/{domain}/{app}/{module}/{file}",
                get(handler::help_get_path).delete(handler::help_delete_doc),
            )
            // 功能启动器（AI 助手「我要…」直接打开功能）
            .route("/launcher/catalog", get(handler::launcher_catalog))
            .route("/launcher/resolve", post(handler::launcher_resolve))
            // 通知中心（任务/消息/日志 + SSE 主动推送）
            .route("/notifications", get(handler::notify_list))
            .route("/notifications/centers", get(handler::notify_centers))
            .route("/notifications/counts", get(handler::notify_counts))
            .route("/notifications/publish", post(handler::notify_publish))
            .route("/notifications/mark-read", post(handler::notify_mark_read))
            .route("/notifications/stream", get(handler::notify_stream))
            // DAM 注册表（读写 CRUD）
            .route("/dam-registry", get(handler::dam_registry))
            .route(
                "/dam-registry/domains",
                get(handler::dam_list_domains).post(handler::dam_upsert_domain),
            )
            .route(
                "/dam-registry/domains/{domain}",
                axum::routing::delete(handler::dam_delete_domain),
            )
            .route(
                "/dam-registry/applications",
                get(handler::dam_list_applications).post(handler::dam_upsert_application),
            )
            .route(
                "/dam-registry/applications/{domain}/{application}",
                axum::routing::delete(handler::dam_delete_application),
            )
            .route(
                "/dam-registry/modules",
                get(handler::dam_list_modules).post(handler::dam_upsert_module),
            )
            .route(
                "/dam-registry/modules/{domain}/{application}/{module}",
                axum::routing::delete(handler::dam_delete_module),
            )
            // 注册表只读派生
            .route("/registry/domains", get(handler::registry_domains))
            .route("/registry/apps", get(handler::registry_apps))
            .route("/registry/modules", get(handler::registry_modules))
            .route("/registry/dam", get(handler::registry_dam))
            // 服务目录（Bruno collection 解析）
            .route("/service-catalog", get(handler::service_catalog_list))
            .route("/service-catalog/{id}", get(handler::service_catalog_get))
            // 模块清单与资源
            .route("/modules", get(handler::list_modules))
            .route(
                "/modules/{domain}/{application}/{module}",
                get(handler::get_module_manifest),
            )
            .route(
                "/modules/{domain}/{application}/{module}/resources/{type}",
                get(handler::get_module_resource),
            )
            .route("/module-resources", get(handler::module_resources))
            // 定义中心（DCT/DOC/BASE）
            .route("/definitions/list", get(handler::definitions_list))
            .route(
                "/definitions/config",
                get(handler::definitions_get)
                    .post(handler::definitions_save)
                    .delete(handler::definitions_delete),
            )
            .route("/definitions/batch", post(handler::definitions_batch))
            // 业务单据数据装载/回存（方案 Phase 4/5）
            // 端点命名：/doc/data/<驱动>-<内存模式>-<传输> —— 一眼可辨驱动/内存/传输三维度。
            //   驱动 sqlx|tokio · 内存 dataset(全拷贝)|zmc(零拷贝) · 传输 json|msgpack
            // ① sqlx + 老 DataSet(全拷贝) + JSON（老链路）
            .route("/doc/data/sqlx-dataset-json", get(doc::doc_data_sqlx_dataset_json))
            // ④ tokio-postgres + ZmcDataSet(零拷贝) + msgpack 二进制
            .route("/doc/data/tokio-zmc-msgpack", get(doc::doc_data_tokio_zmc_msgpack))
            // ③ sqlx + ZmcDataSet(零拷贝) + msgpack 二进制
            .route("/doc/data/sqlx-zmc-msgpack", get(doc::doc_data_sqlx_zmc_msgpack))
            // ⑤ tokio-postgres + ZmcDataSet(零拷贝) + 纯 JSON 出口
            .route("/doc/data/tokio-zmc-json", get(doc::doc_data_tokio_zmc_json))
            // 业务单据**显示元数据**(层序/各层列 caption·类型/父子关系)——通用单据前端页动态建表用
            .route("/doc/meta", get(doc::doc_meta))
            .route("/doc/save", post(doc::doc_save))
            // 业务单据版本化（方案 §6A / Phase 8）
            .route("/doc/revisions", get(doc::doc_revisions))
            .route("/doc/revision", get(doc::doc_revision))
            .route("/doc/restore", post(doc::doc_restore))
            .route(
                "/definitions/default",
                post(handler::definitions_set_default),
            )
            // 模型中心：数据库初始化 + 模块部署（真实落库）
            .route("/model/db-state", get(handler::model_db_state))
            .route("/model/init", post(handler::model_init))
            .route(
                "/model/init-plan-stream",
                post(handler::model_init_plan_stream),
            )
            .route("/model/init-stream", post(handler::model_init_stream))
            .route("/model/deploy", post(handler::model_deploy))
            .route(
                "/model/deploy-plan-stream",
                post(handler::model_deploy_plan_stream),
            )
            .route("/model/deploy-stream", post(handler::model_deploy_stream))
            // 字典检索服务
            .route("/dict/_schemas", get(handler::dict_schemas))
            .route("/dict/_schema", post(handler::dict_register_schema))
            .route("/dict/multi-search", post(handler::dict_multi_search))
            .route("/dict/batch-data", post(handler::dict_batch_data))
            .route("/dict/{dictId}/search", post(handler::dict_search))
            .route("/dict/{dictId}/suggest", get(handler::dict_suggest))
            .route(
                "/dict/{dictId}/entries",
                post(handler::dict_upsert_entries).delete(handler::dict_clear_entries),
            )
            .route(
                "/dict/{dictId}/entries/{id}",
                axum::routing::delete(handler::dict_delete_entry),
            )
            .route("/dict/{dictId}/deactivate", post(handler::dict_deactivate))
            .route("/dict/{dictId}/supersede", post(handler::dict_supersede))
            // 弹性组合
            .route("/flexible-combination/list", get(handler::fc_list))
            .route(
                "/flexible-combination/config",
                get(handler::fc_get_config)
                    .post(handler::fc_save_config)
                    .delete(handler::fc_delete_config),
            )
            .route("/flexible-combination/resolve", get(handler::fc_resolve))
            .route("/flexible-combination/rule", get(handler::fc_rule))
            .route("/flexible-combination/validate", post(handler::fc_validate))
            .route("/flexible-combination/preview", post(handler::fc_preview))
            .route("/flexible-combination/default", post(handler::fc_set_default))
    }

    fn prefix() -> &'static str {
        "portal"
    }

    fn module_name(&self) -> &'static str {
        "portal"
    }
}
