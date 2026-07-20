---
name: "sql-guide"
description: "SQL 编写与维护指南。Invoke when 用户要求编写/维护 SQL 文件，或询问 init/migrations 目录的使用规范。"
---

# SQL 编写与维护指南

本文档定义 cmx-container 项目中 SQL 文件的编写规范和维护流程。

---

## 一、目录结构

```
docs/sql/
├── init/                      # 初始化 SQL（完整定义）
│   ├── init_ddl.sql          # 完整 DDL（表结构、索引、约束）
│   └── init_dml.sql          # 初始数据（INSERT）
│
└── migrations/                # 增量迁移 SQL（应用启动时自动执行）
    ├── 20260510_001_xxx.up.sql
    ├── 20260510_001_xxx.down.sql
    └── ...
```

---

## 二、init 目录（完整定义）

### 2.1 用途

- 供 DBA 或运维人员手动执行
- 代表数据库的**最新完整状态**
- 新人了解数据库结构的参考文档

### 2.2 init_ddl.sql 规范

**核心原则：始终保持最新，不需要 ALTER 语句**

| 操作类型   | 处理方式                  |
|--------|-----------------------|
| 新增表    | 直接添加完整 CREATE TABLE   |
| 新增字段   | 在原表定义中直接添加字段          |
| 删除字段   | 从原表定义中直接删除字段          |
| 新增索引   | 在原表定义后添加 CREATE INDEX |
| 修改字段类型 | 直接修改字段定义              |

**文件格式要求：**

- 每个表独占一个区块，使用分隔注释
- COMMENT 不换行，一行写完
- 字段定义对齐，便于阅读
- 使用 `DROP TABLE IF EXISTS` 确保可重复执行

**示例：**

```sql
-- =============================================
-- 1. 用户表 (cmx_user)
-- =============================================
DROP TABLE IF EXISTS cmx_user;
CREATE TABLE cmx_user (
    id          VARCHAR(64)  NOT NULL,
    username    VARCHAR(100) NOT NULL,
    email       VARCHAR(200),
    status      INT4         DEFAULT 1,
    create_time TIMESTAMP    DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (id)
);

COMMENT ON TABLE cmx_user IS '用户表';
COMMENT ON COLUMN cmx_user.id IS '主键ID';
COMMENT ON COLUMN cmx_user.username IS '用户名';
-- ...

CREATE UNIQUE INDEX uk_cmx_user_username ON cmx_user(username);
```

**禁止外键约束：**

- 不允许使用 `FOREIGN KEY` 约束
- 可以保留关联字段（如 `plugin_id`），但不做外键约束
- 使用 `INDEX` 替代外键保证查询性能

```sql
-- ✅ 正确：保留关联字段，使用索引
CREATE TABLE cmx_plugin_versions (
    plugin_id  VARCHAR(64) NOT NULL,
    ...
);
CREATE INDEX idx_version_plugin ON cmx_plugin_versions(plugin_id);

-- ❌ 错误：使用外键约束
CREATE TABLE cmx_plugin_versions (
    plugin_id  VARCHAR(64) NOT NULL REFERENCES cmx_plugin(id),
    ...
);
```

### 2.3 init_dml.sql 规范

- 仅包含初始数据（INSERT 语句）
- 每个数据类别使用分隔注释
- 不包含业务运行时会产生的数据

**示例：**

```sql
-- =============================================
-- 1. 域数据
-- =============================================
INSERT INTO cmx_domain (id, code, name, type, sort_order, status, archived) VALUES ('1', 'FIN', '财务域', 'business', 1, 1, 0);
INSERT INTO cmx_domain (id, code, name, type, sort_order, status, archived) VALUES ('2', 'LOG', '物流域', 'business', 2, 1, 0);
```

---

## 三、migrations 目录（增量迁移）

### 3.1 用途

- 应用启动时由代码自动执行
- 记录数据库的**增量变更历史**
- 支持版本回滚（.down.sql）

### 3.2 文件命名规范

```
<日期>_<序号>_<简短描述>.up.sql
<日期>_<序号>_<简短描述>.down.sql
```

**格式：**

- 日期：YYYYMMDD
- 序号：3 位数字，按日期从 001 开始；同一天多个文件依次递增（001、002、003...）；新日期重新从 001 开始
- 描述：建议使用下划线分隔的**中文短语**，更直观易读；如 `新增用户手机号` / `新建市场插件表`；英文短语同样允许

**示例：**

```
20260520_001_add_storage_file_id.up.sql
20260520_001_add_storage_file_id.down.sql
20260520_002_add_user_phone.up.sql
20260520_002_add_user_phone.down.sql
20260521_001_create_marketplace_table.up.sql
20260521_001_create_marketplace_table.down.sql

20260522_001_新增用户手机号.up.sql
20260522_001_新增用户手机号.down.sql
20260522_002_新建市场插件表.up.sql
20260522_002_新建市场插件表.down.sql
```

