# cmx-dct/

> 数据字典（DCT）域分组：api（HTTP 皮肤）+ model（语义中立 / SQL 构造）+ store-pg（PG 持久化）经典三件套。

## 分组定位

本分组按平台标准的**三件套**模式组织数据字典域：`cmx-dct-api` 是 HTTP
协议皮肤，薄 axum handler 提参数、调服务、封 `ApiResp` / msgpack 信封；
`cmx-dct-model` 是 DB-free 的语义中立层，负责强类型字典视图与参数化 SQL
构造；`cmx-dct-store-pg` 负责 SQL 执行、事务编排、编码铸号与流式导入导出。
三层职责单向依赖：api → store-pg → model，端点路径与迁移前完全一致。

## 子 crate 清单

| 子 crate | 职责 | README |
| --- | --- | --- |
| `cmx-dct-api` | DCT 模块的 HTTP 协议皮肤：薄 axum handler（提取参数 → `resolve_dict` 解析字典视图 → 调 store-pg 服务 → `ApiResp` / msgpack 信封）+ `DctModule`（impl `cmx_api_core::ModuleRoutes`）路由聚合，端点路径与迁移前完全一致（`/dct/*`） | [README](./cmx-dct-api/README.md) |
| `cmx-dct-model` | DCT 模块的语义中立层（DB-free）：字典表强类型视图（`DictView` / `DictColumn`）、请求坐标 DTO（`DctQuery`），以及列白名单校验、主键铸号 / 临时 id 识别 / 自分级 `parent_id` 重指向、search / upsert / 批量导入导出的参数化 SQL 构造（`$N` 占位 + `DataValue` 绑定） | [README](./cmx-dct-model/README.md) |
| `cmx-dct-store-pg` | DCT 的 PostgreSQL 持久化 / 服务层：SQL 文本全部由 `cmx-dct-model` 构造，本层负责执行、事务编排、编码铸号与流式导入导出 | [README](./cmx-dct-store-pg/README.md) |

## 组织规则

- 三层单向依赖：`cmx-dct-api` → `cmx-dct-store-pg` → `cmx-dct-model`。
- SQL 生成与执行分离：model 只产 SQL 文本与参数绑定，store-pg 才碰连接与事务。
- model 层 DB-free，可在无数据库环境下独立单测。
- 端点前缀：`/dct/*`（与迁移前路径完全一致）。

## 相关背景

- 同款三件套分组：`../cmx-doc/`（业务单据）、`../cmx-mdm/`（主数据）、
  `../cmx-job/`（任务中心，第二层为 core）。
- 主键铸号由 `../cmx-code/` 的 `CodeMinter`（`CodeEngine`）提供。
- HTTP 共享骨架（`CmxAppState` / `ModuleRoutes`）见 `../cmx-apis/cmx-api-core/`。
