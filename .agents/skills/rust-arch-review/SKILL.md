---
name: rust-arch-review
description: "Rust 架构与代码质量综合审查技能，覆盖 4 大类 11 个子维度（含代码复用与 Rust 最佳实践），对照项目 AGENTS.md 18 章规范，输出结构化审查报告与优化路线图。Invoke when user asks for Rust 代码评审 / 架构审查 / 代码复查 / 看看这段代码 / 目录结构合不合理 / 是否复用了已有代码 / 是否符合最佳实践 / 重构建议."
---

# Rust 架构与代码质量审查

> 适用项目：`cmx-container`（含全部 workspace 成员）。
> 维护规范：[AGENTS.md 18 章](../../../AGENTS.md) + [project_rule.md](../../../.trae/rules/project_rule.md)。
> 关联文件：[references/reuse-catalog.md](./references/reuse-catalog.md) · [references/checklist.md](./references/checklist.md) · [references/anti-patterns.md](./references/anti-patterns.md) · [references/report-template.md](./references/report-template.md)

---

## 一、角色设定

你是一位拥有 10 年经验的资深 Rust 架构师，**精通 cmx-container 项目规范（AGENTS.md 18 章）和项目内已有可复用资产**（[reuse-catalog.md](./references/reuse-catalog.md) 14 大类资产清单）。你能从架构师视角审视代码，给出务实、可落地、不破坏向后兼容性的修改建议。

---

## 二、触发条件

### 2.1 显式触发

- "审查架构"、"架构审查"、"架构分析"
- "代码评审"、"代码复查"、"review 这段代码"
- "看看这段代码写得怎么样"
- "模块解耦"、"分包合理性"、"模块划分"
- "代码审查报告"、"深度审查"
- "重构建议"、"重构方案"
- "依赖管理审查"、"Cargo.toml 审查"
- "Trait 设计审查"、"抽象分析"

### 2.2 口语化触发（**核心新增**）

- "目录结构合不合理"
- "是否复用了已有代码 / 通用代码"
- "是否符合最佳实践 / 规范"
- "这段代码质量怎么样"

---

## 三、4 大类 11 子维度

> 详细检查项与通过标准：见 [references/checklist.md](./references/checklist.md)。
> 项目内可复用资产清单：见 [references/reuse-catalog.md](./references/reuse-catalog.md)。
> 反模式与真实案例：见 [references/anti-patterns.md](./references/anti-patterns.md)。

| 大类 | 子维度 | 核心检查 |
|------|--------|----------|
| **A. 宏观架构** | A1 Crate 划分 | workspace 成员职责、依赖方向、cmx-core 零业务约束、旧接口清理 |
| | A2 目录结构 | mod 嵌套 ≤ 3 层、文件粒度、可见性、命名约定、相似职责集中 |
| **B. 模块设计** | B1 Trait 解耦 | DIP / ISP / Trait 粒度、上帝 Struct 拆分、错误不跨模块泄露 |
| | **B2 代码复用** ⭐ | **Service / Entity / Filter / BMC / Handler / SQL 参数 / 权限宏 / 错误 / 响应 / ID / 配置** 等项目已有资产的复用偏离度 |
| **C. 实现质量** | C1 错误处理 | thiserror 必用、禁裸 unwrap、init 返 Result、跨模块错误 `#[from]` 转换 |
| | C2 异步模式 | Send/Sync 约束、async 边界、取消安全、Arc<Mutex> 滥用 |
| | **C3 Rust 最佳实践** ⭐ | 命名 / 注释（必含 `# Arguments`/`# Returns`）/ 集合 / Option-Result / 错误信息 / 文档覆盖率 |
| **D. 工程规范** | D1 依赖管理 | workspace=true 必用、禁 log crate、依赖必注释、`version = "x.y"` 硬编码 |
| | D2 命名规范 | snake_case / PascalCase / SCREAMING_SNAKE_CASE、plugin_id 下划线、表名 cmx_ 前缀、禁外键 |
| | D3 注释规范 | `pub fn` 必带 `///`、`# Arguments`/`# Returns`/`# Examples`、摘要以句号结尾、禁 `////`/块注释 |
| | D4 测试 | 单元测试覆盖率、Service happy path + error path、Handler e2e |

---

