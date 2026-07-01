# 模块为中心的插件解耦与迁移包方案 (V3)

> **本方案性质:** 架构设计与实施方案。回答:
> 1. **修改哪些代码** 把表单/菜单/元数据/权限从插件包内提取到模块层;
> 2. **如何生成模块的迁移包** 实现环境间整体迁移。
>
> **V3 增量(基于反馈):**
> - **模块迁移包引入完整版本管理**(§3.0):主表版本字段 + 版本历史表 + 时间戳 + 导入校验,**避免旧版本覆盖新版本**。
>
> **V2 关键约束(基于反馈):**
> - **保留插件单独安装的接口和功能**(`POST /api/plugin/install` / `/api/plugin/deploy` 等全部保留),仅**注释掉**旧的"元数据初始化"和"菜单/权限/表单分发到其他服务"的相关代码,不删除。
> - 模块的导出/导入是**独立新增**的功能,不破坏现有插件安装/升级/卸载链路。
> - 模块内插件的安装/升级/卸载**复用现有** `InstallService` / `PluginOperationExecutor` 代码。

---

## 一、目标与核心思想

把"插件大聚合包"拆解,使 **模块(module)** 成为环境迁移和资源归属的顶层原子单位。模块下挂五个平级资源:

```
模块 (cmx_module)
├── 表单 forms       (cmx_form)              ← 新增独立表
├── 菜单 menus       (cmx_menu)              ← 新增独立表
├── 元数据 metadata  (cmx_meta_table_define) ← 已有,归属语义调整
├── 权限 permissions (cmx_permission)        ← 已有,字段重命名
└── 插件 plugins     (cmx_plugin)            ← 已有,瘦身后只含 编排+wasm+wit
```

### 核心思想

- **归属字段升格**:所有资源统一以 `domain_code + application_code + module_code` 三段式挂到模块下。
- **资源实体化**:表单、菜单新建独立持久化表(目前表单无表、菜单折叠在权限表内)。
- **插件瘦身(新包格式)**:新格式的插件包内移除 `formdata/ menudata/ permdata/`、部分 `metadata/`(建表配置);**保留** `manifest.json + servicedata/ + *.wasm + wit/ + api/api.json + seeddata/`。
  > **seeddata 全部留在插件包内:** 所有 seeddata(含 `domain_seed/application_seed/module_seed` 字典种子,以及 `account_seed` 等插件业务表种子)**统一保留在插件包内**,不迁移到模块层。理由:模块包没有 seeddata 的存储位置,导出模块包时也无法从 DB 还原 seeddata 的原始文件内容。模块包只负责表单/菜单/元数据/权限/插件子包的聚合。
- **旧链路保留**:插件单独安装功能保留,但其中"建表/元数据初始化"和"分发到外部中心"的代码块**注释掉**(未来由模块包统一处理)。

---

## 二、模块包结构(目标态)

```
module_FIN_GL_20260630103000.zip       # 模块迁移包(单一聚合 zip,版本=导出时间戳)
├── module.manifest.json               # 模块聚合清单(顶层入口)
├── module.json                        # 模块定义(code/name/domain/application/description)
├── forms/                             # 表单
│   ├── voucher_form.json
│   └── account_form.json
├── menus/                             # 菜单(树形)
│   └── gl_menu.json
├── metadata/                          # 表元数据(建表 DDL 配置)
│   ├── domain_app_module_config.json  # 域/应用/模块配置
│   └── tables/                        # 表定义 JSON
│       ├── account_table.json
│       └── voucher_table.json
├── permissions/                       # 权限树(api/menu/button)
│   └── gl_permissions.json
└── plugins/                           # 模块下的多个插件
    ├── plugin_gl_posting.zip
    │   ├── manifest.json
    │   ├── servicedata/*.json         # 服务编排
    │   ├── *.wasm
    │   ├── wit/*.wit
    │   ├── api/api.json
    │   └── seeddata/*.json|csv        # ⭐ 所有种子数据保留在插件包内(含字典种子+业务表种子)
    └── plugin_gl_report.zip
```

### module.manifest.json

```json
{
  "manifest_version": "1.0",
  "module": {
    "code": "GL",
    "name": "总账",
    "domain_code": "FIN",
    "application_code": "FI",
    "description": "财务域-财务应用-总账模块"
  },
  "package_version": "20260630103000",
  "resources": {
    "forms":       ["forms/voucher_form.json", "forms/account_form.json"],
    "menus":       ["menus/gl_menu.json"],
    "metadata":    ["metadata/domain_app_module_config.json", "metadata/tables/*.json"],
    "permissions": ["permissions/gl_permissions.json"]
  },
  "plugins": [
    { "id": "plugin_gl_posting", "version": "1.0.0", "package": "plugins/plugin_gl_posting.zip" },
    { "id": "plugin_gl_report",  "version": "1.0.0", "package": "plugins/plugin_gl_report.zip" }
  ],
  "checksum": "sha256:...",
  "signature_algorithm": "ed25519",
  "signature": "...",
  "signer_key_id": "..."
}
```

> **版本字段说明:** `package_version`(导出时间戳 `yyyyMMddHHmmSS`)由导出服务**自动生成**,无需手动输入。模块包文件名 = `module_{domain}_{module}_{package_version}.zip`,如 `module_FIN_GL_20260630103000.zip`。

---

## 三、数据库变更

遵循 `sql-guide`:新增表时**同时**创建 `migrations/YYYYMMDD_NNN_xxx.up.sql` / `.down.sql` 并同步更新 `docs/sql/init/init_ddl.sql`。遵循 `pg-table-generator`:主键 `id varchar(64)`、审计字段 `archived int4 DEFAULT 0` 等、**无外键约束**(用索引替代)。

下一个迁移序号为 `20260701_001`(现有最高 `20260624_008`)。

### 3.0 ⭐ 模块版本管理(新增,避免旧版本覆盖新版本)

**问题:** `cmx_module` 是字典表(仅 code/name/description 等元信息),**不应**承载版本/导入状态等运行期字段。但导入迁移包需要版本校验能力,否则旧版本会无校验覆盖新版本。

**方案选择(已评估):**

| 维度 | 方案 A:新建 `cmx_module_current_version` 表 ✅ 采用 | 方案 B:用 `cmx_module_version_history.is_current` 标记 |
|---|---|---|
| 字典表纯净度 | ✅ 不碰 `cmx_module` | ✅ 不碰 `cmx_module` |
| 导入校验读路径(高频) | ✅ 读 current 表(固定一行) | ⚠️ 扫不断增长的历史表 |
| 数据冗余 | ⚠️ current 与历史"当前行"轻量冗余 | ✅ 单一数据源 |
| 一致性维护 | ⚠️ 写两张表(事务保护) | ✅ 单表翻转 is_current |
| 查询当前版本 | ✅ 直接查一行 | ⚠️ `WHERE module_code=? AND is_current=1` |
| 对齐 `cmx_plugin` 范式 | ✅ 等价于插件"主表版本字段+历史表"的独立拆分 | ✅ 同 `cmx_plugin_versions.is_current` |

**采用方案 A**:三表各司其职 —— `cmx_module`(字典) / `cmx_module_current_version`(当前态,一行) / `cmx_module_version_history`(历史,多行)。理由:导入校验是高频读路径,读固定小表优于扫历史表;字典表保持纯净。

版本号 = 导出时间戳 `yyyyMMddHHmmSS`,**自动生成、单调递增**,导入时通过字符串比较判定新旧,旧版本被拒绝(除非显式 `force=true`)。

#### 3.0.1 新建 `cmx_module_current_version` 当前版本表(模块维度的"当前态")
**新增迁移:** `docs/sql/migrations/20260701_004_cmx_module_current_version.up.sql`

```sql
-- 模块当前版本表：记录每个模块当前生效的迁移包版本(每个 module_code 一行)
-- cmx_module 保持字典表纯净，版本运行态由本表承载
CREATE TABLE IF NOT EXISTS cmx_module_current_version (
    -- 主键
    id VARCHAR(64) NOT NULL,

    -- 业务字段
    module_id VARCHAR(64) NOT NULL,
    domain_code VARCHAR(64) NOT NULL,
    application_code VARCHAR(64) NOT NULL,
    module_code VARCHAR(64) NOT NULL,
    package_version VARCHAR(14) NOT NULL,
    checksum VARCHAR(128),
    manifest_snapshot JSONB,
    imported_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    imported_by VARCHAR(100),
    source VARCHAR(256),

    -- 标准审计字段
    archived INT4 DEFAULT 0,
    create_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    update_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    create_by VARCHAR(100),
    create_name VARCHAR(100),
    update_by VARCHAR(100),
    update_name VARCHAR(100),

    CONSTRAINT pk_cmx_module_current_version PRIMARY KEY (id),
    CONSTRAINT uk_cmx_module_current_version_module UNIQUE (module_code)
);

CREATE INDEX IF NOT EXISTS idx_cmx_module_current_version_dom_app_mod
    ON cmx_module_current_version (domain_code, application_code, module_code);

COMMENT ON TABLE cmx_module_current_version IS '模块当前版本表：每个模块当前生效的迁移包版本';
COMMENT ON COLUMN cmx_module_current_version.id IS '主键ID';
COMMENT ON COLUMN cmx_module_current_version.module_id IS '关联模块ID(逻辑关联cmx_module.id)';
COMMENT ON COLUMN cmx_module_current_version.domain_code IS '域编码';
COMMENT ON COLUMN cmx_module_current_version.application_code IS '应用编码';
COMMENT ON COLUMN cmx_module_current_version.module_code IS '模块编码(唯一，一个模块一行)';
COMMENT ON COLUMN cmx_module_current_version.package_version IS '当前迁移包版本号(导出时间戳yyyyMMddHHmmSS)';
COMMENT ON COLUMN cmx_module_current_version.checksum IS '当前迁移包校验和sha256';
COMMENT ON COLUMN cmx_module_current_version.manifest_snapshot IS '当前module.manifest.json快照';
COMMENT ON COLUMN cmx_module_current_version.imported_at IS '最近一次导入时间';
COMMENT ON COLUMN cmx_module_current_version.imported_by IS '最近一次导入人';
COMMENT ON COLUMN cmx_module_current_version.source IS '来源(文件名/URL)';
COMMENT ON COLUMN cmx_module_current_version.archived IS '归档标志：0-未归档，1-已归档';
COMMENT ON COLUMN cmx_module_current_version.create_time IS '创建时间';
COMMENT ON COLUMN cmx_module_current_version.update_time IS '更新时间';
COMMENT ON COLUMN cmx_module_current_version.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_module_current_version.create_name IS '创建人姓名';
COMMENT ON COLUMN cmx_module_current_version.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_module_current_version.update_name IS '更新人姓名';
```

