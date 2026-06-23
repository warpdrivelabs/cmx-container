# wasm-plugin-developer 技能与模板 readme 更新方案

## 摘要

以 cmx-plugin-demo 为最佳实践参考，更新 wasm-plugin-developer 技能（SKILL.md + references/project-structure.md）和 wasm-plugin-template/readme.md，主要反映目录结构从扁平到模块化的升级，以及补充 plugin\_id 命名约束。

***

## 一、变更统计

### 需要修改的文件（3 个）

| 文件     | 路径                                                                   |
| ------ | -------------------------------------------------------------------- |
| 技能主文件  | `.trae/skills/wasm-plugin-developer/SKILL.md`                        |
| 技能参考文档 | `.trae/skills/wasm-plugin-developer/references/project-structure.md` |
| 模板说明文档 | `crates/libs/cmx-dev/templates/wasm-plugin-template/readme.md`       |

### 变更点汇总（共 8 类）

| # | 变更类别                          | 涉及文件      | 说明                                                                                    |
| - | ----------------------------- | --------- | ------------------------------------------------------------------------------------- |
| 1 | **src/ 目录结构升级**               | 全部3个      | 从扁平单文件升级为模块化目录：`models/`、`handlers/`、`extism/`、`tests/`                               |
| 2 | **文件命名优化**                    | 全部3个      | `host_traits.rs` → `host.rs`，`core.rs` → `handlers/`，`extism_layer.rs` → `extism/`    |
| 3 | **HostFunctions trait 方法数更新** | 全部3个      | 11 → 13，新增 `call_remote_plugin` 和 `call_remote_service`                               |
| 4 | **plugin\_id 命名约束**           | 全部3个      | 新增规则：plugin\_id 只能使用下划线 `_`，禁止使用连字符 `-`                                               |
| 5 | **.cargo/config.toml 描述修正**   | readme.md | 从"WASM 构建配置"改为"Cargo 配置（镜像源、私有 registry）"                                             |
| 6 | **三层分离架构图更新**                 | 全部3个      | 反映新的目录名和模块化结构                                                                         |
| 7 | **代码示例更新**                    | 全部3个      | 路径引用从 `core.rs`/`extism_layer.rs`/`host_traits.rs` 改为 `handlers/`/`extism/`/`host.rs` |
| 8 | **新增函数步骤更新**                  | readme.md | 从"修改三个文件"改为"修改对应的目录模块"，说明模块化后的文件对应关系                                                  |

***

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
├── host.rs                   # HostFunctions trait
├── models/                   # 业务模型（按实体拆分）
│   ├── mod.rs                # 模块导出 + SDK 类型重导出
│   ├── {entity}.rs           # 业务实体模型（如 order.rs、product.rs）
│   └── common.rs             # 通用模型（RouteInput、OperationResult 等）
├── handlers/                 # 业务处理逻辑（按业务实体拆分）
│   ├── mod.rs                # PluginCore 定义
│   └── {entity}.rs           # 业务实体的全部操作（如 order.rs 包含订单的 CRUD + 业务逻辑）
├── extism/                   # Extism 适配层（与 handlers/ 一一对应）
│   ├── mod.rs                # ExtismHost 实现
│   └── {entity}.rs           # 对应 handlers/{entity}.rs 的 #[plugin_fn] 入口
└── tests/                    # 测试（与 handlers/ 一一对应）
    ├── mod.rs                # 公共测试工具（make_input 等）
    └── {entity}.rs           # 对应 handlers/{entity}.rs 的单元测试
```

**拆分原则**：

- `handlers/`、`extism/`、`tests/` 三个目录的子文件按**业务实体**拆分，而非按宿主函数类型拆分
- 每个业务实体文件包含该实体的全部操作（CRUD、缓存、业务逻辑等），而非将同类操作集中到一起
- 例如 `handlers/order.rs` 包含订单的查询、创建、更新、删除、缓存操作等所有订单相关逻辑
- 当插件只有一个业务实体时，每个目录下只有一个业务文件（如 `handlers/order.rs`）
- 当插件有多个业务实体时，每个实体对应一个文件（如 `handlers/order.rs` + `handlers/product.rs`）
- `extism/` 和 `tests/` 的文件划分与 `handlers/` 保持一致

### 变更 2：文件命名优化

| 旧命名               | 新命名            | 优化理由                                          |
| ----------------- | -------------- | --------------------------------------------- |
| `host_traits.rs`  | `host.rs`      | 更简洁，"traits" 后缀冗余                             |
| `core.rs`         | `handlers/` 目录 | "core" 语义模糊，"handlers" 明确表达"处理请求"的职责，且支持按功能拆分 |
| `extism_layer.rs` | `extism/` 目录   | "layer" 后缀冗余，目录形式支持按功能拆分                      |
| `models.rs`       | `models/` 目录   | 企业级插件模型多，按实体拆分更清晰                             |
| `tests.rs`        | `tests/` 目录    | 按功能分类测试，便于维护和扩展                               |

### 变更 3：HostFunctions trait 方法数更新

新增 2 个远程调用方法：

| 方法                    | 类别   | 说明         |
| --------------------- | ---- | ---------- |
| `call_remote_plugin`  | 插件调用 | 调用远程插件函数   |
| `call_remote_service` | 服务编排 | 调用远程服务编排接口 |

总计从 11 个方法更新为 13 个方法。

### 变更 4：plugin\_id 命名约束

新增规则，在以下位置强调：

1. **SKILL.md** — 在"工程目录结构概览"章节添加命名约束说明
2. **project-structure.md** — 在 manifest.json 规范的 `plugin.id` 字段说明中添加约束
3. **readme.md** — 在 manifest.json 配置说明中添加约束

规则内容：

> **plugin\_id 命名约束**：只能使用下划线 `_` 分隔，禁止使用连字符 `-`。
>
> * 正确：`cmx_account`、`order_plugin`、`test_plugin`
>
> * 错误：`cmx-account`、`order-plugin`、`test-plugin`

### 变更 5：.cargo/config.toml 描述修正

readme.md 中的目录树注释：

* 旧：`# Cargo 构建配置`

