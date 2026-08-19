# cmx-model/

> 模型中心域分组：api（HTTP 皮肤）+ meta（设计期元数据建模，纯 JSON 文件存储）+ deploy（建库与模块部署）三件套。

## 分组定位

本分组组织**模型中心**域，负责平台的设计期定义与落地：`cmx-model-meta`
承载定义中心（DCT / DOC / BASE）、弹性组合规则引擎与字典检索引擎的
元数据建模——纯 JSON 文件存储、不落库；`cmx-model-deploy` 把定义编译成
`TableDefine` 真实建到目标库并维护部署台账；`cmx-model-api` 提供 HTTP
皮肤与路由聚合。与 dct / doc / mdm 三件套的差异：第二层不是 DB-free 的
SQL 构造层，而是"设计期建模 + 部署落地"双轨。

## 子 crate 清单

| 子 crate | 职责 | README |
| --- | --- | --- |
| `cmx-model-meta` | 模型中心元数据层：定义中心（DCT 数据字典 / DOC 业务单据 / BASE 字段集模板）、弹性组合规则引擎与字典检索引擎的设计期元数据建模服务——纯 JSON 文件存储，不落库 | [README](./cmx-model-meta/README.md) |
| `cmx-model-deploy` | 模型中心的数据库初始化与模块部署层：把 DCT / DOC / RPT / SEED / MENU 定义编译成 `TableDefine` 真实建到目标库，并维护 5 张台账系统表（模块部署台账 / 部署历史 / 源 JSON 留档） | [README](./cmx-model-deploy/README.md) |
| `cmx-model-api` | 模型中心 HTTP 协议皮肤：定义中心（DCT / DOC / BASE）+ 弹性组合 + 数据库初始化与模块部署的薄 axum handler 与路由聚合（`ModelModule`） | [README](./cmx-model-api/README.md) |

## 组织规则

- 依赖方向：`cmx-model-api` → `cmx-model-deploy` / `cmx-model-meta`。
- 设计期（meta，文件存储）与运行期落地（deploy，建库 + 台账）分离。
- 部署产物经 cmx-plugin 的模块迁移包导入导出（见 `../cmx-apis/cmx-plugin-api/`）。

## 相关背景

- 运行期消费定义的域分组：`../cmx-dct/`、`../cmx-doc/`、`../cmx-mdm/`。
- 报表定义（RPT）经 deploy 一并部署，报表引擎见独立 workspace
  `cmx-report`，平台侧薄壳 `../cmx-rpt/`。
