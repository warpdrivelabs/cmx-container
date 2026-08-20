//! db_state API：库门闸 + 模块发现 + cell 计算 + 记录组装。
//!
//! 从 lib.rs 拆出：原"四、对外 API"段的 db_state 部分 + 重构后的 §1-§5 函数族。
//!
//! # 扩展性指南
//!
//! 新增资源类型时只动 4 处：
//! 1. `Kind` enum 加变体
//! 2. `Kind::db_backed()` 或 `fs_backed()` 注册
//! 3. `Kind::matrix_kinds()` 注册
//! 4. `compute_cell_for_kind` match 加分支
//!
//! `db_state` 协调者**永远不需要修改**。

use serde_json::{Value, json};

use cmx_api_types::Result;

use crate::db_err;
use crate::ledger::{
    self, LedgerSchemaStatus, main_module_key,
};
use crate::seed_scanner;
use crate::META_VERSION;

/// 按 `(applied, latest, status)` 三元组判定单 cell 的 scenario。
///
/// # 判定规则（顺序敏感）
///
/// 1. `status == "failed"` → `"retry"`（最高优先级，失败任何状态下都要重试）
/// 2. `(None, Some(_))` → `"create"`（未装 + 有定义）
/// 3. `(None, None)` → `"none"`（无任何记录，纯空槽位）
/// 4. `(Some(_), None)` → `"current"`（已装但磁盘定义已删，不视为 drift）
/// 5. `(Some(a), Some(l))`：转 i64 比较
///    - `a < l` → `"upgrade"`（可升级）
///    - `a > l` → `"downgrade"`（降级，前端应警示）
///    - `a == l` 且 `status == "drift"` → `"drift"`
///    - `a == l` 且**两侧 checksum 均存在且不等** → `"drift"`（版本未变但内容漂移；
///      任一侧缺失（老台账未写 checksum）→ 保持 `"current"` 不误报）
///    - `a == l` 否则 → `"current"`
///
/// 解析失败时（version 非数字）回退为 `0`，因此 `"v1"` / `"1"` 解析为相同值，
/// 多数情况不会误判；只有一边能解析一边不能时才可能错位（极少）。
pub(crate) fn scenario_of(
    applied: Option<&str>,
    latest: Option<&str>,
    status: &str,
    applied_checksum: Option<&str>,
    latest_checksum: Option<&str>,
) -> &'static str {
    // 1) 失败优先：任何状态下 status="failed" 都重试
    if status == "failed" {
        return "retry";
    }
    match (applied, latest) {
        (None, Some(_)) => "create",
        (None, None) => "none",
        (Some(_), None) => "current",
        (Some(a), Some(l)) => {
            // 转 i64 比较（解析失败回退 0，多数情况下不会误判）
            let (na, nl) = (a.parse::<i64>().unwrap_or(0), l.parse::<i64>().unwrap_or(0));
            if na < nl {
                "upgrade"
            } else if na > nl {
                "downgrade"
            } else if status == "drift" {
                "drift"
            } else if let (Some(ac), Some(lc)) = (applied_checksum, latest_checksum) {
                // 版本号一致但内容不一致 → 内容漂移；任一侧缺失（老台账）不参与判定
                if ac != lc { "drift" } else { "current" }
            } else {
                "current"
            }
        }
    }
}