**down:** `DROP TABLE IF EXISTS cmx_module_current_version;`
**同步 `init_ddl.sql`:** 在表 3 `cmx_module` 之后追加本表区块。
> 关键:唯一约束 `uk_cmx_module_current_version_module(module_code)` 保证一个模块只有一行当前版本。

#### 3.0.2 新建 `cmx_module_version_history` 版本历史表
**新增迁移:** `docs/sql/migrations/20260701_005_cmx_module_version_history.up.sql`

仿照 `cmx_plugin_versions`(`init_ddl.sql:286-326`)的结构:

```sql
-- 模块版本历史表：记录模块迁移包的完整导入历史(对齐 cmx_plugin_versions)
CREATE TABLE IF NOT EXISTS cmx_module_version_history (
    -- 主键
    id VARCHAR(64) NOT NULL,

    -- 业务字段
    module_id VARCHAR(64) NOT NULL,
    domain_code VARCHAR(64) NOT NULL,
    application_code VARCHAR(64) NOT NULL,
    module_code VARCHAR(64) NOT NULL,
    package_version VARCHAR(14) NOT NULL,
    checksum VARCHAR(128),
    manifest_snapshot JSONB,
    imported_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    imported_by VARCHAR(100),
    source VARCHAR(256),
    notes TEXT,

    -- 标准审计字段
    archived INT4 DEFAULT 0,
    create_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    update_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    create_by VARCHAR(100),
    create_name VARCHAR(100),
    update_by VARCHAR(100),
    update_name VARCHAR(100),

    CONSTRAINT pk_cmx_module_version_history PRIMARY KEY (id),
    CONSTRAINT uk_cmx_module_version_history_pkg UNIQUE (module_code, package_version)
);

CREATE INDEX IF NOT EXISTS idx_cmx_module_version_history_module ON cmx_module_version_history (module_id);
CREATE INDEX IF NOT EXISTS idx_cmx_module_version_history_pkg ON cmx_module_version_history (module_code, package_version);

COMMENT ON TABLE cmx_module_version_history IS '模块迁移包版本历史表';
COMMENT ON COLUMN cmx_module_version_history.id IS '主键ID';
COMMENT ON COLUMN cmx_module_version_history.module_id IS '关联模块ID(逻辑关联cmx_module.id)';
COMMENT ON COLUMN cmx_module_version_history.domain_code IS '域编码';
COMMENT ON COLUMN cmx_module_version_history.application_code IS '应用编码';
COMMENT ON COLUMN cmx_module_version_history.module_code IS '模块编码';
COMMENT ON COLUMN cmx_module_version_history.package_version IS '迁移包版本号(导出时间戳yyyyMMddHHmmSS)';
COMMENT ON COLUMN cmx_module_version_history.checksum IS '迁移包校验和sha256';
COMMENT ON COLUMN cmx_module_version_history.manifest_snapshot IS '导入时的module.manifest.json快照';
COMMENT ON COLUMN cmx_module_version_history.imported_at IS '导入时间';
COMMENT ON COLUMN cmx_module_version_history.imported_by IS '导入人';
COMMENT ON COLUMN cmx_module_version_history.source IS '来源(文件名/URL)';
COMMENT ON COLUMN cmx_module_version_history.notes IS '备注';
COMMENT ON COLUMN cmx_module_version_history.archived IS '归档标志：0-未归档，1-已归档';
COMMENT ON COLUMN cmx_module_version_history.create_time IS '创建时间';
COMMENT ON COLUMN cmx_module_version_history.update_time IS '更新时间';
COMMENT ON COLUMN cmx_module_version_history.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_module_version_history.create_name IS '创建人姓名';
COMMENT ON COLUMN cmx_module_version_history.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_module_version_history.update_name IS '更新人姓名';
```

**down:** `DROP TABLE IF EXISTS cmx_module_version_history;`
**同步 `init_ddl.sql`:** 在当前版本表之后追加本表区块。

> **两表协作:** 导入时,`current_version` 表 `upsert`(唯一约束保证一行) + `version_history` 表 `INSERT`(唯一约束 `(module_code, package_version)` 防重)。查询"当前版本"读 current 表,查询"历史"读 history 表。

#### 3.0.3 版本号设计:纯时间戳自动生成(无需手动输入)

迁移包的版本号**只有一个**:`package_version`,格式为 **`yyyyMMddHHmmSS`**(纯数字时间戳),由导出服务在导出时**自动生成**,调用方无需也不允许手动指定。

| 字段 | 生成方式 | 格式 | 示例 |
|---|---|---|---|
| `package_version` | 导出时刻 = `chrono::Local::now().format("%Y%m%d%H%M%S")` | 14 位纯数字 | `20260630103000` |

**为什么用纯时间戳:**
- **单调递增**:时间戳天然递增,版本比对只需字符串/数值比较,**不需要 SemVer 解析**。旧包(`20260630103000`)**永远小于**新包(`20260630143000`),直接字符串比较即可判定新旧。
- **无需人工维护**:导出方不需要记版本号、不需要决定升大版本还是小版本,避免人为错误。
- **唯一标识一次导出**:14 位精度(秒级)足以区分连续导出;若同秒内重复导出,由 `checksum` 兜底去重。
- **可读**:看版本号即可知导出时间(2026-06-30 10:30:00)。

模块包文件名 = `module_{domain}_{module}_{package_version}.zip`,如 `module_FIN_GL_20260630103000.zip`。

#### 3.0.4 导入校验规则(ModuleInstallService 内实现)

版本比对改为**纯字符串比较**(时间戳定长 14 位,字典序 == 数值序),当前版本从 `cmx_module_current_version` 表读取:

```
导入模块包时：
  1. 解析 manifest → 得到 package_version(14位时间戳) + checksum
  2. 查 cmx_module_current_version WHERE module_code=? (一行当前版本)
  3. 版本校验：
     - 若 checksum == 当前 checksum          → 幂等跳过，返回 "已是当前版本"
     - 若 package_version == 当前 package_version 但 checksum 不同
                                             → 同秒重复导出的不同内容，按"同秒补丁"处理，
                                               更新当前版本，记入历史
     - 若 package_version < 当前 package_version
                                             → 默认拒绝，错误 "无法用旧版本(20260630103000)
                                               覆盖当前版本(20260630143000)"
                                               (force=true 时放行，记入历史标记为 downgrade)
     - 若 package_version > 当前 package_version
                                             → 升级导入(正常路径)
  4. 通过校验后(事务内)：
     a. cmx_module_version_history 插入新历史记录(唯一约束防重)
     b. cmx_module_current_version upsert(唯一约束保证一行):
        package_version / imported_at / checksum / manifest_snapshot
  5. 安装资源 + 插件子包
```

> **核心保证:** 旧版本(`package_version` 字典序更小)**不会**默认覆盖新版本。纯时间戳保证每次导出版本号唯一且单调递增,导入校验只需字符串比较。当前版本读 `cmx_module_current_version`(固定一行,高频读高效),完整历史读 `cmx_module_version_history`,`cmx_module` 字典表保持纯净。

### 3.1 新建 `cmx_form` 表

**新增迁移文件:** `docs/sql/migrations/20260701_001_cmx_form.up.sql`

