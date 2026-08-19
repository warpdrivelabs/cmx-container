# CMX 数据库 SQL v2 —— 主库 / 业务库分离结构

> 本目录是 SQL 维护的**现行结构**（2026-08-19 起）。旧结构 `docs/sql/init| migrations | seed/`
> 保留为历史归档，**不再维护、不再被迁移引擎读取**；新变更一律落在本目录。

## 一、目录结构与库划分

```
docs/sql/v2/
├── platform/                     # → 主库（[[databases]] default = true 的数据源）
│   ├── init_ddl.sql              # 全部 cmx_ 平台表全量 DDL（无损幂等）
│   ├── init_dml.sql              # 主库全部内置种子（无损幂等）
│   └── migrations/               # 引擎启动时自动执行（先跑）
│       └── 20260819_001_baseline.up.sql
└── biz/                          # → 业务库（source_type = "biz" 的数据源）
    ├── init_ddl.sql              # md_* 治理表 11 张 + cmx_flow_* 流程运行态表 15 张
    ├── init_dml.sql              # MDM 治理种子
    ├── migrations/               # 引擎启动时自动执行（后跑）
    │   └── 20260819_001_baseline.up.sql
    └── seeds/                    # 手工种子（引擎不执行）
        ├── cf_dct_seed.sql       #   cf_* 总账字典（表由模型中心部署后执行）
        ├── cr_dict_seed.sql      #   cr_* 报表字典
        ├── cr_report_seed.sql    #   cr_* 报表清单
        ├── gen_cr_report_seed.py #   ↑ 的生成器
        └── demo/                 #   演示数据（非初始化必跑）
```

**表归属规则**：

- `cmx_` 前缀表 → `platform/`（主库）；**例外：`cmx_flow_*` 流程运行态表 → `biz/`**
  （与流程引擎运行时一致——`cmx-flowengine` 的 `FLOW_DB_ID = "fico-db"` 即业务库；
  流程引擎启动时还会在业务库 `ensure_schema` 自建这套表）。
- 其余前缀（`md_*`、`cf_*`、`cr_*`、`cm_*` 等）→ `biz/`（业务库）。
- 流程的 IAM 侧表（`cmx_org` / `cmx_position` / `cmx_user_position`，候选人解析用）
  与 `cmx_user`/`cmx_role` 同库，留 `platform/`（引擎 `IAM_DB_ID = "primary"`）。
- 这与运行时 `resolve_db_id` 的路由回退（`db_id` 头 → 业务库）一致。
  例外：迁移台账表 `cmx_schema_migrations` 由引擎在**两个库各自**创建，跟随目标库。

**每表区块布局**（init_ddl 与基线内）：`CREATE TABLE → 结构对齐 ALTER → COMMENT → 索引`。
COMMENT 跟随建表语句；结构对齐 ALTER（历史 `ADD COLUMN IF NOT EXISTS` 的幂等积累）
插在建表后、COMMENT 前——存量库表结构停留在旧链中途时先补列，后续 COMMENT / 种子
引用缺失列才不报错；新库则全部即建即过。

**cf_*/cr_*/cm_\* 表的 DDL 不在本目录**——由模型中心/插件运行时部署；对应种子放
`biz/seeds/`，需在建表完成后手工执行。

## 二、引擎行为（cmx-platform-app 启动时）

1. **platform 轮**：对默认库执行 `<migration.dir>/platform/migrations/` 下的待执行迁移
   （分布式锁键 `cmx:database:migration:platform`）。
2. **biz 轮**：对业务库执行 `<migration.dir>/biz/migrations/`（锁键
   `cmx:database:migration:biz`）。**未配置业务库时整轮跳过，不回退主库**。
3. 台账表 `cmx_schema_migrations`（version 主键）在两库各自记录；主库的旧链记录
   （20260501~20260818）与新基线（20260819_001）版本号不冲突、共存。

配置（`[migration]`）：`enabled`（默认 false）、`dir`（默认 `docs/sql/v2`）、
`validate_checksum`、`lock_wait_timeout`（默认 120 秒，抢不到锁时等其他节点的
轮询超时；等待结束按台账决定跳过或接管，最多 3 轮）。

