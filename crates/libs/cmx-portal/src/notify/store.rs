//! 通知存储：`notification-center/<userId>/<center>/<file>.json`，一条通知一个文件。

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::config::data_path;
use crate::error::{PortalError, PortalResult};
use crate::fsutil::{read_json, write_json_atomic};
use crate::notify::hub::{self, NotifyEvent};
use crate::util::{is_safe_segment, write_lock};

/// 三个中心。值即落盘目录名；label 为前端默认显示名。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotifyCenter {
    Task,
    Message,
    Log,
}

impl NotifyCenter {
    pub fn as_str(self) -> &'static str {
        match self {
            NotifyCenter::Task => "task",
            NotifyCenter::Message => "message",
            NotifyCenter::Log => "log",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            NotifyCenter::Task => "任务中心",
            NotifyCenter::Message => "消息中心",
            NotifyCenter::Log => "日志中心",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "task" => Some(NotifyCenter::Task),
            "message" => Some(NotifyCenter::Message),
            "log" => Some(NotifyCenter::Log),
            _ => None,
        }
    }
    pub fn all() -> [NotifyCenter; 3] {
        [NotifyCenter::Task, NotifyCenter::Message, NotifyCenter::Log]
    }
}

/// 一条通知（落盘结构 = API 返回结构）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifyItem {
    pub id: String,
    pub center: String,
    pub title: String,
    #[serde(default)]
    pub body: String,
    /// 业务等级：info | success | warning | error。
    #[serde(default = "default_level")]
    pub level: String,
    /// 点击通知后可跳转/打开的目标（可选，前端按需用，如 help:/node: 等）。
    #[serde(default)]
    pub link: String,
    #[serde(default)]
    pub read: bool,
    /// 创建时间（epoch 毫秒）。
    #[serde(default)]
    pub created_at: i64,
}

fn default_level() -> String {
    "info".to_string()
}

/// 发布入参。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifyInput {
    /// 目标用户；缺省时由 handler 用当前登录用户回填。
    #[serde(default)]
    pub user_id: Option<String>,
    pub center: String,
    pub title: String,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub level: Option<String>,
    #[serde(default)]
    pub link: Option<String>,
}

/// 计数（前端角标用）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotifyCounts {
    pub task: i64,
    pub message: i64,
    pub log: i64,
    /// 三中心未读合计 = shellbar 红色数字。
    pub total: i64,
}

fn safe_user(user_id: &str) -> PortalResult<String> {
    let u = user_id.trim();
    if u.is_empty() {
        return Err(PortalError::bad_request("缺少用户标识"));
    }
    if !is_safe_segment(u) {
        return Err(PortalError::bad_request(format!("用户标识非法（仅允许字母数字 _-）：\"{user_id}\"")));
    }
    Ok(u.to_string())
}

fn center_dir(user_id: &str, center: NotifyCenter) -> std::path::PathBuf {
    data_path(["notification-center", user_id, center.as_str()])
}

fn item_path(user_id: &str, center: NotifyCenter, file: &str) -> std::path::PathBuf {
    data_path(["notification-center", user_id, center.as_str(), file])
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// 读取某用户某中心的全部通知（按 createdAt 倒序）。
async fn read_center(user_id: &str, center: NotifyCenter) -> PortalResult<Vec<NotifyItem>> {
    let dir = center_dir(user_id, center);
    let mut rd = match tokio::fs::read_dir(&dir).await {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(PortalError::Io(e)),
    };
    let mut out = Vec::new();
    while let Some(entry) = rd.next_entry().await.map_err(PortalError::Io)? {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') || !name.ends_with(".json") {
            continue;
        }
        match read_json::<NotifyItem>(&entry.path()).await {
            Ok(mut it) => {
                it.center = center.as_str().to_string(); // 以目录为准回填
                out.push(it);
            }
            Err(_) => continue, // 单条损坏不影响其余
        }
    }
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(out)
}

