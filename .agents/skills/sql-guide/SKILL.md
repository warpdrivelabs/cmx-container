---
name: sql-guide
description: SQL 编写与维护指南（docs/sql/v2 双库目录 platform/biz、init_ddl/init_dml、migrations 增量迁移）。当用户要求编写或维护 SQL 文件、新建表（必须先询问归属主库还是业务库）、建迁移文件、或询问 init/migrations 使用规范时必用。
---

# SQL 编写与维护指南（v2 · 主库/业务库分离）

本文档定义 cmx-container 项目中 SQL 文件的编写规范和维护流程。
结构与引擎行为自 2026-08-19 起按 v2 执行；旧目录 `docs/sql/init | migrations | seed/`
是历史归档，**只读不改不再被引擎读取**。

---

## 一、目录结构与库划分规则

```
docs/sql/v2/
├── platform/                     # → 主库（[[databases]] default = true 的数据源）
│   ├── init_ddl.sql              # 全部 cmx_ 平台表全量 DDL（无损幂等）
│   ├── init_dml.sql              # 主库全部内置种子（无损幂等）
│   └── migrations/               # 引擎启动时自动执行（先跑）
└── biz/                          # → 业务库（source_type = "biz" 的数据源）
    ├── init_ddl.sql              # md_* 治理表 + mdm_activation + cmx_code_* + cmx_flow_*
    ├── init_dml.sql              # 业务库种子
    ├── migrations/               # 引擎启动时自动执行（后跑）
    └── seeds/                    # 手工种子（引擎不执行；表由模型中心部署后手工跑）
```

**新建表必须先询问归属（首要流程）：**

**新建任何表之前，必须先询问用户该表属于主库（platform）还是业务库（biz）**，
得到明确答复后再落对应目录与建表 SQL。表归属是设计决策，不能只凭表名前缀自行判定；
下述前缀规则仅在用户未明示时作为默认参考，且询问时应把它作为建议项一并呈现。

**前缀归属规则（默认参考 / 事后校验）：**

- `cmx_` 前缀表 → `platform/`（主库）；**两组例外前缀 → `biz/`（业务库）**：
  - `cmx_flow_*` 流程运行态表（与流程引擎运行时一致，`FLOW_DB_ID = "fico-db"` 即业务库）
  - `cmx_code_*` 编码引擎表（rule/gap/seq；运行时 code API 经 `resolve_db_id` 回退业务库）
- 其余前缀（`md_*`、`mdm_*`、`cf_*`、`cr_*`、`cm_*` 等业务表）→ `biz/`（业务库）
- 流程 IAM 侧表（`cmx_org`/`cmx_position`/`cmx_user_position`，候选人解析用）留
  `platform/`（引擎 `IAM_DB_ID = "primary"`）

该规则与运行时 `resolve_db_id` 的路由回退（`db_id` 头 → 业务库）一致。
例外：迁移台账 `cmx_schema_migrations` 由引擎在两个库各自创建。

**引擎行为**（`cmx-platform-app` 启动，见 `config/migration.rs`）：

1. platform 轮：默认库 + `<migration.dir>/platform/migrations/`
2. biz 轮：业务库 + `<migration.dir>/biz/migrations/`；未配置业务库时整轮跳过（不回退主库）
3. 两轮各有独立分布式锁与台账（`cmx_schema_migrations`，version 主键）

配置项 `[migration]`：`enabled`（默认 false）、`dir`（默认 `docs/sql/v2`）、
`validate_checksum`、`lock_wait_timeout`（默认 120 秒）。

---

## 二、init 文件（完整定义，手工重建/参考用）

### 2.1 用途

- 供 DBA 或运维人员手动执行；代表该库的**最新完整状态**
- 新人了解数据库结构的参考文档
- 基线迁移 = init_ddl + 各表结构对齐 ALTER + init_dml（见 2.4 基线说明）

### 2.2 init_ddl.sql 规范

**核心原则：始终保持最新 + 无损幂等（可重复执行、重跑不丢数据）+ 表定义即终态**

