//! 通知中心 handler(任务/消息/日志 + SSE 主动推送 + 群发)。

use axum::Json;
use axum::extract::Query;

use crate::middleware::CmxSvrContext;
use crate::{ApiResp, Result};

/// 从认证上下文取当前用户 id(通知按用户隔离;纯服务身份无用户 id)。
fn notify_user_id(c: &cmx_core::model::service::context::SVRContext) -> Result<String> {
    c.auth_context
        .as_ref()
        .map(|a| a.user_id.clone())
        .filter(|u| !u.trim().is_empty())
        .ok_or_else(|| cmx_api_types::Error::unauthorized("未登录或无用户标识"))
}

/// 从认证上下文构建发布方身份(权限矩阵/服务身份判定用)。
fn publish_ctx(c: &cmx_core::model::service::context::SVRContext) -> cmx_portal::notify::PublishCtx {
    let a = c.auth_context.as_ref();
    cmx_portal::notify::PublishCtx {
        user_id: a.map(|x| x.user_id.clone()).unwrap_or_default(),
        username: a.map(|x| x.username.clone()).unwrap_or_default(),
        is_admin: a
            .map(|x| x.has_role("admin") || x.has_permission("system:all"))
            .unwrap_or(false),
        is_service: a.and_then(|x| x.auth_method.as_deref()) == Some("api_key"),
    }
}

/// `GET /api/notifications` 通知列表过滤 + keyset 分页参数。
#[derive(Debug, serde::Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct NotifyListQuery {
    /// 通知中心:task / message / log;缺省查全部中心。
    #[serde(default)]
    pub center: Option<String>,
    /// 业务类型过滤(如 mdm.dead_letter)。
    #[serde(default)]
    pub r#type: Option<String>,
    /// 等级过滤:info / success / warning / error。
    #[serde(default)]
    pub level: Option<String>,
    /// 已读状态过滤(true 仅已读 / false 仅未读);缺省全部。
    #[serde(default)]
    pub is_read: Option<bool>,
    /// 页大小(1..=200,缺省 50)。
    #[serde(default)]
    pub limit: Option<i64>,
    /// 分页游标(上一页响应的 nextCursor;首页不传)。
    #[serde(default)]
    pub cursor: Option<String>,
}

/// 通知中心元信息。
///
/// `GET /api/notifications/centers` —— 三中心(task / message / log)元信息,
/// 前端下拉用。
#[utoipa::path(
    get,
    path = "/api/notifications/centers",
    responses(
        (status = 200, description = "三中心元信息（id / 名称等）", body = ApiResp<serde_json::Value>)
    ),
    tag = "门户接口"
)]
pub async fn notify_centers(
    CmxSvrContext(_c): CmxSvrContext,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    Ok(Json(ApiResp::ok(cmx_portal::notify::store::centers_meta())))
}

/// 通知未读计数。
///
/// `GET /api/notifications/counts` —— 当前用户各中心未读数 + 合计(红色角标)。
#[utoipa::path(
    get,
    path = "/api/notifications/counts",
    responses(
        (status = 200, description = "各中心未读数 {task, message, log, total}", body = ApiResp<serde_json::Value>)
    ),
    tag = "门户接口"
)]
pub async fn notify_counts(
    CmxSvrContext(c): CmxSvrContext,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let uid = notify_user_id(&c)?;
    let counts = cmx_portal::notify::store::counts(&uid).await?;
    Ok(Json(ApiResp::ok(
        serde_json::to_value(counts).map_err(cmx_portal::PortalError::from)?,
    )))
}