### 3.2.1 SQL 文件头注释规范

每个 `.up.sql` / `.down.sql` 文件**必须**在开头使用注释写明本次迁移的主要目的，便于审查与回溯。推荐格式：

```sql
-- =============================================
-- 迁移说明：<一句话描述本次变更做了什么>
-- 影响表：<涉及的表名，多个用逗号分隔>
-- 操作类型：<ADD COLUMN / CREATE TABLE / CREATE INDEX / INSERT / ...>
-- 回滚方式：<对应的 down.sql 文件名 或 "无">
-- =============================================

-- 实际 SQL 语句
ALTER TABLE cmx_user ADD COLUMN IF NOT EXISTS phone VARCHAR(20);
COMMENT ON COLUMN cmx_user.phone IS '手机号';
```

**要求：**

- 注释块放在文件最顶部，紧跟在注释之后立即写 SQL 语句
- `迁移说明` 用一句中文概括本次变更的目的
- 涉及多张表时在 `影响表` 列出全部表名
- `回滚方式` 必须填写对应的 `.down.sql` 文件名，若无回滚则写 `无`

### 3.2.2 同日迁移合并原则（避免序号膨胀）

**核心约束：当日 migrations 未 git commit 前，禁止新建下一个序号的迁移文件。**

- 同一天内对**未提交**的迁移变更，必须**直接编辑当日已有迁移文件**（在 .up.sql / .down.sql 中追加 SQL 区块），而不是新建下一个序号。
- 仅当满足下列条件之一才允许新建下一个序号：
  1. 当日已有迁移**已 git commit**（已产生历史记录不可再改，再追改会污染历史）
  2. 新变更是**完全独立无关的功能模块**，与当日已有迁移无逻辑关联（即便如此，也优先合并到同一文件分区）
- **禁止**出现"一天 10+ 个序号"的碎片化场景。反例：`20260718_001` ~ `20260718_011` 共 11 个序号，其中多数是同一功能的多次增量，本应合并到 1~2 个文件。

**为什么：**

- 未提交的迁移本质是"草稿"，新建序号会让同日文件数虚高、回滚顺序混乱、init_ddl 同步成本翻倍。
- 合并到同一文件后，一次 commit = 一日的工作集合，审查与回滚都更清晰。
- 同一日序号控制在 ≤3 个为佳；超过则视为设计碎片化，必须反思。

**示例：**

```
# ❌ 错误：同一天未提交就连续新建多个序号
20260718_001_cmx_flow_identity.up.sql
20260718_002_cmx_flow_cc.up.sql
20260718_003_cmx_flow_delegation.up.sql
20260718_004_cmx_flow_subflow.up.sql
... (未提交继续建到 011)

# ✅ 正确：未提交的变更合并到当日已有文件（按功能聚合成一个大文件）
20260718_001_cmx_flow_engine.up.sql     # 含 identity/cc/delegation/subflow/... 分区
20260718_001_cmx_flow_engine.down.sql

# 提交后若需新增无关变更，再用 002
20260718_002_xxx.up.sql
```

**合并写法示例**（同一 .up.sql 内用分隔注释划区块）：

```sql
-- =============================================
-- 迁移说明：cmx-flow M4 身份与转签家族（identity + cc + delegation）
-- 影响表：cmx_org, cmx_position, cmx_user_position, cmx_flow_task_candidate,
--         cmx_flow_cc, cmx_flow_task_delegation, cmx_flow_task
-- 操作类型：CREATE TABLE / ADD COLUMN / CREATE INDEX
-- 回滚方式：20260718_001_cmx_flow_engine.down.sql
-- =============================================

-- ----- 区块 1：组织/岗位/候选人池 -----
CREATE TABLE IF NOT EXISTS cmx_org (...);
...

-- ----- 区块 2：抄送记录 -----
CREATE TABLE IF NOT EXISTS cmx_flow_cc (...);
...

-- ----- 区块 3：转签 -----
ALTER TABLE cmx_flow_task ADD COLUMN IF NOT EXISTS owner_user_id VARCHAR(64);
...
```

### 3.3 up.sql 规范

**允许的操作：**

- `INSERT`（插入初始数据）
- `ALTER TABLE ADD COLUMN`
- `ALTER TABLE DROP COLUMN`
- `ALTER TABLE ALTER COLUMN TYPE`
- `CREATE INDEX / DROP INDEX`
- `CREATE TABLE / DROP TABLE`
- `COMMENT ON`

**禁止的操作：**

- 禁止修改 init_ddl.sql
- 禁止直接编辑历史迁移文件

**INSERT 幂等性要求：**

```sql
-- ✅ 必须使用 ON CONFLICT，确保可重复执行
INSERT INTO cmx_domain (id, code, name, type, sort_order, status, archived)
VALUES ('1', 'FIN', '财务域', 'business', 1, 1, 0)
ON CONFLICT (id) DO NOTHING;

INSERT INTO cmx_domain (id, code, name, type, sort_order, status, archived)
VALUES ('2', 'LOG', '物流域', 'business', 2, 1, 0)
ON CONFLICT (id) DO NOTHING;
```