- 建表一律 `CREATE TABLE IF NOT EXISTS`；索引一律 `CREATE [UNIQUE] INDEX IF NOT EXISTS`
- **禁止 `DROP TABLE`**（重建式清库已废弃；需要重置环境用旧版归档脚本手工处理）
- **禁止任何 `ALTER` 语句**：字段变更直接改表定义（不留 ALTER），
  同时按第四章工作流在 migrations 新增迁移文件
- 面向新库手工重建与结构参考；**存量库升级走基线迁移**（基线内含对齐区，本文件不含）

**每表区块布局（顺序固定，COMMENT 跟随建表语句）：**

```
CREATE TABLE IF NOT EXISTS <t> (...)   -- 表定义即终态，列全
COMMENT ON TABLE/COLUMN <t>...         -- 紧跟建表，不集中后置
CREATE [UNIQUE] INDEX IF NOT EXISTS ... ON <t> (...)
```

**格式与硬约束（承 2.2 init_ddl）：**

- 每个表独占一个区块，使用分隔注释；COMMENT 不换行一行写完；字段定义对齐，便于阅读
- 禁止 `FOREIGN KEY` 外键约束；保留关联字段（如 `plugin_id`），用 `CREATE INDEX` 替代保证查询性能

### 2.3 init_dml.sql 规范

- 仅含内置数据（INSERT），按数据类别分隔注释；不含运行时产生的数据
- 每条 INSERT 必须幂等：`ON CONFLICT (...) DO NOTHING|DO UPDATE` 或 `WHERE NOT EXISTS`
- 配置类注册表（cmx_domain / cmx_application / cmx_module）沿用 `ON CONFLICT (id) DO UPDATE`
  刷新语义；其余统一 `DO NOTHING`（不覆盖运行时改动）
- `ON CONFLICT DO NOTHING`（不指定列）= 任意唯一约束命中即跳过，可用于不知键的种子

### 2.4 基线迁移与结构对齐区（仅迁移文件，init_ddl 不含）

基线迁移（如 `migrations/20260819_001_baseline.up.sql`）= init_ddl 内容 + **每表区块内
插入一段「结构对齐 ALTER」** + init_dml，布局：`CREATE TABLE → 结构对齐 ALTER →
COMMENT → 索引 → 种子`。

> 为什么基线要含对齐区而 init_ddl 不含：存量库表结构可能停在旧链中途，建表语句对
> 已存在表是 no-op 不补列，须先由对齐区（历史 `ADD COLUMN IF NOT EXISTS` 的幂等积累）
> 补齐到终态，后续 `COMMENT ON COLUMN` / 种子引用新列才不报错；新库则全部即建即过。
> **迁移文件允许 ALTER**（它就是增量变更的载体）；init_ddl 是终态快照，禁止 ALTER。

后续新增迁移照常追加到 migrations（与基线共存，按 version 排序在基线后执行）。

---

## 三、migrations 目录（增量迁移，引擎自动执行）

### 3.1 命名规范

```
<日期>_<序号>_<简短描述>.up.sql / .down.sql     （放在对应库的 migrations/ 下，如 20260820_001_新增用户手机号.up.sql）
```

- 日期 YYYYMMDD；序号 3 位、同日从 001 递增、新日重置、**禁止跳号**；建议中文短语描述
- **version（日期_序号）在各自库内全局唯一**（台账按 version 主键去重）
- 跨库变更（同时改主库表和业务库表）拆成两个文件分别落 `platform/` 与 `biz/`

### 3.2 文件头注释规范（必须）

```sql
-- =============================================
-- 迁移说明：<一句话中文描述>
-- 影响表：<全部表名，逗号分隔>
-- 操作类型：<ADD COLUMN / CREATE TABLE / INSERT / ...>
-- 回滚方式：<对应 .down.sql 文件名 或 "无">
-- =============================================
```

### 3.3 同日迁移合并原则（防序号膨胀）

**当日 migrations 未 git commit 前，禁止新建下一个序号**——未提交变更直接编辑当日
已有迁移文件追加 SQL 区块；仅当日已 commit 或变更完全独立无关时才新建序号。
同一日序号 ≤3 为佳。

### 3.4 up.sql 允许 / 禁止

