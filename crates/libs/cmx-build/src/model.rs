//! 构建作业模型（W1）。对应表 `cmx_plugin_build_job`。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 构建作业状态机。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BuildStatus {
    Queued,
    Building,
    Scanning,
    Signing,
    Deploying,
    Success,
    Failed,
}

impl BuildStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            BuildStatus::Queued => "queued",
            BuildStatus::Building => "building",
            BuildStatus::Scanning => "scanning",
            BuildStatus::Signing => "signing",
            BuildStatus::Deploying => "deploying",
            BuildStatus::Success => "success",
            BuildStatus::Failed => "failed",
        }
    }
    pub fn is_terminal(self) -> bool {
        matches!(self, BuildStatus::Success | BuildStatus::Failed)
    }
}

/// 构建请求（提交时）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildRequest {
    /// 工作区 id（定位插件工程源码目录）。
    pub workspace_id: String,
    /// 插件工程相对/绝对路径（cargo 项目根）。
    pub plugin_path: String,
    /// 目标三元组（默认 wasm32-wasip1）。
    #[serde(default = "default_target")]
    pub target: String,
    /// 编译特性（默认 ["extism"]）。
    #[serde(default = "default_features")]
    pub features: Vec<String>,
    /// profile（默认 release）。
    #[serde(default = "default_profile")]
    pub profile: String,
    /// 构建成功后是否自动串链 doc→签名→deploy。
    #[serde(default)]
    pub auto_publish: bool,
    #[serde(default)]
    pub tenant_id: Option<String>,
    #[serde(default)]
    pub submitted_by: Option<String>,
}

fn default_target() -> String {
    "wasm32-wasip1".to_string()
}
fn default_features() -> Vec<String> {
    vec!["extism".to_string()]
}
fn default_profile() -> String {
    "release".to_string()
}

/// 构建作业记录（落库）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildJob {
    pub id: String,
    pub workspace_id: String,
    #[serde(default)]
    pub plugin_id: Option<String>,
    #[serde(default)]
    pub tenant_id: Option<String>,
    pub status: BuildStatus,
    pub target: String,
    pub profile: String,
    #[serde(default)]
    pub wasm_path: Option<String>,
    #[serde(default)]
    pub artifact_zip_path: Option<String>,
    /// 内容哈希（wasm 产物），供幂等/版本对齐。
    #[serde(default)]
    pub rev: Option<String>,
    #[serde(default)]
    pub error_summary: Option<String>,
    #[serde(default)]
    pub submitted_by: Option<String>,
    pub submitted_at: DateTime<Utc>,
    #[serde(default)]
    pub finished_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub duration_ms: Option<i64>,
}

/// 构建产物（Builder 成功输出）。
#[derive(Debug, Clone)]
pub struct BuildArtifact {
    /// 产物 wasm 绝对路径。
    pub wasm_path: String,
    /// 内容哈希（十六进制）。
    pub rev: String,
    /// 编译日志尾部（截断）。
    pub log_tail: String,
}
