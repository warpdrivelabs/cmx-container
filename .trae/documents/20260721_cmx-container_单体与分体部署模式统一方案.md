# CMX-Container 单体与分体部署模式统一方案

> **方案日期**：2026-07-21
> **方案类型**：架构梳理 + 最小改动落地
> **依据文档**：
> - [docs/deployment-mode-review.md](file:///media/yqs/工作/rustspace/cmx/cmx-container/docs/deployment-mode-review.md)（2026-07-03 评审报告 B-4/C-3 建议）
> - [dev.toml](file:///media/yqs/工作/rustspace/cmx/cmx-container/dev.toml) `[app]` 配置段
> - [crates/web/web-server/src/config/datasource.rs](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/web/web-server/src/config/datasource.rs)
> - [crates/libs/cmx-utils/src/config/config_impl.rs:348-379](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/libs/cmx-utils/src/config/config_impl.rs#L348-L379)（`get_app_id`）
> - [crates/libs/cmx-plugin/src/service/module_install.rs:100-110](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/libs/cmx-plugin/src/service/module_install.rs#L100-L110)（导入守卫）

---

## 一、摘要

当前 `[app] domain_code/application_code/module_code` 三元组同时承担了"本实例身份"、"数据源过滤条件"、"插件/服务隔离键（app_id）"三重职责，导致**单体模式下无法表达"加载全部资源"的语义**，产生悖论：

- 配置具体值（如 `fi/cmxfico/gl`）→ `get_app_id()` 返回 `"gl"`，`cmx_plugin`/`cmx_service_define`/`cmx_model_*` 等表只能看到 `app_id='gl'` 的数据；其他模块（ap/ar/fa）被忽略
- 配置 `default/default/default` → 业务数据源、业务插件全部过滤不到（因为它们不打 `default` 标）
- `module_install` 导入守卫（[module_install.rs:105](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/libs/cmx-plugin/src/service/module_install.rs#L105)）强制 `manifest.module.code == get_app_id()`，mono 模式下无法导入多个模块包

本方案通过**引入显式 `[deploy] mode` 开关**，把"部署意图"从"运行时副作用"提升为"启动期契约"，以**最小改动**让服务既支持单体（资源全局共享）又支持分体（按 module 隔离）。改动范围：

- `cmx-utils/src/config/` — 新增 `DeployMode` 与 `get_app_id()` 分支（**底层契约**）
- `web-server/src/config/` — 数据源加载按模式分支
- `cmx-plugin/src/service/module_install.rs` — 导入守卫按模式分支
- 4 个 toml 配置文件 + 1 个数据迁移脚本

**不触及** `cmx-service`、`cmx-runtime`、`cmx-model-center`、`CmxAppState` 的代码（依赖 `get_app_id()` 的统一入口改造自然生效）。

---

## 二、现状分析

### 2.1 悖论根源（代码证据）

#### 2.1.1 数据源过滤悖论

**AppIdentity 使用范围**（仅 `web-server/src/config/datasource.rs`）：

| 使用点 | 文件:行 | 作用 |
|---|---|---|
| 启动期数据源打标 | [datasource.rs:41-50](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/web/web-server/src/config/datasource.rs#L41-L50) | 把 `[app]` 三元组硬塞给所有配置文件数据源 |
| 启动期过滤加载 | [datasource.rs:269-282](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/web/web-server/src/config/datasource.rs#L269-L282) | 用精确 `Eq` 过滤 `cmx_sys_datasource` |
| 持久化查重 | [datasource.rs:137-149](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/web/web-server/src/config/datasource.rs#L137-L149) | 按 `db_id + D-A-M` 联合查重 |
| 清理已删除记录 | [datasource.rs:198-210](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/web/web-server/src/config/datasource.rs#L198-L210) | 按 `source='config' + D-A-M` 过滤待清理记录 |

#### 2.1.2 `app_id` 过滤悖论（本次新增）

**`get_app_id()` 就是 `[app].module_code` 的别名**（[config_impl.rs:358-379](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/libs/cmx-utils/src/config/config_impl.rs#L358-L379)）：

```rust
pub fn get_app_id(&self) -> String {
    if let Ok(v) = self.get_string("app.module_code")   // ★ 第一优先级
        && !v.is_empty() {
        return v;
    }
    // ... 环境变量兜底 ...
    "default".to_string()
}
```

**`app_id` 作为过滤维度**覆盖以下表（`WHERE app_id = $1`）：

| 表 | 文件:行 | 单体下的影响 |
|---|---|---|
| `cmx_plugin` | cmx-plugin/src/service/persistence.rs 等 | mono 下配 `gl` 只能看 gl 的插件 |
| `cmx_plugin_versions` | cmx-plugin/src/infrastructure/.../repository.rs | 同上 |
| `cmx_service_define` | [cmx-service/src/repository.rs:137](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/libs/cmx-service/src/repository.rs#L137) | 同上 |
| `cmx_service_define_version` | 同上 | 同上 |
| `cmx_model_meta` / `cmx_model_module` / `cmx_model_module_kind` / `cmx_model_deploy_history` / `cmx_model_source` | cmx-model-center/src/lib.rs | 同上 |
| `cmx_audit_log` | cmx-audit（写入字段，查询可过滤） | 审计行 `app_id` 固定 |

**最硬的耦合** — 模块导入守卫（[module_install.rs:100-110](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/libs/cmx-plugin/src/service/module_install.rs#L100-L110)）：

```rust
let module_code = &manifest.module.code;
let current_service_app_id = cmx_utils::ConfigManager::global().get_app_id();
if module_code != &current_service_app_id {
    return Err(PluginError::CenterData(format!(
        "导入的模块资源不属于当前模块: 模块包 module_code={}, 当前服务 app_id={}",
        module_code, current_service_app_id
    )));
}
```

→ mono 模式下，即使数据源问题解决了，模块包导入仍然只能导入与 `[app].module_code` 一致的那一个。

### 2.2 关键事实

- `AppIdentity` **不进入** `CmxAppState`（[app_state.rs:62-83](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/libs/cmx-api/src/app_state.rs#L62-L83)）
- `CmxAppState.app_id`（[app_state.rs:64,94](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/libs/cmx-api/src/app_state.rs#L94)）来自 `ConfigManager::global().get_app_id()`，即 `[app].module_code`
- `cmx-runtime` 完全不感知 D-A-M / app_id
- `cmx-service`、`cmx-plugin`、`cmx-model-center` 把 `app_id` 作为**数据字段**处理，所有过滤 SQL 都走 `WHERE app_id = $1`，参数源是启动期一次性读取的 `String app_id`
- HTML/Native 页面、字典、模块清单、fact 等**按调用方传参过滤**，与 `[app]` 无关

### 2.3 已有铺垫（数据库层）

| 铺垫项 | 文件 | 含义 |
|---|---|---|
| `cmx_sys_datasource` 的 D-A-M 字段允许 NULL | [init_ddl.sql:200-204](file:///media/yqs/工作/rustspace/cmx/cmx-container/docs/sql/init/init_ddl.sql#L200-L204) | 单体模式下数据源可不打标 |
| `uk_cmx_datasource_db_id` 唯一索引已删除 | [20260701_001_...up.sql:25](file:///media/yqs/工作/rustspace/cmx/cmx-container/docs/sql/migrations/20260701_001_datasource_domain_app_module.up.sql#L25) | 同一 `db_id` 可在不同 D-A-M 下重复 |
| 评审报告已建议引入 `[deploy] mode` | [deployment-mode-review.md:181,217,339,348](file:///media/yqs/工作/rustspace/cmx/cmx-container/docs/deployment-mode-review.md) | B-4/C-3 已明确路线图 |
| `cmx_plugin`/`cmx_service_define` 等表的 `app_id` 是普通列（无 FK） | 多处 DDL | 值统一为 `'default'` 不冲突唯一索引 |

### 2.4 用户决策（已对齐）

| 决策点 | 选择 | 影响 |
|---|---|---|
| 单体模式下数据源加载策略 | **加载全部** | mono 模式跳过 D-A-M 过滤 |
| 是否引入显式模式开关 | **引入 `[deploy] mode`** | 新增配置项作为强约束来源 |
| 分体拆分粒度 | **按 module（最小单元）** | micro 模式保留现有 `[app]` 三元组语义 |
| **mono 模式下 `app_id` 策略** | **固定常量 `'default'` + 数据迁移** | `get_app_id()` 不读 `[app].module_code`，历史数据迁移 |
| **mono 模式下导入守卫** | **放宽（允许任意 module_code）** | micro 模式保留守卫 |

---

## 三、方案设计

### 3.1 主推方案：`[deploy] mode` 双模开关 + `app_id` 按 mode 切换

#### 3.1.1 核心语义

```toml
[deploy]
# 部署模式 — 启动期契约，决定数据源加载策略、app_id 取值、导入守卫
# - mono（默认）：单体模式，一个进程服务所有域/应用/模块
#   * 加载全部 status=1/archived=0 的数据源（忽略 D-A-M 过滤）
#   * get_app_id() 固定返回 "default"（不读 [app].module_code）
#   * 模块导入守卫放宽（允许任意 module_code 的模块包）
#   * 启动期校验默认库 db_url ≡ 业务库 db_url（误配告警）
#   * [app] 块整体不生效（仅作 micro 切换的预留注释）
# - micro       ：微服务模式，一个进程只服务 [app] 三元组指定的模块
#   * 按 domain_code/application_code/module_code 精确过滤数据源
#   * get_app_id() 返回 [app].module_code（维持现状）
#   * 模块导入守卫保留（module_code != app_id 则拒绝）
#   * [app] 三元组必需，缺省值 default 会被拒绝启动
# 支持环境变量覆盖：DEPLOY__MODE
mode = "mono"
```

#### 3.1.2 双模行为对照表

| 行为 | mono 模式 | micro 模式 |
|---|---|---|
| **`get_app_id()`**（[config_impl.rs:358](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/libs/cmx-utils/src/config/config_impl.rs#L358)） | **固定返回 `"default"`**，不读 `[app].module_code` | 维持现状（读 `[app].module_code`） |
| `[app]` 块 | **整体不生效**（可省略或保留作 micro 预留） | 必需，三元组不能为 `default` |
| 数据源打标（[datasource.rs:41-50](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/web/web-server/src/config/datasource.rs#L41-L50)） | **不强制覆盖** D-A-M（保留配置文件原值） | 维持现状（未声明则继承 `[app]`） |
| `load_active_datasources` 过滤 | **移除 D-A-M 过滤**，仅 `status=1 AND archived=0` | 维持现状（精确 Eq 过滤） |
| `persist_datasource_configs` 查重 | 查重键改为**仅 `db_id + source='config'`** | 维持现状（`db_id + D-A-M`） |
| 清理逻辑 | 按 `source='config'` 全局清理 | 维持现状（按 D-A-M） |
| **模块导入守卫**（[module_install.rs:105](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/libs/cmx-plugin/src/service/module_install.rs#L105)） | **放宽**（允许任意 `module_code`） | 维持现状（`module_code != app_id` 拒绝） |
| **`cmx_plugin`/`cmx_service_define`/`cmx_model_*` 等表** | 数据 `app_id` 字段统一为 `'default'`（历史数据迁移） | 按模块隔离，`app_id = [app].module_code` |
| 启动期一致性校验（**新增**） | 校验默认库 `db_url` ≡ 业务库 `db_url`，不一致 `warn!` | 不校验（允许分库） |

#### 3.1.3 为什么把 `DeployMode` 定义在 `cmx-utils`？

`get_app_id()` 在 `cmx-utils/src/config/config_impl.rs`，被 `cmx-plugin`、`cmx-service`、`cmx-api`、`web-server` 等所有上层 crate 依赖。若 `DeployMode` 定义在 `web-server`，`cmx-utils` 无法依赖它（循环依赖）。因此 `DeployMode` 必须**下移到 `cmx-utils`** 作为最底层的配置契约。

---

## 四、具体改动清单

### 4.1 配置文件（4 个）

| 文件 | 改动 |
|---|---|
| [config/config_template.toml](file:///media/yqs/工作/rustspace/cmx/cmx-container/config/config_template.toml) | 新增 `[deploy]` 节，含 `mode = "mono"` 及注释说明 |
| [dev.toml](file:///media/yqs/工作/rustspace/cmx/cmx-container/dev.toml) | 新增 `[deploy] mode = "mono"`；`[app]` 块注释为"仅 micro 模式生效" |
| [dev-local.toml](file:///media/yqs/工作/rustspace/cmx/cmx-container/dev-local.toml) | 同上 |
| [dev-vpn.toml](file:///media/yqs/工作/rustspace/cmx/cmx-container/dev-vpn.toml) | 同上 |

**`[deploy]` 节建议位置**：紧邻 `[app]` 节之前（顶部"应用标识配置"区）。

**`dev.toml` 改动示例**（替换现有 L10-27）：

```toml
# ============================================
# 部署模式配置
# ============================================
# 启动期契约：决定数据源加载策略、app_id 取值、模块导入守卫。
# - mono（默认）：单体模式，一个进程服务所有域/应用/模块
#   * 数据源：加载 cmx_sys_datasource 中所有 status=1/archived=0 的记录（忽略 D-A-M）
#   * app_id：固定返回 "default"（不读 [app].module_code）
#   * 模块导入：允许任意 module_code 的模块包
#   * [app] 块整体不生效（可省略或保留作 micro 切换预留）
# - micro       ：微服务模式，一个进程只服务 [app] 三元组指定的模块
#   * 数据源：按 [app] 三元组精确过滤
#   * app_id：返回 [app].module_code
#   * 模块导入：要求 module_code == app_id
#   * [app] 三元组必需，缺省值 default 会被拒绝启动
# 支持环境变量覆盖：DEPLOY__MODE
[deploy]
mode = "mono"

# ============================================
# 应用标识配置（仅 micro 模式生效）
# ============================================
# 当前实例所属的域/应用/模块。
# mono 模式下本块整体不生效（get_app_id 固定返回 "default"，数据源不按此过滤）。
# micro 模式下用于数据源过滤、插件/服务隔离（app_id = module_code）、模块导入守卫。
# 支持环境变量覆盖：APP__DOMAIN_CODE / APP__APPLICATION_CODE / APP__MODULE_CODE
[app]
# 所属域编码（micro 必需，默认 default）
domain_code = "fi"
# 所属应用编码（micro 必需，默认 default）
application_code = "cmxfico"
# 所属模块编码（micro 必需，默认 default；决定 app_id）
module_code = "gl"
```

> **关键变化**：`[app]` 块从"必需"改为"仅 micro 必需"。mono 模式下保留这些值不会造成危害（不读取），但建议运维在 mono 部署中注释掉，避免误导。

### 4.2 代码改动（4 个文件）

#### 4.2.1 `crates/libs/cmx-utils/src/config/config_impl.rs`（**核心底层契约**）

**改动 1**：新增 `DeployMode` 枚举与读取方法（紧邻 `get_app_id()` 定义之前）：

```rust
/// 部署模式 — 启动期契约，决定数据源加载策略、app_id 取值、模块导入守卫。
///
/// - `Mono`：单体模式，加载全部数据源，app_id 固定为 `"default"`
/// - `Micro`：微服务模式，按 [app] 三元组精确过滤，app_id = [app].module_code
///
/// 从 `[deploy] mode` TOML 节读取，支持 `DEPLOY__MODE` 环境变量覆盖。
/// 缺省为 `Mono`（向后兼容）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeployMode {
    Mono,
    Micro,
}

impl Default for DeployMode {
    fn default() -> Self {
        Self::Mono
    }
}

impl std::str::FromStr for DeployMode {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "mono" | "monolithic" | "single" => Ok(Self::Mono),
            "micro" | "microservice" => Ok(Self::Micro),
            other => Err(format!("未知的 deploy.mode: {}（支持 mono/micro）", other)),
        }
    }
}

impl DeployMode {
    /// 从全局配置读取部署模式
    pub fn from_config() -> Self {
        let cm = ConfigManager::global();
        cm.get_string("deploy.mode")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or_default()
    }
}
```

**改动 2**：`get_app_id()` 按 `DeployMode` 分支（[config_impl.rs:358-379](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/libs/cmx-utils/src/config/config_impl.rs#L358-L379)）：

```rust
/// 获取应用隔离标识(app_id)，统一入口。
///
/// **按 `[deploy] mode` 分支**：
/// - `Mono`：固定返回 `"default"`，**不读** `[app].module_code`（单体下 [app] 块不生效）
/// - `Micro`：维持原有查找顺序（[app].module_code → 环境变量 → "default"）
///
/// 全项目应通过此方法获取 app_id，避免散落的 `get_string("app.module_code")` 调用。
pub fn get_app_id(&self) -> String {
    // ★ 新增：按部署模式分支
    if DeployMode::from_config() == DeployMode::Mono {
        return "default".to_string();
    }

    // micro 模式：维持原有查找顺序
    // 1. 配置项
    if let Ok(v) = self.get_string("app.module_code")
        && !v.is_empty()
    {
        return v;
    }
    // 2-4. 环境变量
    for key in [
        "APP_ID",
        "SERVICE_REGISTRY_NAME",
        "NACOS_NAMING_SERVICE_NAME",
    ] {
        if let Ok(v) = std::env::var(key)
            && !v.is_empty()
        {
            return v;
        }
    }
    // 5. 兜底
    "default".to_string()
}
```

**注意**：`DeployMode::from_config()` 每次调用都会读配置，但配置读取是内存级的（`Arc<Config>`），性能影响可忽略。若担心，可在 `Config` 初始化时一次性缓存。

#### 4.2.2 `crates/web/web-server/src/config/mod.rs`

**改动**：`web-server` 的 `DeployMode` 改为从 `cmx-utils` re-export（避免重复定义）：

```rust
// 移除本地的 DeployMode 定义，改为从 cmx-utils 导入
pub use cmx_utils::config::DeployMode;

pub fn load_deploy_mode() -> DeployMode {
    DeployMode::from_config()
}
```

#### 4.2.3 `crates/web/web-server/src/config/datasource.rs`

**4 处修改**（与原方案一致，简要列出）：

**改动 1**：`init_datasources` 入口（[L29](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/web/web-server/src/config/datasource.rs#L29)）加载 `DeployMode` + micro 模式校验 + 打标分支：

```rust
pub async fn init_datasources() -> crate::Result<()> {
    info!("开始初始化数据源...");
    let deploy_mode = crate::config::load_deploy_mode();
    info!("部署模式: {:?}", deploy_mode);

    // ... 配置加载 ...

    let app_identity = crate::config::load_app_identity();

    // ★ micro 模式校验 [app] 三元组不能为 default
    if deploy_mode == DeployMode::Micro {
        if app_identity.domain_code == "default"
            || app_identity.application_code == "default"
            || app_identity.module_code == "default"
        {
            return Err(Error::DatasourceInit(format!(
                "micro 模式下 [app] 三元组必须配置具体值（当前: {}/{}/{}）",
                app_identity.domain_code, app_identity.application_code, app_identity.module_code
            )));
        }
    }

    for c in &mut configs {
        // ★ mono 模式不强制覆盖 D-A-M；micro 模式维持现有兜底逻辑
        if deploy_mode == DeployMode::Micro {
            if c.domain_code.is_none() {
                c.domain_code = Some(app_identity.domain_code.clone());
            }
            if c.application_code.is_none() {
                c.application_code = Some(app_identity.application_code.clone());
            }
            if c.module_code.is_none() {
                c.module_code = Some(app_identity.module_code.clone());
            }
        }
        // source_type 与 db_name 的兜底逻辑保持不变
        // ...
    }

    // ... persist/load 调用需传入 deploy_mode ...
}
```

**改动 2**：`persist_datasource_configs` 签名新增 `deploy_mode` 参数；mono 模式查重键改为 `db_id + source='config'`；清理逻辑同步分支（[L127-L245](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/web/web-server/src/config/datasource.rs#L127-L245)）。

**改动 3**：`load_active_datasources` 签名新增 `deploy_mode` 参数；mono 模式仅 `status=1 AND archived=0`，移除 D-A-M 过滤（[L259-L307](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/web/web-server/src/config/datasource.rs#L259-L307)）。

**改动 4**：新增 `check_mono_datasource_consistency` 函数（实现 B-4 建议，mono 模式校验默认库≡业务库）。

#### 4.2.4 `crates/libs/cmx-plugin/src/service/module_install.rs`（**导入守卫放宽**）

**改动**：[L100-L110](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/libs/cmx-plugin/src/service/module_install.rs#L100-L110) 的导入守卫按 `DeployMode` 分支：

```rust
let module_code = &manifest.module.code;
let current_service_app_id = cmx_utils::ConfigManager::global().get_app_id();
let deploy_mode = cmx_utils::config::DeployMode::from_config();

// ★ 改动：mono 模式放宽守卫；micro 模式维持现状
if deploy_mode == cmx_utils::config::DeployMode::Micro
    && module_code != &current_service_app_id
{
    return Err(PluginError::CenterData(format!(
        "导入的模块资源不属于当前模块: 模块包 module_code={}, 当前服务 app_id={}",
        module_code, current_service_app_id
    )));
}

if deploy_mode == cmx_utils::config::DeployMode::Mono {
    info!(
        module_code = %manifest.module.code,
        "单体模式下导入模块包（跳过 module_code 与 app_id 一致性校验）"
    );
}
```

### 4.3 数据迁移脚本（**新增**）

**文件**：[docs/sql/migrations/20260721_001_deploy_mode_mono_app_id_unification.up.sql](file:///media/yqs/工作/rustspace/cmx/cmx-container/docs/sql/migrations/)（新建）

**目的**：把历史数据中 `app_id` 为具体模块值（如 `'gl'`、`'ap'`、`'fi_cmxfico_gl'` 等）的记录统一改为 `'default'`，让 mono 模式下历史数据可见。

#### 4.3.1 影响表清单（基于 init_ddl.sql 全量核查）

**需迁移的表（13 张，均有 `app_id` 列且数据可变）**：

| 表 | DDL 行 | 唯一键含 app_id | 说明 |
|---|---|---|---|
| `cmx_plugin` | init_ddl.sql:238 | 是 `(app_id, plugin_id)` | 插件主表 |
| `cmx_plugin_versions` | init_ddl.sql:323 | 是 `(plugin_id, app_id, version)` | 插件版本 |
| `cmx_plugin_audit_log` | init_ddl.sql:489 | 否（普通索引） | **插件审计日志**（修订新增） |
| `cmx_meta_table_define` | init_ddl.sql:788 | 否（普通索引） | **表元数据**（修订新增） |
| `cmx_meta_table_define_version` | init_ddl.sql:837 | 否（普通索引） | **表元数据版本**（修订新增） |
| `cmx_service_define` | init_ddl.sql:882 | 是 `(app_id, service_key)` | 服务定义 |
| `cmx_service_define_version` | init_ddl.sql:932 | 否（普通索引） | 服务定义版本 |
| `cmx_model_meta` | init_ddl.sql:2224 | 是 `(db_id, app_id)` | 模型台账 |
| `cmx_model_module` | init_ddl.sql:2266 | 是 `(db_id, app_id, D, A, M)` | 模型模块 |
| `cmx_model_module_kind` | init_ddl.sql:2315 | 是六元组 | 模型模块分类 |
| `cmx_model_deploy_history` | init_ddl.sql:2368 | 否（追加式） | 部署历史 |
| `cmx_model_source` | init_ddl.sql:2434 | 是七元组 | 模型源 |
| `cmx_model_registry` | init_ddl.sql:2494 | 是 `(db_id, app_id)` | **跨库总览**（修订新增） |

**不迁移的表（1 张）**：

| 表 | 原因 |
|---|---|
| `cmx_audit_log` | 全局审计表，数据量大；遵循"审计数据不可变"原则保留原值；查询侧 mono 模式**不做 app_id 过滤**（见 4.3.4） |

**不涉及的表**：
- 注释掉的表（`cmx_plugin_dependencies`、`cmx_plugin_deployments`、`cmx_system_plugins`、`cmx_plugin_nodes`、`cmx_plugin_features`）— 未启用
- 字典表（`cmx_domain`、`cmx_application`、`cmx_module`）— 无 `app_id` 列
- `cmx_form`、`cmx_menu` — 无 `app_id` 列，使用 D-A-M 三字段
- `cmx_permission` — 使用 `app_code`（非 `app_id`），保持不变

#### 4.3.2 脚本内容

```sql
-- 20260721_001_deploy_mode_mono_app_id_unification.up.sql
-- 目的：把历史 app_id 为具体模块值的记录统一为 'default'，
-- 配合 [deploy] mode = "mono" 下 get_app_id() 固定返回 "default" 的行为。
--
-- 注意：此迁移仅在切到 mono 模式时执行；
-- 若未来切回 micro 模式并希望恢复按模块隔离，需用 down.sql 提示手动恢复。
-- 迁移前**必须**执行下方备份脚本。

-- ============ 备份（运维必须执行，可选但强烈建议） ============
-- CREATE TABLE cmx_app_id_backup_20260721 AS
-- SELECT id, app_id, 'cmx_plugin' AS src FROM cmx_plugin WHERE app_id != 'default'
-- UNION ALL SELECT id, app_id, 'cmx_plugin_versions' FROM cmx_plugin_versions WHERE app_id != 'default'
-- UNION ALL SELECT id, app_id, 'cmx_plugin_audit_log' FROM cmx_plugin_audit_log WHERE app_id != 'default'
-- UNION ALL SELECT id, app_id, 'cmx_meta_table_define' FROM cmx_meta_table_define WHERE app_id != 'default'
-- UNION ALL SELECT id, app_id, 'cmx_meta_table_define_version' FROM cmx_meta_table_define_version WHERE app_id != 'default'
-- UNION ALL SELECT id, app_id, 'cmx_service_define' FROM cmx_service_define WHERE app_id != 'default'
-- UNION ALL SELECT id, app_id, 'cmx_service_define_version' FROM cmx_service_define_version WHERE app_id != 'default'
-- UNION ALL SELECT id, app_id, 'cmx_model_meta' FROM cmx_model_meta WHERE app_id != 'default'
-- UNION ALL SELECT id, app_id, 'cmx_model_module' FROM cmx_model_module WHERE app_id != 'default'
-- UNION ALL SELECT id, app_id, 'cmx_model_module_kind' FROM cmx_model_module_kind WHERE app_id != 'default'
-- UNION ALL SELECT id, app_id, 'cmx_model_deploy_history' FROM cmx_model_deploy_history WHERE app_id != 'default'
-- UNION ALL SELECT id, app_id, 'cmx_model_source' FROM cmx_model_source WHERE app_id != 'default'
-- UNION ALL SELECT id, app_id, 'cmx_model_registry' FROM cmx_model_registry WHERE app_id != 'default';

-- ============ 统一 app_id 为 'default' ============
UPDATE cmx_plugin SET app_id = 'default' WHERE app_id != 'default';
UPDATE cmx_plugin_versions SET app_id = 'default' WHERE app_id != 'default';
UPDATE cmx_plugin_audit_log SET app_id = 'default' WHERE app_id != 'default';
UPDATE cmx_meta_table_define SET app_id = 'default' WHERE app_id != 'default';
UPDATE cmx_meta_table_define_version SET app_id = 'default' WHERE app_id != 'default';
UPDATE cmx_service_define SET app_id = 'default' WHERE app_id != 'default';
UPDATE cmx_service_define_version SET app_id = 'default' WHERE app_id != 'default';
UPDATE cmx_model_meta SET app_id = 'default' WHERE app_id != 'default';
UPDATE cmx_model_module SET app_id = 'default' WHERE app_id != 'default';
UPDATE cmx_model_module_kind SET app_id = 'default' WHERE app_id != 'default';
UPDATE cmx_model_deploy_history SET app_id = 'default' WHERE app_id != 'default';
UPDATE cmx_model_source SET app_id = 'default' WHERE app_id != 'default';
UPDATE cmx_model_registry SET app_id = 'default' WHERE app_id != 'default';

-- cmx_audit_log 不迁移（审计数据不可变；查询侧 mono 模式不做 app_id 过滤，见方案 4.3.4）

-- ============ 验证（应全部返回 0） ============
-- SELECT 'cmx_plugin' AS t, COUNT(*) FROM cmx_plugin WHERE app_id != 'default'
-- UNION ALL SELECT 'cmx_plugin_versions', COUNT(*) FROM cmx_plugin_versions WHERE app_id != 'default'
-- UNION ALL SELECT 'cmx_plugin_audit_log', COUNT(*) FROM cmx_plugin_audit_log WHERE app_id != 'default'
-- UNION ALL SELECT 'cmx_meta_table_define', COUNT(*) FROM cmx_meta_table_define WHERE app_id != 'default'
-- UNION ALL SELECT 'cmx_meta_table_define_version', COUNT(*) FROM cmx_meta_table_define_version WHERE app_id != 'default'
-- UNION ALL SELECT 'cmx_service_define', COUNT(*) FROM cmx_service_define WHERE app_id != 'default'
-- UNION ALL SELECT 'cmx_service_define_version', COUNT(*) FROM cmx_service_define_version WHERE app_id != 'default'
-- UNION ALL SELECT 'cmx_model_meta', COUNT(*) FROM cmx_model_meta WHERE app_id != 'default'
-- UNION ALL SELECT 'cmx_model_module', COUNT(*) FROM cmx_model_module WHERE app_id != 'default'
-- UNION ALL SELECT 'cmx_model_module_kind', COUNT(*) FROM cmx_model_module_kind WHERE app_id != 'default'
-- UNION ALL SELECT 'cmx_model_deploy_history', COUNT(*) FROM cmx_model_deploy_history WHERE app_id != 'default'
-- UNION ALL SELECT 'cmx_model_source', COUNT(*) FROM cmx_model_source WHERE app_id != 'default'
-- UNION ALL SELECT 'cmx_model_registry', COUNT(*) FROM cmx_model_registry WHERE app_id != 'default';
```

**down.sql**（回滚需基于备份恢复，无法自动逆向）：

```sql
-- 20260721_001_deploy_mode_mono_app_id_unification.down.sql
-- 警告：此迁移不可自动逆向（UPDATE 已丢失原值）。
-- 回滚步骤：
-- 1. 从 cmx_app_id_backup_20260721 备份表逐表恢复 app_id（按 src 列区分）
-- 2. 或根据 cmx_module 表的三元组重新计算 app_id（仅适用于从未改过 module_code 的场景）
-- 此 down.sql 不执行任何 SQL，仅作占位。
SELECT '无法自动回滚 app_id 统一迁移，请手动从 cmx_app_id_backup_20260721 恢复' AS warning;
```

#### 4.3.3 `init_dml.sql` 是否需要改动？

**不需要**。grep `app_id` 在 init_dml.sql 中**零匹配**，说明种子数据不显式写 `app_id`，依赖 DDL 的 `DEFAULT 'default'`。首次部署时所有种子数据自动得到 `app_id='default'`，与 mono 模式行为一致。

#### 4.3.4 `cmx_audit_log` 的查询侧策略

- **写入侧**：mono 模式下新审计行 `app_id='default'`（由 `get_app_id()` 保证）
- **查询侧**：审计查询页面/接口在 mono 模式下**不做 `WHERE app_id = ?` 过滤**，返回所有审计行
- **实现方式**：审计查询通常按 `create_time`、`user_id`、`action` 等维度过滤，`app_id` 不是主要查询维度；若现有代码有 `WHERE app_id = ?`，需在审计 handler 处按 `DeployMode` 分支（与 `module_install.rs` 类似）
- **影响范围**：需检查 `cmx-audit` 的查询路径；若发现硬过滤，纳入本方案 Step 4 一并修改

**注意事项**：
- 迁移前**必须备份**（脚本注释中有建议的 `CREATE TABLE ... AS` 语句）
- `cmx_module`、`cmx_domain`、`cmx_application` 字典表**不迁移**（它们不含 `app_id` 列，与部署模式无关）
- 迁移后，mono 模式下所有新数据自动写入 `app_id='default'`（由 `get_app_id()` 保证）
- `module_export.rs:179`（`let app_id = get_app_id()`）**无需改动**：mono 模式下自动得到 `'default'`，配合数据迁移后能导出全部插件；导出包的 `manifest.module.code` 仍是模块自身 code（如 `'gl'`），导入 micro 实例时守卫比较 `'gl' == 'gl'` 通过

### 4.4 文档改动（3 个文件，**必需**）

| 文件 | 改动 |
|---|---|
| [config/config_template.toml](file:///media/yqs/工作/rustspace/cmx/cmx-container/config/config_template.toml) | 新增 `[deploy]` 节，含 `mode = "mono"` 及注释说明 |
| [config/CONFIG_MANUAL.md](file:///media/yqs/工作/rustspace/cmx/cmx-container/config/CONFIG_MANUAL.md) | 新增 `[deploy]` 节说明 + 双模对照表 |
| [config/ENV_MANUAL.md](file:///media/yqs/工作/rustspace/cmx/cmx-container/config/ENV_MANUAL.md) | 新增 `DEPLOY__MODE` 环境变量说明 |
| [AGENTS.md](file:///media/yqs/工作/rustspace/cmx/cmx-container/AGENTS.md) 第六章 | **更新 6.1 约束**：从"`app_id ≡ module_code`"改为"按 `[deploy] mode` 切换：mono 下 `app_id='default'`，micro 下 `app_id = [app].module_code`"；6.2 规则同步调整 |

**AGENTS.md 第六章改动要点**（在 Step 7 执行）：

- **6.1 标题**：从"当前约束：`app_id ≡ module_code`"改为"app_id 取值规则（按 `[deploy] mode` 切换）"
- **6.1 正文**：补充 mono / micro 双模说明，引用本方案
- **6.2.1 规则**：维持"禁止硬编码 `'default'` 作为 app_id 兜底"——调用点仍必须走 `get_app_id()`；但说明 `get_app_id()` 内部按 `DeployMode` 分支
- **6.2.4 规则**：mono 模式下 `app_id` 全局为 `'default'`，所有表数据共享；micro 模式下 `app_id` 是唯一隔离键

### 4.5 不需要改动的部分（明确边界）

- **`cmx-service`、`cmx-plugin`、`cmx-model-center` 的 SQL 过滤逻辑**：依赖 `get_app_id()` 统一入口，mono 模式下自动得到 `'default'`，配合数据迁移后所有数据 `app_id='default'`，过滤自然生效
- `CmxAppState`：不新增字段，`app_id` 字段值变化但语义不变
- `cmx-runtime`：完全不感知 D-A-M / app_id
- HTML/Native 页面、字典、模块清单、fact：按调用方传参过滤，与 `[app]` 无关
- 数据库 schema：D-A-M 字段已允许 NULL，无需迁移

---

## 五、假设与决策

### 5.1 关键假设

1. **向后兼容**：缺省 `[deploy] mode = "mono"`，与现有部署行为等价或更宽松
2. **`get_app_id()` 性能**：每次调用读配置是内存级（`Arc<Config>`），可接受；若有顾虑可在 `Config` 初始化时缓存 `DeployMode`
3. **数据迁移一次性**：mono 模式落地时执行一次，之后新数据自动 `app_id='default'`
4. **`cmx_audit_log` 不迁移**：审计数据不可变，查询时 mono 模式不做 app_id 过滤（审计查询通常按时间/用户，不按模块）

### 5.2 关键决策

| 决策 | 理由 |
|---|---|
| `DeployMode` 定义在 `cmx-utils`（而非 `web-server`） | 避免 `cmx-utils` → `web-server` 的循环依赖；`get_app_id()` 与 `module_install.rs` 都能直接使用 |
| mono 模式 `get_app_id()` 返回固定 `"default"` | 符合用户"固定常量 + 数据迁移"决策；`"default"` 与现有兜底值一致，无需新增常量 |
| mono 模式数据源打标改为"保留原值" | 避免"所有数据源被错误标为同一模块"的归属污染，为分体化预留元数据 |
| mono 模式查重键仅 `db_id` + `source='config'` | 避免同一 `db_id` 因 D-A-M 为 NULL 与历史记录产生重复 |
| micro 模式启动校验 `[app]` 不能为 `default` | 强约束，避免运维漏配 |
| mono 模式一致性校验仅 `warn!` 不阻断 | 评审报告 B-4 建议，软告警策略 |
| 导入守卫 mono 放宽 / micro 保留 | 用户决策；mono 下需导入任意模块包 |
| 数据迁移脚本含 `cmx_audit_log` 之外的所有 `app_id` 表 | 审计表按时间查询为主，且数据量大不宜 UPDATE |

### 5.3 风险与缓解

| 风险 | 缓解措施 |
|---|---|
| 现有 `dev.toml` `[app] fi/cmxfico/gl` 在切到 mono 后，历史数据 `app_id='gl'` 看不到 | 必须执行数据迁移脚本 4.3 |
| 数据迁移误操作（未备份） | 脚本注释强制要求备份；down.sql 不自动回滚 |
| mono 模式下 `cmx_sys_datasource` 存在历史多 D-A-M 重复 `db_id` 导致 `register_data_source` 冲突 | `init_datasources` 现有 [L94-99](file:///media/yqs/工作/rustspace/cmx/cmx-container/crates/web/web-server/src/config/datasource.rs#L94-L99) 已按 `db_manager.get_db()` 跳过已注册 |
| `DeployMode::from_config()` 每次调用读配置的性能 | 可在 `Config` 内缓存（`OnceCell<DeployMode>`），但首版不做 |
| micro → mono → micro 切换时 app_id 数据丢失 | mono → micro 切换需反向迁移（从备份恢复或按 `cmx_module` 三元组重建 app_id），文档明确说明 |

---

## 六、验证步骤

### 6.1 mono 模式验证

1. **配置**：`dev.toml` 设 `[deploy] mode = "mono"`，`[app]` 维持 `fi/cmxfico/gl` 或注释掉
2. **执行数据迁移**：运行 `20260721_001_...up.sql`
3. **启动**：观察日志包含 `部署模式: Mono`
4. **断言**：
   - `ConfigManager::global().get_app_id()` 返回 `"default"`（不读 module_code）
   - 启动成功，加载全部 `status=1/archived=0` 的数据源（不限于 gl 模块）
   - `cmx_plugin` 查询返回所有模块的插件（`app_id='default'`）
   - 导入任意 `module_code` 的模块包均成功（守卫放宽）
   - 若默认库与业务库 `db_url` 不同，启动日志包含 warn

### 6.2 micro 模式验证

1. **配置**：`dev.toml` 设 `[deploy] mode = "micro"`，`[app]` 维持 `fi/cmxfico/gl`
2. **启动**：观察日志包含 `部署模式: Micro`
3. **断言**：
   - `get_app_id()` 返回 `"gl"`（读 `[app].module_code`）
   - 仅加载 `cmx_sys_datasource` 中 D-A-M = `fi/cmxfico/gl` 的记录
   - `cmx_plugin` 查询仅返回 `app_id='gl'` 的插件
   - 导入 `module_code != 'gl'` 的模块包被拒绝（守卫生效）
   - 把 `[app]` 改为 `default/default/default` 重启 → 启动失败

### 6.3 回归验证

1. **缺省配置**：删除 `[deploy]` 节 → 默认 `mono` 模式，行为与改造前等价或更宽松
2. **环境变量覆盖**：`DEPLOY__MODE=micro cargo run` → 进入 micro 模式
3. **数据迁移可重复**：多次执行迁移脚本无副作用（`WHERE app_id != 'default'` 命中 0 行）

### 6.4 编译与测试

```bash
cd /media/yqs/工作/rustspace/cmx/cmx-container
cargo build -p cmx-utils -p web-server -p cmx-plugin
cargo clippy -p cmx-utils -p web-server -p cmx-plugin -- -D warnings
cargo test -p cmx-utils
```

---

## 七、实施顺序建议

1. **Step 1**：在 `cmx-utils/src/config/config_impl.rs` 新增 `DeployMode` 枚举 + `from_config()` + 修改 `get_app_id()` 分支
2. **Step 2**：在 `web-server/src/config/mod.rs` 改为 re-export `DeployMode`，移除本地定义（若有）
3. **Step 3**：在 `web-server/src/config/datasource.rs` 修改 `init_datasources` / `persist_datasource_configs` / `load_active_datasources`（签名 + 分支）+ 新增 `check_mono_datasource_consistency`
4. **Step 4**：在 `cmx-plugin/src/service/module_install.rs` 修改导入守卫分支
5. **Step 5**：新建数据迁移脚本 `20260721_001_deploy_mode_mono_app_id_unification.{up,down}.sql`
6. **Step 6**：更新 4 个 toml 配置文件（新增 `[deploy]` 节，调整 `[app]` 注释）
7. **Step 7**：更新 `CONFIG_MANUAL.md`、`ENV_MANUAL.md`、`AGENTS.md` 第六章（部署模式约束）
8. **Step 8**：按第 6 节执行验证（含手动执行数据迁移）
9. **Step 9**（可选）：检查 `cmx-audit` 查询路径是否硬过滤 `app_id`，若是则按 mode 分支

---

## 八、与评审报告路线图的关系

本方案落地 [docs/deployment-mode-review.md](file:///media/yqs/工作/rustspace/cmx/cmx-container/docs/deployment-mode-review.md) 的：

- **B-4**（🟡）：mono 模式默认库≡业务库一致性校验 + 显式 `[deploy] mode` ✅ **完全覆盖**
- **C-3**（🔵）：编译期/显式部署 profile ✅ **运行时配置形态完全覆盖**（未做编译期 feature flag）
- **C-2**（🔵）：身份三元组作为请求上下文 ⏸️ **部分覆盖**（get_app_id 按 mode 切换，但跨服务 CallContext 未实现）

**不覆盖**的部分（保持现状，后续推进）：
- A-1/A-2/A-3（微服务化安全红线，拆分前必做）
- B-1/B-2/B-3/B-5（数据库治理中长期项）

本方案是**最小可行的单体/分体切换**，**消除了 `[app]` 配置的悖论**，让单体运行合理、分体化有明确路径，且**不引入新的架构债务**。
