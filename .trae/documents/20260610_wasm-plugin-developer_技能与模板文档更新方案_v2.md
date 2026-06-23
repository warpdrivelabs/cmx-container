# wasm-plugin-developer 技能与模板文档更新方案（修订版）

## 摘要

以 cmx-plugin-demo 为参考，更新 wasm-plugin-developer 技能（SKILL.md + references/project-structure.md）和 wasm-plugin-template/readme.md。核心变更：src/ 目录从扁平升级为模块化，handlers/extism/tests 按业务实体拆分（非按宿主函数类型），补充 plugin_id 命名约束，HostFunctions 从 11→13 方法。所有内容保持通用，不涉及订单等具体业务。

---

## 一、变更统计

### 需要修改的文件（3 个）

| 文件 | 路径 |
|------|------|
| 技能主文件 | `.trae/skills/wasm-plugin-developer/SKILL.md` |
| 技能参考文档 | `.trae/skills/wasm-plugin-developer/references/project-structure.md` |
| 模板说明文档 | `crates/libs/cmx-dev/templates/wasm-plugin-template/readme.md` |

### 变更点汇总（共 8 类）

| # | 变更类别 | 涉及文件 | 说明 |
|---|---------|---------|------|
| 1 | **src/ 目录结构升级** | 全部3个 | 从扁平单文件升级为模块化目录：`models/`、`handlers/`、`extism/`、`tests/` |
| 2 | **文件命名优化** | 全部3个 | `host_traits.rs` → `host.rs`，`core.rs` → `handlers/`，`extism_layer.rs` → `extism/` |
| 3 | **HostFunctions trait 方法数更新** | 全部3个 | 11 → 13，新增 `call_remote_plugin` 和 `call_remote_service` |
| 4 | **plugin_id 命名约束** | 全部3个 | 新增规则：plugin_id 只能使用下划线 `_`，禁止使用连字符 `-` |
| 5 | **.cargo/config.toml 描述修正** | readme.md | 从"Cargo 构建配置"改为"Cargo 配置（镜像源、私有 registry）" |
| 6 | **三层分离架构图更新** | 全部3个 | 反映新的目录名和模块化结构 |
| 7 | **代码示例更新** | 全部3个 | 路径引用从旧文件名改为新目录 |
| 8 | **新增函数步骤更新** | readme.md | 从"修改三个文件"改为"修改对应的目录模块" |

---

## 二、详细变更内容

### 变更 1：src/ 目录结构升级

**旧结构（扁平）**：

```
src/
├── lib.rs
├── models.rs
├── host_traits.rs
├── core.rs
├── extism_layer.rs
└── tests.rs
```

**新结构（模块化）**：

```
src/
├── lib.rs                    # 模块入口
├── host.rs                   # HostFunctions trait 定义
├── models/                   # 业务模型（按实体拆分）
│   ├── mod.rs                # 模块导出 + SDK 类型重导出
│   ├── common.rs             # 通用模型（RouteInput、OperationResult 等）
│   └── {entity}.rs           # 业务实体模型（按需创建，如 account.rs、product.rs）
├── handlers/                 # 业务处理逻辑（按业务实体拆分）
│   ├── mod.rs                # PluginCore<H> 定义
│   └── {entity}.rs           # 业务实体的全部操作（按需创建）
├── extism/                   # Extism 适配层（与 handlers/ 一一对应）
│   ├── mod.rs                # ExtismHost 实现
│   └── {entity}.rs           # 对应 handlers/{entity}.rs 的 #[plugin_fn] 入口
└── tests/                    # 测试（与 handlers/ 一一对应）
    ├── mod.rs                # 公共测试工具（make_input 等）
    └── {entity}.rs           # 对应 handlers/{entity}.rs 的单元测试
```

**拆分原则**：

- `handlers/`、`extism/`、`tests/` 的子文件按**业务实体**拆分，每个实体文件包含该实体的全部操作
- 例如一个"账户"实体文件中可包含：账户查询、创建、更新、删除、缓存操作、业务校验等所有账户相关逻辑
- `{entity}.rs` 是占位符，开发者根据实际业务创建对应文件，文件名不限
- 当插件只有一个业务实体时，每个目录下只有一个业务文件
- 当插件有多个业务实体时，每个实体对应一个文件
- `extism/` 和 `tests/` 的文件划分与 `handlers/` 保持一一对应
- `models/` 的实体文件与 `handlers/` 的实体文件对应，`common.rs` 存放跨实体共享的通用模型