/// 列出用户通知(过滤 + keyset 分页)。
///
/// `GET /api/notifications?center=&type=&level=&isRead=&limit=&cursor=` —— 当前用户
/// 通知列表(按用户隔离);响应 `{items, nextCursor, total(仅首页)}`;旧前端只读 items 兼容。
#[utoipa::path(
    get,
    path = "/api/notifications",
    params(NotifyListQuery),
    responses(
        (status = 200, description = "通知列表 {items, nextCursor, total}", body = ApiResp<serde_json::Value>)
    ),
    tag = "门户接口"
)]
pub async fn notify_list(
    CmxSvrContext(c): CmxSvrContext,
    Query(q): Query<NotifyListQuery>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let uid = notify_user_id(&c)?;
    let center = match q.center.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => Some(
            cmx_portal::notify::store::NotifyCenter::parse(s).ok_or_else(|| {
                cmx_api_types::Error::bad_request("center 仅支持 task/message/log")
            })?,
        ),
        None => None,
    };
    let filter = cmx_portal::notify::NotifyListFilter {
        center,
        msg_type: q.r#type,
        level: q.level,
        is_read: q.is_read,
        limit: q.limit,
        cursor: q.cursor,
    };
    let result = cmx_portal::notify::store::list(&uid, &filter).await?;
    Ok(Json(ApiResp::ok(
        serde_json::to_value(result).map_err(cmx_portal::PortalError::from)?,
    )))
}

/// 发布一条通知(跨服务推送门户用户消息的统一入口)。
///
/// `POST /api/notifications/publish` —— 后端 / 服务端 / 其它微服务主动推送经此入口
/// (服务身份携带 `X-API-Key`,显式给收件目标)。body:
///
/// ```json
/// {
///   "userId": "可选,旧单发契约(等价 targets.userIds)",
///   "center": "task | message | log",
///   "title": "通知标题",
///   "body": "正文（可选）",
///   "level": "info | success | warning | error（可选）",
///   "type": "业务类型(可选,默认 system,如 mdm.dead_letter)",
///   "link": "点击跳转目标（可选,node:<id> / menu:<key> / https URL)",
///   "source": "来源服务标识(服务代发时填,如 mdm)",
///   "aggKey": "聚合键(可选):同键同收件人 1h 窗口内合并计数",
///   "expireAt": "过期时间 epoch 毫秒(可选,缺省按保留期 90 天)",
///   "targets": {
///     "userIds": ["指定用户 id"],
///     "usernames": ["指定用户名"],
///     "orgIds": ["部门 id"], "includeChildren": false,
///     "roleCodes": ["角色 code"], "all": false
///   }
/// }
/// ```
///
/// 权限矩阵:用户身份群发(部门/角色/全员)或指定 >20 人需管理员(403);服务身份
/// (api_key)放行群发但必须显式给收件目标(空 → 400);目标均空回填当前登录用户;
/// 频率超限 429;解析后收件人为空 400。收件人 ≥ 阈值(默认 2000)转后台异步展开,
/// 立即返回主体(渐进可见)。
#[utoipa::path(
    post,
    path = "/api/notifications/publish",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "发布后的通知记录", body = ApiResp<serde_json::Value>)
    ),
    tag = "门户接口"
)]
pub async fn notify_publish(
    CmxSvrContext(c): CmxSvrContext,
    Json(input): Json<cmx_portal::notify::NotifyInput>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let ctx = publish_ctx(&c);
    let saved = cmx_portal::notify::store::publish(&ctx, input).await?;
    Ok(Json(ApiResp::ok(
        serde_json::to_value(saved).map_err(cmx_portal::PortalError::from)?,
    )))
}

/// `POST /api/notifications/mark-read` 标记已读入参。
///
/// `{center, id}` 标单条;`{all: true, center?}` 标全部。center 仅旧客户端兼容
/// (收件人模型下 notification_id + 本人已唯一定位)。
#[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NotifyMarkInput {
    /// 通知中心:task / message / log;仅兼容保留,标单条可不传。
    #[serde(default)]
    pub center: Option<String>,
    /// 通知 id;标单条时必填。
    #[serde(default)]
    pub id: Option<String>,
    /// true 时标记该中心(或全部中心)所有通知为已读。
    #[serde(default)]
    pub all: bool,
}

