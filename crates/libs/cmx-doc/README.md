# cmx-doc/

> 业务单据（DOC）域分组：api（HTTP 皮肤）+ model（语义中立 / SQL 生成）+ store-pg（PG 持久化）经典三件套。

## 分组定位

本分组按平台标准的**三件套**模式组织业务单据域：`cmx-doc-api` 是 HTTP
协议皮肤；`cmx-doc-model` 是 DB-free 的语义中立层，把单据定义 JSON 解析为
强类型 `DocMetaView` 并生成对 tokio-postgres / sqlx 双驱动通用的 SQL；
`cmx-doc-store-pg` 负责装载 / 回存 / 版本化与元数据缓存。
三层单向依赖：api → store-pg → model。

## 子 crate 清单

| 子 crate | 职责 | README |
| --- | --- | --- |
| `cmx-doc-api` | DOC 模块的 HTTP 协议皮肤：薄 axum handler（提取参数 → 解析 `DocMetaView`（带缓存）→ 调 store-pg 装载 / 回存 → `ApiResp` / msgpack 信封）+ `DocModule` 路由聚合，端点路径与迁移前完全一致（`/doc/*`） | [README](./cmx-doc-api/README.md) |
| `cmx-doc-model` | DOC 模块的语义中立层（DB-free）：单据定义 JSON → 强类型 `DocMetaView`（层序 / 各层列 / 父子关系），富查询模型（`DocQuery` / `Filter` / 游标）、公式求值、校验规则、层级 SELECT 生成，SQL 对 tokio-postgres 与 sqlx 双驱动通用 | [README](./cmx-doc-model/README.md) |
| `cmx-doc-store-pg` | DOC 的 PostgreSQL 持久化 / 服务层：`cv_*` 单据物理表装载（`DocLoader` 全拷贝 + `ZmcDocLoader` 零拷贝双驱动）、回存（`DocSaver` merge / replace 双模式 + 铸号 + 审计 + 乐观锁）、版本化（`DocRevision` 列式快照台账）与 `DocMetaView` 进程内缓存 | [README](./cmx-doc-store-pg/README.md) |

## 组织规则

- 三层单向依赖：`cmx-doc-api` → `cmx-doc-store-pg` → `cmx-doc-model`。
- SQL 生成与执行分离：model 产通用 SQL 文本，store-pg 才碰连接与事务。
- 单据 CRUD 页面（列表 / 详情 / 新建三页一体）的后端即由本分组支撑。
- 端点前缀：`/doc/*`。

## 相关背景

- 同款三件套分组：`../cmx-dct/`（数据字典）、`../cmx-mdm/`（主数据）、
  `../cmx-job/`（任务中心）。
- 主键铸号由 `../cmx-code/` 的 `CodeMinter`（`CodeEngine`）提供；
  零拷贝行来源抽象见 `../cmx-infra/cmx-rowsource/`。