/// 计算 SEED/MENU 的 scenario（实时扫描文件 + 对比 cmx_model_module_kind.def_checksum）。
///
/// 与 `scenario_of`（DCT/DOC/RPT 走版本号对比）不同，SEED/MENU 没有语义版本概念，
/// 用文件聚合 SHA256 与库里的 `def_checksum` 对比判断 drift。
///
/// 字段策略：
/// - `version`/`applied`/`latest`：**给用户看的日期版本**（YYYY-MM-DD，取文件 mtime）
/// - `applied_checksum`/`latest_checksum`：**内部 drift 判断依据**（SHA256，hash 变了说明有更新）
///
/// 入参：
/// - `applied_kind_row`：来自 `read_modules` 的某模块下 `applied_modules[key]["seed"|"menu"]` 子对象；
///   无部署记录时传 `None`。需含 `status` 和 `def_checksum` 字段（旧库可能没有 def_checksum）。
///
/// 返回 `(scenario, cell_json)`。scenario 由调用方累计到 `counts`，cell_json 直接放入 db_state。
pub(crate) fn compute_seed_menu_cell(
    kind: &str,
    domain: &str,
    app: &str,
    module: &str,
    applied_kind_row: Option<&Value>,
) -> (&'static str, Value) {
    let files = if kind == "SEED" {
        seed_scanner::scan_seed_files(domain, app, module)
    } else {
        seed_scanner::scan_menu_files(domain, app, module)
    };
    let row_count: usize = files.iter().map(|f| f.row_count).sum();
    let latest_checksum = if files.is_empty() {
        None
    } else {
        Some(seed_scanner::aggregate_sha256(&files))
    };
    // 用户可见版本：取所有文件 mtime 中最新的日期（YYYY-MM-DD）
    // 同一天多次修改只算一个版本（语义上"今天有更新"）
    let latest_version = files
        .iter()
        .filter_map(|f| f.modified_date.as_deref())
        .max()
        .unwrap_or("");

    let status_str = applied_kind_row
        .and_then(|a| a.get("status"))
        .and_then(|v| v.as_str())
        .unwrap_or("none");
    let applied_checksum = applied_kind_row
        .and_then(|a| a.get("def_checksum"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty());
    // 已部署版本：来自 cmx_model_module_kind.version（部署时写入的当时日期）
    let applied_version = applied_kind_row
        .and_then(|a| a.get("version"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("");

    let scenario: &'static str = if status_str == "failed" {
        "retry"
    } else {
        match (applied_checksum, latest_checksum.as_deref()) {
            (None, None) => "none",
            (None, Some(_)) => "create",
            (Some(_), None) => "current", // 文件已删，但库里有部署记录：视为当前态（避免误报 drift）
            (Some(a), Some(l)) if a == l => "current",
            (Some(_), Some(_)) => "drift",
        }
    };

    let cell = json!({
        // 用户可见字段（与 DCT/DOC 的 cell 对齐：version/applied/latest 都是给人看的）
        "version": latest_version,
        "applied": applied_version,
        "latest": latest_version,
        "status": status_str,
        "scenario": scenario,
        "file": if kind == "SEED" { "seed/" } else { "menu-pages/" },
        "row_count": row_count,
        "table_count": files.len(),
        // 内部字段（drift 判断依据，前端不展示）
        "applied_checksum": applied_checksum,
        "latest_checksum": latest_checksum,
    });
    (scenario, cell)
}

/// 组合 db-state：库门闸 + 每模块每 kind scenario（真实读台账 + 定义列表）。
///
/// # 返回结构（顶层字段）
///
/// | 字段 | 类型 | 说明 |
/// |------|------|------|
/// | `db_id` | string | 数据库标识 |
/// | `initialized` | bool | 模型中心台账是否已初始化（`cmx_model_meta` 有记录） |
/// | `meta_version` | i32 | 当前台账版本号 |
/// | `expected_meta_version` | i32 | 代码期望版本号（`META_VERSION`），低于则需升级 |
/// | `db_status` | string | `"CURRENT"` / `"UNINITIALIZED"` / `"META_UPGRADE_REQUIRED"` |
/// | `page_mode` | string | 前端视图模式：`"normal"` / `"init"`（未初始化）/ `"meta_upgrade"`（需升级） |
/// | `scenario_counts` | object | 全模块 kind 格的场景统计：`{create, upgrade, current, retry, drift}` |
/// | `installed_modules` | array | **已安装模块**（在 `cmx_model_module` 有记录，至少装过一个 kind） |
/// - `modules` | array | **全部已定义模块 + menu-only 补全模块**（磁盘上有 DCT/DOC 定义的，或 `data/menu-pages/<d>/<a>/<m>/` 下有菜单文件的），含已装和未装 |
///
/// # `installed_modules` vs `modules`
///
/// - `installed_modules`：遍历 `cmx_model_module`（台账），每条代表"装过 ≥1 个 kind 的模块"。
///   - 注意：一个模块进此列表后，它**所有** kind（DCT/DOC/RPT/SEED/MENU）的 cell 都会带出，
///     未装的 kind cell `status="none"` / `applied=null` / `scenario="create"`。
///   - 前端"已创建模块"面板据此显示，并按 cell.scenario 过滤出真正装过的 kind。
/// - `modules`：遍历磁盘定义目录（`data/meta/definitions/<domain>/<app>/<module>/`），每条带
///   `installed: bool`（是否在 `cmx_model_module` 有记录）。此外对**仅靠 `data/menu-pages/`
///   反向发现**的模块（无 DCT/DOC 定义但有菜单文件）也补一条 minimal 条目，其
///   `_source="menu-only"`、dct/doc cell `scenario="none"`、menu/seed cell 走
///   `compute_seed_menu_cell` 正常计算（"只有菜单"的轻量入口模块也能进矩阵）。
///   - 前端"可创建 / 安装 / 升级模块"面板据此展示，列出所有待装（scenario=create）或可升级的格。
///
/// # 模块级字段（两个数组共有）
///
/// | 字段 | 说明 |
/// |------|------|
/// | `key` / `domain` / `application` / `module` | 模块四段式标识（`modules` 无 `key`） |
/// | `module_name` | 模块显示名（权威来源：主库 `cmx_module.name`，按 `code={domain}_{app}_{module}` 匹配；缺失回退 `module` 短 id） |
/// | `installed` | （仅 `modules`）是否已安装 |
/// | `table_count` | 物理表数（DCT+DOC 表数之和，SEED/MENU 不计） |
/// | `created_at` / `updated_at` | 首次部署 / 最近部署时间 |
/// | `deployed_by` / `deployed_name` | 部署人 id / 名 |
/// | `dct` / `doc` / `rpt` / `seed` / `menu` | 各 kind 的 cell（见下） |
///
/// # kind cell 字段（DCT/DOC/RPT）
///
/// | 字段 | 说明 |
/// |------|------|
/// | `applied` | 库中已应用版本（`cmx_model_module_kind.version`）；未装为 `null` |
/// | `latest` | 磁盘最新定义版本；无定义为 `null` |
/// | `status` | 部署状态：`current` / `none` / `failed` / `drift` |
/// | `scenario` | **场景**（前端决定动作）：`current`(无需动) / `create`(待装) / `upgrade`(可升级) / `retry`(失败重试) / `drift`(漂移重应用) / `downgrade` / `none` |
/// | `file` / `title` / `summary` | 最新定义的文件名 / 标题 / 摘要 |
/// | `table_count` | 该 kind 定义的表数 |
/// | `is_default` | 该版本是否默认版本 |
/// | `versions` | 历史版本数组（`{version,file,title,summary,table_count,is_default}`，降序） |
///
/// # kind cell 字段（SEED/MENU，无版本概念）
///
/// SEED/MENU 用文件 SHA256 + mtime 日期，不语义化为 v1/v2：
///
/// | 字段 | 说明 |
/// |------|------|
/// | `version` / `latest` | 磁盘最新文件的日期（`YYYY-MM-DD`，取所有文件 mtime 最大者） |
/// | `applied` | 库中已应用版本（部署时写入的当时日期）；未装为 `""` |
/// | `status` | 同上 |
/// | `scenario` | 同上；但 drift 判断改用 `applied_checksum != latest_checksum` |
/// | `row_count` | 种子总行数 / 菜单总节点数 |
/// | `table_count` | 种子文件数 / 菜单文件数 |
/// | `file` | 固定 `"seed/"` 或 `"menu-pages/"` |
/// | `applied_checksum` / `latest_checksum` | 内部 drift 判断依据（前端不展示） |
///
/// # scenario 判定规则
///
/// DCT/DOC/RPT（`scenario_of`）：失败→`retry`；未装+有定义→`create`；已装+无定义→`current`；
/// applied<latest→`upgrade`；applied>latest→`downgrade`；status=drift→`drift`；否则→`current`。
///
/// SEED/MENU（`compute_seed_menu_cell`）：失败→`retry`；否则按 checksum：无部署+有文件→`create`；
/// checksum 一致→`current`；不一致→`drift`。
pub async fn db_state(db_id: &str) -> Result<Value> {
    let meta = ledger::read_meta(db_id).await?;
    let initialized = meta.is_some();
    let meta_version = meta.as_ref().map(|m| m.meta_version).unwrap_or(0);
    if !initialized {
        return Ok(gate_state_uninitialized(db_id));
    }
    let schema = ledger::ledger_schema_status(db_id).await?;
    let needs_upgrade = meta_version < META_VERSION || schema.needs_upgrade;
    if needs_upgrade {
        return Ok(gate_state_meta_upgrade(db_id, meta_version, &schema));
    }
    // 2) 读台账 + 主库模块名
    let applied_modules = ledger::read_modules(db_id).await?;
    let main_names = ledger::read_main_module_names().await;

    // 3) 发现所有模块（DB 列表 + FS 补全）
    let db_items = collect_db_definitions().await?;
    let fs_keys = scan_fs_module_keys();
    let mut descriptors = discover_modules(db_items, fs_keys);

    // 3-b) 预填 latest 定义的规范化 checksum（drift 检测用）。
    // 复用 compile::read_def → definitions store 内存缓存（bump_generation 失效），
    // 缓存命中时无磁盘 IO；读文件失败保持 None（无 latest checksum 不参与 drift 判定）。
    for d in descriptors.iter_mut() {
        for kind in Kind::db_backed() {
            let Some(kd) = d.latest.get_mut(kind) else { continue };
            kd.checksum = crate::compile::read_def(&d.domain, &d.application, &d.module, &kd.file)
                .await
                .ok()
                .map(|doc| crate::checksum::normalized_def_checksum(&doc));
        }
    }

    // 4) 组装 modules 数组 + 累计 counts
    let mut counts = Counts::default();
    let modules: Vec<Value> = descriptors
        .iter()
        .map(|d| {
            let key = d.key();
            build_module_record(d, applied_modules.get(&key), &main_names, &mut counts)
        })
        .collect();

    // 5) 组装 installed_modules 数组（不累计 counts）
    // 预构建 key → descriptor 索引，避免 applied_modules 循环内 O(N) find + format! 分配
    let desc_by_key: std::collections::HashMap<String, &ModuleDescriptor> = descriptors
        .iter()
        .map(|d| (d.key(), d))
        .collect();
    let installed_modules: Vec<Value> = applied_modules
        .iter()
        .map(|(k, v)| {
            let defined = desc_by_key.get(k).copied();
            build_installed_record(k, v, defined, &main_names)
        })
        .collect();

    // 6) 顶层 JSON 拼装（与原行为一致：db_status="CURRENT", page_mode="normal"）
    Ok(json!({
        "db_id": db_id,
        "initialized": initialized,
        "meta_version": meta_version,
        "expected_meta_version": META_VERSION,
        "db_status": "CURRENT",
        "page_mode": "normal",
        "scenario_counts": counts.to_json(),
        "installed_modules": installed_modules,
        "modules": modules,
    }))
}

// =====================================================================
// §§ 重构后的辅助类型与函数族
//
//   db_state 协调者上方已就位。本块集中定义：① 类型 ② 库门闸 ③ 模块发现
//   ④ cell 计算 ⑤ 记录组装。新增资源类型时只动 4 处：
//     a) `Kind` enum 加变体
//     b) `Kind::db_backed()` 或 `fs_backed()` 注册
//     c) `Kind::matrix_kinds()` 注册（决定是否在 modules/installed_modules 中占 cell）
//     d) `compute_cell_for_kind` 的 match 加分支
//   db_state 协调者**永远不需要修改**。
// =====================================================================

// ---------------------------------------------------------------------
// §1 类型
// ---------------------------------------------------------------------

/// 资源类型枚举。新增资源类型时只动 4 处（见本块顶部注释）。
///
/// `#[allow(dead_code, clippy::upper_case_acronyms)]` 是因为 RPT 变体与 from_str 目前
/// 无调用方——这是"扩展点预留"，且 DCT/DOC/RPT/SEED/MENU 是领域标准缩写。
///
/// 变体语义：
/// - `DCT`：字典（data dictionary），业务用枚举/分类的载体
/// - `DOC`：单据（document），业务单据表（如凭证、报销单等）
/// - `RPT`：报表（report），报表落地的 3 张 `cr_*` 物理表（由 base_rpt_meta 共享）
/// - `SEED`：种子数据，业务表的初始行（如 cf_client 的初始客户清单）
/// - `MENU`：菜单，平台库 `cmx_module` 表的行
#[allow(dead_code, clippy::upper_case_acronyms)] // DCT/DOC/RPT/SEED/MENU 是领域标准缩写
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Kind {
    /// 字典（DCT）
    DCT,
    /// 单据（DOC）
    DOC,
    /// 报表（RPT）
    RPT,
    /// 种子数据（SEED）
    SEED,
    /// 菜单（MENU）
    MENU,
}

impl Kind {
    /// Kind → 字符串（与台账 DB 列 `kind` 的取值完全一致）。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::DCT => "DCT",
            Self::DOC => "DOC",
            Self::RPT => "RPT",
            Self::SEED => "SEED",
            Self::MENU => "MENU",
        }
    }

    /// 字符串 → Kind（大小写不敏感）。未识别的字符串返回 `None`。
    #[allow(dead_code)] // 扩展点预留：未来外部调用方可能用 Kind::from_str("DCT")
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_ascii_uppercase().as_str() {
            "DCT" => Some(Self::DCT),
            "DOC" => Some(Self::DOC),
            "RPT" => Some(Self::RPT),
            "SEED" => Some(Self::SEED),
            "MENU" => Some(Self::MENU),
            _ => None,
        }
    }

    /// 走 `list_definitions` 收集定义的 kind（DCT/DOC）。RPT 模板目前运行期未消费，
    /// 默认不进收集；放开时把 `Self::RPT` 加进数组即可。
    pub const fn db_backed() -> &'static [Kind] {
        &[Self::DCT, Self::DOC]
    }

    /// 走文件系统目录扫描发现模块位置的 kind（MENU）。SEED 与 DCT/DOC 同目录自然携带，
    /// 无需独立 enum。新增 FS 类资源（如 RPT 模板目录）时在这里注册。
    pub const fn fs_backed() -> &'static [Kind] {
        &[Self::MENU]
    }

    /// 在 `modules` 与 `installed_modules` 数组中需要算 cell 的 kind 集合。
    /// `db_state` 协调者按此数组循环算 cells。
    pub const fn matrix_kinds() -> &'static [Kind] {
        &[Self::DCT, Self::DOC, Self::SEED, Self::MENU]
    }
}

