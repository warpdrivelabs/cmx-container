# cmx-rule/

> 决策规则引擎域的平台**反代薄壳**分组（proxy-only）：引擎本体已迁独立 workspace，此处仅保留网关转发 crate `cmx-rule-api`。

## 分组定位

决策规则引擎已从 cmx-container 迁出，落地为**独立规则微服务 workspace**
`cmx-rulesengine`（位于仓库根目录、与 `cmx-container` 平级，即相对
cmx-container 根的 `../cmx-rulesengine`）。本分组只剩一个反代薄壳 crate：
门户进程内的 `cmx-rule-api` 把规则相关请求透明转发到独立服务的
`cmx-rule-server`，前端与调用方零改动。薄壳无内嵌引擎逻辑，
规则决策的真源完全在独立服务侧。

## 子 crate 清单

| 子 crate | 职责 | README |
| --- | --- | --- |
| `cmx-rule-api` | 决策规则引擎的平台**反代薄壳**（proxy-only，无内嵌）：把门户 `/api/rules/*` 与规则拥有的前端页取页请求透明转发到独立规则微服务的 `cmx-rule-server`，前端零改 | [README](./cmx-rule-api/README.md) |

## 目录结构

```text
cmx-rule/
└── cmx-rule-api/    # 反代薄壳 crate（本分组唯一成员）
```

## 组织规则

- **薄壳不写业务**：本分组不承载规则集 / 决策表 / 表达式求值等任何引擎
  实现，只做 HTTP 透传与必要的鉴权 / 头部处理。
- 引擎能力（规则定义、版本、执行、审计）真源在独立 workspace
  `cmx-rulesengine`（其服务进程为 `cmx-rule-server`）。
- 端点前缀：`/api/rules/*` 及规则拥有的前端页取页请求。

## 相关背景

- 同为反代薄壳的兄弟分组：`../cmx-flow/`（转发至 `cmx-flowengine`）、
  `../cmx-rpt/`（转发至 `cmx-report`）。
- 平台内驻留的域 HTTP 皮肤集中在 `../cmx-apis/`。
