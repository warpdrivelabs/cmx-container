/*
 * @Describe: cmx-flow 引擎单例（web-server 集成用）。
 *
 * 与 RPT 无状态模块的关键差异：Engine::deploy / set_resolver / set_subflow_router 都要 &mut self，
 * 引擎必须在启动时建好、装载完已发布定义、注入 resolver/router 后才能包 Arc 共享。故用
 * tokio::sync::OnceCell 存一个 FlowRuntime 单例，首次访问（get_or_try_init）时构建一次。
 *
 * db_id 对齐 web-server（dev-local.toml 注册）：
 *   FLOW_DB_ID="fico-db" —— 运行态 store + 定义 store（cmx_flow_* 表所在库）
 *   IAM_DB_ID ="primary" —— 候选人 resolver + 子流程 router（cmx_user/cmx_role/cmx_org 所在库）
 * 两库均已由 web-server init_datasources 注册进 cmx-database-pg 全局 manager，本 crate 不再注册。
 *
 * 生产不 seed demo 的 include_str! BPMN 夹具；定义经设计器 save_draft/publish 落库，启动装载。
 */

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::json;
use tokio::sync::{OnceCell, RwLock};

use cmx_flow_def::{DefinitionService, PgDefinitionStore};
use cmx_flow_engine::{DelegateContext, Engine, JavaDelegate, ProcessDefinition};
use cmx_flow_store_pg::{
    PgIamAssigneeResolver, PgRuntimeStore, PgSubflowBindingStore, PgSubflowRouter,
};

/// 运行态 store + 定义所在库（cmx_flow_* 表）。
pub const FLOW_DB_ID: &str = "fico-db";
/// IAM 所在库（候选人解析 + 子流程组织路由）。
pub const IAM_DB_ID: &str = "primary";

/// 流程运行态聚合：共享引擎 + 已装载定义（供前端画图）+ 定义服务（设计器草稿/发布）。
pub struct FlowRuntime {
    pub engine: Arc<Engine<PgRuntimeStore>>,
    /// 已装载定义列表。RwLock 便于发布后热追加（引擎本身不热部署，见下）。
    pub definitions: Arc<RwLock<Vec<ProcessDefinition>>>,
    pub def_svc: Arc<DefinitionService<PgDefinitionStore>>,
    /// 子流程组织绑定管理（设计态 CRUD + 组织树；与运行期 PgSubflowRouter 同表 IAM 库）。
    pub binding_store: Arc<PgSubflowBindingStore>,
}

static FLOW: OnceCell<FlowRuntime> = OnceCell::const_new();

/// 取全局流程运行时（首次调用构建：建表 + 注入 resolver/router + 装载已发布定义）。
pub async fn flow() -> cmx_api::Result<&'static FlowRuntime> {
    FLOW.get_or_try_init(build).await
}

/// 构建流程运行时（仅一次）。所有 &mut 调用在包 Arc 之前完成，规避 &mut/单例竞态。
async fn build() -> cmx_api::Result<FlowRuntime> {
    // 1) 运行态 store + 定义 store（fico-db），建表。
    let store = PgRuntimeStore::new(FLOW_DB_ID);
    store
        .ensure_schema()
        .await
        .map_err(|e| bridge(format!("流程运行态建表失败: {e}")))?;

    let def_svc = DefinitionService::new(PgDefinitionStore::new(FLOW_DB_ID));
    def_svc
        .ensure_schema()
        .await
        .map_err(|e| bridge(format!("流程定义建表失败: {e}")))?;

    // 2) 引擎：注入 delegate + 候选人 resolver + 子流程 router（primary 库 IAM）。
    let mut engine = Engine::new(store);
    engine.register_delegate("riskDelegate", RiskDelegate);
    engine.set_resolver(Arc::new(PgIamAssigneeResolver::new(IAM_DB_ID)));
    engine.set_subflow_router(Arc::new(PgSubflowRouter::new(IAM_DB_ID)));

    // 2b) 子流程绑定管理面（IAM 库）。生产库不由引擎 ensure_schema 覆盖，故此处兜底建表，
    //     补上历史缺口（原来绑定表只靠 demo 的 CREATE TABLE 种入，生产从未建）。
    let binding_store = PgSubflowBindingStore::new(IAM_DB_ID);
    if let Err(e) = binding_store.ensure_schema().await {
        tracing::warn!(error = %e, "子流程绑定表建表失败（组织路由配置将不可用）");
    }

    // 3) 装载库里已发布的定义（设计器产物）。编译失败项跳过不阻断整体启动。
    let mut definitions: Vec<ProcessDefinition> = Vec::new();
    match def_svc.load_published_definitions().await {
        Ok((loaded, errors)) => {
            for (k, e) in &errors {
                tracing::warn!(def = %k, error = %e, "已发布定义编译失败，跳过装载");
            }
            for def in loaded {
                tracing::info!(key = %def.key, "装载已发布流程定义");
                definitions.push(def.clone());
                if let Err(e) = engine.deploy(def) {
                    tracing::warn!(error = %e, "装载流程定义失败");
                }
            }
        }
        Err(e) => tracing::warn!(error = %e, "读取已发布定义失败"),
    }

    Ok(FlowRuntime {
        engine: Arc::new(engine),
        definitions: Arc::new(RwLock::new(definitions)),
        def_svc: Arc::new(def_svc),
        binding_store: Arc::new(binding_store),
    })
}

/// 启动后台定时器 poller：每 5 秒推进一次到期边界定时器（引擎不自带后台线程，宿主驱动）。
///
/// web-server 在 init_datasources 后调一次。顺带触发 OnceCell 构建（fail-fast 暴露 DB/schema 问题）。
pub async fn spawn_timer_poller() -> cmx_api::Result<()> {
    let rt = flow().await?;
    let engine = rt.engine.clone();
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(5));
        loop {
            ticker.tick().await;
            match engine.trigger_due_timers(100).await {
                Ok(fired) if !fired.is_empty() => {
                    for f in &fired {
                        tracing::info!(
                            instance = %f.instance_id,
                            boundary = %f.boundary_bpmn_id,
                            interrupting = f.cancel_activity,
                            "⏰ 流程定时器触发"
                        );
                    }
                }
                Ok(_) => {}
                Err(e) => tracing::warn!(error = %e, "流程定时器推进出错"),
            }
        }
    });
    tracing::info!("✅ 流程引擎已就绪（定时器 poller 已启动）");
    Ok(())
}

/// serviceTask delegate：按金额算风险等级写回变量（从 demo 移植）。
struct RiskDelegate;

#[async_trait]
impl JavaDelegate for RiskDelegate {
    async fn execute(&self, ctx: &mut DelegateContext<'_>) -> Result<(), String> {
        let amount = ctx
            .variables
            .get("amount")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let level = if amount > 50000.0 {
            "高"
        } else if amount > 10000.0 {
            "中"
        } else {
            "低"
        };
        ctx.variables.set("riskLevel", json!(level));
        Ok(())
    }
}

/// 把任意错误消息桥成 cmx_api::Error（同 RPT 的 BizError 桥）。
fn bridge(msg: String) -> cmx_api::Error {
    cmx_biz::BizError::business(msg).into()
}