### 变更 2：文件命名优化

| 旧命名 | 新命名 | 优化理由 |
|--------|--------|---------|
| `host_traits.rs` | `host.rs` | 更简洁，"traits" 后缀冗余 |
| `core.rs` | `handlers/` 目录 | "core" 语义模糊，"handlers" 明确表达"处理请求"的职责，且支持按实体拆分 |
| `extism_layer.rs` | `extism/` 目录 | "layer" 后缀冗余，目录形式支持按实体拆分 |
| `models.rs` | `models/` 目录 | 企业级插件模型多，按实体拆分更清晰 |
| `tests.rs` | `tests/` 目录 | 按实体分类测试，便于维护和扩展 |

### 变更 3：HostFunctions trait 方法数更新

新增 2 个远程调用方法：

| 方法 | 类别 | 说明 |
|------|------|------|
| `call_remote_plugin` | 插件调用 | 调用远程插件函数 |
| `call_remote_service` | 服务编排 | 调用远程服务编排接口 |

总计从 11 个方法更新为 13 个方法。完整列表：

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

### 变更 4：plugin_id 命名约束

新增规则，在以下位置强调：

1. **SKILL.md** — 在"工程目录结构概览"章节添加命名约束说明
2. **project-structure.md** — 在 manifest.json 规范的 `plugin.id` 字段说明中添加约束
3. **readme.md** — 在 manifest.json 配置说明中添加约束

规则内容：

> **plugin_id 命名约束**：只能使用下划线 `_` 分隔，禁止使用连字符 `-`。
>
> - 正确：`cmx_account`、`order_plugin`、`test_plugin`
> - 错误：`cmx-account`、`order-plugin`、`test-plugin`

### 变更 5：.cargo/config.toml 描述修正

readme.md 中的目录树注释：

- 旧：`# Cargo 构建配置`
- 新：`# Cargo 配置（镜像源、私有 registry）`

### 变更 6：三层分离架构图更新

旧图：

```
core.rs（纯业务逻辑）
  ↓ 通过泛型 H: HostFunctions
host_traits.rs（抽象接口）
  ↑ impl HostFunctions for ExtismHost
extism_layer.rs（Extism 适配）
```

新图：

```
handlers/（纯业务逻辑，按业务实体拆分）
  ↓ 通过泛型 H: HostFunctions
host.rs（抽象接口）
  ↑ impl HostFunctions for ExtismHost
extism/（Extism 适配，与 handlers/ 一一对应）
```

### 变更 7：代码示例更新

所有代码示例中的路径引用更新：

- `src/core.rs` → `src/handlers/` 对应实体文件
- `src/extism_layer.rs` → `src/extism/` 对应实体文件
- `src/host_traits.rs` → `src/host.rs`
- `src/models.rs` → `src/models/` 对应实体文件
- `src/tests.rs` → `src/tests/` 对应实体文件

### 变更 8：新增函数步骤更新

readme.md 中的"开发注意事项"：

旧：

> 新增函数必须同时修改三个文件：`models.rs`（模型）→ `core.rs`（逻辑）→ `extism_layer.rs`（暴露）

新：

> 新增函数必须同时修改对应的模块文件：
>
> 1. `models/` — 在对应实体文件中添加模型（或新建实体文件并在 mod.rs 中注册）
> 2. `handlers/` — 在对应实体文件中添加业务逻辑（或新建实体文件并在 mod.rs 中注册）
> 3. `extism/` — 在对应实体文件中添加 `#[plugin_fn]` 入口（或新建实体文件并在 mod.rs 中注册）
> 4. `tests/` — 在对应实体文件中添加单元测试

---

## 三、各文件具体修改清单

### 3.1 SKILL.md 修改清单

| 位置 | 修改内容 |
|------|---------|
| §1.1 标准目录树 | 替换 src/ 为新的模块化目录结构 |
| §1.1 目录树下方 | 新增 plugin_id 命名约束说明 |
| §2.1 三层分离模式图 | 更新为新的文件/目录名 |
| §2.2 HostFunctions trait 表格 | 从 11 个方法更新为 13 个，新增 call_remote_plugin 和 call_remote_service |
| §3.1 典型开发流程步骤4 | 更新为模块化目录下的代码编写流程 |