**示例：**

```sql
-- 新增字段
ALTER TABLE cmx_plugin ADD COLUMN IF NOT EXISTS marketplace_source_id VARCHAR(64);
COMMENT ON COLUMN cmx_plugin.marketplace_source_id IS '市场版本来源ID';

-- 新增索引
CREATE INDEX IF NOT EXISTS idx_plugin_marketplace ON cmx_plugin(marketplace_source_id);

-- 新增表
CREATE TABLE IF NOT EXISTS cmx_marketplace_plugin (
    id VARCHAR(64) NOT NULL,
    plugin_id VARCHAR(128) NOT NULL,
    ...
);
```

### 3.3 幂等性要求

**尽量使用 `IF NOT EXISTS` 语法，确保可重复执行：**

```sql
-- ✅ 推荐：幂等
CREATE TABLE IF NOT EXISTS cmx_user (...);
CREATE INDEX IF NOT EXISTS idx_user_name ON cmx_user(name);

-- ⚠️ 仅在必要时使用
CREATE TABLE cmx_user (...);  -- 全新表，不存在时才创建
```

### 3.4 down.sql 规范

- **不是必需的**，可以没有
- 如需提供，是 up.sql 的反向操作

**示例：**

```sql
-- 回滚：删除字段
ALTER TABLE cmx_plugin DROP COLUMN IF EXISTS marketplace_source_id;

-- 回滚：删除索引
DROP INDEX IF EXISTS idx_plugin_marketplace;

-- 回滚：删除表
DROP TABLE IF EXISTS cmx_marketplace_plugin;
```

---

## 四、工作流程

### 4.1 新增功能需要数据库变更时

1. **migrations 目录**：创建 `YYYYMMDD_XXX.up.sql` 和 `.down.sql`
2. **init 目录**：同步更新 `init_ddl.sql`（将变更合并到最新表定义中）

### 4.2 修改现有表结构时

| 场景   | migrations   | init_ddl.sql |
|------|--------------|--------------|
| 新增字段 | ADD COLUMN   | 在字段列表中添加     |
| 删除字段 | DROP COLUMN  | 从字段列表中删除     |
| 新增索引 | CREATE INDEX | 在表定义后添加      |

### 4.3 注意事项

- **不要**在 migrations 中修改 init_ddl.sql
- **不要**使用 ALTER TABLE MODIFY COLUMN（用 ADD + DROP 代替）
- **不要**跳过序号（保持连续递增）
- **必须**为每个迁移提供 down.sql

---

## 五、示例：完整流程

假设需要在 `cmx_user` 表中添加 `phone` 字段：

**Step 1: 创建迁移文件**

`20260521_001_新增用户手机号.up.sql`:

```sql
-- =============================================
-- 迁移说明：在 cmx_user 表中新增 phone 字段
-- 影响表：cmx_user
-- 操作类型：ADD COLUMN
-- 回滚方式：20260521_001_新增用户手机号.down.sql
-- =============================================

ALTER TABLE cmx_user ADD COLUMN IF NOT EXISTS phone VARCHAR(20);
COMMENT ON COLUMN cmx_user.phone IS '手机号';
```

`20260521_001_新增用户手机号.down.sql`:

```sql
-- =============================================
-- 迁移说明：回滚——删除 cmx_user 表的 phone 字段
-- 影响表：cmx_user
-- 操作类型：DROP COLUMN
-- 回滚方式：无
-- =============================================

ALTER TABLE cmx_user DROP COLUMN IF EXISTS phone;
```

**Step 2: 更新 init_ddl.sql**

在 `cmx_user` 表定义中添加 `phone` 字段：

```sql
CREATE TABLE cmx_user (
    id          VARCHAR(64)  NOT NULL,
    username    VARCHAR(100) NOT NULL,
    email       VARCHAR(200),
    phone       VARCHAR(20),          -- 新增字段
    status      INT4         DEFAULT 1,
    create_time TIMESTAMP    DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (id)
);

COMMENT ON COLUMN cmx_user.phone IS '手机号';
```

---

## 六、快速检查清单

编写 SQL 时，确认以下事项：

- [ ] 文件命名符合规范（日期_序号_描述，中文描述更直观）
- [ ] 同日未提交的变更已合并到当日已有迁移文件（未新建序号，见 3.2.2）
- [ ] migrations 提供了 down.sql
- [ ] SQL 文件开头已写迁移说明 / 影响表 / 操作类型 / 回滚方式 注释块
- [ ] init_ddl.sql 已同步最新变更
- [ ] COMMENT 不换行，一行写完
- [ ] 使用 `IF NOT EXISTS` / `DROP IF EXISTS` 确保幂等
