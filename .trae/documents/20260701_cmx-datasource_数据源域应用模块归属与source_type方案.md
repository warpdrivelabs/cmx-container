# 数据源域应用模块归属与 source_type 改造方案

> 评审通过版本。实施时按 Task 1→6 顺序推进。

## 需求
- `cmx_sys_datasource` 表新增 domain_code/application_code/module_code（所属域/应用/模块）和 source_type（default/biz/other）字段
- 去除 db_id 的唯一索引（DB 层不强制唯一），但同一配置文件内 db_id 不允许重复
- `load_active_datasources` 按当前实例的域应用模块过滤
- `persist_datasource_configs` 持久化查询也带上域应用模块
- 本实例的域应用模块标识配置在 `[app]` TOML 节，支持环境变量覆盖（APP__DOMAIN_CODE 格式）
- 向后兼容：迁移时回填默认值

## 决策确认
| 决策点 | 选择 |
|---|---|
| 字段命名 | domain_code/application_code/module_code VARCHAR(64)（与 cmx_plugin 一致）|
| source_type | default/biz/other，与 default_flag 共存（正交）|
| 本实例标识 | `[app]` TOML 节 + 环境变量覆盖 |
| 配置文件数据源 | [[databases]] 不配域，统一归属 [app] 节声明的本实例域 |
| DbConfig | 加可选字段（Option + serde default）|
| db_id 唯一性 | DB 无唯一索引，配置文件层去重 |
| persist 查询 | db_id + domain_code + application_code + module_code |
| 向后兼容 | 迁移时回填默认值 |

## 改动清单

### 1. SQL（sql-guide + pg-table-generator 规范）
- migration `20260701_001_datasource_domain_app_module.up/down.sql`：ALTER ADD 4 字段 + 回填默认值 + DROP db_id 唯一索引 + CREATE 联合索引
- `init_ddl.sql` 同步：表定义加 4 字段，删唯一索引，加联合索引

### 2. cmx-database（DbConfig 加字段）
- DbConfig 新增 domain_code/application_code/module_code/source_type（Option + serde default）

### 3. cmx-biz（Entity/Filter 加字段）
- entity.rs：SysDatasource / SysDatasourceForCreate / SysDatasourceForUpdate 各加 4 字段
- filter.rs：SysDatasourceFilter 加 4 个 OpValsString

### 4. web-server（核心逻辑）
- config/mod.rs：AppIdentity 结构 + load_app_identity() 从 [app] 节读取
- datasource.rs：init_datasources 填充域字段、persist 按域查重、load_active 按域过滤、转换函数填充新字段

### 5. 配置同步（config-sync 技能）
- config_template.toml 新增 [app] 节
- CONFIG_MANUAL.md 新增 [app] 节文档

## 验收标准
- [ ] 表有 4 个新字段，db_id 唯一索引已移除
- [ ] persist 按域应用模块查重，load_active 按域过滤
- [ ] [app] 节 + 环境变量覆盖可用
- [ ] config_template.toml + CONFIG_MANUAL.md 已同步
- [ ] 迁移回填默认值，向后兼容
- [ ] cargo check --workspace 通过，clippy --tests 零 warning