> `dir` 是相对服务进程工作目录的路径：cmx-container 内服务用 `docs/sql/v2`；
> cmx-portalservice（工作目录不同）须写 `../cmx-container/docs/sql/v2`。
> 引擎内部再拼 `<dir>/platform/migrations` 与 `<dir>/biz/migrations`
> （MigrationLoader 为**非递归**单目录扫描，目录必须拼到 `migrations` 子目录层）。

## 三、幂等规范（所有新 SQL 必须遵守）

- DDL：`CREATE TABLE / CREATE INDEX / CREATE UNIQUE INDEX IF NOT EXISTS`；
  `ALTER TABLE ... ADD COLUMN IF NOT EXISTS`。**禁止** up 文件中出现 `DROP TABLE`。
- DML：`INSERT ... ON CONFLICT (...) DO NOTHING|DO UPDATE` 或 `WHERE NOT EXISTS` 防重。
  `ON CONFLICT DO NOTHING`（不指定列）= 任意唯一约束命中即跳过，可用于不知键的种子。
- 基线/种子统一**无损幂等**：重复执行零报错、不丢数据、不覆盖运行时已改数据
  （DAM 注册表 cmx_domain/application/module 例外，沿用 20260714 的 DO UPDATE 刷新语义）。

## 四、变更工作流

产生数据库变更时两步走（与 sql-guide 技能一致）：

1. 在对应库的 `migrations/` 新建 `YYYYMMDD_NNN_描述.up.sql`（+ 可选 `.down.sql`），
   文件头写四行注释块（迁移说明 / 影响表 / 操作类型 / 回滚方式）；
2. 同步更新同库 `init_ddl.sql` / `init_dml.sql` 为最新完整状态。

## 五、存量环境升级指引（重要）

- **md_* 与 cmx_flow_* 表迁库**：旧链把 `md_*`、`cmx_flow_*` 建在**主库**；v2 基线在
  **业务库**新建。存量环境切换 v2 后：
  - 若主库曾有这两组表的数据（现网 cmxlocal 实测无 `md_*`，`cmx_flow_*` 由流程引擎
    在业务库自建），需一次性搬运到业务库（各表无外键，按表 `INSERT INTO ... SELECT`
    搬运后核对行数，再酌情归档主库旧表）；
  - 主库遗留的 `cmx_flow_*` 旧表确认业务库数据完整后可手工 `DROP`（platform 基线
    不再触碰它们）。
- 主库跑 v2 platform 基线：全部 `IF NOT EXISTS` / `ON CONFLICT` + 区块内对齐 ALTER，
  对已有库为幂等 no-op / 结构补齐，安全。
- 旧索引名差异：线上库由旧链建的索引名（如 `uk_cmx_core_application_code`）与 v2 终态名
  （`uk_cmx_coreapplication_code`）可能并存，冗余无害，可择机手工清理。

## 六、基线构成溯源

platform 基线 = 旧 `init/init_ddl.sql` 终态（cmx_ 平台表 52 张，不含 flow）
+ 补丁（20260501 的 idx_version_current）
+ 每表区块内结构对齐（迁移链历史 ALTER / 部分唯一索引重建）
+ 种子（dam注册 7/11/10 + 角色 3 + 权限 24 + 菜单 147+5 + 编码规则 15 + 激活映射 26）。

biz 基线 = `md_*` 11 表 + `cmx_flow_*` 15 表（13 张来自旧 init_ddl 流程段 +
补丁 2 张 cmx_flow_biz_link / cmx_flow_task_comment）+ 治理种子（查重规则 1+13、
分发水位 1）+ cr_report_sheet 索引修正（原 20260720_001）。

有意不迁移（覆盖核对白名单）：
- 5 张死表 `cmx_plugin_nodes/features/dependencies/deployments`、`cmx_system_plugins`
  （无运行时代码引用，旧 init_ddl 已注释废弃）；
- 7 个历史改名/等价索引（详见核对脚本 `/tmp/coverage_check.py` 白名单注释）；
- `example/cmxold` 遗留示例、`.trae` 设计文档配套 DDL（从未进入维护链）。