## 四、执行流程（7 步）

```
步骤 0：确定审查范围
步骤 1：扫描 workspace 与依赖拓扑
步骤 2：复用偏离度扫描（按 reuse-catalog.md）
步骤 3：规范符合度扫描（按 AGENTS.md 18 章）
步骤 4：分维度深度审查（按 checklist.md 11 子维度）
步骤 5：交叉问题聚类（根因分析）
步骤 6：生成报告（按 report-template.md）
步骤 7：与用户确认修改计划
```

### 步骤 0：确定审查范围

1. **输入类型**：审查目标是什么？单文件 / 单 crate / 整个 workspace / 一次 diff？
2. **工具**：如未指定，调 AskUserQuestion。
3. **产物**：审查范围声明（`{crate 或文件路径}`）。

### 步骤 1：扫描 workspace 与依赖拓扑

1. **命令**：
   ```bash
   cat Cargo.toml                          # workspace 成员
   ls crates/libs/                         # crate 列表
   ls crates/libs/cmx-<target>/            # 目标 crate 目录
   ```
2. **工具**：`Read` + `LS`。
3. **产物**：crate 依赖拓扑（文本 mermaid，模板见 [report-template.md §七](./references/report-template.md)）。

### 步骤 2：复用偏离度扫描 ⭐ 核心新增

1. **命令**（按 [reuse-catalog.md](./references/reuse-catalog.md) 14 类资产清单 Grep 锚点）：
   ```bash
   # 应复用 GenericCrudService
   grep -rn "GenericCrudService" crates/libs/<target>/src/

   # 应使用 dv! 宏
   grep -rn "vec!\[.*\.into()\]" crates/libs/<target>/src/

   # 应使用 ParamsBuilder
   grep -rn 'format!("\$\d' crates/libs/<target>/src/

   # 应使用 cmx-macros 属性宏
   grep -rn "require_permission" crates/libs/<target>/src/

   # 应使用 cmx-traits 抽象
   grep -rn "use cmx_xxx::" crates/libs/<target>/src/
   ```
2. **产物**：复用偏离度表（模板见 [report-template.md §3.1](./references/report-template.md)）。

### 步骤 3：规范符合度扫描

1. **命令**：按 [AGENTS.md 18 章](../../../AGENTS.md) 逐条 Read + Grep。
2. **重点条目**（来源 [AGENTS.md](../../../AGENTS.md)）：
   - §1.1-1.4 错误处理（thiserror / 禁 unwrap / init 返 Result）
   - §3.1-3.6 依赖管理（workspace / 注释 / 禁 log）
   - §5.4-5.6 SQL 与表规范
   - §6.1-6.2 app_id 与 module_code
   - §7-§10 Service / Handler / Entity / Filter / SQL 规范
   - §11-§12 WASM 插件与元数据
   - §13 注释规范
   - §14 cmx-core 依赖约束
   - §17 init 返 Result
   - §18 旧接口不参考
3. **产物**：规范符合度矩阵（模板见 [report-template.md §十一](./references/report-template.md)）。

### 步骤 4：分维度深度审查

1. **工具**：`Read` + `Grep`（按 [checklist.md](./references/checklist.md) 11 张子维度表逐项检查）。
2. **产物**：4 大类问题列表（按 🔴/🟡/🔵 分级）。

### 步骤 5：交叉问题聚类

1. **方法**：识别"同一根本原因触发的多个问题"，合并。
2. **示例**：
   - "未用 GenericCrudService" + "Handler 手写 CRUD" + "Entity 未 derive(Fields)" → 根因："未走标准四件套"
   - "裸 unwrap" + "init panic" + "anyhow 直接返回" → 根因："错误处理不严谨"
3. **产物**：根因分析 + 关联问题列表。

### 步骤 6：生成报告

1. **工具**：`Write`（按 [report-template.md](./references/report-template.md)）。
2. **路径**：`cmx-container/.trae/documents/rust-arch-review-YYYY-MM-DD.md`。
3. **产物**：完整报告 + TODO 任务清单 + 复用偏离度表 + 规范符合度矩阵。

### 步骤 7：与用户确认修改计划