/// 列出某用户的通知。center=None 表示三中心全部。
pub async fn list(user_id: &str, center: Option<NotifyCenter>) -> PortalResult<Vec<NotifyItem>> {
    let u = safe_user(user_id)?;
    let mut out = Vec::new();
    match center {
        Some(c) => out.extend(read_center(&u, c).await?),
        None => {
            for c in NotifyCenter::all() {
                out.extend(read_center(&u, c).await?);
            }
            out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        }
    }
    Ok(out)
}

/// 计算某用户各中心未读数 + 合计。
pub async fn counts(user_id: &str) -> PortalResult<NotifyCounts> {
    let u = safe_user(user_id)?;
    let mut c = NotifyCounts { task: 0, message: 0, log: 0, total: 0 };
    for center in NotifyCenter::all() {
        let unread = read_center(&u, center).await?.iter().filter(|x| !x.read).count() as i64;
        match center {
            NotifyCenter::Task => c.task = unread,
            NotifyCenter::Message => c.message = unread,
            NotifyCenter::Log => c.log = unread,
        }
    }
    c.total = c.task + c.message + c.log;
    Ok(c)
}

/// 发布一条通知：落盘 + 广播 SSE（新通知事件 + 最新 counts 事件）。
pub async fn publish(input: NotifyInput) -> PortalResult<NotifyItem> {
    let user_id = safe_user(input.user_id.as_deref().unwrap_or(""))?;
    let center = NotifyCenter::parse(input.center.trim())
        .ok_or_else(|| PortalError::bad_request("center 仅支持 task/message/log"))?;
    let title = input.title.trim().to_string();
    if title.is_empty() {
        return Err(PortalError::bad_request("title 不能为空"));
    }
    let ts = now_millis();
    let id = format!("n_{}_{}", ts, std::process::id());
    let item = NotifyItem {
        id: id.clone(),
        center: center.as_str().to_string(),
        title,
        body: input.body.unwrap_or_default(),
        level: input.level.filter(|s| !s.trim().is_empty()).unwrap_or_else(default_level),
        link: input.link.unwrap_or_default(),
        read: false,
        created_at: ts,
    };

    {
        let _guard = write_lock().lock().await;
        write_json_atomic(&item_path(&user_id, center, &format!("{id}.json")), &item, true).await?;
    }

    // 广播：先发新通知，再发最新计数（前端据此更新列表与红色角标）。
    hub::publish_event(NotifyEvent {
        user_id: user_id.clone(),
        kind: "notify".to_string(),
        data: serde_json::to_value(&item).unwrap_or(serde_json::Value::Null),
    });
    if let Ok(c) = counts(&user_id).await {
        hub::publish_event(NotifyEvent {
            user_id: user_id.clone(),
            kind: "counts".to_string(),
            data: serde_json::to_value(&c).unwrap_or(serde_json::Value::Null),
        });
    }
    Ok(item)
}

/// 标记单条已读。返回是否发生变化。
pub async fn mark_read(user_id: &str, center: NotifyCenter, id: &str) -> PortalResult<bool> {
    let u = safe_user(user_id)?;
    if !is_safe_segment(&id.replace('.', "_")) && !id.starts_with("n_") {
        // id 形如 n_<ts>_<pid>；宽松校验避免穿越
    }
    let file = format!("{}.json", id.trim());
    if !crate::util::is_safe_json_file(&file) {
        return Err(PortalError::bad_request("通知 id 非法"));
    }
    let path = item_path(&u, center, &file);
    let _guard = write_lock().lock().await;
    let mut item = match read_json::<NotifyItem>(&path).await {
        Ok(it) => it,
        Err(PortalError::NotFound(_)) => return Err(PortalError::not_found("通知不存在")),
        Err(e) => return Err(e),
    };
    if item.read {
        return Ok(false);
    }
    item.read = true;
    write_json_atomic(&path, &item, true).await?;
    drop(_guard);
    broadcast_counts(&u).await;
    Ok(true)
}

