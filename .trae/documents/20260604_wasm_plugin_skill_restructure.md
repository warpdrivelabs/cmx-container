# wasm-plugin-developer 技能重构计划

## 背景

当前 `wasm-plugin-developer/SKILL.md` 有 384 行，包含了从工程目录结构、manifest.json 规范到代码架构的全部内容。根据 [Agent Skills 规范](https://agentskills.io/specification)的渐进式披露原则：

- SKILL.md body 应 < 5000 tokens（推荐 < 500 行）
- 详细参考材料应移到 `references/` 目录下按需加载

## 当前内容分析

| 章节 | 行数 | 性质 | 每次都需要？ |
|------|------|------|-------------|
| 一、工程目录结构全景（目录树 + 速查表） | ~50 行 | 概览 | 是 |
| 二、目录详细规范（config/metadata/seeddata/servicedata/预留） | ~80 行 | 参考 | 创建工程时 |
| 三、manifest.json 规范（完整格式 + 字段说明 + 编码约定） | ~85 行 | 参考 | 创建工程时 |
| 四、代码架构（文件职责 + 三层分离 + SDK类型 + HostFunctions + 函数注释 + Cargo.toml） | ~130 行 | 参考 | 编写代码时 |
| 五、技能使用指引（技能表 + 开发流程） | ~25 行 | 核心 | 是 |

## 重构方案

### 文件结构

```
.trae/skills/wasm-plugin-developer/
├── SKILL.md                              # 主文件（精简至 ~150 行）
└── references/
    └── project-structure.md              # 工程结构详细规范
```

### SKILL.md 保留内容（~150 行）

1. **frontmatter** — 不变
2. **概述** — 简短定位说明
3. **工程目录结构概览** — 保留目录树 + 速查表（一、1.1 + 1.2），这是理解插件的骨架信息
4. **代码架构概览** — 仅保留三层分离模式说明 + HostFunctions trait 一览表（四、4.1 + 4.2 + 4.4 的简表），不展开详细说明
5. **技能使用指引** — 保留完整（五），包括技能表和开发流程
6. **参考资料引导** — 新增，列出 references/ 下可用的文件及其适用场景

### references/project-structure.md 内容（~250 行）

从 SKILL.md 迁出的详细内容：

1. **目录详细规范** — 原二节全部（config/metadata/seeddata/servicedata/预留目录的详细说明）
2. **manifest.json 完整规范** — 原三节全部（完整 JSON 格式 + 字段说明 + 编码约定）
3. **代码架构详细规范** — 原四节展开内容：
   - 文件职责表
   - cmx-plugin-sdk 核心类型表
   - HostFunctions trait 11 个方法详情
   - 函数注释规范（func + branch_fn）+ 引用 plugin-fn-doc 技能
   - Cargo.toml 关键配置 + 编译命令

## 具体修改

### 1. 创建 `references/project-structure.md`

将原 SKILL.md 的二、三、四节的详细内容迁移至此文件，适当调整标题层级。

### 2. 精简 SKILL.md

- 保留一节（目录树 + 速查表）
- 删除二节（改为引用 references）
- 删除三节（改为引用 references）
- 保留四节中的三层分离概览 + HostFunctions 简表，删除展开内容
- 保留五节（技能使用指引）
- 新增"参考资料"小节，引导 AI 按需读取

### 3. 预期效果

- SKILL.md 从 384 行精简至 ~150 行
- AI 激活技能时只加载定位和指引信息
- 需要创建工程或编写代码时，才读取 references/project-structure.md 获取详细规范

## 验证

- 确认 SKILL.md < 500 行
- 确认 references/project-structure.md 内容完整无遗漏
- 确认 SKILL.md 中有正确的文件引用路径