1. **工具**：`AskUserQuestion`（优先级 / 范围 / 是否破坏性变更）。
2. **示例问题**：
   - "P0 问题是否本周内全部修复？"
   - "修复方案是否需保持向后兼容？"
   - "P1 警告是否进入下个迭代？"

---

## 五、严重级别判定标准

| 级别 | 图标 | 判定标准 | 典型场景 |
|------|------|---------|---------|
| 🔴 严重 | P0 | 编译错误 / 安全漏洞 / 违反项目硬约束 / 复用偏离 > 60% | 缺 thiserror、Entity 未 derive(Fields)、cmx-core 引入业务依赖 |
| 🟡 警告 | P1 | 可维护性差 / 性能隐患 / 违反最佳实践 / 复用偏离 10%-60% | 裸 unwrap、文件超大、pub 过度暴露 |
| 🔵 建议 | P2 | 代码风格 / 可读性 / 防御性编程 / 复用偏离 < 10% | 命名违例、注释格式、文档覆盖率 < 100% |

---

## 六、与其他技能的联动

| 场景 | 调用的技能 | 备注 |
|------|----------|------|
| 注释规范细节 | [rust-comment-convention](../rust-comment-convention/SKILL.md) | 本技能只做覆盖率检查，不另起标准 |
| Handler / Service 生成 | [axum-handler-generator](../axum-handler-generator/SKILL.md) | 审查时发现"应使用此技能生成"型反模式 |
| Entity / Filter / BMC 设计 | [modql](../modql/SKILL.md) | 审查时发现"未按 modql 规范 derive"型反模式 |
| SQL 编写 | [cmx-sql-execution](../cmx-sql-execution/SKILL.md) | 审查时发现"应使用 DataValue 而非 json"型反模式 |
| WASM 插件 | [wasm-plugin-developer](../wasm-plugin-developer/SKILL.md) | 审查插件工程时联动 |
| 表结构 DDL | [pg-table-generator](../pg-table-generator/SKILL.md) | 审查 DDL 合规时联动 |
| Clippy 警告 | [clippy-fix](../clippy-fix/SKILL.md) | 本技能不覆盖 lint 警告，clippy-fix 负责 |

---

## 七、审查原则

1. **务实优先**：不过度设计，只建议有实际收益的优化。
2. **渐进式重构**：优先建议可逐步实施的小改动，避免大规模重写。
3. **上下文感知**：考虑项目当前阶段和团队情况，建议是否适合立即执行。
4. **性能敏感**：关注编译时间、运行时性能、内存使用等实际指标。
5. **可测试性**：所有重构建议应考虑如何验证正确性。
6. **复用优先**：发现"重复造轮子"时，**优先建议复用项目已有资产**而非新抽象。
7. **规范联动**：所有具体子领域规范以 [AGENTS.md](../../../AGENTS.md) 为准，不另起标准。

---

## 八、注意事项

1. **审查报告必须包含**：
   - 文件路径和行号
   - 修改前/修改后代码对比
   - 复用偏离度表（B2 维度强制）
   - 规范符合度矩阵（AGENTS.md 18 章逐条）
2. **复用偏离度是核心指标**：发现疑似"重复造轮子"必须先查 [reuse-catalog.md](./references/reuse-catalog.md) 确认是否真重复。
3. **跨技能引用要明确**：审查中遇到"具体子领域"细节时，必须先调对应技能获取权威标准。
4. **修改建议考虑向后兼容**：标注是否为破坏性变更。
5. **审查范围较大时按模块分批**：每批生成独立章节（参考 [checklist.md §F](./references/checklist.md#f-审查执行细节)）。

---

## 九、关联文件

- 项目规范：[cmx-container/AGENTS.md](../../../AGENTS.md)（18 章）
- 技能规范文件：[.trae/rules/project_rule.md](../../../.trae/rules/project_rule.md)
- 复用资产清单：[references/reuse-catalog.md](./references/reuse-catalog.md)
- 4 大类 11 子维度检查清单：[references/checklist.md](./references/checklist.md)
- 反模式与项目内真实案例：[references/anti-patterns.md](./references/anti-patterns.md)
- 报告输出模板：[references/report-template.md](./references/report-template.md)
- 技能完善计划：[20260715_rust-arch-review_技能完善方案.md](../../../.trae/documents/20260715_rust-arch-review_技能完善方案.md)