/// 标记通知已读。
///
/// `POST /api/notifications/mark-read` —— `{center, id}` 标单条;`{all: true, center?}`
/// 标全部;标单条缺 id 返回 400。
#[utoipa::path(
    post,
    path = "/api/notifications/mark-read",
    request_body = NotifyMarkInput,
    responses(
        (status = 200, description = "标单条返回 {changed}；标全部返回 {marked}", body = ApiResp<serde_json::Value>)
    ),
    tag = "门户接口"
)]
pub async fn notify_mark_read(
    CmxSvrContext(c): CmxSvrContext,
    Json(input): Json<NotifyMarkInput>,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    let uid = notify_user_id(&c)?;
    let center = match input
        .center
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(s) => Some(
            cmx_portal::notify::store::NotifyCenter::parse(s).ok_or_else(|| {
                cmx_api_types::Error::bad_request("center 仅支持 task/message/log")
            })?,
        ),
        None => None,
    };
    if input.all {
        let n = cmx_portal::notify::store::mark_all_read(&uid, center).await?;
        return Ok(Json(ApiResp::ok(serde_json::json!({ "marked": n }))));
    }
    let id = input
        .id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| cmx_api_types::Error::bad_request("标单条需提供 id"))?;
    let changed = cmx_portal::notify::store::mark_read(&uid, id).await?;
    Ok(Json(ApiResp::ok(serde_json::json!({ "changed": changed }))))
}

/// 订阅通知推送。
///
/// `GET /api/notifications/stream` —— SSE:服务端主动推送本用户的新通知与角标刷新;
/// 连接建立先推一次当前 counts 保证角标立刻准确。事件类型:`notify`(新通知)、
/// `counts`(角标刷新)、`fanout`(大群发提示,客户端收到后应拉取一次 counts)。
/// 浏览器用 fetch + 流读消费(携带 Authorization 头)。集群部署下事件经 Redis
/// pub/sub 跨实例广播(cmx:notify 频道),任意实例发布的消息所有连接都能收到。
#[utoipa::path(
    get,
    path = "/api/notifications/stream",
    responses(
        (status = 200, description = "SSE 事件流：连接时先发 counts，其后按通知事件类型推送", content_type = "text/event-stream")
    ),
    tag = "门户接口"
)]
pub async fn notify_stream(
    CmxSvrContext(c): CmxSvrContext,
) -> axum::response::Response {
    use axum::response::sse::{Event, KeepAlive};
    use axum::response::{IntoResponse, Sse};

    let uid = match notify_user_id(&c) {
        Ok(u) => u,
        Err(e) => return e.into_response(),
    };

    type SseItem = std::result::Result<Event, std::convert::Infallible>;
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<SseItem>();

    // 连接建立先推一次当前 counts,保证角标立刻准确(不必等下一次推送)。
    if let Ok(counts) = cmx_portal::notify::store::counts(&uid).await {
        let _ = tx.send(Ok(Event::default()
            .event("counts")
            .json_data(counts)
            .unwrap_or_default()));
    }

    // 订阅 broadcast:只转发属于本用户的事件;fanout 事件(大群发提示)对所有连接
    // 就地拉取一次本人 counts 下发。连接断开时该 task 自然结束。
    let mut sub = cmx_portal::notify::hub::subscribe();
    let uid_filter = uid.clone();
    tokio::spawn(async move {
        loop {
            match sub.recv().await {
                Ok(ev) => {
                    if ev.kind == "fanout" {
                        // 大群发:不逐人发事件,各连接按本人拉取最新 counts。
                        if let Ok(counts) = cmx_portal::notify::store::counts(&uid_filter).await {
                            let _ = tx.send(Ok(Event::default()
                                .event("counts")
                                .json_data(counts)
                                .unwrap_or_default()));
                        }
                        continue;
                    }
                    if ev.user_id != uid_filter {
                        continue;
                    }
                    let sent = tx.send(Ok(Event::default()
                        .event(&ev.kind)
                        .json_data(&ev.data)
                        .unwrap_or_default()));
                    if sent.is_err() {
                        break; // 客户端已断开
                    }
                }
                // 滞后丢消息:忽略,继续(计数以库为准,下次 counts 事件会纠正)。
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    let stream = futures::stream::unfold(
        rx,
        |mut rx| async move { rx.recv().await.map(|ev| (ev, rx)) },
    );
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}
