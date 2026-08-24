//! 页面投递配置：目录解析（统一 `[assets]` 段）与 html 投递开关。
//!
//! ## 页面资产目录规范（v2，合并单体友好）
//!
//! ```text
//! web/ui-native/
//! ├── index.json              # 唯一索引 {"version","pages":[...]}；relPath 相对本文件
//! ├── <svc>/                  # 服务自有页（rule/ rpt/ flow/ portal/mdm/ ...）
//! ├── <domain>/               # 跨服务共享业务域页（如 fi/，与页面 id 域前缀一致）
//! └── vendor/                 # 跨服务共享构建产物
//! web/ui-html/
//! ├── index.json              # v2 manifest {"domains":[...]}
//! ├── index/<domain>.pages.json  # 域分片；行内 relPath 相对 manifest 所在目录（ui-html 根）
//! └── <domain>/<svc>/...      # 源文件同样相对 ui-html 根
//! ```
//!
//! 合并单体时：把各服务的 `<svc>/`（及其引用的 `<domain>/`、`vendor/`）拷入目标根，
//! 合并各 `index.json` 的 `pages` 数组（html 合并 `domains` 清单）重新生成索引即可；
//! 加载器不感知任何布局约定。

use std::path::PathBuf;

/// html 投递开关。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HtmlLayout {
    /// 不注册 `/html-pages` 三端点（无 html 页资产的服务，如 rule）。
    Disabled,
    /// v2 分片索引：manifest `index.json`（`{domains:[]}`）+ 分片 `index/<domain>.pages.json`。
    ShardedV2,
}

/// 页面只读投递配置。
#[derive(Debug, Clone)]
pub struct PageServeConfig {
    /// native 页资产根（index.json 所在目录；relPath 相对此目录解析）。
    pub native_dir: PathBuf,
    /// html 页资产根（manifest index.json 所在目录；分片行 relPath 相对此目录解析）。
    pub html_dir: PathBuf,
    /// html 投递开关。
    pub html: HtmlLayout,
}

impl PageServeConfig {
    /// 按统一 `[assets]` 段解析目录并构造默认配置（ShardedV2 开启）。
    ///
    /// 目录解析顺序（收编自五份副本逐字相同的 assets_dir 逻辑）：
    /// ConfigManager（toml ← env 合并）→ env 直读兜底 → 默认值。
    ///
    /// # Returns
    ///
    /// 返回以 `assets.ui_native_dir` / `assets.ui_html_dir`（默认 `web/ui-native` /
    /// `web/ui-html`，相对服务 cwd）为目录的配置。
    pub fn from_assets() -> Self {
        Self {
            native_dir: assets_dir(
                "assets.ui_native_dir",
                "ASSETS__UI_NATIVE_DIR",
                "web/ui-native",
            ),
            html_dir: assets_dir("assets.ui_html_dir", "ASSETS__UI_HTML_DIR", "web/ui-html"),
            html: HtmlLayout::ShardedV2,
        }
    }
}

/// 解析单个资产目录：ConfigManager → env 直读兜底 → 默认值。
fn assets_dir(cfg_key: &str, env_key: &str, default: &str) -> PathBuf {
    if let Some(cm) = cmx_utils::ConfigManager::try_global()
        && let Ok(v) = cm.get_string(cfg_key)
    {
        let v = v.trim();
        if !v.is_empty() {
            return PathBuf::from(v);
        }
    }
    if let Ok(v) = std::env::var(env_key) {
        let v = v.trim();
        if !v.is_empty() {
            return PathBuf::from(v);
        }
    }
    PathBuf::from(default)
}
