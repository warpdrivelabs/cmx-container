---
name: "wasm-plugin-developer"
description: "WASM 插件开发指南，介绍工程结构、目录规范、manifest.json、代码架构和技能使用指引。Invoke when 用户需要创建、开发或理解 WASM 插件工程结构时。"
---

# WASM 插件开发指南

> 基于 cmx-container 插件平台的 WASM 插件开发完整指南，采用渐进式披露方式组织。

---

## 一、工程目录结构全景

### 1.1 标准目录树

```
my-plugin/
├── manifest.json              # 插件清单文件（必须）
├── Cargo.toml                 # Rust 项目配置
├── .cargo/config.toml         # Cargo 构建配置
├── .vscode/launch.json        # VS Code 调试配置
├── config/                    # 表定义配置（注册表结构和种子数据）
│   └── {name}_config.json
├── metadata/                  # 表结构定义（DDL 元数据）
│   └── {name}_tables.json
├── seeddata/                  # 种子数据（初始化数据）
│   ├── {table_name}_seed.json
│   └── {table_name}_seed.csv
├── servicedata/               # 服务编排流程定义（每个文件对应一个接口）
│   ├── save_xxx.json
│   ├── list_xxx.json
│   └── get_xxx_detail.json
├── formdata/                  # 表单配置（预留）
│   └── {name}_form.json
├── menudata/                  # 菜单配置（预留）
│   └── {name}_menu.json
├── permdata/                  # 权限数据（预留）
├── flowdata/                  # 流程定义数据（预留）
├── mcpdata/                   # MCP/Skills 配置（预留）
├── bin/                       # 额外资源文件（预留）
├── api/                       # 插件接口定义（预留）
├── wit/                       # WASM Interface Type（预留）
└── src/                       # Rust 源码
    ├── lib.rs
    ├── models.rs
    ├── host_traits.rs
    ├── core.rs
    ├── extism_layer.rs
    └── tests.rs
```

### 1.2 目录用途速查表

| 目录 | 用途 | 必须性 | 参考技能 |
|------|------|--------|---------|
| `config/` | 表定义配置清单，注册表结构和种子数据关系 | 推荐 | plugin-metadata-generator |
| `metadata/` | 表结构定义（列、索引、主键等 DDL 元数据） | 推荐 | plugin-metadata-generator |
| `seeddata/` | 插件安装时自动执行的初始化数据 | 推荐 | plugin-metadata-generator |
| `servicedata/` | 服务编排流程定义，每个文件对应一个接口 | 推荐 | service-orchestration-generator |
| `formdata/` | 前端表单配置 | 预留 | — |
| `menudata/` | 前端菜单配置 | 预留 | — |
| `permdata/` | 权限数据配置 | 预留 | — |
| `flowdata/` | 流程定义数据 | 预留 | — |
| `mcpdata/` | MCP/Skills 配置 | 预留 | — |

---

## 二、目录详细规范

### 2.1 config/ — 表定义配置

配置文件是表结构的入口注册点，声明了要加载哪些表定义文件和种子数据。

**文件命名**：`{name}_config.json`

**格式**：

```json
{
  "name": "account",
  "description": "配置描述",
  "depends_on": [],
  "priority": 0,
  "files": ["account_tables.json"],
  "seed_data": [
    {
      "table_name": "cmx_account",
      "file": "seeddata/account_seed.json",
      "conflict_columns": ["code"],
      "enabled": true
    }
  ]
}
```

**关键字段说明**：

| 字段 | 说明 |
|------|------|
| `name` | 配置名称（唯一标识），用于拓扑排序时的依赖引用 |
| `depends_on` | 依赖的其他配置名称数组（拓扑排序） |
| `priority` | 优先级，数值越小越先执行 |
| `files` | 引用 `metadata/` 目录下的表定义文件名（仅文件名，不含路径） |
| `seed_data` | 种子数据配置数组，每个条目指定表名、数据文件、冲突列 |

**执行顺序**：`depends_on` 拓扑排序 → `priority` → `files`（建表）→ `seed_data`（插数据）

### 2.2 metadata/ — 表结构定义

存放 DDL 元数据文件，定义表的列、索引、主键、外键等。

- 文件由 `config/` 中的 `files` 字段引用（仅文件名）
- 加载器自动从 `metadata/` 目录查找
- 详细生成规范请使用 **plugin-metadata-generator** 技能

### 2.3 seeddata/ — 种子数据

插件安装时自动执行的初始化数据，支持 JSON 和 CSV 两种格式。