/// 单个 kind 的"定义版本"快照：来自 `list_definitions` 反序列化后的 `Value` 投影。
///
/// 仅保留 cell 计算所需的 6 个字段，避免在 `ModuleDescriptor.versions` 中持有原始 `Value`
/// 引发后续 `Value` 借用生命周期问题。
#[derive(Clone, Debug)]
struct KindDef {
    /// 版本号（i64：兼容 DCT/DOC 的语义版本；SEED/MENU 不会用此结构）
    ver: i64,
    /// 定义文件名（如 `cmxfico_dct_meta_v1.json`）
    file: String,
    /// 标题（前端"模块定义"列表展示用）
    title: String,
    /// 是否为默认版本（前端升级时优先指向 default）
    is_default: bool,
    /// 本版本产生的物理表数（DCT/DOC 编译出的 TableDefine 个数）
    tables: i64,
    /// 摘要（前端"模块定义"列表展示用）
    summary: String,
    /// 定义内容规范化 checksum（drift 检测用）。发现阶段不填（`None`），
    /// 由 `db_state` 协调者预填（读文件 + `normalized_def_checksum`）；
    /// 读文件失败保持 `None`（无 latest checksum 时不参与 drift 判定，不误报）。
    checksum: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ModuleSource {
    /// 来自 DCT/DOC 定义目录（标准模块）
    Defined,
    /// 仅靠 `data/menu-pages/<d>/<a>/<m>/` 反向发现（无 DCT/DOC 定义）
    MenuOnly,
}

/// 模块描述符：发现阶段的"中间表示"。所有模块来源（defined / menu-only）共用同一结构，
/// 唯一区别是 `source` 字段 + `latest/versions` 是否填充。组装阶段才区分 cell 计算路径。
#[derive(Clone, Debug)]
struct ModuleDescriptor {
    /// 领域编码（如 "fi"）
    domain: String,
    /// 应用编码（如 "cmxfico"）
    application: String,
    /// 模块编码（如 "report"）
    module: String,
    /// 每 kind 的"最新版本"（默认 / 最大版本）。仅 Defined 源会填充；MenuOnly 全空。
    latest: std::collections::HashMap<Kind, KindDef>,
    /// 每 kind 的"全部版本"列表。仅 Defined 源会填充。
    versions: std::collections::HashMap<Kind, Vec<KindDef>>,
    /// 模块来源（Defined 标准 / MenuOnly 仅菜单反向发现）
    source: ModuleSource,
}

impl ModuleDescriptor {
    /// 唯一标识：`{domain}/{application}/{module}`，与 `cmx_model_module` 的 DB 唯一索引对齐。
    fn key(&self) -> String {
        format!("{}/{}/{}", self.domain, self.application, self.module)
    }
    /// 构造一个"标准 Defined"模块描述符（来自 DCT/DOC 定义列表）。
    /// `latest` / `versions` 初始为空，由 `ingest_db_definitions` 填充。
    fn new_defined(
        domain: impl Into<String>,
        application: impl Into<String>,
        module: impl Into<String>,
    ) -> Self {
        Self {
            domain: domain.into(),
            application: application.into(),
            module: module.into(),
            latest: std::collections::HashMap::new(),
            versions: std::collections::HashMap::new(),
            source: ModuleSource::Defined,
        }
    }
    /// 构造一个"menu-only"模块描述符（无 DCT/DOC 定义，仅靠菜单文件反向发现）。
    /// `latest` / `versions` 始终空——menu-only 不会填充 kindDef。
    fn new_menu_only(
        domain: impl Into<String>,
        application: impl Into<String>,
        module: impl Into<String>,
    ) -> Self {
        Self {
            domain: domain.into(),
            application: application.into(),
            module: module.into(),
            latest: std::collections::HashMap::new(),
            versions: std::collections::HashMap::new(),
            source: ModuleSource::MenuOnly,
        }
    }
}

/// scenario 累计计数。仅统计 create/upgrade/current/retry/drift（与原行为一致）；
/// downgrade/none 增量被忽略（与原 `json!({"create": 0, "upgrade": 0, "current": 0, "retry": 0, "drift": 0})` 对齐）。
#[derive(Clone, Debug, Default)]
struct Counts {
    /// 待新建：模块的某 kind 库内无记录、磁盘有定义
    create: i64,
    /// 可升级：模块的某 kind 库内旧版本、磁盘有更新版本
    upgrade: i64,
    /// 当前态：模块的某 kind 库内版本与磁盘一致
    current: i64,
    /// 失败重试：上次部署 status='failed'，需重做
    retry: i64,
    /// 漂移：库内版本与磁盘定义等价但 checksum/字段不一致
    drift: i64,
}

impl Counts {
    /// 累加一个 scenario。downgrade / none 静默忽略（与原行为对齐）。
    fn bump(&mut self, sc: &str) {
        match sc {
            "create" => self.create += 1,
            "upgrade" => self.upgrade += 1,
            "current" => self.current += 1,
            "retry" => self.retry += 1,
            "drift" => self.drift += 1,
            _ => {} // 静默忽略 downgrade/none 等（与原 behavior 对齐）
        }
    }
    /// 序列化为前端 `scenario_counts` 字段。
    fn to_json(&self) -> Value {
        json!({
            "create": self.create,
            "upgrade": self.upgrade,
            "current": self.current,
            "retry": self.retry,
            "drift": self.drift,
        })
    }
}

// ---------------------------------------------------------------------
// §2 库门闸
// ---------------------------------------------------------------------

/// 库门闸：未初始化时早返回（仅返回 db_state 的"最小骨架"）。
///
/// 与 `db_state` 协调者**返回字段完全一致**（除 `db_status` / `page_mode` / `installed_modules` /
/// `modules` 全部置空），前端可在 `page_mode="init"` 模式下用同套字段渲染"初始化引导页"。
fn gate_state_uninitialized(db_id: &str) -> Value {
    json!({
        "db_id": db_id,
        "initialized": false,
        "meta_version": 0,
        "expected_meta_version": META_VERSION,
        "db_status": "UNINITIALIZED",
        "page_mode": "init",
        // 全 0 counts（与初始化前置状态语义对齐）
        "scenario_counts": Counts::default().to_json(),
        "installed_modules": [],
        "modules": [],
    })
}

/// 库门闸：需升级时早返回（meta_version 不匹配 或 台账对象缺失）。
///
/// 携带 `upgrade_reasons` / `missing_tables` 给前端展示"为什么需要升级"。
/// 同样不返回 `installed_modules` / `modules`（升级未完成前禁止业务操作）。
fn gate_state_meta_upgrade(db_id: &str, meta_version: i32, schema: &LedgerSchemaStatus) -> Value {
    json!({
        "db_id": db_id,
        "initialized": true,
        "meta_version": meta_version,
        "expected_meta_version": META_VERSION,
        "db_status": "META_UPGRADE_REQUIRED",
        "page_mode": "meta_upgrade",
        "upgrade_required": true,
        "upgrade_reasons": schema.reasons,
        "missing_tables": schema.missing_tables,
        "scenario_counts": Counts::default().to_json(),
        "installed_modules": [],
        "modules": [],
    })
}

// ---------------------------------------------------------------------
// §3 模块发现（统一入口）
// ---------------------------------------------------------------------

/// 收集所有"DB-backed" kind（DCT/DOC）的定义项。返回按 kind 分组的 Vec<Value>。
///
/// 走 `cmx_model_meta::definitions::store::list_definitions`（内存缓存 + 磁盘读），按 kind
/// 分别拉取定义清单。新增 DB 类资源时只需：
/// 1) `Kind` enum 加变体
/// 2) `Kind::db_backed()` 注册
async fn collect_db_definitions() -> Result<std::collections::HashMap<Kind, Vec<Value>>> {
    let mut out = std::collections::HashMap::new();
    for &kind in Kind::db_backed() {
        let items = cmx_model_meta::definitions::store::list_definitions(
            Some(kind.as_str()),
            None,
            None,
            None,
        )
        .await
        .map_err(db_err(&format!("列出 {} 失败", kind.as_str())))?;
        out.insert(kind, items);
    }
    Ok(out)
}

/// 扫描所有"FS-backed" kind（MENU）的模块位置。返回按 kind 分组的三段式目录列表。
///
/// 当前仅 MENU 走 FS（菜单 JSON 独立于 DCT/DOC 定义目录）。新增 FS 类资源
/// （如 RPT 模板目录）时在 match 里加分支。
fn scan_fs_module_keys() -> std::collections::HashMap<Kind, Vec<(String, String, String)>> {
    let mut out = std::collections::HashMap::new();
    for &kind in Kind::fs_backed() {
        let keys = match kind {
            // MENU: 枚举 data/menu-pages/ 下所有含 *.json 的 (domain, app, module)
            Kind::MENU => seed_scanner::scan_all_menu_module_keys(),
            // 当前无其它 FS 来源；新增 FS 类资源（如 RPT 模板目录）时在此 match 加分支
            _ => Vec::new(),
        };
        out.insert(kind, keys);
    }
    out
}

/// 单一发现入口：合并 DB 列表 + FS 枚举，生成模块描述符列表。
///
/// 去重规则：相同 `(domain, application, module)` 合并到同一描述符；
/// ① DB 列表先填（标记 `source=Defined`），② FS 列表的 menu-only 模块补全
/// 只在 `Defined` 源不存在时新增（不会覆盖已 Defined 的模块）。
///
/// 用 `BTreeMap` 临时存储保证结果按 key 字典序稳定（前端列表展示一致）。
fn discover_modules(
    db_items: std::collections::HashMap<Kind, Vec<Value>>,
    fs_keys: std::collections::HashMap<Kind, Vec<(String, String, String)>>,
) -> Vec<ModuleDescriptor> {
    // BTreeMap 临时存储 → 字典序稳定
    let mut by_key: std::collections::BTreeMap<String, ModuleDescriptor> =
        std::collections::BTreeMap::new();
    // 1) DB-backed kind（DCT/DOC）归并
    for (kind, items) in db_items {
        ingest_db_definitions(&mut by_key, kind, items);
    }
    // 2) FS-backed kind（MENU）枚举：仅补全"mods 里没有的模块"
    if let Some(menu_keys) = fs_keys.get(&Kind::MENU) {
        for (d, a, m) in menu_keys {
            let key = format!("{d}/{a}/{m}");
            // entry().or_insert_with：仅在该 key 还没条目时新建 menu-only
            by_key.entry(key).or_insert_with(|| {
                ModuleDescriptor::new_menu_only(d.clone(), a.clone(), m.clone())
            });
        }
    }
    // 收集 values，丢弃临时 BTreeMap 拥有权
    by_key.into_values().collect()
}

/// 把 `list_definitions` 返回的某 kind 一组定义项归并进 `by_key`。
///
/// 同时维护 `versions[]` 全量版本与 `latest`（按"默认版优先 → 同状态取较大 version"规则选一）。
///
/// # 容错
///
/// - `domain` / `application` / `module` / `file` 任一缺失或空串 → 跳过该项
/// - 其它字段（version/isDefault/tableCount/title/summary）缺失走默认（version=1, isDefault=false, ...）
fn ingest_db_definitions(
    by_key: &mut std::collections::BTreeMap<String, ModuleDescriptor>,
    kind: Kind,
    items: Vec<Value>,
) {
    for it in items {
        // 4 个必填字段任一缺失/空 → 跳过（不可用定义）
        let domain = match it.get("domain").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => continue,
        };
        // app 与 application 兼容：优先 application 字段，老定义可能用 app
        let app = match it
            .get("application")
            .or_else(|| it.get("app"))
            .and_then(|v| v.as_str())
        {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => continue,
        };
        let module = match it.get("module").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => continue,
        };
        let file = match it.get("file").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => continue, // 无 file 视为不可用定义
        };
        // 可选字段走默认（version=1, isDefault=false, tableCount=0）
        let ver = it.get("version").and_then(|v| v.as_i64()).unwrap_or(1);
        let is_def = it.get("isDefault").and_then(|v| v.as_bool()).unwrap_or(false);
        // 投影到 KindDef（仅保留 cell 计算需要的 6 个字段）
        let kd = KindDef {
            ver,
            file,
            title: it.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            is_default: is_def,
            tables: it.get("tableCount").and_then(|v| v.as_i64()).unwrap_or(0),
            // summary 与 details 兼容（部分老定义用 details）
            summary: it
                .get("summary")
                .or_else(|| it.get("details"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            // checksum 由 db_state 协调者预填（需读文件内容，本函数为同步投影阶段）
            checksum: None,
        };
        let key = format!("{domain}/{app}/{module}");
        // entry().or_insert_with：仅在该 key 还没条目时新建 Defined 描述符
        let entry = by_key
            .entry(key)
            .or_insert_with(|| ModuleDescriptor::new_defined(&domain, &app, &module));
        // 1) 全部版本：追加到 versions[kind]
        entry.versions.entry(kind).or_default().push(kd.clone());
        // 2) 选 latest：默认版优先；同 default 状态取较大 version
        match entry.latest.get(&kind) {
            // (a) 之前无 latest → 直接放
            None => {
                entry.latest.insert(kind, kd);
            }
            // (b) 旧 latest.file 为空（理论上的"未建模"占位）→ 替换
            Some(cur) if cur.file.is_empty() => {
                entry.latest.insert(kind, kd);
            }
            // (c) 旧非默认 + 新是默认 → 默认版优先
            Some(cur) if is_def && !cur.is_default => {
                entry.latest.insert(kind, kd);
            }
            // (d) 同 default 状态 + 新 version 更大 → 升级
            Some(cur) if is_def == cur.is_default && ver > cur.ver => {
                entry.latest.insert(kind, kd);
            }
            // (e) 其它：保留旧的
            Some(_) => {} // 保留旧的
        }
    }
}

