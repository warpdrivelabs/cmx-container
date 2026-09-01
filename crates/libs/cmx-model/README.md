# cmx-model/

> 模型中心域分组。引擎已抽为**独立微服务**（对标 flow/report/rules/mdm），本分组现仅剩**平台反代薄壳**。

## 分组现状

| crate | 说明 |
| --- | --- |
| `cmx-model-proxy` | 平台反代薄壳：`ModelProxyModule` 把门户模型中心七前缀（`/api/{dct,dict,doc,model,definitions,flexible-combination,code}/*`）+ `portal.model.*` native/html 页取页请求透明转发到独立 cmx-model-server（`[service_rpc.services].model` per-key 定位）。**无进程内嵌兜底**：没配目标 = 门户不挂模型中心路由 |

## 迁移对照

原「api + meta + deploy 三件套」及配套域库已物理迁至独立 workspace `../cmx-model`
（与 cmx-container 并排），由那边的 `cmx-model-server`（:8093 chassis bin）承载：

| 本分组原 crate | 去向（../cmx-model） |
| --- | --- |
| `cmx-model-api`（HTTP 皮肤 + 路由聚合） | 收敛进中立核 `cmx-model-app`，由 `cmx-model-server` 承载 |
| `cmx-model-meta`（设计期元数据建模） | `crates/cmx-model-meta` |
| `cmx-model-deploy`（建库与模块部署台账） | `crates/cmx-model-deploy` |
| dct/doc/code/master-slave 域库 | `crates/cmx-dct-*` / `cmx-doc-*` / `cmx-code-*` / `cmx-master-slave` |

模型中心微服务同时承载 dct/doc/model/code 四能力，对外 URL 与平台一致（无 `/v1`），
壳的转发为恒等映射 `{model_base}/api{path}{query}`；切换只看
`[service_rpc.services].model`——配了才挂模型中心路由，前端零改。

## 相关背景

- 转发核（头卫生 / 三层出站鉴权 / 超时拆分 / 流式转发 / 502/503 兜底）在
  `../cmx-proxy-core`，各域反代壳共用。
- 主数据域同构抽取：见 `../cmx-mdm/`（仅剩 `cmx-mdm-proxy` 反代薄壳）。
- 运行期消费定义的报表域薄壳：`../cmx-rpt/`；报表引擎见独立 workspace `../cmx-report`。