- 由 `config/` 中的 `seed_data.file` 字段引用（相对路径）
- 支持 UPSERT（通过 `conflict_columns` 配置）
- 种子数据失败**不阻断安装**
- 详细生成规范请使用 **plugin-metadata-generator** 技能

### 2.4 servicedata/ — 服务编排

每个 JSON 文件定义一个服务编排流程（对应一个接口）。

**关键约定**：

- `code` 字段是服务的唯一标识（service_key），不要带 `_flow` 后缀
- 每个文件对应一个对外暴露的接口
- 支持线性流程、分支路由（switch）、事务处理等
- 详细生成规范请使用 **service-orchestration-generator** 技能

### 2.5 预留目录

| 目录 | 用途 | 说明 |
|------|------|------|
| `formdata/` | 前端表单配置 | 定义表单字段、类型、校验规则等 |
| `menudata/` | 前端菜单配置 | 定义菜单树结构、图标、路径等 |
| `permdata/` | 权限数据 | 定义功能权限和数据权限 |
| `flowdata/` | 流程定义数据 | 定义审批流、工作流等 |
| `mcpdata/` | MCP/Skills 配置 | 定义 AI 能力扩展 |

这些目录当前为预留，插件打包时会在 `manifest.json` 的 `entries` 中包含（如 `formdata/**/*`），但安装时不强制要求。

---

## 三、manifest.json 规范

### 3.1 完整格式

```json
{
  "manifest_version": "1.0",
  "plugin": {
    "type": "wasm-plugin",
    "id": "cmx_account",
    "name": "科目管理",
    "version": "1.0.0",
    "description": "会计科目管理插件",
    "url": "https://example.com/plugin/cmx_account",
    "source_path": "/path/to/source",
    "dependencies": [],
    "main_file": "cmx_account.wasm",
    "datasource_id": "primary",
    "extra_files": [],
    "table_config_files": ["config/account_config.json"],
    "supported_databases": ["postgres"],
    "domain_code": "FIN",
    "application_code": "FI",
    "module_code": "GL",
    "vendor_name": "供应商名称",
    "vendor_url": "https://example.com",
    "vendor_logo": "https://example.com/logo.png",
    "vendor_description": "供应商描述",
    "vendor_contact_name": "联系人",
    "vendor_contact_email": "email@example.com",
    "vendor_contact_phone": "13800138000",
    "vendor_contact_address": "地址",
    "vendor_contact": "support@example.com",
    "development_languages": ["rust"]
  },
  "entries": [
    "manifest.json",
    "config/**/*",
    "metadata/**/*",
    "servicedata/**/*",
    "menudata/**/*",
    "formdata/**/*",
    "mcpdata/**/*",
    "seeddata/**/*",
    "api/**/*",
    "target/wasm32-wasip1/release/*.wasm"
  ]
}
```

### 3.2 字段说明

| 字段 | 必填 | 说明 |
|------|------|------|
| `manifest_version` | 是 | 清单格式版本，固定 `"1.0"` |
| `plugin.type` | 是 | 插件类型，固定 `"wasm-plugin"` |
| `plugin.id` | 是 | 插件唯一标识，下划线命名（如 `cmx_account`） |
| `plugin.name` | 是 | 插件显示名称 |
| `plugin.version` | 是 | 语义化版本号（SemVer） |
| `plugin.description` | 否 | 插件功能描述 |
| `plugin.main_file` | 是 | WASM 入口文件名 |
| `plugin.datasource_id` | 是 | 数据源ID |
| `plugin.table_config_files` | 否 | 建表配置文件路径列表 |
| `plugin.supported_databases` | 否 | 支持的数据库类型 |
| `plugin.domain_code` | 是 | 所属域编码 |
| `plugin.application_code` | 是 | 所属应用编码 |
| `plugin.module_code` | 是 | 所属模块编码 |
| `plugin.dependencies` | 否 | 依赖的其他插件ID列表 |
| `entries` | 是 | ZIP 包内文件 glob 列表 |

### 3.3 域/应用/模块编码约定

采用三层结构：`域(Domain) → 应用(Application) → 模块(Module)`

示例（财务域）：

| 编码 | 层级 | 名称 |
|------|------|------|
| FIN | 域 | 财务域 |
| FI | 应用 | 会计核算 |
| GL | 模块 | 总账管理 |

---

## 四、代码架构

### 4.1 文件职责

| 文件 | 职责 |
|------|------|
| `lib.rs` | 模块入口，条件编译 `extism_layer` |
| `models.rs` | SDK 类型重导出 + 自定义业务模型 |
| `host_traits.rs` | `HostFunctions` trait 定义（宿主能力抽象） |
| `core.rs` | `PluginCore<H>` 业务逻辑（纯逻辑，不依赖 Extism） |
| `extism_layer.rs` | `#[plugin_fn]` 适配层，仅在 `extism` feature 下编译 |
| `tests.rs` | `MockHostFunctions` 单元测试 |

