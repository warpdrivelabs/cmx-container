---
name: "wasm-plugin-developer"
description: "WASM 插件开发指南，介绍工程结构、目录规范、manifest.json、代码架构和技能使用指引。Invoke when 用户需要创建、开发或理解 WASM 插件工程结构时。"
---

# WASM 插件开发指南

> 基于 cmx-container 插件平台的 WASM 插件开发完整指南，采用渐进式披露方式组织。

---

## 一、工程目录结构概览

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
├── menudata/                  # 菜单配置（预留）
├── permdata/                  # 权限数据（预留）
├── flowdata/                  # 流程定义数据（预留）
├── mcpdata/                   # MCP/Skills 配置（预留）
└── src/                       # Rust 源码
    ├── lib.rs                 # 模块入口
    ├── host.rs                # HostFunctions trait 定义
    ├── models/                # 业务模型（按实体拆分）
    │   ├── mod.rs             # 模块导出 + SDK 类型重导出
    │   ├── common.rs          # 通用模型
    │   └── {entity}.rs        # 业务实体模型（按需创建）
    ├── handlers/              # 业务处理逻辑（按业务实体拆分）
    │   ├── mod.rs             # PluginCore<H> 定义
    │   └── {entity}.rs        # 业务实体的全部操作（按需创建）
    ├── extism/                # Extism 适配层（与 handlers/ 一一对应）
    │   ├── mod.rs             # ExtismHost 实现
    │   └── {entity}.rs        # 对应 handlers/ 的 #[plugin_fn] 入口
    └── tests/                 # 测试（与 handlers/ 一一对应）
        ├── mod.rs             # 公共测试工具
        └── {entity}.rs        # 对应 handlers/ 的单元测试
```

> **plugin_id 命名约束**：只能使用下划线 `_` 分隔，禁止使用连字符 `-`。
>
> - 正确：`cmx_account`、`order_plugin`、`test_plugin`
> - 错误：`cmx-account`、`order-plugin`、`test-plugin`

**src/ 目录拆分原则**：

- `handlers/`、`extism/`、`tests/` 的子文件按**业务实体**拆分，每个实体文件包含该实体的全部操作
- 例如一个"账户"实体文件中可包含：账户查询、创建、更新、删除、缓存操作、业务校验等所有账户相关逻辑
- `{entity}.rs` 是占位符，开发者根据实际业务创建对应文件，文件名不限
- 当插件只有一个业务实体时，每个目录下只有一个业务文件
- 当插件有多个业务实体时，每个实体对应一个文件
- `extism/` 和 `tests/` 的文件划分与 `handlers/` 保持一一对应
- `models/` 的实体文件与 `handlers/` 的实体文件对应，`common.rs` 存放跨实体共享的通用模型

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

## 二、代码架构概览

### 2.1 三层分离模式

```
handlers/（纯业务逻辑，按业务实体拆分）
  ↓ 通过泛型 H: HostFunctions
host.rs（抽象接口）
  ↑ impl HostFunctions for ExtismHost
extism/（Extism 适配，与 handlers/ 一一对应）
```

**设计原则**：
- `handlers/` 不知道 Extism 的存在，只依赖 `HostFunctions` trait
- `extism/` 是薄适配层，仅做 `ExtismHost → HostCaller` 的委托
- `tests/` 使用 `mockall` 自动生成 `MockHostFunctions`

### 2.2 HostFunctions trait（13 个宿主能力）

| 方法 | 类别 | 说明 |
|------|------|------|
| `log_info / log_error / log_debug / log_warn` | 日志 | 四级日志 |
| `db_query` | 数据库 | 执行 SELECT 查询 |
| `db_execute` | 数据库 | 执行 INSERT/UPDATE/DELETE |
| `cache_get / cache_set / cache_delete` | 缓存 | 缓存读写删除 |
| `call_plugin` | 插件调用 | 调用本插件函数 |
| `call_remote_plugin` | 插件调用 | 调用远程插件函数 |
| `call_service_by_key` | 服务编排 | 调用本服务编排接口 |
| `call_remote_service` | 服务编排 | 调用远程服务编排接口 |

---

## 三、技能使用指引

在开发插件时，根据不同任务使用对应技能：

| 任务                                              | 使用技能                                | 必须性    |
|-------------------------------------------------|-------------------------------------|--------|
| 编写插件函数文档注释（extism_layer.rs或者有#[plugin_fn]属性的函数） | **plugin-fn-doc**                   | **必须** |
| 生成服务编排流程（servicedata/）                          | **service-orchestration-generator** | 推荐     |
| 生成表结构定义（metadata/）和种子数据（seeddata/）              | **plugin-metadata-generator**       | 推荐     |

### 3.1 典型开发流程

1. 确定业务需求，设计数据表结构
2. 使用 **plugin-metadata-generator** 生成 `metadata/` 和 `seeddata/` 文件
3. 创建 `config/` 配置文件，注册表定义和种子数据
4. 编写 `src/` 代码（models/ → host.rs → handlers/ → extism/ → tests/）
5. 使用 **service-orchestration-generator** 生成 `servicedata/` 服务编排
6. 编写 `manifest.json` 插件清单
7. 使用 **plugin-fn-doc** 规范化函数文档注释
8. 编译验证：`cargo test` + `cargo build --release --target wasm32-wasip1 --features extism`

---

## 四、参考资料

当需要创建工程、编写 manifest.json 或编写代码时，读取详细规范：

| 场景 | 参考文档 |
|------|---------|
| 创建插件工程、配置目录结构、编写 manifest.json | [project-structure.md](references/project-structure.md) |
| 了解代码架构详情、SDK 类型、函数注释规范、Cargo.toml 配置 | [project-structure.md](references/project-structure.md) |