```sql
-- 表单定义表(模块级一等公民)
CREATE TABLE IF NOT EXISTS cmx_form (
    -- 主键
    id VARCHAR(64) NOT NULL,

    -- 业务字段
    code VARCHAR(128) NOT NULL,
    name VARCHAR(256) NOT NULL,
    description TEXT,
    definition JSONB,
    version VARCHAR(64) DEFAULT '1.0.0',
    domain_code VARCHAR(64) NOT NULL,
    application_code VARCHAR(64) NOT NULL,
    module_code VARCHAR(64) NOT NULL,
    status INT4 DEFAULT 1,

    -- 标准审计字段
    archived INT4 DEFAULT 0,
    create_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    update_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    create_by VARCHAR(100),
    create_name VARCHAR(100),
    update_by VARCHAR(100),
    update_name VARCHAR(100),

    CONSTRAINT pk_cmx_form PRIMARY KEY (id),
    CONSTRAINT uk_cmx_form_code UNIQUE (code)
);

CREATE INDEX IF NOT EXISTS idx_cmx_form_module ON cmx_form (domain_code, application_code, module_code);

COMMENT ON TABLE cmx_form IS '表单定义表';
COMMENT ON COLUMN cmx_form.id IS '主键ID';
COMMENT ON COLUMN cmx_form.code IS '表单编码，模块内唯一，如 gl:voucher_form';
COMMENT ON COLUMN cmx_form.name IS '表单名称';
COMMENT ON COLUMN cmx_form.description IS '表单描述';
COMMENT ON COLUMN cmx_form.definition IS '表单完整定义JSON(字段/布局/校验)';
COMMENT ON COLUMN cmx_form.version IS '表单版本';
COMMENT ON COLUMN cmx_form.domain_code IS '所属域编码';
COMMENT ON COLUMN cmx_form.application_code IS '所属应用编码';
COMMENT ON COLUMN cmx_form.module_code IS '所属模块编码';
COMMENT ON COLUMN cmx_form.status IS '状态：0-禁用，1-启用';
COMMENT ON COLUMN cmx_form.archived IS '归档标志：0-未归档，1-已归档';
COMMENT ON COLUMN cmx_form.create_time IS '创建时间';
COMMENT ON COLUMN cmx_form.update_time IS '更新时间';
COMMENT ON COLUMN cmx_form.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_form.create_name IS '创建人姓名';
COMMENT ON COLUMN cmx_form.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_form.update_name IS '更新人姓名';
```

**down:** `docs/sql/migrations/20260701_001_cmx_form.down.sql` → `DROP TABLE IF EXISTS cmx_form;`

**同步 `init_ddl.sql`:** 在文件末尾(表 32 `cmx_permission` 之后)追加表 33 区块,字段定义与上同(用 `DROP TABLE IF EXISTS cmx_form;` 起头,符合 init 规范)。

### 3.2 新建 `cmx_menu` 表

**新增迁移文件:** `docs/sql/migrations/20260701_002_cmx_menu.up.sql`

```sql
-- 菜单定义表(模块级一等公民，独立于权限表)
-- 树形结构字段(parent_id/parent_code/full_path/is_leaf/level)对齐 cmx_permission 的命名约定
CREATE TABLE IF NOT EXISTS cmx_menu (
    -- 主键
    id VARCHAR(64) NOT NULL,

    -- 业务字段
    code VARCHAR(128) NOT NULL,
    name VARCHAR(256) NOT NULL,
    parent_id VARCHAR(64),
    parent_code VARCHAR(128),
    full_path VARCHAR(1000) NOT NULL,
    is_leaf INT4 DEFAULT 1,
    level INT4 DEFAULT 1,
    description VARCHAR(500),
    path VARCHAR(512),
    icon VARCHAR(128),
    component VARCHAR(512),
    sort_order INT4 DEFAULT 0,
    visible INT4 DEFAULT 1,
    extension TEXT,
    domain_code VARCHAR(64) NOT NULL,
    application_code VARCHAR(64) NOT NULL,
    module_code VARCHAR(64) NOT NULL,
    status INT4 DEFAULT 1,

    -- 标准审计字段
    archived INT4 DEFAULT 0,
    create_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    update_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    create_by VARCHAR(100),
    create_name VARCHAR(100),
    update_by VARCHAR(100),
    update_name VARCHAR(100),

    CONSTRAINT pk_cmx_menu PRIMARY KEY (id),
    CONSTRAINT uk_cmx_menu_code UNIQUE (code)
);

CREATE INDEX IF NOT EXISTS idx_cmx_menu_module ON cmx_menu (domain_code, application_code, module_code);
CREATE INDEX IF NOT EXISTS idx_cmx_menu_parent ON cmx_menu (parent_id);
CREATE INDEX IF NOT EXISTS idx_cmx_menu_parent_code ON cmx_menu (parent_code);
CREATE INDEX IF NOT EXISTS idx_cmx_menu_full_path ON cmx_menu (full_path);

COMMENT ON TABLE cmx_menu IS '菜单定义表';
COMMENT ON COLUMN cmx_menu.id IS '主键ID';
COMMENT ON COLUMN cmx_menu.code IS '菜单编码，唯一，如 gl:dashboard';
COMMENT ON COLUMN cmx_menu.name IS '菜单名称';
COMMENT ON COLUMN cmx_menu.parent_id IS '父菜单ID(逻辑关联，无外键约束)';
COMMENT ON COLUMN cmx_menu.parent_code IS '父菜单编码(根为NULL)';
COMMENT ON COLUMN cmx_menu.full_path IS '菜单全路径，如 /gl:finance/gl:dashboard';
COMMENT ON COLUMN cmx_menu.is_leaf IS '是否叶子节点：1-是，0-否';
COMMENT ON COLUMN cmx_menu.level IS '层级深度，根=1';
COMMENT ON COLUMN cmx_menu.description IS '菜单描述';
COMMENT ON COLUMN cmx_menu.path IS '前端路由路径';
COMMENT ON COLUMN cmx_menu.icon IS '菜单图标';
COMMENT ON COLUMN cmx_menu.component IS '前端组件路径';
COMMENT ON COLUMN cmx_menu.sort_order IS '排序序号';
COMMENT ON COLUMN cmx_menu.visible IS '是否可见：0-隐藏，1-显示';
COMMENT ON COLUMN cmx_menu.extension IS '扩展字段(用户自定义JSON文本)';
COMMENT ON COLUMN cmx_menu.domain_code IS '所属域编码';
COMMENT ON COLUMN cmx_menu.application_code IS '所属应用编码';
COMMENT ON COLUMN cmx_menu.module_code IS '所属模块编码';
COMMENT ON COLUMN cmx_menu.status IS '状态：0-禁用，1-启用';
COMMENT ON COLUMN cmx_menu.archived IS '归档标志：0-未归档，1-已归档';
COMMENT ON COLUMN cmx_menu.create_time IS '创建时间';
COMMENT ON COLUMN cmx_menu.update_time IS '更新时间';
COMMENT ON COLUMN cmx_menu.create_by IS '创建人ID';
COMMENT ON COLUMN cmx_menu.create_name IS '创建人姓名';
COMMENT ON COLUMN cmx_menu.update_by IS '更新人ID';
COMMENT ON COLUMN cmx_menu.update_name IS '更新人姓名';
```

> **树形字段说明(对齐 `cmx_permission` 命名约定):**
> - `parent_id` — 父节点 ID,逻辑关联(无外键约束)
> - `parent_code` — 父节点编码,根为 NULL
> - `full_path` — 节点全路径(如 `/gl:finance/gl:dashboard`),方便前缀查询子树
> - `is_leaf` — 是否叶子节点(1/0),插入/删除子节点时需维护父节点该字段
> - `level` — 层级深度,根 = 1
> - `extension` — 扩展字段(用户自定义 JSON 文本),用于放置按钮权限点、参数等
>
> 这些字段在 `MenuService::create` 时按 `parent_id` 自动计算(参考 `cmx-iam/src/permission/service/crud.rs:55-72` 计算 `parent_code/full_code_path/level` 的逻辑),并在事务内更新父节点 `is_leaf`。

**down:** `DROP TABLE IF EXISTS cmx_menu;`
**同步 `init_ddl.sql`:** 追加表 34 区块。

> **菜单与权限解耦约定:** `cmx_menu` 是菜单展示的**唯一权威来源**;`cmx_permission` 中 `resource_type='menu'` 的记录保留作为"菜单访问权限点"(通过 `code` 与 `cmx_menu` 逻辑关联),不再承担菜单展示职责。迁移脚本负责把现有 menu 类型权限同步到 `cmx_menu`。

### 3.3 权限表字段重命名

`cmx_permission.app_code`(`init_ddl.sql:1743`)与全局 `application_code` 命名不一致。

**新增迁移文件:** `docs/sql/migrations/20260701_003_cmx_permission_rename.up.sql`

```sql
-- 将 cmx_permission.app_code 重命名为 application_code，统一命名规范
ALTER TABLE cmx_permission RENAME COLUMN app_code TO application_code;

ALTER INDEX IF EXISTS idx_cmx_permission_app_code RENAME TO idx_cmx_permission_application_code;

COMMENT ON COLUMN cmx_permission.application_code IS '所属应用编码（原 app_code，统一命名）';
```

**down:** `ALTER TABLE cmx_permission RENAME COLUMN application_code TO app_code;`
**同步 `init_ddl.sql`:** 把表 32 中 `app_code VARCHAR(100)` 改为 `application_code VARCHAR(100)`,索引与注释一并改名(L1743/1770/1787 附近)。

> 代码同步:`crates/libs/cmx-iam/src/permission/bmc.rs`(SQL 中的列名)、`crates/libs/cmx-iam/src/permission/service/import.rs`、`crates/libs/cmx-core/src/model/iam/permission.rs`(`app_code` → `application_code`)。