### 4.2 三层分离模式

```
core.rs（纯业务逻辑）
  ↓ 通过泛型 H: HostFunctions
host_traits.rs（抽象接口）
  ↑ impl HostFunctions for ExtismHost
extism_layer.rs（Extism 适配）
```

**设计原则**：

- `core.rs` 不知道 Extism 的存在，只依赖 `HostFunctions` trait
- `extism_layer.rs` 是薄适配层，仅做 `ExtismHost → HostCaller` 的委托
- `tests.rs` 使用 `mockall` 自动生成 `MockHostFunctions`

### 4.3 HostFunctions trait（11 个宿主能力）

| 方法 | 类别 | 说明 |
|------|------|------|
| `log_info / log_error / log_debug / log_warn` | 日志 | 四级日志 |
| `db_query` | 数据库 | 执行 SELECT 查询 |
| `db_execute` | 数据库 | 执行 INSERT/UPDATE/DELETE |
| `cache_get / cache_set / cache_delete` | 缓存 | 缓存读写删除 |
| `call_plugin` | 插件调用 | 调用其他插件函数 |
| `call_service_by_key` | 服务编排 | 调用服务编排接口 |

### 4.4 函数注释规范（必须使用 plugin-fn-doc 技能）

**重要**：所有带有 `#[plugin_fn]` 属性的函数的文档注释**必须**使用 **plugin-fn-doc** 技能生成。无论函数定义在哪个文件中（`extism_layer.rs` 或其他文件），只要使用了 `#[plugin_fn]` 属性，就必须遵循此规范。该技能确保注释格式正确，cmx-cli 能够正确解析生成 `api.json`。

不要手动编写函数注释，必须调用 `Use Skill: plugin-fn-doc` 技能。

#### func — 普通函数

```rust
/// 函数简述
///
/// # Arguments
///
/// * `input` - `RequestType` 输入参数描述。
///
/// | 字段 | 类型 | 必填 | 说明 |
/// |------|------|------|------|
/// | `field` | type | 是 | 描述 |
///
/// # Returns
///
/// 返回描述。
#[plugin_fn]
pub fn my_function(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    let core = PluginCore::new(ExtismHost);
    let output = core.my_function(&input).map_err(Error::msg)?;
    Ok(Msgpack(output))
}
```

#### branch_fn — 分支路由函数

分支路由函数用于 `skylake-switch` 节点，返回值决定走哪个分支。

```rust
/// 路由判断函数
///
/// # Returns
///
/// 返回 "1"、"2" 等，对应不同分支。
#[plugin_fn]
#[doc_type = "branch_fn"]
pub fn my_route_check(Msgpack(input): Msgpack<FunctionInput>) -> FnResult<Msgpack<FunctionOutput>> {
    // ...
}
```

### 4.5 Cargo.toml 关键配置

```toml
[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
extism-pdk = { version = "1.4.1", optional = true }
cmx-plugin-sdk = { version = "0.1.8", registry = "nora", default-features = false }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
chrono = { version = "0.4", features = ["serde"] }

[dev-dependencies]
mockall = "0.11"

[features]
extism = ["extism-pdk", "cmx-plugin-sdk/extism"]

[profile.release]
lto = true
opt-level = "s"
```

**编译命令**：

```bash
cargo build --release --target wasm32-wasip1 --features extism
```

---

## 五、技能使用指引

在开发插件时，根据不同任务使用对应技能：

| 任务 | 使用技能 | 必须性 |
|------|---------|--------|
| 编写插件函数文档注释（extism_layer.rs） | **plugin-fn-doc** | **必须** |
| 生成服务编排流程（servicedata/） | **service-orchestration-generator** | 推荐 |
| 生成表结构定义（metadata/）和种子数据（seeddata/） | **plugin-metadata-generator** | 推荐 |

### 5.1 典型开发流程

1. 确定业务需求，设计数据表结构
2. 使用 **plugin-metadata-generator** 生成 `metadata/` 和 `seeddata/` 文件
3. 创建 `config/` 配置文件，注册表定义和种子数据
4. 编写 `src/` 代码（models → host_traits → core → extism_layer → tests）
5. 使用 **service-orchestration-generator** 生成 `servicedata/` 服务编排
6. 编写 `manifest.json` 插件清单
7. 使用 **plugin-fn-doc** 规范化函数文档注释
8. 编译验证：`cargo test` + `cargo build --release --target wasm32-wasip1 --features extism`
