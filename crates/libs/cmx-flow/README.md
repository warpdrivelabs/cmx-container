# cmx-flow/

> 流程域的平台**反代薄壳**分组（proxy-only）：引擎本体已迁独立 workspace，此处仅保留网关转发 crate `cmx-flow-api`。

## 分组定位

流程引擎已从 cmx-container 迁出，落地为**独立流程微服务 workspace**
`cmx-flowengine`（位于仓库根目录、与 `cmx-container` 平级，即相对
cmx-container 根的 `../cmx-flowengine`）。本分组只剩一个反代薄壳 crate：
门户进程内的 `cmx-flow-api` 把流程相关请求透明转发到独立服务的
`cmx-flow-server`，前端与调用方零改动。

## 子 crate 清单

| 子 crate | 职责 | README |
| --- | --- | --- |
| `cmx-flow-api` | 流程引擎的平台**反代薄壳**（proxy-only）：把门户 `/api/flow/*` 与流程拥有的前端页取页请求透明转发到独立流程微服务的 `cmx-flow-server`，前端零改 | [README](./cmx-flow-api/README.md) |

## 目录结构

```text
cmx-flow/
└── cmx-flow-api/    # 反代薄壳 crate（本分组唯一成员）
```

## 组织规则

- **薄壳不写业务**：本分组不承载流程编排 / BPMN / 待办等任何业务实现，
  只做 HTTP 透传与必要的鉴权 / 头部处理。
- 引擎、流程定义部署、待办中心等全部能力的真源在独立 workspace
  `cmx-flowengine`（其服务进程为 `cmx-flow-server`）。
- 端点前缀：`/api/flow/*` 及流程拥有的前端页取页请求。

## 相关背景

- 同为反代薄壳的兄弟分组：`../cmx-rpt/`（转发至 `cmx-report`）、
  `../cmx-rule/`（转发至 `cmx-rulesengine`）。
- 平台内驻留的域 HTTP 皮肤集中在 `../cmx-apis/`；
  主数据接流程的回环客户端见 `../cmx-mdm/cmx-mdm-api/`。