### 3.2 references/project-structure.md 修改清单

| 位置 | 修改内容 |
|------|---------|
| §3.1 文件职责表 | 更新为新的目录结构，每个目录说明其下文件的职责 |
| §3.3 HostFunctions trait 表格 | 从 11 个方法更新为 13 个 |
| §2.2 plugin.id 字段说明 | 添加 plugin_id 命名约束（禁止连字符 `-`，只能用下划线 `_`） |
| §3.4 函数注释规范中的代码示例 | 更新文件路径引用 |
| §3.5 Cargo.toml 关键配置 | 保持不变（已正确） |

### 3.3 readme.md 修改清单

| 位置 | 修改内容 |
|------|---------|
| 项目结构目录树 | 替换 src/ 为新的模块化目录结构 |
| 项目结构目录树 .cargo/config.toml 注释 | 改为"Cargo 配置（镜像源、私有 registry）" |
| 架构设计三层分离图 | 更新为新的目录名 |
| 编写自定义函数步骤 | 更新文件路径引用，步骤一改为 models/ 目录，步骤二改为 handlers/ 目录，步骤三改为 extism/ 目录，步骤四改为 tests/ 目录 |
| 宿主功能 API 表格 | 新增 call_remote_plugin 和 call_remote_service |
| manifest.json 配置说明 plugin.id | 添加 plugin_id 命名约束 |
| 开发注意事项第1条 | 更新为模块化目录下的修改说明 |

---

## 四、假设与决策

1. **通用性**：所有文档和技能描述保持通用，不涉及订单等具体业务
2. **向后兼容**：旧的单文件结构仍然可用，新结构是推荐的最佳实践
3. **plugin_id 约束**：在多处强调，确保开发者不会遗漏
4. **handlers 命名**：使用 "handlers" 而非 "core"，更明确表达职责
5. **目录拆分粒度**：models/handlers/extism/tests 均按业务实体拆分，handlers/ 中每个实体文件包含该实体的全部操作（CRUD、缓存、业务逻辑等），而非按宿主函数类型拆分
6. **cmx-plugin-demo 现状**：cmx-plugin-demo 当前 handlers/ 仍按宿主函数类型拆分（basic.rs/cache.rs/database.rs 等），文档描述的是推荐的最佳实践（按业务实体拆分），两者结构不同，文档不引用 cmx-plugin-demo 的具体文件名

---

## 五、实施步骤

### 步骤 1：修改 SKILL.md

按 §3.1 修改清单更新 SKILL.md，主要修改：
1. 替换 §1.1 标准目录树中的 src/ 部分
2. 在目录树下方添加 plugin_id 命名约束
3. 更新 §2.1 三层分离模式图
4. 更新 §2.2 HostFunctions trait 表格（11→13）
5. 更新 §3.1 典型开发流程步骤4

### 步骤 2：修改 references/project-structure.md

按 §3.2 修改清单更新 project-structure.md，主要修改：
1. 更新 §3.1 文件职责表
2. 更新 §3.3 HostFunctions trait 表格（11→13）
3. 在 §2.2 plugin.id 字段说明中添加命名约束
4. 更新 §3.4 函数注释规范中的代码示例路径

### 步骤 3：修改 readme.md

按 §3.3 修改清单更新 readme.md，主要修改：
1. 替换项目结构目录树
2. 修正 .cargo/config.toml 注释
3. 更新架构设计三层分离图
4. 更新编写自定义函数步骤
5. 新增宿主功能 API（call_remote_plugin/call_remote_service）
6. 添加 plugin_id 命名约束
7. 更新开发注意事项

---

## 六、验证步骤

1. 检查 SKILL.md 中的目录树反映模块化结构（按业务实体拆分）
2. 检查 HostFunctions trait 方法数为 13 个
3. 检查 plugin_id 命名约束在 3 个文件中均有体现
4. 检查所有代码示例的文件路径引用已更新
5. 检查 .cargo/config.toml 描述已修正
6. 检查内容为通用描述，不包含订单等具体业务内容
7. 检查 handlers/ 目录描述强调按业务实体拆分，而非列举固定文件名