// ---------------------------------------------------------------------
// §4 cell 计算（按 kind 派发）
// ---------------------------------------------------------------------

/// 从 `applied_modules`（Value）抽某 kind 的 `(version, status)`。
///
/// `applied_modules` 结构：`{ "dct": {"version": "v1", "status": "current", ...}, "doc": {...}, ... }`
/// - `version` 缺失 / 非字符串 → `None`（用于 scenario_of 的"未装"路径）
/// - `status` 缺失 / 非字符串 → 默认 `"none"`（与历史台账行为对齐）
fn extract_applied_status(applied: Option<&Value>, kind: Kind) -> (Option<String>, String) {
    // kind 转小写后作 key（DCT → "dct"，DOC → "doc" 等）
    let key = kind.as_str().to_ascii_lowercase();
    match applied.and_then(|a| a.get(key.as_str())) {
        Some(k) => (
            k.get("version").and_then(|v| v.as_str()).map(String::from),
            k.get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("none")
                .to_string(),
        ),
        // applied 缺失或该 kind 无子项 → 视为"未装"（scenario_of 会回退 "none" / "create"）
        None => (None, "none".to_string()),
    }
}

/// 安全取 cell.scenario：缺失 / 非字符串默认 `"none"`（避免下游 panic）。
fn cell_scenario(cell: &Value) -> &str {
    cell.get("scenario")
        .and_then(|v| v.as_str())
        .unwrap_or("none")
}