---

## 四、代码变更清单(按 crate 组织)

### 4.1 `crates/libs/cmx-core/` — 数据模型

#### 4.1.1 表单模型
**新建:** `crates/libs/cmx-core/src/model/form/mod.rs` + `entity.rs`

参考 `crates/libs/cmx-core/src/model/iam/permission.rs` 模式,定义 `Form` / `FormForCreate` / `FormForUpdate` / `FormFilter`。`definition: serde_json::Value`。错误类型用 `thiserror`(遵循 AGENTS.md §1)。

#### 4.1.2 菜单模型
**新建:** `crates/libs/cmx-core/src/model/menu/mod.rs` + `entity.rs`

`Menu` / `MenuForCreate` / `MenuForUpdate` / `MenuFilter`,含 `parent_id`/`parent_code` 树形结构。

#### 4.1.3 模块清单模型
**新建:** `crates/libs/cmx-core/src/model/module/manifest.rs`

```rust
use serde::{Deserialize, Serialize};

/// 模块聚合包清单(对应 module.manifest.json)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleManifest {
    pub manifest_version: String,
    pub module: ModuleInfo,
    /// 迁移包版本号 = 导出时间戳 yyyyMMddHHmmSS(导出服务自动生成,无需手动输入)
    pub package_version: String,
    pub resources: ModuleResources,
    pub plugins: Vec<ModulePluginEntry>,
    pub checksum: Option<String>,
    pub signature_algorithm: Option<String>,
    pub signature: Option<String>,
    pub signer_key_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleInfo {
    pub code: String,
    pub name: String,
    pub domain_code: String,
    pub application_code: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModuleResources {
    #[serde(default)]
    pub forms: Vec<String>,
    #[serde(default)]
    pub menus: Vec<String>,
    #[serde(default)]
    pub metadata: Vec<String>,
    #[serde(default)]
    pub permissions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModulePluginEntry {
    pub id: String,
    pub version: String,
    pub package: String,
}

/// 签名载荷(复用插件签名思路)
impl ModuleManifest {
    pub fn to_canonical_bytes(&self) -> anyhow::Result<Vec<u8>> {
        let payload = ModuleManifestSigningPayload {
            manifest_version: self.manifest_version.clone(),
            module: self.module.clone(),
            package_version: self.package_version.clone(),
            resources: self.resources.clone(),
            plugins: self.plugins.clone(),
        };
        // 稳定序列化:排序键
        let s = serde_json::to_string(&payload)?;
        Ok(s.into_bytes())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ModuleManifestSigningPayload {
    manifest_version: String,
    module: ModuleInfo,
    package_version: String,
    resources: ModuleResources,
    plugins: Vec<ModulePluginEntry>,
}
```

#### 4.1.4 导出与权限模型更新
- **修改:** `crates/libs/cmx-core/src/model/mod.rs` — 增加 `pub mod form; pub mod menu;` 并在 `module` 模块导出 `manifest`。
- **修改:** `crates/libs/cmx-core/src/model/iam/permission.rs` — `app_code` → `application_code`。

---

### 4.2 `crates/libs/cmx-biz/` + `crates/libs/cmx-iam/` — 业务持久化层(BMC + modql + GenericCrudService)

仓库采用 **BMC + modql + GenericCrudService** 标准模式,非 sea-orm。每个实体一个目录,文件结构 `entity.rs` / `filter.rs` / `bmc.rs` / `service.rs` / `mod.rs`(完全对齐现有 `crates/libs/cmx-biz/src/domain/`)。

#### 4.2.1 表单模块 `cmx-biz/src/form/`

**entity.rs** — 完整实体 / ForCreate / ForUpdate(派生 `Fields`)

```rust
//! Form 实体定义
use modql::field::Fields;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

/// 表单实体(完整字段,查询返回)
#[derive(Debug, Clone, Serialize, Deserialize, Fields, FromRow)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct Form {
    pub id: String,
    pub code: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// 表单完整定义 JSON
    #[serde(skip_serializing_if = "Option::is_none")]
    pub definition: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub domain_code: String,
    pub application_code: String,
    pub module_code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<i32>,
    // ... 标准审计字段 archived/create_time/.../update_name 同 Domain
}

/// 创建 DTO(不含 id 与自动生成字段)
#[derive(Debug, Clone, Serialize, Deserialize, Fields)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct FormForCreate {
    pub code: String,
    pub name: String,
    pub description: Option<String>,
    pub definition: Option<serde_json::Value>,
    pub domain_code: String,
    pub application_code: String,
    pub module_code: String,
}

/// 更新 DTO(全 Option,仅更新非 None 字段)
#[derive(Debug, Clone, Serialize, Deserialize, Fields, Default)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
pub struct FormForUpdate {
    pub name: Option<String>,
    pub description: Option<String>,
    pub definition: Option<serde_json::Value>,
    pub status: Option<i32>,
}
```

**filter.rs** — modql Filter(派生 `FilterNodes`)

```rust
//! Form Filter 定义
use modql::filter::{FilterNodes, OpValsString, OpValsInt64};
use serde::Deserialize;

#[derive(Debug, Clone, FilterNodes, Deserialize, Default)]
pub struct FormFilter {
    pub code: Option<OpValsString>,
    pub name: Option<OpValsString>,
    pub domain_code: Option<OpValsString>,
    pub application_code: Option<OpValsString>,
    pub module_code: Option<OpValsString>,
    pub status: Option<OpValsInt64>,
    pub archived: Option<OpValsInt64>,
}
```

**bmc.rs** — 表元信息(对齐 `domain/bmc.rs`)

```rust
//! Form 实体的 DbBmc 实现
use cmx_database::crud::DbBmc;

pub struct FormBmc;

impl DbBmc for FormBmc {
    const TABLE: &'static str = "cmx_form";
    const PK_COLUMN: &'static str = "id";
    fn has_timestamps() -> bool { true }
    fn has_owner_id() -> bool { false }
}
```

**service.rs** — 静态 Service,`list` / `page` 遵循 `filters + list_options` 最佳实践

```rust
//! Form Service
use cmx_database::crud::GenericCrudService;
use cmx_database::DatabaseManager;
use cmx_core::model::data::dataset::DataSet;
use modql::filter::ListOptions;
use sea_query::Value;

use crate::form::{FormBmc, FormForCreate, FormForUpdate, FormFilter};
use crate::error::Result;

pub struct FormService;

impl FormService {
    pub async fn create(mm: &DatabaseManager, db_id: &str, data: FormForCreate) -> Result<DataSet> {
        GenericCrudService::<FormBmc>::create(mm, db_id, None, data).await
    }
    pub async fn get(mm: &DatabaseManager, db_id: &str, id: &str) -> Result<DataSet> {
        GenericCrudService::<FormBmc>::get(mm, db_id, None, id.into()).await
    }
    pub async fn update(mm: &DatabaseManager, db_id: &str, id: Value, data: FormForUpdate) -> Result<DataSet> {
        GenericCrudService::<FormBmc>::update(mm, db_id, None, id, data).await
    }
    pub async fn delete(mm: &DatabaseManager, db_id: &str, ids: Vec<Value>) -> Result<DataSet> {
        GenericCrudService::<FormBmc>::delete(mm, db_id, None, ids).await
    }
    /// 列表查询(filters + list_options 透传)
    pub async fn list(
        mm: &DatabaseManager, db_id: &str,
        filters: Option<Vec<FormFilter>>,
        list_options: Option<ListOptions>,
    ) -> Result<DataSet> {
        GenericCrudService::<FormBmc, FormFilter>::list(mm, db_id, None, filters, list_options).await
    }
    /// 分页查询(返回 DataSet + total)
    pub async fn page(
        mm: &DatabaseManager, db_id: &str,
        filters: Option<Vec<FormFilter>>,
        list_options: ListOptions,
    ) -> Result<(DataSet, i64)> {
        GenericCrudService::<FormBmc, FormFilter>::page(mm, db_id, None, filters, list_options).await
    }
}
```

**mod.rs** — 导出

```rust
//! 表单管理模块
pub mod entity;
pub mod bmc;
pub mod filter;
pub mod service;

pub use entity::{Form, FormForCreate, FormForUpdate};
pub use bmc::FormBmc;
pub use filter::FormFilter;
pub use service::FormService;
```

并在 `crates/libs/cmx-biz/src/lib.rs` 增加 `pub mod form;`。

#### 4.2.2 菜单模块 `cmx-biz/src/menu/`

结构与 form 一致(同样 `entity.rs`/`filter.rs`/`bmc.rs`/`service.rs`/`mod.rs` 五件套),差异点:
- `MenuBmc` → `TABLE = "cmx_menu"`
- `Menu` 实体含树形字段 `parent_id` / `parent_code` / `full_path` / `is_leaf` / `level` / `extension`(对齐 `cmx_permission` 命名)
- `MenuFilter` 含:`parent_id: Option<OpValsString>`、`parent_code: Option<OpValsString>`、`full_path: Option<OpValsString>`(支持 `$startsWith` 前缀查子树)、`is_leaf: Option<OpValsInt64>`、`level: Option<OpValsInt64>`、`visible: Option<OpValsInt64>` + 模块三段式字段
- `MenuForCreate` 含:`code` / `name` / `parent_id`(可选) / `path` / `icon` / `component` / `sort_order` / `visible` / `extension` / 模块三段式;**不含** `full_path`/`is_leaf`/`level`/`parent_code`(由 Service 自动计算)
- `MenuService::create` — 参考 `cmx-iam/src/permission/service/crud.rs:55-72`:开事务 → 按 `parent_id` 计算 `parent_code/full_path/level` → INSERT → 更新父节点 `is_leaf=0`(原子)
- `MenuService::list_tree(mm, db_id, filters, list_options)` — 查全量后内存组装为树(参考 `domain/service.rs` 的 `get_tree`)

