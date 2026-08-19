# cmx-mdm/

> 主数据管理（MDM）域分组：api（HTTP 皮肤 + 流程回环 + 分发引擎）+ model（语义中立层）+ store-pg（PG 持久化）三件套。

## 分组定位

本分组按平台标准**三件套**模式组织主数据管理域：`cmx-mdm-api` 是 HTTP
协议皮肤（额外内置 M7 流程平台回环客户端与 M5 分发订阅引擎）；
`cmx-mdm-model` 是纯逻辑、DB-free 的语义中立层；`cmx-mdm-store-pg` 编排
激活 / 合并 / 还原三套单事务主流程并集合域内全部 store。
主数据接入全链路元数据驱动、复用 `cv_mdm_apply` CR 单据。

## 子 crate 清单

| 子 crate | 职责 | README |
| --- | --- | --- |
| `cmx-mdm-api` | MDM 模块的 HTTP 协议皮肤：薄 axum handler 集合 + `MdmModule`（实现 `cmx-api-core` 的 `ModuleRoutes`）路由聚合 + M7 流程平台回环客户端 + M5 分发订阅引擎，由 web-server 合并进主路由（`/api/mdm/*`） | [README](./cmx-mdm-api/README.md) |
| `cmx-mdm-model` | MDM 模块的语义中立层：纯逻辑、DB-free——主数据生命周期状态、激活器字段搬运规则、匹配 / 聚类算法、字段级存活策略与分发通道契约，可独立单测 | [README](./cmx-mdm-model/README.md) |
| `cmx-mdm-store-pg` | MDM 的 PostgreSQL 持久化 / 服务层：激活器 / 合并 / 还原三套单事务主流程编排，`cm_*` 主数据写入闸口，CR 单据、治理表、匹配组、查重配置、扫描发现项与分发引擎的 store 集合 | [README](./cmx-mdm-store-pg/README.md) |

## 组织规则

- 三层单向依赖：`cmx-mdm-api` → `cmx-mdm-store-pg` → `cmx-mdm-model`。
- 主数据写入统一走 store-pg 的 `cm_*` 闸口，保证审计与治理约束生效。
- 主键铸号复用 `../cmx-code/` 的 `CodeMinter`；端点前缀 `/api/mdm/*`。

## 相关背景

- 同款三件套分组：`../cmx-dct/`、`../cmx-doc/`、`../cmx-job/`。
- MDM 审批流经流程平台：引擎在独立 workspace `cmx-flowengine`，平台侧
  反代薄壳见 `../cmx-flow/cmx-flow-api/`。