/// 单一 cell 计算入口：按 kind 派发到不同的计算器。
///
/// **新增资源类型时**在此 match 加分支即可。`db_state` 协调者不需要修改。
fn compute_cell_for_kind(kind: Kind, desc: &ModuleDescriptor, applied: Option<&Value>) -> Value {
    match kind {
        // 版本型 cell（DCT/DOC/RPT）：按语义版本号对比 applied vs latest
        Kind::DCT | Kind::DOC | Kind::RPT => compute_versioned_cell(kind, desc, applied),
        // 文件型 cell（SEED/MENU）：按文件 checksum + mtime 对比
        Kind::SEED | Kind::MENU => {
            let applied_kind = applied.and_then(|a| {
                a.get(kind.as_str().to_ascii_lowercase().as_str())
            });
            // compute_seed_menu_cell 返回 (scenario, cell_json)；这里只取 cell
            compute_seed_menu_cell(
                kind.as_str(),
                &desc.domain,
                &desc.application,
                &desc.module,
                applied_kind,
            )
            .1
        }
    }
}

/// 版本型 cell（DCT/DOC/RPT）。
///
/// 流程：
/// 1. 从 `desc.latest[kind]` 取"最新版本"快照；缺失时 latest=null
/// 2. 从 `desc.versions[kind]` 取全部版本列表，按版本号倒序（前端展示）
/// 3. 抽 `(applied_version, status)` 走 `scenario_of` 算 scenario
/// 4. 拼接 cell JSON
fn compute_versioned_cell(kind: Kind, desc: &ModuleDescriptor, applied: Option<&Value>) -> Value {
    let def = desc.latest.get(&kind);
    // 全部版本按 ver 倒序（最新在前，前端"历史版本"列表）
    let mut vers = desc
        .versions
        .get(&kind)
        .cloned()
        .unwrap_or_default();
    vers.sort_by_key(|b| std::cmp::Reverse(b.ver));
    let latest = def.map(|k| k.ver.to_string());
    // applied status（与库内 cmx_model_module_kind 行对齐）
    let (app_ver, status) = extract_applied_status(applied, kind);
    // 双侧 checksum（drift 判定依据）：
    // - applied 侧：台账 cmx_model_module_kind.def_checksum（read_modules 已透出；老台账可能没有）
    // - latest 侧：db_state 协调者预填的 KindDef.checksum（读文件失败为 None）
    let kind_key = kind.as_str().to_ascii_lowercase();
    let app_cs = applied
        .and_then(|a| a.get(kind_key.as_str()))
        .and_then(|k| k.get("def_checksum"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(String::from);
    let latest_cs = def.and_then(|k| k.checksum.clone());
    // 算 scenario：失败→retry；未装→create；版本对比→upgrade / downgrade / current / drift
    let scenario = scenario_of(
        app_ver.as_deref(),
        latest.as_deref(),
        &status,
        app_cs.as_deref(),
        latest_cs.as_deref(),
    );
    // 历史版本 JSON 列表（ver 倒序）
    let versions_json: Vec<Value> = vers
        .iter()
        .map(|v| {
            json!({
                "version": v.ver.to_string(),
                "file": v.file,
                "title": v.title,
                "summary": v.summary,
                "table_count": v.tables,
                "is_default": v.is_default,
            })
        })
        .collect();
    json!({
        "applied": app_ver,
        "latest": latest,
        "status": status,
        "scenario": scenario,
        "file": def.map(|k| k.file.clone()).unwrap_or_default(),
        "title": def.map(|k| k.title.clone()).unwrap_or_default(),
        "summary": def.map(|k| k.summary.clone()).unwrap_or_default(),
        "table_count": def.map(|k| k.tables).unwrap_or(0),
        "is_default": def.map(|k| k.is_default).unwrap_or(false),
        "versions": versions_json,
    })
}

/// 兜底 cell：menu-only 模块的 DCT/DOC 槽位（latest=null），
/// 或 installed_modules 数组里"无 defined 描述符"的模块。
///
/// 字段全部默认 / 空：前端能区分"无 DCT/DOC 定义"和"已装 DCT/DOC 但 latest=null"。
fn compute_none_cell(applied: Option<&Value>) -> Value {
    // applied 走 DCT 槽位（其它 kind 共享同结构）
    let (app_ver, status) = extract_applied_status(applied, Kind::DCT);
    // latest=None（未建模）；scenario 落到 "current"（若已装）或 "none"（未装）
    // （无磁盘定义 → 无 latest checksum，drift 判定不参与）
    let scenario = scenario_of(app_ver.as_deref(), None, &status, None, None);
    json!({
        "applied": app_ver,
        "latest": Value::Null,
        "status": status,
        "scenario": scenario,
        "file": "",
        "title": "",
        "summary": "",
        "table_count": 0,
        "is_default": false,
        "versions": [],
    })
}

// ---------------------------------------------------------------------
// §5 记录组装
// ---------------------------------------------------------------------

/// 按 matrix_kinds 循环计算 DCT/DOC/SEED/MENU 四个 cell。
///
/// # 派发规则
///
/// - `(DCT|DOC, MenuOnly)`：模块无 DCT/DOC 定义但有菜单，走 `compute_none_cell`
///   （latest=null，给前端一个"该槽位空"的明确信号）
/// - 其它：走 `compute_cell_for_kind` 派发到版本型 / 文件型计算器
///
/// # 矩阵内 kind 位置
///
/// 返回值按 `(dct, doc, seed, menu)` 顺序；RPT 在 matrix_kinds 外因此不进结果。
fn compute_four_cells(
    desc: &ModuleDescriptor,
    applied: Option<&Value>,
) -> (Value, Value, Value, Value) {
    let mut dct = Value::Null;
    let mut doc = Value::Null;
    let mut seed = Value::Null;
    let mut menu = Value::Null;
    for &kind in Kind::matrix_kinds() {
        let cell = match (kind, desc.source) {
            // menu-only 模块的 DCT/DOC：latest=null，走 compute_none_cell
            (Kind::DCT | Kind::DOC, ModuleSource::MenuOnly) => {
                let applied_kind =
                    applied.and_then(|a| a.get(kind.as_str().to_ascii_lowercase().as_str()));
                compute_none_cell(applied_kind)
            }
            // 其它：版本型 / 文件型
            (kind, _) => compute_cell_for_kind(kind, desc, applied),
        };
        // 写入返回值的对应槽位
        match kind {
            Kind::DCT => dct = cell,
            Kind::DOC => doc = cell,
            Kind::SEED => seed = cell,
            Kind::MENU => menu = cell,
            // RPT 不在 matrix_kinds 里——防御性兜底，避免编译期 exhaustive 警告
            Kind::RPT => {}
        }
    }
    (dct, doc, seed, menu)
}

/// 组装 modules[i]：按 matrix_kinds 循环算 cell；累计 counts。
///
/// modules 数组面向"前端可创建 / 安装 / 升级模块"面板，每条都带 installed 标志。
///
/// 字段策略：
/// - `table_count` = 所有 kind latest.tables 之和（仅 Defined 模块会累加；MenuOnly 始终 0）
/// - `module_name` 优先从主库 `cmx_module.name` 查（按 `code={domain}_{app}_{module}` 匹配），
///   缺失回退到 module 短 id（与原行为对齐）
/// - `created_at` / `updated_at` / `deployed_*` 直接从 applied 透传
fn build_module_record(
    desc: &ModuleDescriptor,
    applied: Option<&Value>,
    main_names: &std::collections::HashMap<String, String>,
    counts: &mut Counts,
) -> Value {
    let key = desc.key();
    // applied 缺失时用空对象（避免 .get() 链式 None 报错）
    let app_obj = applied.cloned().unwrap_or_else(|| json!({}));
    let (dct_cell, doc_cell, seed_cell, menu_cell) = compute_four_cells(desc, applied);
    // 累计 counts：4 个 cell 每个都算（与原"对每种 kind 累加"对齐）
    for cell in [&dct_cell, &doc_cell, &seed_cell, &menu_cell] {
        counts.bump(cell_scenario(cell));
    }
    // table_count = 所有 kind latest.tables 之和（仅 Defined 模块会累加；MenuOnly 始终 0）
    let tblc: i64 = desc.latest.values().map(|k| k.tables).sum();
    json!({
        "key": key,
        "domain": desc.domain,
        "application": desc.application,
        "module": desc.module,
        // module_name 权威来源：主库 cmx_module.name；缺失回退 module 短 id
        "module_name": main_names
            .get(&main_module_key(&desc.domain, &desc.application, &desc.module))
            .cloned()
            .unwrap_or_else(|| desc.module.clone()),
        "installed": applied.is_some(),
        "created_at": app_obj.get("first_deployed_at").and_then(|v| v.as_str()),
        "updated_at": app_obj
            .get("current_deployed_at")
            .and_then(|v| v.as_str())
            .or_else(|| app_obj.get("update_time").and_then(|v| v.as_str())),
        "deployed_by": app_obj.get("deployed_by").and_then(|v| v.as_str()),
        "deployed_name": app_obj.get("deployed_name").and_then(|v| v.as_str()),
        "table_count": tblc,
        "source": match desc.source {
            ModuleSource::Defined => "defined",
            ModuleSource::MenuOnly => "menu-only",
        },
        "dct": dct_cell,
        "doc": doc_cell,
        "seed": seed_cell,
        "menu": menu_cell,
    })
}

/// 组装 installed_modules[i]：兜底（台账里有但 discover 没发现的也走这里）。
///
/// **不累计 counts**（与原行为一致）。
///
/// 适用场景：
/// - 台账有记录但磁盘定义已删除（理论上的孤儿，DB 仍保留一行）
/// - 模块在主库有 `cmx_module` 注册但未在 `data/meta/definitions` 下放定义
///
/// `table_count` 直接取台账（不再求 latest.tables 之和），保证"已装"面板反映 DB 真实态。
fn build_installed_record(
    key: &str,
    applied: &Value,
    defined: Option<&ModuleDescriptor>,
    main_names: &std::collections::HashMap<String, String>,
) -> Value {
    // 拆 key：期望 `domain/application/module` 三段
    let parts: Vec<&str> = key.split('/').collect();
    // 优先取 applied 里的 domain/app/module（更权威），缺失回退 key 拆段
    let domain = applied
        .get("domain")
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| parts.first().copied().unwrap_or(""))
        .to_string();
    let app = applied
        .get("application")
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| parts.get(1).copied().unwrap_or(""))
        .to_string();
    let module = applied
        .get("module")
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| parts.get(2).copied().unwrap_or(""))
        .to_string();

    // 若 defined 缺失，构造一个"虚拟 Defined 描述符"（latest 全空），使 compute_cell_for_kind
    // 能正常取到 desc.domain/app/module 而不崩——其 latest=null 会让 versioned cell 走到空值路径。
    let virtual_desc;
    let desc_ref = if let Some(d) = defined {
        d
    } else {
        virtual_desc = ModuleDescriptor::new_defined(&domain, &app, &module);
        &virtual_desc
    };

    let (dct_cell, doc_cell, seed_cell, menu_cell) = compute_four_cells(desc_ref, Some(applied));
    json!({
        "key": key,
        "domain": domain,
        "application": app,
        "module": module,
        "module_name": main_names
            .get(&main_module_key(&domain, &app, &module))
            .cloned()
            .unwrap_or_else(|| module.clone()),
        "table_count": applied.get("table_count").and_then(|v| v.as_i64()).unwrap_or(0),
        "created_at": applied
            .get("first_deployed_at")
            .and_then(|v| v.as_str())
            .or_else(|| applied.get("create_time").and_then(|v| v.as_str())),
        "updated_at": applied
            .get("current_deployed_at")
            .and_then(|v| v.as_str())
            .or_else(|| applied.get("update_time").and_then(|v| v.as_str())),
        "deployed_by": applied.get("deployed_by").and_then(|v| v.as_str()),
        "deployed_name": applied.get("deployed_name").and_then(|v| v.as_str()),
        "dct": dct_cell,
        "doc": doc_cell,
        "seed": seed_cell,
        "menu": menu_cell,
    })
}