#### 4.2.3 权限 BMC 字段重命名
- **修改:** `crates/libs/cmx-iam/src/permission/bmc.rs:8` 及其中 SQL — `app_code` → `application_code`
- **修改:** `crates/libs/cmx-iam/src/permission/service/import.rs` — 导入逻辑字段重命名

#### 4.2.4 模块版本管理 BMC `cmx-biz/src/module/version/`
新建版本管理子模块(供 §4.3 的 ModuleInstallService 调用):
- `ModuleCurrentVersionBmc` → `TABLE = "cmx_module_current_version"`(当前态,一行/模块)
  - `get_by_module_code(code)` — 读当前版本(导入校验用,高频)
  - `upsert(record)` — 写当前版本(唯一约束 `module_code` 保证一行)
- `ModuleVersionHistoryBmc` → `TABLE = "cmx_module_version_history"`(历史,多行)
  - `insert(record)` — 插入历史(唯一约束 `(module_code, package_version)` 防重)
- `ModuleVersionService::record_import(record)` — **事务内**同时 upsert current + insert history,保证两表一致

#### 4.2.5 模块字典 BMC 扩展
- **修改:** `crates/libs/cmx-biz/src/module/bmc.rs` 与 `service.rs`(`cmx_module` 保持字典表纯净):
  - `ModuleService::upsert_by_code(info)` — 仅写 code/name/description 等字典字段
  - 聚合查询(供模块导出使用):`list_plugins_by_module / list_forms_by_module / list_menus_by_module / list_permissions_by_module / list_metadata_by_module`

---

### 4.3 `crates/libs/cmx-plugin/` — 保留插件安装 + 注释旧分发逻辑

#### 4.3.1 ⭐ 保留插件单独安装接口与功能(只注释,不删除)

插件单独安装链路 `POST /api/plugin/install`、`/api/plugin/deploy`、升级、卸载**全部保留**。在执行器与持久化层中,把以下两块**注释掉**(保留代码 + 加 `// TODO(module):` 说明,便于将来切换或回退):

**(a) 元数据初始化(建表 DDL)** — `crates/libs/cmx-plugin/src/service/persistence.rs`

注释掉安装路径的建表步骤(L230 附近):
```rust
// // TODO(module): 元数据初始化(建表)已迁移到模块安装流程(ModuleInstallService)，
// // 单独安装插件时不再自动建表，避免与模块包冲突。
// crate::service::utils::execute_ddl_with_lock(
//     &tx,
//     ...参数...
// ).await?;
```
对应 `upgrade_persist`(L471)与 `reinstall_persist` 同样注释。

> 注:`utils.rs` 的 `execute_ddl_with_lock` / `create_plugin_tables` / `save_plugin_table_metadata` 函数**保留不删**(模块安装流程会复用它们)。

**(b) 中心数据分发(菜单/权限/表单推送)** — `crates/libs/cmx-plugin/src/service/executor.rs`

注释掉 `execute_install` 中的"1.5 中心数据分发"整块(L94-131):
```rust
// // TODO(module): 菜单/权限/表单/流程的分发到外部中心已迁移到模块安装流程，
// // 单独安装插件时不再推送。dispatch_install 在插件包瘦身(无 formdata/menudata/permdata)后
// // 天然为空，此处分发块整体注释保留，待模块中心分发(HttpServiceCenterSender 补全 Form/Menu 端点)后由模块流程统一触发。
// let ctx = DispatchContext { ... };
// let dispatch_result = self.center_dispatcher.dispatch_install(&ctx).await?;
// if !dispatch_result.is_all_success() { ... 补偿卸载 ... }
```
`execute_upgrade`(L182-209)、`execute_reinstall` 中对应的分发块同样注释。卸载流程 `execute_uninstall` 的中心清理当前已是注释状态( executor.rs L338-365 的 "fixme yqs" ),保持不变。

**(c) 服务编排解析保留**
`parse_and_save_services`(`persistence.rs:318`)继续保留 — 服务编排属于插件本身,仍随插件安装。

> **效果:** 单独安装插件仍然可用(写入 `cmx_plugin` + 复制 wasm + 注册运行时 + 审计 + 事件),只是不再建表、不再推送菜单权限到外部中心。`center_client/` 整个模块代码保留(模块流程会用到)。

#### 4.3.2 中心分发器扩展(供模块流程使用)
**修改:** `crates/libs/cmx-plugin/src/center_client/dispatcher.rs`

- `dispatch_install`(L51-111)现有逻辑保留(从 `install_path/{dir}` 读取)。
- **新增** `dispatch_module_resources(&self, ctx: &ModuleDispatchContext)`,从**模块包解压目录**读取 `forms/ menus/ permissions/` 并分发到对应中心。
- HTTP 发送器 `http_sender.rs`(`category_to_http_path` L157-167)补全 Form/Menu 端点(目前仅 Perm 实现)。

#### 4.3.3 新增模块包安装服务(关键新代码)
**新建:** `crates/libs/cmx-plugin/src/service/module_install.rs`

`ModuleInstallService` — 模块包导入编排器。**核心:复用现有 `InstallService::install` 装插件子包**。

```rust
pub struct ModuleInstallService {
    // 复用现有依赖
    package_utils: PackageUtils,
    module_bmc: ...,        // cmx-biz
    module_version_bmc: ..., // cmx-biz 模块版本历史
    form_bmc: ...,          // cmx-biz
    menu_bmc: ...,          // cmx-biz
    permission_import: ..., // cmx-iam
    center_dispatcher: ..., // 复用现有
    install_service: Arc<InstallService>, // ⭐ 复用现有插件安装
}

impl ModuleInstallService {
    /// 安装/导入模块包
    pub async fn install_module_package(&self, source: ModulePackageSource, force: bool)
        -> Result<ModuleInstallResult>
    {
        // 1. fetch + 解压模块 zip 到临时目录 (复用 PackageUtils::fetch_package + extract_zip)
        let module_dir = self.package_utils.fetch_and_extract(&source).await?;

        // 2. 解析 module.manifest.json
        let manifest: ModuleManifest = parse_module_manifest(&module_dir.join("module.manifest.json"))?;
        // package_version 是导出时自动生成的 14 位时间戳(yyyyMMddHHmmSS)
        let pkg_version = manifest.package_version.clone();

        // 3. ⭐ 版本校验(避免旧版本覆盖新版本)—— 从 cmx_module_current_version 读当前版本
        let existing = self.module_current_version_bmc
            .get_by_module_code(&manifest.module.code)  // 读 current_version 表一行
            .await?;
        let import_action = self.validate_import(&existing, &manifest, force).await?;
        match import_action {
            ImportAction::SkipSame => {
                return Ok(ModuleInstallResult { skipped: true, reason: "已是当前版本".into(), .. });
            }
            ImportAction::RejectOldVersion(msg) => {
                return Err(PluginError::ModuleVersionConflict(msg));
            }
            ImportAction::AllowUpgrade | ImportAction::AllowForceDowngrade |
            ImportAction::AllowSameSecondPatch => { /* 继续安装 */ }
        }

        // 4. 安装/更新模块字典(cmx_module 保持纯净,仅 code/name 等字典字段)
        self.module_bmc.upsert_by_code(&manifest.module).await?;

        // 5. ⭐ 版本登记(事务内,写两张表):
        //    a. cmx_module_current_version upsert(唯一约束 module_code 保证一行):
        //       package_version / imported_at / checksum / manifest_snapshot
        //    b. cmx_module_version_history INSERT(唯一约束 module_code+package_version 防重)
        self.module_version_service.record_import(VersionRecord {
            module_code: manifest.module.code.clone(),
            domain_code: manifest.module.domain_code.clone(),
            application_code: manifest.module.application_code.clone(),
            package_version: pkg_version.clone(),
            checksum: manifest.checksum.clone(),
            manifest_snapshot: serde_json::to_value(&manifest)?,
            imported_by: source.operator.clone(),
            source: source.label.clone(),
        }).await?;

        // 6. 安装模块级资源(事务化,失败补偿):
        //    6a. metadata: 复用 execute_ddl_with_lock 建表 + save_plugin_table_metadata
        //        (传 module_code 归属;plugin_id 填主插件或留空)
        //    6b. permissions: 解析 permissions/*.json → 复用 cmx-iam 权限导入逻辑 upsert cmx_permission
        //    6c. forms: 解析 forms/*.json → FormBmc.upsert_by_code
        //    6d. menus: 解析 menus/*.json → MenuBmc.upsert_by_code
        //    (注:seeddata 不在模块层处理 —— 见下方第 7 步,随插件子包安装)
        //    6e. (可选) center_dispatcher.dispatch_module_resources 推送到外部中心

        // 7. ⭐ 遍历 manifest.plugins,逐个安装插件子包(完全复用现有 InstallService::install)
        //    seeddata 全部留在插件子包内,随插件安装一并执行(现有 InstallService 已支持 seeddata 加载)
        let domain = &manifest.module.domain_code;
        let app    = &manifest.module.application_code;
        let module = &manifest.module.code;
        for entry in &manifest.plugins {
            let plugin_zip = module_dir.join(&entry.package);
            let install_req = InstallRequest {
                source: PluginSource::Local { path: plugin_zip.to_string_lossy().to_string() },
                // 用模块归属填充,保证插件挂到正确模块
                domain_code: Some(domain.clone()),
                application_code: Some(app.clone()),
                module_code: Some(module.clone()),
                ..Default::default()
            };
            // ⭐ 复用现有插件安装(其内部的建表/分发已被注释,不会重复;seeddata 仍随插件加载)
            self.install_service.install(install_req).await?;
        }

        // 8. 事件发布 + 审计(复用现有 event_publisher / audit_logger)
        Ok(ModuleInstallResult { ... })
    }

    /// 版本校验核心逻辑(对应 §3.0.4 的规则)—— 纯字符串比较(时间戳定长14位,字典序==数值序)
    async fn validate_import(&self, existing: &Option<Module>, manifest: &ModuleManifest, force: bool)
        -> Result<ImportAction>
    {
        let Some(cur) = existing else { return Ok(ImportAction::AllowUpgrade); }; // 新模块直接放行
        let cur_pv = cur.package_version.as_deref().unwrap_or("");
        let new_pv = manifest.package_version.as_str();

        // 1. checksum 幂等(同一包重复导入)
        if let (Some(a), Some(b)) = (&manifest.checksum, &cur.checksum) {
            if a == b { return Ok(ImportAction::SkipSame); }
        }
        // 2. 时间戳字符串比较(定长14位,字典序正确)
        use std::cmp::Ordering;
        match new_pv.cmp(cur_pv) {
            Ordering::Equal => Ok(ImportAction::AllowSameSecondPatch), // 同秒导出不同内容 → 补丁
            Ordering::Less if !force => Ok(ImportAction::RejectOldVersion(format!(
                "无法用旧版本 {} 覆盖当前版本 {}（可用 force=true 强制降级）",
                new_pv, cur_pv
            ))),
            Ordering::Less => Ok(ImportAction::AllowForceDowngrade),
            Ordering::Greater => Ok(ImportAction::AllowUpgrade),
        }
    }
}

enum ImportAction {
    SkipSame,                 // 幂等跳过(同 checksum)
    RejectOldVersion(String), // 拒绝(旧版本时间戳更小)
    AllowUpgrade,             // 升级(新时间戳更大)
    AllowForceDowngrade,      // 强制降级(force=true)
    AllowSameSecondPatch,     // 同秒导出的不同内容,按补丁处理
}
```

