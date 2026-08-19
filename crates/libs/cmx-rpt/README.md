# cmx-rpt/

> 报表域的平台**反代薄壳**分组（proxy-only）：引擎本体已迁独立 workspace，此处仅保留网关转发 crate `cmx-rpt-api`。

## 分组定位

报表引擎已从 cmx-container 迁出，落地为**独立报表微服务 workspace**
`cmx-report`（位于仓库根目录、与 `cmx-container` 平级，即相对 cmx-container
根的 `../cmx-report`）。本分组只剩一个反代薄壳 crate：门户进程内的
`cmx-rpt-api` 把报表相关请求透明转发到独立服务的 `cmx-rpt-server`，
前端与调用方零改动。

## 子 crate 清单

| 子 crate | 职责 | README |
| --- | --- | --- |
| `cmx-rpt-api` | 报表域的平台**反代薄壳**（proxy-only）：把门户 `/api/report-design/*`、`/api/report-source-bindings/*`、`/api/rpt/*` 与报表拥有的前端页取页请求透明转发到独立报表微服务的 `cmx-rpt-server`，前端零改 | [README](./cmx-rpt-api/README.md) |

## 目录结构

```text
cmx-rpt/
└── cmx-rpt-api/    # 反代薄壳 crate（本分组唯一成员）
```

## 组织规则

- **薄壳不写业务**：本分组不承载报表设计器 / 数据源绑定等任何业务实现，
  只做 HTTP 透传与必要的鉴权 / 头部处理。
- 报表引擎与前端 Web 资产真源在独立 workspace `cmx-report`
  （其服务进程为 `cmx-rpt-server`）。
- 端点前缀：`/api/report-design/*`、`/api/report-source-bindings/*`、
  `/api/rpt/*` 及报表拥有的前端页取页请求。

## 相关背景

- 同为反代薄壳的兄弟分组：`../cmx-flow/`（转发至 `cmx-flowengine`）、
  `../cmx-rule/`（转发至 `cmx-rulesengine`）。
- 报表定义（RPT）随模块部署落地：见 `../cmx-model/cmx-model-deploy/`。