* 新：`# Cargo 配置（镜像源、私有 registry）`

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

* `src/core.rs` → `src/handlers/` 对应功能文件

* `src/extism_layer.rs` → `src/extism/` 对应功能文件

* `src/host_traits.rs` → `src/host.rs`

* `src/models.rs` → `src/models/` 对应实体文件

* `src/tests.rs` → `src/tests/` 对应功能文件

### 变更 8：新增函数步骤更新

readme.md 中的"开发注意事项"：

旧：

> 新增函数必须同时修改三个文件：`models.rs`（模型）→ `core.rs`（逻辑）→ `extism_layer.rs`（暴露）

新：

> 新增函数必须同时修改对应的模块文件：
>
> 1. `models/` — 在对应实体文件中添加模型（或新建实体文件并在 mod.rs 中注册）
> 2. `handlers/` — 在对应功能文件中添加业务逻辑（或新建功能文件并在 mod.rs 中注册）
> 3. `extism/` — 在对应功能文件中添加 `#[plugin_fn]` 入口（或新建功能文件并在 mod.rs 中注册）
> 4. `tests/` — 在对应功能文件中添加单元测试

***

## 三、各文件具体修改清单

### 3.1 SKILL.md 修改清单

| 位置                          | 修改内容                                                             |
| --------------------------- | ---------------------------------------------------------------- |
| §1.1 标准目录树                  | 替换为新的模块化 src/ 目录结构                                               |
| §1.1 目录树下方                  | 新增 plugin\_id 命名约束说明                                             |
| §2.1 三层分离模式图                | 更新为新的文件名                                                         |
| §2.2 HostFunctions trait 表格 | 从 11 个方法更新为 13 个，新增 call\_remote\_plugin 和 call\_remote\_service |
| §3.1 典型开发流程步骤4              | 更新为模块化目录下的代码编写流程                                                 |

### 3.2 references/project-structure.md 修改清单

| 位置                          | 修改内容                                     |
| --------------------------- | ---------------------------------------- |
| §3.1 文件职责表                  | 更新为新的目录结构，每个目录说明其下文件的职责                  |
| §3.3 HostFunctions trait 表格 | 从 11 个方法更新为 13 个                         |
| §2.2 plugin.id 字段说明         | 添加 plugin\_id 命名约束（禁止连字符 `-`，只能用下划线 `_`） |
| §3.4 函数注释规范中的代码示例           | 更新文件路径引用                                 |
| §3.5 Cargo.toml 关键配置        | 保持不变（已正确）                                |

### 3.3 readme.md 修改清单

| 位置                            | 修改内容                                                                          |
| ----------------------------- | ----------------------------------------------------------------------------- |
| 项目结构目录树                       | 替换为新的模块化 src/ 目录结构                                                            |
| 项目结构目录树 .cargo/config.toml 注释 | 改为"Cargo 配置（镜像源、私有 registry）"                                                 |
| 架构设计三层分离图                     | 更新为新的目录名                                                                      |
| 编写自定义函数步骤                     | 更新文件路径引用，步骤一改为 models/ 目录，步骤二改为 handlers/ 目录，步骤三改为 extism/ 目录，步骤四改为 tests/ 目录 |
| 宿主功能 API 表格                   | 新增 call\_remote\_plugin 和 call\_remote\_service                               |
| manifest.json 配置说明 plugin.id  | 添加 plugin\_id 命名约束                                                            |
| 开发注意事项第1条                     | 更新为模块化目录下的修改说明                                                                |

***

## 四、假设与决策

1. **通用性**：所有文档和技能描述保持通用，不涉及订单等具体业务
2. **向后兼容**：旧的单文件结构仍然可用，新结构是推荐的最佳实践
3. **plugin\_id 约束**：在多处强调，确保开发者不会遗漏
4. **handlers 命名**：使用 "handlers" 而非 "core"，更明确表达职责
5. **目录拆分粒度**：models/handlers/extism/tests 均按业务实体拆分，handlers/ 中每个实体文件包含该实体的全部操作（CRUD、缓存、业务逻辑等）

***

## 五、验证步骤

1. 检查 SKILL.md 中的目录树与 cmx-plugin-demo 实际结构一致
2. 检查 HostFunctions trait 方法数为 13 个
3. 检查 plugin\_id 命名约束在 3 个文件中均有体现
4. 检查所有代码示例的文件路径引用已更新
5. 检查 .cargo/config.toml 描述已修正
6. 检查内容为通用描述，不包含订单等具体业务内容