#[cfg(test)]
mod scenario_tests {
    use super::scenario_of;

    #[test]
    fn version_equal_checksum_drift() {
        // 版本一致 + 两侧 checksum 都在且不等 → drift（本次新增的内容漂移检测）
        assert_eq!(scenario_of(Some("3"), Some("3"), "current", Some("aaa"), Some("bbb")), "drift");
        // checksum 一致 → current
        assert_eq!(scenario_of(Some("3"), Some("3"), "current", Some("aaa"), Some("aaa")), "current");
    }

    #[test]
    fn version_equal_missing_checksum_stays_current() {
        // 老台账兼容：任一侧 checksum 缺失 → 不参与判定，保持 current（不误报 drift）
        assert_eq!(scenario_of(Some("3"), Some("3"), "current", None, Some("bbb")), "current");
        assert_eq!(scenario_of(Some("3"), Some("3"), "current", Some("aaa"), None), "current");
        assert_eq!(scenario_of(Some("3"), Some("3"), "current", None, None), "current");
    }

    #[test]
    fn version_compare_takes_precedence() {
        // 版本不相等时 checksum 不参与（upgrade / downgrade / create / retry 优先级更高）
        assert_eq!(scenario_of(Some("2"), Some("3"), "current", Some("a"), Some("b")), "upgrade");
        assert_eq!(scenario_of(Some("3"), Some("2"), "current", Some("a"), Some("b")), "downgrade");
        assert_eq!(scenario_of(None, Some("3"), "none", None, Some("b")), "create");
        assert_eq!(scenario_of(Some("3"), Some("3"), "failed", Some("a"), Some("b")), "retry");
    }
}