> **复用要点:** 模块内每个插件子包调用现有 `InstallService::install`,由于 4.3.1 已注释掉单插件安装中的建表与分发,不会与模块级资源安装冲突。

---

### 4.4 模块迁移包导出(核心新功能)

**新建:** `crates/libs/cmx-plugin/src/service/module_export.rs`

`ModuleExportService` — 从 DB + 文件系统聚合导出为单一 zip。

```rust
pub struct ModuleExportService {
    module_bmc: ...,
    form_bmc: ...,
    menu_bmc: ...,
    permission_bmc: ...,
    meta_table_bmc: ...,
    plugin_bmc: ...,        // 查 cmx_plugin
    plugin_root: PathBuf,   // 插件安装根目录
    // 复用 ZipCompressor (cmx_utils::zip)
}

impl ModuleExportService {
    /// 导出模块为迁移包 zip 字节
    pub async fn export_module(&self, domain_code: &str, app_code: &str, module_code: &str)
        -> Result<Vec<u8>>
    {
        // 1. 查询模块元信息 → 写 module.json
        let module = self.module_bmc.get_by_code(domain_code, app_code, module_code).await?;

        // 2. 查 cmx_form WHERE module_code=... → forms/{code}.json
        let forms = self.form_bmc.list_by_module(...).await?;

        // 3. 查 cmx_menu WHERE module_code=... → 组装菜单树 → menus/{module_code}_menu.json
        let menus = self.menu_bmc.list_tree(...).await?;

        // 4. 查 cmx_permission WHERE module_code=... → permissions/{module_code}_permissions.json
        let perms = self.permission_bmc.list_by_module(...).await?;

        // 5. 查 cmx_meta_table_define WHERE module_code=... → metadata/tables/*.json + domain_app_module_config.json
        let metas = self.meta_table_bmc.list_by_module(...).await?;

        // 6. 查 cmx_plugin WHERE module_code=... → 对每个插件:
        //    读取安装目录 {plugin_root}/{app_id}/{plugin_id}/{version}/
        //    只取 manifest.json + servicedata/ + *.wasm + wit/ + api/
        //    打成子 zip → plugins/{plugin_id}.zip (复用 ZipCompressor::compress_dir_to_memory)
        let plugins = self.plugin_bmc.list_by_module(...).await?;

        // 7. 组装 module.manifest.json
        //    - 列出资源文件 + 插件子包
        //    - ⭐ package_version = chrono::Local::now().format("%Y%m%d%H%M%S")
        //      (导出时间戳,自动生成,无需调用方传入)
        let manifest = ModuleManifest {
            package_version: chrono::Local::now().format("%Y%m%d%H%M%S").to_string(),
            module: module_info,
            resources,
            plugins: plugin_entries,
            checksum: None,    // 第 8 步计算后回填
            ..Default::default()
        };

        // 8. 计算 checksum (sha256),回填到 manifest;可选 Ed25519 签名
        //    (复用 manifest 签名机制,对 ModuleManifest::to_canonical_bytes 签名)

        // 9. 用 ZipCompressor 把 module.json / module.manifest.json /
        //    forms/ / menus/ / metadata/ / permissions/ / plugins/ 全部打成单一 zip
        Ok(zip_bytes)
    }
}
```

**复用的现有组件:**
- `cmx_utils::zip::ZipCompressor::compress_dir_to_memory`(底层压缩)
- `crates/libs/cmx-plugin/src/center_client/packer.rs`(L54)的打包思路(反向:DB → zip)
- manifest 签名机制(`PluginManifestSigningPayload`,`crates/libs/cmx-core/src/model/meta/plugin.rs:133-141`)的 Ed25519 流程

---

### 4.5 `crates/libs/cmx-api/` — API 路由与 Handler(遵循 axum-handler-generator 规范)

cmx-api 是纯 HTTP 适配层。表单/菜单的标准 CRUD 用宏系统生成,模块包导入/导出手写 handler(参考 `handlers/application/handler.rs` 与 `handlers/plugin/handler.rs`)。

#### 4.5.1 表单标准 CRUD(宏生成)

**handlers/form/mod.rs** — re-export cmx-biz 类型 + ModuleRoutes

```rust
//! Form 模块 HTTP API
//! Entity/BMC/Filter/Service 已在 cmx-biz 中定义
pub mod handler;

// 从业务 crate re-export(供宏系统使用)
pub use cmx_biz::form::{Form, FormBmc, FormFilter, FormForCreate, FormForUpdate, FormService};

use crate::app_state::CmxAppState;
use crate::routes::traits::ModuleRoutes;
use axum::Router;

pub struct FormModule;

impl ModuleRoutes for FormModule {
    fn routes(self) -> Router<CmxAppState> {
        let router = Router::new();
        // 注册标准 CRUD 路由(create/create-many/get/update/update-many/delete/list/page)
        let router = crate::register_crud_handlers_module!(router, form_crud, "/form");
        // 自定义路由(如 get_by_code)在此追加
        router
    }
    fn prefix() -> &'static str { "form" }
    fn module_name(&self) -> &'static str { "form" }
}
```

**routes/crud_handlers.rs** — 注册宏调用

```rust
use crate::declare_crud_handlers;

declare_crud_handlers!(
    form_crud,
    crate::handlers::form::Form,
    crate::handlers::form::FormBmc,
    crate::handlers::form::FormForCreate,
    crate::handlers::form::FormForUpdate,
    crate::handlers::form::FormFilter,
    "Form",
    "/form"
);
```

宏自动生成 8 个 handler,路由为 `/form/create`、`/form/page`、`/form/list` 等。

**handlers/form/handler.rs** — 自定义查询(如按模块查列表),遵循 `filters + list_options` 透传