/// 标记某用户全部（或某中心）已读。返回标记的条数。
pub async fn mark_all_read(user_id: &str, center: Option<NotifyCenter>) -> PortalResult<i64> {
    let u = safe_user(user_id)?;
    let centers: Vec<NotifyCenter> = match center {
        Some(c) => vec![c],
        None => NotifyCenter::all().to_vec(),
    };
    let mut n = 0i64;
    {
        let _guard = write_lock().lock().await;
        for c in centers {
            for item in read_center(&u, c).await? {
                if !item.read {
                    let mut it = item;
                    it.read = true;
                    write_json_atomic(&item_path(&u, c, &format!("{}.json", it.id)), &it, true).await?;
                    n += 1;
                }
            }
        }
    }
    if n > 0 {
        broadcast_counts(&u).await;
    }
    Ok(n)
}

/// 重新计算并广播某用户的 counts（标记已读后刷新角标）。
async fn broadcast_counts(user_id: &str) {
    if let Ok(c) = counts(user_id).await {
        hub::publish_event(NotifyEvent {
            user_id: user_id.to_string(),
            kind: "counts".to_string(),
            data: serde_json::to_value(&c).unwrap_or(serde_json::Value::Null),
        });
    }
}

/// 三中心元信息（前端下拉用：值/标签/图标）。静态注册。
pub fn centers_meta() -> serde_json::Value {
    json!({
        "centers": [
            { "id": "task", "label": "任务中心", "icon": "task" },
            { "id": "message", "label": "消息中心", "icon": "email" },
            { "id": "log", "label": "日志中心", "icon": "history" }
        ]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn publish_count_read_roundtrip() {
        let _env = crate::util::test_data_root_lock().lock().unwrap();
        let unique = format!("notify-it-{}-{}", std::process::id(), now_millis());
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target").join("test-data").join(unique);
        unsafe { std::env::set_var("CMX_PORTAL_DATA_ROOT", &root) };

        let uid = "u1";
        // 初始计数为 0
        let c0 = counts(uid).await.unwrap();
        assert_eq!(c0.total, 0);

        // 发布 2 条 task + 1 条 message
        for t in ["t1", "t2"] {
            publish(NotifyInput { user_id: Some(uid.into()), center: "task".into(), title: t.into(), body: None, level: None, link: None }).await.unwrap();
        }
        let m = publish(NotifyInput { user_id: Some(uid.into()), center: "message".into(), title: "m1".into(), body: None, level: None, link: None }).await.unwrap();

        let c1 = counts(uid).await.unwrap();
        assert_eq!((c1.task, c1.message, c1.log, c1.total), (2, 1, 0, 3), "未读计数");

        // 列表（全部）应有 3 条，倒序
        let all = list(uid, None).await.unwrap();
        assert_eq!(all.len(), 3);

        // 标记 message 这条已读 → 合计降到 2
        assert!(mark_read(uid, NotifyCenter::Message, &m.id).await.unwrap());
        let c2 = counts(uid).await.unwrap();
        assert_eq!((c2.message, c2.total), (0, 2));

        // 全部已读 → 0
        let n = mark_all_read(uid, None).await.unwrap();
        assert_eq!(n, 2, "剩余 2 条 task 未读被标记");
        assert_eq!(counts(uid).await.unwrap().total, 0);

        // 用户隔离：u2 看不到 u1 的通知
        assert_eq!(counts("u2").await.unwrap().total, 0);

        // 非法 center / 用户
        assert!(publish(NotifyInput { user_id: Some(uid.into()), center: "bad".into(), title: "x".into(), body: None, level: None, link: None }).await.is_err());
        assert!(counts("../etc").await.is_err());

        let _ = tokio::fs::remove_dir_all(&root).await;
    }
}