**允许：** INSERT、`ALTER TABLE ADD COLUMN IF NOT EXISTS`、`ALTER COLUMN TYPE`、
`CREATE/DROP INDEX IF [NOT] EXISTS`、`CREATE TABLE IF NOT EXISTS`、`COMMENT ON`
（新列的 COMMENT 紧跟其 ADD COLUMN 之后写）

**禁止：**

- ❌ up 文件中出现 `DROP TABLE`（破坏性；废弃表走归档说明）
- ❌ 直接编辑历史迁移文件（已 commit 的）
- ❌ 修改 `docs/sql/init|migrations|seed/` 旧归档目录
- ❌ `ON CONFLICT` 缺失的裸 INSERT（重复执行必炸）

**注意：** 迁移引擎按分号切分语句（感知单引号字符串 / `$tag$` 美元引用 / `--` 注释），
但不支持 `/* */` 块注释与存储过程体；JSONB 字面量建议用 `$tag$...$tag$` 美元引用。

### 3.5 down.sql

非必需；提供则为 up 的反向操作，全部带 `IF EXISTS`。

---

## 四、变更工作流

产生数据库变更时**三步走**：

0. **新建表必先询问归属**：向用户确认该表属于主库（platform）还是业务库（biz），
   可附前缀规则建议（见第一章），用户确认后才开始写 SQL；
1. **对应库的 migrations/** 新建 `YYYYMMDD_NNN_描述.up.sql`（+ 可选 `.down.sql`），
   文件头写四行注释块；
2. **同步更新同库** `init_ddl.sql`（直接改表定义为终态，**不留 ALTER**）与
   `init_dml.sql`（种子），保持 init 与迁移链终态一致。

| 场景 | migrations | init_ddl.sql |
| --- | --- | --- |
| 新增表 | CREATE TABLE IF NOT EXISTS | 新增表区块（建表→COMMENT→索引） |
| 新增字段 | ADD COLUMN IF NOT EXISTS + COMMENT | 表定义直接加字段 + COMMENT 行（无 ALTER） |
| 新增索引 | CREATE INDEX IF NOT EXISTS | 索引随表区块 |
| 种子数据 | INSERT ON CONFLICT | init_dml 对应分类追加 |

---

## 五、示例：完整流程

给 `cmx_user`（主库表）加 `phone` 字段：

**Step 1** `docs/sql/v2/platform/migrations/20260820_001_新增用户手机号.up.sql`

```sql
-- =============================================
-- 迁移说明：cmx_user 新增 phone 字段
-- 影响表：cmx_user
-- 操作类型：ADD COLUMN
-- 回滚方式：20260820_001_新增用户手机号.down.sql
-- =============================================
ALTER TABLE cmx_user ADD COLUMN IF NOT EXISTS phone VARCHAR(20);
COMMENT ON COLUMN cmx_user.phone IS '手机号';
```

**Step 2** 同步 `platform/init_ddl.sql` 的 cmx_user 区块：表定义直接加 `phone` 字段
+ COMMENT 行加 `COMMENT ON COLUMN`（**不写 ALTER**）。

（若改的是业务库表如 `md_match_config` 或 `cmx_flow_*`，则文件落在 `docs/sql/v2/biz/...`，
流程相同。）

---

## 六、快速检查清单

- [ ] **新建表已询问用户归属**（主库 platform / 业务库 biz），并按答复落对应目录
- [ ] 表归属与前缀规则一致：`cmx_` → platform/（例外 `cmx_flow_*`/`cmx_code_*` 与非 `cmx_` → biz/）；跨库变更已拆分两文件
- [ ] 命名规范 + version 在库内唯一；同日未提交变更已合并（未新建序号）
- [ ] 文件头四行注释块齐全；up 无 DROP TABLE、无裸 INSERT
- [ ] DDL 全部 IF NOT EXISTS；种子全部 ON CONFLICT / NOT EXISTS
- [ ] 同库 init_ddl / init_dml 已同步（init_ddl 无 ALTER：表定义直接改 + COMMENT 行 + 索引）
- [ ] COMMENT 一行写完；未触碰 docs/sql 旧归档目录