```rust
//! Form 自定义 Handler
use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use cmx_core::ListParams;
use cmx_database::get_default_db_manager;
use cmx_biz::form::{FormFilter, FormService};
use crate::api_response::ApiResp;
use crate::app_state::CmxAppState;
use crate::error::Result;
use crate::middleware::CmxSvrContext;
use crate::rest::header_parse::get_db_id_from_header;
use cmx_core::model::data::dataset::DataSet;

/// 表单列表查询
#[utoipa::path(
    post,
    path = "/api/form/list",
    request_body = ListParams<serde_json::Value>,  // ✅ 文档用 serde_json::Value
    responses((status = 200, description = "查询成功", body = ApiResp<DataSet>)),
    tag = "Form"
)]
pub async fn form_list(
    State(_cmx_state): State<CmxAppState>,
    CmxSvrContext(_svr_ctx): CmxSvrContext,
    headers: HeaderMap,
    Json(params): Json<ListParams<FormFilter>>,  // ✅ 签名用具体 Filter
) -> Result<Json<ApiResp<DataSet>>> {
    let mm = get_default_db_manager();
    let db_id = get_db_id_from_header(&headers).await;
    let list_options = params.to_list_options();
    let filters = params.filters.clone().filter(|v| !v.is_empty());
    let dataset = FormService::list(mm, &db_id, filters, Some(list_options)).await?;
    Ok(Json(ApiResp::ok(dataset)))
}
```

> 菜单模块 `handlers/menu/` 结构完全一致,宏调用替换为 `Menu / MenuBmc / MenuFilter / ...`,`"/menu"` 路由前缀。菜单额外有自定义 `menu_tree` handler(返回树形结构)。

#### 4.5.2 模块包导入/导出端点(手写 handler)

**handlers/module/package_handler.rs** — 参考 `handlers/plugin/handler.rs` 的 multipart 上传

```rust
//! 模块迁移包导入/导出 Handler
use axum::extract::{Multipart, Query, State};
use axum::http::HeaderMap;
use axum::Json;
use serde::Deserialize;
use cmx_plugin::service::module_install::{ModuleInstallService, ModulePackageSource};
use cmx_plugin::service::module_export::ModuleExportService;
use crate::api_response::ApiResp;
use crate::app_state::CmxAppState;
use crate::error::Result;
use crate::middleware::CmxSvrContext;

/// 导入查询参数
#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct ImportQuery {
    /// 是否强制降级覆盖新版本
    pub force: Option<bool>,
}

/// 导出查询参数
#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct ExportQuery {
    pub domain_code: String,
    pub application_code: String,
    pub module_code: String,
}

/// 导出模块迁移包(返回 zip 字节,自动生成 package_version 时间戳)
#[utoipa::path(
    get,
    path = "/api/module/package/export",
    params(ExportQuery),
    responses((status = 200, description = "导出成功", content_type = "application/zip")),
    tag = "Module"
)]
pub async fn module_package_export(
    State(_): State<CmxAppState>,
    CmxSvrContext(_): CmxSvrContext,
    Query(q): Query<ExportQuery>,
) -> Result<axum::response::Response> {
    let export_svc = ModuleExportService::global();
    let zip_bytes = export_svc
        .export_module(&q.domain_code, &q.application_code, &q.module_code)
        .await
        .map_err(|e| crate::error::Error::InternalError(format!("导出失败: {}", e)))?;
    // 返回 zip 文件流(文件名含 package_version 时间戳)
    Ok(axum::response::AppendHeaders([(
        axum::http::header::CONTENT_DISPOSITION,
        // package_version 在 export_module 内已生成,此处从返回值携带文件名
        "attachment; filename=\"module_package.zip\"".parse().unwrap_or_else(|_| {
            axum::http::HeaderValue::from_static("attachment")
        }),
    )])
    .into_response(axum::body::Body::from(zip_bytes)))
}

/// 导入模块迁移包(multipart 上传 zip)
#[utoipa::path(
    post,
    path = "/api/module/package/import",
    params(ImportQuery),
    responses((status = 200, description = "导入成功")),
    tag = "Module"
)]
pub async fn module_package_import(
    State(_): State<CmxAppState>,
    CmxSvrContext(_): CmxSvrContext,
    Query(q): Query<ImportQuery>,
    mut multipart: Multipart,
) -> Result<Json<ApiResp<serde_json::Value>>> {
    // 1. 接收 multipart 文件(参考 plugin_deploy 的 multipart 处理)
    let zip_bytes = receive_multipart_zip(&mut multipart).await?;

    // 2. 调用模块安装服务(含版本校验)
    let install_svc = ModuleInstallService::global();
    let result = install_svc
        .install_module_package(
            ModulePackageSource::Bytes(zip_bytes),
            q.force.unwrap_or(false),
        )
        .await
        .map_err(|e| match e {
            cmx_plugin::error::PluginError::ModuleVersionConflict(msg) =>
                crate::error::Error::InternalError(msg),
            other => crate::error::Error::InternalError(format!("导入失败: {}", other)),
        })?;

    Ok(Json(ApiResp::ok(serde_json::to_value(&result)?)))
}
```

| 端点 | 方法 | 用途 |
|---|---|---|
| `/api/module/package/export` | GET | 导出模块迁移包(自动生成 `package_version` 时间戳) |
| `/api/module/package/import` | POST (multipart) | 上传模块 zip 并导入安装 |
| `/api/module/package/import?force=true` | POST (multipart) | 强制导入(允许降级覆盖新版本) |
| `/api/module/{code}/versions` | POST | 查询模块版本历史(标准 page 宏生成) |

> 导入端点内部调用 `ModuleInstallService::install_module_package`;导出端点调用 `ModuleExportService::export_module`。

#### 4.5.3 路由注册
- **修改:** `handlers/module/mod.rs` — 在现有 `ModuleModule` 的 `routes()` 中 nest 模块包路由(`/module/package/export`、`/module/package/import`),并合并表单/菜单模块。
- **修改:** `routes/routes_impl.rs` — 注册 `FormModule`、`MenuModule`。
- **不改动**现有 `/api/plugin/*` 路由(插件单独安装链路保留)。

---

## 五、迁移脚本(旧格式 → 新格式)

**决策:** 提供一次性迁移工具,扫描已安装的旧插件,按 `module_code` 拆分重组。

**新建:** `crates/libs/cmx-plugin/src/bin/migrate_to_module_packages.rs`(独立二进制)或 `cmx-cli` 子命令。

```rust
fn migrate(dry_run: bool) -> Result<()> {
    // 1. 查询所有已安装插件 cmx_plugin
    let plugins = plugin_bmc.list_all().await?;

    // 2. 按 (domain_code, application_code, module_code) 分组
    let groups = group_by_module(plugins);

    // 3. 对每个模块组:
    for ((domain, app, module), plugin_list) in groups {
        // 3a. 确保 cmx_module 记录存在(从插件归属推导)
        module_bmc.upsert_by_code(...).await?;

        // 3b. 遍历组内每个插件的安装目录,提取旧目录:
        for plugin in plugin_list {
            let install_path = format!("{plugin_root}/{app_id}/{plugin_id}/{version}");
            // formdata/*.json  → 写入 cmx_form
            // menudata/*.json  → 写入 cmx_menu (并去重 cmx_permission resource_type='menu')
            // permdata/*.json  → 确认 cmx_permission (重命名 app_code→application_code)
            // metadata/*.json  → 重新挂到 module_code(保留 plugin_id 来源标记)
        }

        // 3c. (可选)生成该模块的迁移包 zip 供备份
        //     export_service.export_module(domain, app, module).await?;
    }

    // 4. 输出迁移报告(成功/失败/跳过统计)
    Ok(())
}
```

- 提供 `--dry-run` 预检模式。
- 幂等(基于 `code` 唯一索引 upsert,可重复运行)。
- 遵循 AGENTS.md:用 `thiserror` 错误、`tracing` 日志、`?` 传播错误(禁裸 `unwrap`)。

---

## 六、实施顺序(分阶段,每阶段可独立测试提交)

### 阶段 1:数据模型与持久化(基础设施)
1. 新建 `cmx_form` / `cmx_menu` SQL 迁移 + 同步 `init_ddl.sql`(3.1, 3.2)。
2. 权限表 `app_code → application_code` 迁移 + 同步 init(3.3)。
3. **模块版本管理:新建 `cmx_module_current_version`(当前态) + `cmx_module_version_history`(历史) + 同步 init(3.0)。`cmx_module` 字典表保持不变。**
4. `cmx-core` 新增表单/菜单模型 + ModuleManifest 模型(含 `package_version`)(4.1.1-4.1.4)。
5. `cmx-biz` 新增表单/菜单 BMC、**模块版本管理 BMC**(current + history 两表);权限 BMC 字段更新(4.2.1-4.2.5)。
6. **交付物:** 数据库可建表,Rust 可 CRUD 表单/菜单,模块版本可记录与查询。

### 阶段 2:API 层(表单/菜单管理)
6. 新增表单/菜单 CRUD Handler + 路由(4.5.2, 4.5.3)。
7. **交付物:** 表单/菜单可通过 REST 独立管理。

### 阶段 3:注释旧分发逻辑(保留插件单独安装)
8. 注释 `persistence.rs` 建表 DDL 调用(4.3.1a)。
9. 注释 `executor.rs` install/upgrade/reinstall 的中心分发块(4.3.1b)。
10. 验证:`POST /api/plugin/install` 仍可单独安装插件(不建表/不推送),插件运行正常。
11. **交付物:** 插件单独安装链路保留且无副作用。

### 阶段 4:模块包导入
13. `center_client/dispatcher.rs` 新增 `dispatch_module_resources`;`http_sender.rs` 补全 Form/Menu 端点(4.3.2)。
14. 实现 `ModuleInstallService` **含版本校验 `validate_import`**(4.3.3 + §3.0.4)— 复用 `InstallService::install` 装插件子包。
15. 新增模块包导入 API 端点(4.5.1),支持 `force` 查询参数 + 版本历史查询端点。
16. **交付物:** 上传一个模块 zip 可完成导入(资源 + 多插件),旧版本默认被拒绝,版本历史可追溯。

### 阶段 5:模块包导出
17. 实现 `ModuleExportService`(4.4),**导出时自动生成 `package_version` 时间戳**。
18. 新增导出 API 端点。
19. **交付物:** 导出任意已存在模块为迁移包(带时间戳版本号)。

### 阶段 6:迁移脚本
20. 实现旧格式迁移脚本(第五节)。
21. **交付物:** 旧环境可迁移到新结构。

---

## 七、关键风险与注意事项

1. **菜单与权限解耦的过渡:** `cmx_permission` 中现存大量 `resource_type='menu'` 的种子(`20260615_004_iam_tables.up.sql:252-269`、`init_dml.sql:135-152`)。迁移脚本需把这些记录同步到 `cmx_menu`;`cmx_permission` 中 menu 类型保留为"菜单访问权限点",与 `cmx_menu` 通过 `code` 逻辑关联。

2. **插件单独安装的兼容:** 注释旧逻辑后,若仍用**旧格式插件包**(含 formdata/menudata/permdata)单独安装,这些目录会被忽略(不再建表/分发)—— 这是预期行为(资源应通过模块包管理)。`security_validator` 不必改动,旧目录存在不影响验证。

3. **多实例/集群一致性:** `executor.rs` 通过 Redis 通知(L157 `event_publisher`)做集群同步。模块安装/导出需复用同一事件总线,新增 `ModuleInstalled` / `ModuleExported` 事件类型。

4. **签名链:** 模块包整体签名(覆盖 manifest + 所有资源 + 子插件包)在 `ModuleManifest` 上实现;插件子包保留各自 manifest 签名(双层签名)。

5. **DDL 分布式锁:** 模块安装建表步骤复用 `execute_ddl_with_lock`(`persistence.rs:230` 现有实现),保证多实例并发安装同一模块时表结构一致。

6. **事务边界与补偿:** 模块安装涉及多资源 + 多插件,需设计补偿机制 — 复用 `executor.rs` L119-131 的补偿卸载模式:任一资源/插件安装失败,回滚已安装部分。

7. **版本号语义(纯时间戳比对):** `package_version` 是导出时自动生成的 14 位时间戳 `yyyyMMddHHmmSS`(本地时区),**无需手动输入**。版本比对用**纯字符串比较**(定长 14 位保证字典序 == 时间先后序),不依赖 `semver` crate。导出服务用 `chrono::Local::now().format("%Y%m%d%H%M%S")` 生成。同秒内重复导出由 `checksum` 兜底区分(同 checksum 跳过,不同 checksum 按补丁处理)。

8. **并发导入:** 同一模块的两个迁移包并发导入时,`cmx_module_current_version` 的唯一约束 `uk(module_code)` 保证一行,`cmx_module_version_history` 的唯一约束 `uk(module_code, package_version)` 防历史重复。两表的 upsert/insert 必须在同一事务内完成,避免 current 与 history 不一致。

9. **迁移脚本的版本处理:** 旧环境迁移脚本(第五节)在为旧模块建立版本记录时,`package_version` 填迁移执行时刻的时间戳,作为版本历史的起点(`is_current=1`)。

---

## 八、影响范围汇总表

| 文件 | 类型 | 变更概要 |
|---|---|---|
| `docs/sql/migrations/20260701_001_cmx_form.up/down.sql` | 新建 | cmx_form 表 |
| `docs/sql/migrations/20260701_002_cmx_menu.up/down.sql` | 新建 | cmx_menu 表 |
| `docs/sql/migrations/20260701_003_cmx_permission_rename.up/down.sql` | 新建 | app_code→application_code |
| `docs/sql/migrations/20260701_004_cmx_module_current_version.up/down.sql` | 新建 | 模块当前版本表(cmx_module 不变) |
| `docs/sql/migrations/20260701_005_cmx_module_version_history.up/down.sql` | 新建 | 模块版本历史表 |
| `docs/sql/init/init_ddl.sql` | 修改 | 同步新增表 + 权限字段重命名(cmx_module 字典表保持不变) |
| `crates/libs/cmx-core/src/model/form/` | 新建 | 表单模型 |
| `crates/libs/cmx-core/src/model/menu/` | 新建 | 菜单模型 |
| `crates/libs/cmx-core/src/model/module/manifest.rs` | 新建 | ModuleManifest 模型 |
| `crates/libs/cmx-core/src/model/mod.rs` | 修改 | 导出新模块 |
| `crates/libs/cmx-core/src/model/iam/permission.rs` | 修改 | app_code→application_code |
| `crates/libs/cmx-biz/src/form/{entity,filter,bmc,service,mod}.rs` | 新建 | 表单 Entity+Filter+Bmc+Service(modql+GenericCrudService) |
| `crates/libs/cmx-biz/src/menu/{entity,filter,bmc,service,mod}.rs` | 新建 | 菜单 Entity+Filter+Bmc+Service(含 list_tree) |
| `crates/libs/cmx-biz/src/module/bmc.rs` | 修改 | 增加资源聚合查询(cmx_module 字典表保持纯净) |
| `crates/libs/cmx-biz/src/module/version/` | 新建 | 模块版本管理 BMC (current_version + version_history 两表,事务内 record_import) |
| `crates/libs/cmx-iam/src/permission/bmc.rs` | 修改 | 字段重命名 |
| `crates/libs/cmx-iam/src/permission/service/import.rs` | 修改 | 字段重命名 |
| `crates/libs/cmx-plugin/src/service/persistence.rs` | 修改(注释) | 注释建表 DDL 调用(保留函数) |
| `crates/libs/cmx-plugin/src/service/executor.rs` | 修改(注释) | 注释 install/upgrade 中心分发块 |
| `crates/libs/cmx-plugin/src/center_client/dispatcher.rs` | 修改 | 新增 dispatch_module_resources |
| `crates/libs/cmx-plugin/src/center_client/http_sender.rs` | 修改 | 补全 Form/Menu 端点 |
| `crates/libs/cmx-plugin/src/service/module_install.rs` | 新建 | 模块包导入服务(复用 InstallService) |
| `crates/libs/cmx-plugin/src/service/module_export.rs` | 新建 | 模块包导出服务 |
| `crates/libs/cmx-api/src/handlers/module/package_handler.rs` | 新建 | 模块导入/导出端点(multipart) |
| `crates/libs/cmx-api/src/handlers/form/{mod,handler}.rs` | 新建 | 表单 CRUD(宏生成 + 自定义 list) |
| `crates/libs/cmx-api/src/handlers/menu/{mod,handler}.rs` | 新建 | 菜单 CRUD(宏生成 + menu_tree) |
| `crates/libs/cmx-api/src/routes/crud_handlers.rs` | 修改 | 注册 form_crud / menu_crud 宏 |
| `crates/libs/cmx-api/src/routes/routes_impl.rs` | 修改 | 注册 FormModule / MenuModule |
| `crates/libs/cmx-api/src/routes/` | 修改 | 注册新路由(不动 /api/plugin/*) |
| `crates/libs/cmx-plugin/src/bin/migrate_to_module_packages.rs` | 新建 | 旧格式迁移脚本 |

---

## 九、端到端流程

### 9.1 导出迁移包
```
源环境
─────────
cmx_module (GL)          ─┐
cmx_form (模块下表单)     │
cmx_menu (模块下菜单)     │  ModuleExportService     module_FIN_GL_20260630103000.zip
cmx_permission(模块权限)  ├────────────────────▶    (单一聚合 zip)
cmx_meta_table_define    │  export_module()
  (模块表元数据)          │
插件安装目录             │┘
  (wasm+servicedata)
```

### 9.2 导入迁移包
```
目标环境
─────────
module_FIN_GL_20260630103000.zip ──POST──▶ /api/module/package/import
                                          │
                                  ModuleInstallService
                                          │
                    ┌─────────────────────┼─────────────────────┐
                    ▼                     ▼                     ▼
              upsert cmx_module    安装模块级资源           逐个安装插件子包
              (模块元信息)        (forms/menus/perms/      ⭐ 复用 InstallService::install
                                   metadata)               (归属填模块三段式)
                                                           seeddata 随插件子包加载
                                          │
                                  复用 execute_ddl_with_lock (建表)
                                  复用 dispatch_module_resources (推送)
                                          │
                                  事件发布 + 审计
```

### 9.3 单独安装插件(保留,与模块流程并行可用)
```
plugin_xxx.zip ──POST──▶ /api/plugin/install   (现有端点不变)
                              │
                       InstallService::install
                              │
                ┌─────────────┴─────────────┐
                ▼                           ▼
          持久化插件                 运行时注册
          (写 cmx_plugin,          (Registry+Cache)
           复制 wasm)
                │
          [建表 DDL —— 已注释]
          [中心分发 —— 已注释]
                │
          审计 + 事件
```

---

**方案完成。** 按第六节 6 个阶段实施,每阶段产出可独立测试。如需把某阶段拆解为带测试代码的逐步 TDD 实施计划,可基于本方案再生成。
